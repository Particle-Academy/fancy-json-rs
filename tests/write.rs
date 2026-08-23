//! What the writers emit, and what round-trips.

use fancy_json::{parse, to_string, to_string_canonical, to_string_pretty, Map, Value};

#[test]
fn writes_every_scalar() {
    assert_eq!(to_string(&Value::Null), "null");
    assert_eq!(to_string(&Value::Bool(true)), "true");
    assert_eq!(to_string(&Value::from(0)), "0");
    assert_eq!(to_string(&Value::from(-7)), "-7");
    assert_eq!(to_string(&Value::from("hi")), "\"hi\"");
}

#[test]
fn writes_compactly_with_no_spaces() {
    // Separator-free, matching `JSON.stringify` and `json_encode`: an
    // interpolated object must read the same whichever runtime sent it.
    let v = parse(r#"{ "a" : [ 1 , 2 ] }"#).unwrap();
    assert_eq!(to_string(&v), r#"{"a":[1,2]}"#);
}

#[test]
fn a_float_keeps_a_decimal_point_so_it_round_trips_as_a_float() {
    // Rust's own `{}` prints `3` for 3.0_f64. Writing that loses the fact that
    // the value was a float, and the next reader gets an integer.
    let v = parse("3.0").unwrap();
    assert_eq!(to_string(&v), "3.0");
    assert_eq!(to_string(&parse("1e3").unwrap()), "1000.0");
    assert_eq!(to_string(&parse("-0.0").unwrap()), "-0.0");
    assert_eq!(to_string(&parse("1.5").unwrap()), "1.5");
}

#[test]
fn a_float_round_trips_exactly() {
    // Shortest round-trip formatting: writing then re-reading must give back
    // the identical double, or a checkpoint stops being a checkpoint.
    for text in [
        "0.1",
        "1e-7",
        "1.7976931348623157e308",
        "5e-324",
        "123456789.123456789",
    ] {
        let once = parse(text).unwrap();
        let twice = parse(&to_string(&once)).unwrap();
        assert_eq!(once.as_f64(), twice.as_f64(), "{text} did not round-trip");
    }
}

#[test]
fn escapes_exactly_what_json_requires_and_no_more() {
    let v = Value::from("q\"b\\s\u{8}\u{c}\n\r\tz\u{1}");
    assert_eq!(to_string(&v), r#""q\"b\\s\b\f\n\r\tz\u0001""#);
}

#[test]
fn does_not_escape_the_solidus_or_non_ascii() {
    // PHP's `json_encode` escapes `/` by default and every fancy-flow fixture
    // that carries a URL was written without those escapes. Non-ASCII stays
    // literal: the output is UTF-8, and `\u` escaping it triples the size of
    // every CJK document for no gain.
    assert_eq!(
        to_string(&Value::from("https://a.example/b")),
        "\"https://a.example/b\""
    );
    assert_eq!(to_string(&Value::from("日本語 😀")), "\"日本語 😀\"");
    assert_eq!(to_string(&Value::from("—")), "\"—\"");
}

#[test]
fn preserves_insertion_order() {
    let v = parse(r#"{"z":1,"a":2}"#).unwrap();
    assert_eq!(to_string(&v), r#"{"z":1,"a":2}"#);
}

#[test]
fn canonical_output_sorts_keys_at_every_depth() {
    // What a consumer hashes or signs. Insertion order is authored data;
    // canonical order is a function of the value alone, so two writers that
    // built the same value byte-identically agree.
    let v = parse(r#"{"z":{"y":1,"b":2},"a":[{"d":1,"c":2}]}"#).unwrap();
    assert_eq!(
        to_string_canonical(&v),
        r#"{"a":[{"c":2,"d":1}],"z":{"b":2,"y":1}}"#
    );
    // Arrays are NOT sorted — their order is the value, not a presentation of it.
    assert_eq!(to_string_canonical(&parse("[3,1,2]").unwrap()), "[3,1,2]");
}

#[test]
fn canonical_output_is_stable_across_insertion_orders() {
    let a = parse(r#"{"a":1,"b":2}"#).unwrap();
    let b = parse(r#"{"b":2,"a":1}"#).unwrap();
    assert_ne!(to_string(&a), to_string(&b));
    assert_eq!(to_string_canonical(&a), to_string_canonical(&b));
}

#[test]
fn canonical_key_order_is_by_unicode_scalar_value() {
    // Byte order and char order agree for UTF-8, but only if the comparison is
    // on the encoded bytes. Spelling it out because "sorted" is ambiguous and a
    // signature that depends on it cannot be renegotiated later.
    //
    // Note what this is NOT: not case-insensitive, and not locale-aware. Every
    // uppercase ASCII letter sorts before every lowercase one, and `É` (U+00C9)
    // sorts after both because its scalar value is larger — a collation that
    // grouped it with `E` would be a different function on every platform.
    //
    // '1' = U+0031, 'A' = U+0041, 'a' = U+0061, 'b' = U+0062, 'É' = U+00C9.
    let v = parse(r#"{"b":1,"A":2,"a":3,"É":4,"1":5}"#).unwrap();
    assert_eq!(
        to_string_canonical(&v),
        r#"{"1":5,"A":2,"a":3,"b":1,"É":4}"#
    );
}

#[test]
fn pretty_output_indents_and_round_trips() {
    let v = parse(r#"{"a":[1,{"b":null}],"c":{}}"#).unwrap();
    let pretty = to_string_pretty(&v, 2);
    assert_eq!(
        pretty,
        "{\n  \"a\": [\n    1,\n    {\n      \"b\": null\n    }\n  ],\n  \"c\": {}\n}"
    );
    assert_eq!(parse(&pretty).unwrap(), v);
}

#[test]
fn pretty_output_keeps_empty_containers_on_one_line() {
    assert_eq!(to_string_pretty(&parse("[]").unwrap(), 2), "[]");
    assert_eq!(to_string_pretty(&parse("{}").unwrap(), 2), "{}");
}

#[test]
fn every_document_round_trips_through_both_writers() {
    let docs = [
        "null",
        "[]",
        "{}",
        r#"{"a":1,"b":[true,false,null],"c":{"d":"e"}}"#,
        r#"["\u0001","\\","\"","日本語"]"#,
        "[0,-0,1.5,-2.25,1e-7]",
        "18446744073709551615",
        "-9223372036854775808",
    ];
    for doc in docs {
        let v = parse(doc).unwrap();
        assert_eq!(parse(&to_string(&v)).unwrap(), v, "compact: {doc}");
        assert_eq!(
            parse(&to_string_canonical(&v)).unwrap(),
            v,
            "canonical: {doc}"
        );
        assert_eq!(parse(&to_string_pretty(&v, 4)).unwrap(), v, "pretty: {doc}");
    }
}

#[test]
fn writing_is_iterative_so_a_deep_value_cannot_overflow_the_stack() {
    // Parsing caps depth; writing must survive whatever parsing produced, and
    // a value built in code is not capped at all.
    let mut v = Value::Null;
    for _ in 0..200_000 {
        v = Value::Array(vec![v]);
    }
    let text = to_string(&v);
    assert!(text.starts_with("[[[["));
    assert_eq!(text.len(), 400_004);
    drop(v); // and dropping it must not overflow either
}

#[test]
fn builds_a_value_without_parsing() {
    let mut obj = Map::new();
    obj.insert("kind", Value::from("@particle-academy/branch"));
    obj.insert("x", Value::from(1.5));
    obj.insert(
        "tags",
        Value::Array(vec![Value::from("a"), Value::from("b")]),
    );
    let v = Value::Object(obj);
    assert_eq!(
        to_string(&v),
        r#"{"kind":"@particle-academy/branch","x":1.5,"tags":["a","b"]}"#
    );
}
