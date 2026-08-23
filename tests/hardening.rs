//! What the parser REFUSES.
//!
//! Every case here is a document this crate must reject rather than interpret.
//! The consumer parses graphs that arrived over a wire and executes them, so a
//! lenient reader is not a convenience — it is the gap between what the sender
//! wrote and what the runtime ran.

use fancy_json::{parse, parse_with, ErrorKind, ParseOptions};

fn err(input: &str) -> ErrorKind {
    parse(input).expect_err(&alloc_msg(input)).kind
}

fn alloc_msg(input: &str) -> String {
    format!("expected {input:?} to be refused, but it parsed")
}

// -- strictness ----------------------------------------------------------

#[test]
fn refuses_trailing_data_after_the_value() {
    // The classic: a reader that stops at the first complete value accepts a
    // document whose remainder the sender believed was included.
    assert_eq!(err("{} {}"), ErrorKind::TrailingData);
    assert_eq!(err("1 2"), ErrorKind::TrailingData);
    assert_eq!(err("nullnull"), ErrorKind::TrailingData);
}

#[test]
fn refuses_an_empty_or_whitespace_only_document() {
    assert_eq!(err(""), ErrorKind::UnexpectedEof);
    assert_eq!(err("   \n"), ErrorKind::UnexpectedEof);
}

