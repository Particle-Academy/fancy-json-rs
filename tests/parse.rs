//! What the parser accepts, and what it produces.

use fancy_json::{parse, Number, Value};

#[test]
fn parses_every_scalar() {
    assert_eq!(parse("null").unwrap(), Value::Null);
    assert_eq!(parse("true").unwrap(), Value::Bool(true));
    assert_eq!(parse("false").unwrap(), Value::Bool(false));
    assert_eq!(parse("\"hi\"").unwrap(), Value::from("hi"));
    assert_eq!(parse("0").unwrap(), Value::from(0));
}

#[test]
fn parses_containers() {
    let v = parse(r#"{"a":[1,2,{"b":null}]}"#).unwrap();
    let a = v.get("a").unwrap().as_array().unwrap();
    assert_eq!(a.len(), 3);
    assert_eq!(a[0], Value::from(1));
    assert_eq!(a[2].get("b"), Some(&Value::Null));
}

#[test]
fn ignores_whitespace_between_tokens() {
    // The four JSON whitespace characters, and only those.
    let v = parse(" \t\r\n{ \"a\" : [ 1 , 2 ] } \t\r\n").unwrap();
    assert_eq!(v.get("a").unwrap().as_array().unwrap().len(), 2);
}

// -- numbers: the whole point of this crate ------------------------------

#[test]
fn an_integer_literal_stays_an_integer() {
    // 2^53 + 1. Round-tripping this through f64 gives 9007199254740992 — the
    // silent corruption every "just use a double" JSON reader performs on an
    // account balance in minor units.
    let v = parse("9007199254740993").unwrap();
    assert_eq!(v.as_u64(), Some(9_007_199_254_740_993));
    assert!(matches!(v, Value::Number(Number::PosInt(_))));

    let v = parse("-9007199254740993").unwrap();
    assert_eq!(v.as_i64(), Some(-9_007_199_254_740_993));
    assert!(matches!(v, Value::Number(Number::NegInt(_))));
}

#[test]
fn u64_max_survives() {
    let v = parse("18446744073709551615").unwrap();
    assert_eq!(v.as_u64(), Some(u64::MAX));
}

#[test]
fn a_fraction_or_exponent_makes_it_a_float() {
    assert!(matches!(
        parse("1.5").unwrap(),
        Value::Number(Number::Float(_))
    ));
    assert!(matches!(
        parse("1e3").unwrap(),
        Value::Number(Number::Float(_))
    ));
    // `1.0` is written with a fraction, so it is a float even though its value
    // is integral. The literal's SHAPE decides, not its value — otherwise a
    // document could not express "this is a measurement, not a count".
    assert!(matches!(
        parse("1.0").unwrap(),
        Value::Number(Number::Float(_))
    ));
    assert_eq!(parse("1e3").unwrap().as_f64(), Some(1000.0));
    assert_eq!(parse("-0.5").unwrap().as_f64(), Some(-0.5));
}

#[test]
fn an_integer_is_readable_as_a_float_but_not_the_reverse() {
    // A caller that genuinely wants a float gets one; a caller asking for an
    // exact integer NEVER silently receives a rounded float.
    assert_eq!(parse("3").unwrap().as_f64(), Some(3.0));
    assert_eq!(parse("3.0").unwrap().as_i64(), None);
    assert_eq!(parse("3.0").unwrap().as_u64(), None);
}

#[test]
fn a_negative_integer_is_not_readable_as_u64() {
    assert_eq!(parse("-1").unwrap().as_u64(), None);
    assert_eq!(parse("-1").unwrap().as_i64(), Some(-1));
}

#[test]
fn minus_zero_keeps_its_sign_as_a_float_but_zero_is_an_integer() {
    assert!(matches!(
        parse("0").unwrap(),
        Value::Number(Number::PosInt(0))
    ));
    // -0 has no integer representation distinct from 0, and JSON writers that
    // collapse it lose information a float carries. It parses as the integer 0,
    // matching every mainstream reader; -0.0 is available as a float literal.
    assert_eq!(parse("-0").unwrap().as_i64(), Some(0));
    assert_eq!(parse("-0.0").unwrap().as_f64(), Some(-0.0));
}

// -- strings -------------------------------------------------------------

#[test]
fn decodes_every_simple_escape() {
    let v = parse(r#""\"\\\/\b\f\n\r\t""#).unwrap();
    assert_eq!(v.as_str(), Some("\"\\/\u{8}\u{c}\n\r\t"));
}

#[test]
fn decodes_unicode_escapes() {
    assert_eq!(parse(r#""A""#).unwrap().as_str(), Some("A"));
    assert_eq!(parse(r#""é""#).unwrap().as_str(), Some("é"));
    // The em dash the fancy-flow runtimes disagreed on.
    assert_eq!(parse(r#""—""#).unwrap().as_str(), Some("—"));
}

#[test]
fn decodes_a_surrogate_pair() {
    // U+1F600, which cannot be expressed as a single \uXXXX escape.
    assert_eq!(parse(r#""😀""#).unwrap().as_str(), Some("😀"));
}

#[test]
fn accepts_raw_multibyte_utf8() {
    assert_eq!(parse("\"日本語 😀\"").unwrap().as_str(), Some("日本語 😀"));
}

// -- objects -------------------------------------------------------------

#[test]
fn object_keys_keep_their_insertion_order() {
    // Order is preserved because the peer runtimes preserve it: a PHP array, a
    // Python dict and a JS object all round-trip a document with its keys where
    // the author put them. `to_string_canonical` is where sorting lives.
    let v = parse(r#"{"z":1,"a":2,"m":3}"#).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["z", "a", "m"]);
}

#[test]
fn a_duplicate_key_keeps_the_last_value_at_the_first_position() {
    // Matches JavaScript, PHP and Python. Silently dropping the second value
    // instead would make a document mean different things in different readers.
    let v = parse(r#"{"a":1,"b":2,"a":3}"#).unwrap();
    let obj = v.as_object().unwrap();
    assert_eq!(obj.get("a"), Some(&Value::from(3)));
    assert_eq!(obj.keys().collect::<Vec<_>>(), ["a", "b"]);
}

#[test]
fn empty_containers_are_distinct() {
    // The distinction PHP cannot make, and the reason a fancy-flow golden
    // recorded `[]` for an empty header MAP for two years.
    assert_ne!(parse("[]").unwrap(), parse("{}").unwrap());
    assert!(parse("[]").unwrap().as_array().unwrap().is_empty());
    assert!(parse("{}").unwrap().as_object().unwrap().is_empty());
}