#[test]
fn refuses_trailing_commas() {
    assert_eq!(err("[1,]"), ErrorKind::UnexpectedChar);
    assert_eq!(err(r#"{"a":1,}"#), ErrorKind::UnexpectedChar);
}

#[test]
fn refuses_comments() {
    assert_eq!(err("// x\n1"), ErrorKind::UnexpectedChar);
    assert_eq!(err("/* x */ 1"), ErrorKind::UnexpectedChar);
    assert_eq!(err("[1 /* x */]"), ErrorKind::UnexpectedChar);
}

#[test]
fn refuses_javascript_spellings_that_are_not_json() {
    assert_eq!(err("'a'"), ErrorKind::UnexpectedChar);
    assert_eq!(err("{a:1}"), ErrorKind::UnexpectedChar);
    assert_eq!(err("NaN"), ErrorKind::UnexpectedChar);
    assert_eq!(err("Infinity"), ErrorKind::UnexpectedChar);
    assert_eq!(err("-Infinity"), ErrorKind::InvalidNumber);
    assert_eq!(err("undefined"), ErrorKind::UnexpectedChar);
    assert_eq!(err("True"), ErrorKind::UnexpectedChar);
}

// -- the number grammar, per RFC 8259 -----------------------------------

#[test]
fn refuses_numbers_json_does_not_allow() {
    assert_eq!(err("01"), ErrorKind::TrailingData); // a leading zero ends the number
    assert_eq!(err("+1"), ErrorKind::UnexpectedChar);
    assert_eq!(err(".5"), ErrorKind::UnexpectedChar);
    assert_eq!(err("5."), ErrorKind::InvalidNumber);
    assert_eq!(err("1e"), ErrorKind::InvalidNumber);
    assert_eq!(err("1e+"), ErrorKind::InvalidNumber);
    assert_eq!(err("-"), ErrorKind::InvalidNumber);
    assert_eq!(err("1.2.3"), ErrorKind::TrailingData);
    assert_eq!(err("0x10"), ErrorKind::TrailingData);
}

#[test]
fn refuses_an_integer_too_large_to_represent_exactly() {
    // The decision this crate exists for. serde_json (without
    // `arbitrary_precision`) turns an out-of-range integer into an f64, which
    // is a SILENT precision loss on a value the sender wrote exactly. Here it
    // is an error, and the caller opts in to the lossy reading if it wants one.
    assert_eq!(err("18446744073709551616"), ErrorKind::NumberOutOfRange);
    assert_eq!(err("-9223372036854775809"), ErrorKind::NumberOutOfRange);

    let lossy = ParseOptions::new().with_lossy_big_integers(true);
    let v = parse_with("18446744073709551616", lossy).unwrap();
    assert_eq!(
        v.as_u64(),
        None,
        "a lossy read must not masquerade as exact"
    );
    assert_eq!(v.as_f64(), Some(18_446_744_073_709_551_616.0));
}

#[test]
fn refuses_a_float_literal_that_is_not_finite() {
    // `1e400` overflows to infinity, which is not representable in JSON — so a
    // reader that accepts it produces a value it cannot write back out.
    assert_eq!(err("1e400"), ErrorKind::NumberOutOfRange);
    assert_eq!(err("-1e400"), ErrorKind::NumberOutOfRange);
    // Underflow to zero is ordinary IEEE behaviour, not an error.
    assert_eq!(parse("1e-400").unwrap().as_f64(), Some(0.0));
}

// -- strings -------------------------------------------------------------

#[test]
fn refuses_an_unescaped_control_character() {
    assert_eq!(err("\"a\nb\""), ErrorKind::ControlCharacterInString);
    assert_eq!(err("\"a\tb\""), ErrorKind::ControlCharacterInString);
    assert_eq!(err("\"a\u{0}b\""), ErrorKind::ControlCharacterInString);
    // 0x7F is not a JSON control character; only U+0000..U+001F are.
    assert_eq!(parse("\"a\u{7f}b\"").unwrap().as_str(), Some("a\u{7f}b"));
}

#[test]
fn refuses_an_unknown_escape() {
    assert_eq!(err(r#""\x41""#), ErrorKind::InvalidEscape);
    assert_eq!(err(r#""\'""#), ErrorKind::InvalidEscape);
    assert_eq!(err(r#""\U0041""#), ErrorKind::InvalidEscape);
}

#[test]
fn refuses_a_malformed_unicode_escape() {
    assert_eq!(err(r#""\u00""#), ErrorKind::InvalidEscape);
    assert_eq!(err(r#""\uZZZZ""#), ErrorKind::InvalidEscape);
}

#[test]
fn refuses_a_lone_surrogate() {
    // A high surrogate with no low one, and a bare low surrogate. Both are
    // unrepresentable in a Rust `String`; a reader that substitutes U+FFFD
    // silently changes the document.
    assert_eq!(err(r#""\uD83D""#), ErrorKind::LoneSurrogate);
    assert_eq!(err(r#""\uD83Dx""#), ErrorKind::LoneSurrogate);
    assert_eq!(err(r#""\uDE00""#), ErrorKind::LoneSurrogate);
    assert_eq!(err(r#""\uD83DA""#), ErrorKind::LoneSurrogate);
}

#[test]
fn refuses_an_unterminated_string() {
    assert_eq!(err("\"abc"), ErrorKind::UnexpectedEof);
    assert_eq!(err("\"abc\\"), ErrorKind::UnexpectedEof);
}

// -- structure -----------------------------------------------------------

#[test]
fn refuses_unterminated_containers() {
    assert_eq!(err("[1,2"), ErrorKind::UnexpectedEof);
    assert_eq!(err(r#"{"a":1"#), ErrorKind::UnexpectedEof);
    assert_eq!(err("["), ErrorKind::UnexpectedEof);
    assert_eq!(err("{"), ErrorKind::UnexpectedEof);
}

#[test]
fn refuses_a_non_string_object_key() {
    assert_eq!(err("{1:2}"), ErrorKind::UnexpectedChar);
}

#[test]
fn refuses_nesting_deeper_than_the_cap_instead_of_overflowing_the_stack() {
    // The one refusal that is a DoS control rather than a grammar rule. A
    // recursive-descent reader with no cap dies by stack overflow on a document
    // an attacker can write in a few hundred bytes — and a stack overflow in a
    // blockchain node is not an exception it can catch.
    let deep = "[".repeat(50_000) + &"]".repeat(50_000);
    assert_eq!(err(&deep), ErrorKind::DepthLimitExceeded);

    let deep_obj = r#"{"a":"#.repeat(50_000) + "1" + &"}".repeat(50_000);
    assert_eq!(err(&deep_obj), ErrorKind::DepthLimitExceeded);

    // The cap is a number the caller can see and lower, not a magic constant.
    assert_eq!(
        ParseOptions::new().max_depth(),
        fancy_json::DEFAULT_MAX_DEPTH
    );
    let shallow = ParseOptions::new().with_max_depth(3);
    assert!(parse_with("[[[1]]]", shallow).is_ok());
    assert_eq!(
        parse_with("[[[[1]]]]", shallow).unwrap_err().kind,
        ErrorKind::DepthLimitExceeded
    );
}

// -- errors that can be acted on -----------------------------------------

#[test]
fn an_error_reports_where_it_happened() {
    let e = parse("{\n  \"a\": 1,\n  \"b\": x\n}").unwrap_err();
    assert_eq!(e.line, 3, "line is 1-based");
    assert_eq!(e.column, 8, "column is 1-based and counts characters");
    assert!(
        e.to_string().contains("line 3"),
        "the Display form must locate the fault: {e}"
    );
}

#[test]
fn error_positions_count_characters_not_bytes() {
    // A byte offset reported as a column points into the middle of a character
    // for anyone whose data is not ASCII, which is most senders.
    let e = parse("[\"日本語\", x]").unwrap_err();
    assert_eq!(e.line, 1);
    assert_eq!(e.column, 9);
}
