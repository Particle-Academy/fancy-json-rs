//! Byte-determinism: the property a consensus consumer actually needs.
//!
//! Two validators on different targets must parse and re-serialise the same
//! value to the same bytes, forever. A divergence there is a chain halt, not a
//! bug.
//!
//! **The property is NOT "output reproduces input bytes".** No canonicalising
//! serialiser does that, and a chain does not want it: `1e3`, `1000.0` and
//! `1.0e3` are the same value, and a rule that preserved the spelling would
//! make the bytes depend on who typed them. What a chain needs is the other
//! direction — **same value, same bytes, everywhere, always**. These tests
//! measure that, and `input_bytes_are_not_reproduced_and_here_is_exactly_why`
//! measures the gap so nobody has to guess at it.
//!
//! Run with `--nocapture` to see the measured numbers and the corpus digest.

use fancy_json::{parse, to_string, to_string_canonical, Value};

/// A deterministic 64-bit PRNG, so the float sample is identical on every
/// machine and every run. `rand` would be a dependency, and this crate has none.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// FNV-1a. A digest two platforms can compare with one number.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Documents that exercise every axis a canonical form has to settle.
fn corpus() -> Vec<&'static str> {
    vec![
        "null",
        "true",
        "[]",
        "{}",
        "0",
        "-0",
        "18446744073709551615",
        "-9223372036854775808",
        "9007199254740993",
        "[0.1, -0.0, 1e-7, 1.5, 3.0, 1e3]",
        r#"{"z":1,"a":2,"m":3}"#,
        r#"{"a":1,"b":2,"a":3}"#,
        r#"{"b":{"y":1,"b":2},"a":[{"d":1,"c":2}]}"#,
        r#""\u0041\u00e9\ud83d\ude00""#,
        r#""日本語 😀 \t\n\\ \" \/""#,
        r#"["\u0000","\u001f","\u007f"]"#,
        r#"{"nested":{"deep":{"deeper":[1,[2,[3,[4]]]]}}}"#,
    ]
}

// -- floats ---------------------------------------------------------------

#[test]
fn every_finite_float_round_trips_bit_for_bit() {
    // The strongest float claim available: write then read reproduces the
    // IDENTICAL double, not merely a close one. Shortest-round-trip formatting
    // is what buys this, and without it a checkpoint stops being a checkpoint.
    let mut rng = Xorshift(0x2026_0823_0000_0001);
    let mut checked = 0_u32;
    let mut longest = 0_usize;
    let mut longest_text = String::new();

    // Every bit pattern, not just plausible-looking decimals: subnormals, huge
    // magnitudes and negative zero are where a formatter goes wrong.
    for _ in 0..200_000 {
        let value = f64::from_bits(rng.next());
        if !value.is_finite() {
            continue;
        }

        let text = to_string(&Value::from(value));
        let back = parse(&text).unwrap().as_f64().unwrap();

        assert_eq!(
            value.to_bits(),
            back.to_bits(),
            "{value:?} wrote as {text} and read back as {back:?}"
        );

        if text.len() > longest {
            longest = text.len();
            longest_text = text;
        }
        checked += 1;
    }

    assert!(checked > 190_000, "the sample should be almost all finite");
    println!("floats round-tripped bit-for-bit: {checked}");
    println!(
        "longest float rendering: {longest} bytes (starts {}...)",
        &longest_text[..longest_text.len().min(24)]
    );
}

#[test]
fn the_decimal_landmarks_render_to_exactly_these_bytes() {
    // Pinned literals, so a change in how Rust renders a float is a RED BUILD
    // here rather than a silent change in what two validators produce.
    //
    // This is the load-bearing test for the cross-version question. `core`'s
    // f64 `Display` is shortest-round-trip and platform-independent, but Rust
    // does not PROMISE the exact bytes across releases -- so the guarantee is
    // this assertion, not the standard library.
    let cases: &[(&str, &str)] = &[
        ("0.1", "0.1"),
        ("-0.0", "-0.0"),
        ("1.0", "1.0"),
        ("3.0", "3.0"),
        ("1e3", "1000.0"),
        ("1.5", "1.5"),
        ("1e-7", "0.0000001"),
        ("123456789.123456789", "123456789.12345679"),
    ];

    for (input, expected) in cases {
        let rendered = to_string(&parse(input).unwrap());
        assert_eq!(&rendered, expected, "{input} rendered as {rendered}");
    }

    // The extremes are pinned by LENGTH + DIGEST rather than by a 326-character
    // literal. That pins the bytes exactly and stays readable — a golden nobody
    // can read is a golden nobody will check, and the first draft of this test
    // hand-typed the 5e-324 expansion and got the digit count wrong.
    //
    // Every value below was produced by running this crate, not by reasoning
    // about what it should be.
    let extremes: &[(&str, usize, u64)] = &[
        ("5e-324", 326, 0xf0ef_7532_66be_25bc),
        ("1.7976931348623157e308", 311, 0xffcf_13d6_1201_75fb),
        ("1e300", 303, 0xac02_5558_8e4b_1bd2),
        ("1e-300", 302, 0xbe16_baef_4d85_72d0),
    ];

    for (input, length, digest) in extremes {
        let rendered = to_string(&parse(input).unwrap());
        assert_eq!(rendered.len(), *length, "{input} changed length");
        assert_eq!(fnv1a(rendered.as_bytes()), *digest, "{input} changed bytes");
    }
}

#[test]
fn rust_never_writes_exponent_notation_and_that_costs_bytes() {
    // Deterministic, and verbose. A chain paying for bytes should know: the
    // widest float is 311 bytes here where a Ryu-style writer emits 24.
    // Recorded as a measured cost, not a defect.
    for text in ["1e300", "1e-300", "1.7976931348623157e308", "5e-324"] {
        let rendered = to_string(&parse(text).unwrap());
        assert!(
            !rendered.contains(['e', 'E']),
            "{text} rendered with an exponent: {rendered}"
        );
        println!("{text:>24} -> {} bytes", rendered.len());
    }
}

// -- key ordering ---------------------------------------------------------

#[test]
fn canonical_output_is_a_function_of_the_value_alone() {
    // Whitespace, key order, escape spelling and duplicate position all vary in
    // the input; the canonical bytes must not.
    let spellings = [
        r#"{"a":1,"b":[2,3],"c":"A"}"#,
        r#"{ "b" : [ 2 , 3 ] , "a" : 1 , "c" : "A" }"#,
        "{\n  \"c\": \"\\u0041\",\n  \"a\": 1,\n  \"b\": [2, 3]\n}",
        r#"{"c":"x","a":1,"b":[2,3],"c":"A"}"#,
    ];

    let canonical: Vec<String> = spellings
        .iter()
        .map(|s| to_string_canonical(&parse(s).unwrap()))
        .collect();

    for (index, form) in canonical.iter().enumerate() {
        assert_eq!(
            form, &canonical[0],
            "spelling {index} produced different canonical bytes"
        );
    }
    assert_eq!(canonical[0], r#"{"a":1,"b":[2,3],"c":"A"}"#);
    println!("canonical form of 4 spellings: {}", canonical[0]);
}

#[test]
fn insertion_order_survives_the_plain_writer_and_only_the_plain_writer() {
    // Two writers, two deliberate answers. `to_string` keeps the order the
    // document was authored with, because every peer runtime does. Canonical
    // sorts, because a signature must not depend on authoring order.
    let value = parse(r#"{"z":1,"a":2}"#).unwrap();
    assert_eq!(to_string(&value), r#"{"z":1,"a":2}"#);
    assert_eq!(to_string_canonical(&value), r#"{"a":2,"z":1}"#);
}

#[test]
fn canonical_key_order_is_utf8_byte_order_at_every_depth() {
    // Not locale-aware, not case-insensitive, and stated so a signature that
    // depends on it cannot be renegotiated later.
    let value = parse(r#"{"b":1,"A":2,"a":3,"É":4,"1":5,"":6,"ß":7}"#).unwrap();
    assert_eq!(
        to_string_canonical(&value),
        r#"{"":6,"1":5,"A":2,"a":3,"b":1,"É":4,"ß":7}"#
    );
}

// -- idempotence ----------------------------------------------------------

#[test]
fn canonical_output_is_a_fixed_point_after_one_pass() {
    // THE property for a chain: canonicalise once, and re-canonicalising never
    // moves again. If this held only for the corpus below it would be weak, so
    // it also runs over every float rendering, which is where a writer that
    // was not shortest-round-trip would drift on the second pass.
    for document in corpus() {
        let once = to_string_canonical(&parse(document).unwrap());
        let twice = to_string_canonical(&parse(&once).unwrap());
        assert_eq!(once, twice, "not a fixed point: {document}");

        // And the plain writer is a fixed point too, given a canonical input.
        let plain = to_string(&parse(&once).unwrap());
        assert_eq!(plain, once, "plain writer moved a canonical document");
    }

    let mut rng = Xorshift(0x2026_0823_0000_0002);
    for _ in 0..20_000 {
        let value = f64::from_bits(rng.next());
        if !value.is_finite() {
            continue;
        }
        let once = to_string(&Value::from(value));
        let twice = to_string(&parse(&once).unwrap());
        assert_eq!(once, twice, "float rendering moved on the second pass");
    }
}

#[test]
fn the_corpus_digest_is_stable() {
    // One number to compare across targets and toolchains. If a validator on
    // another platform prints a different digest, the two would not agree on
    // bytes -- and this is how that is found in a test rather than in a halt.
    let mut joined = String::new();
    for document in corpus() {
        joined.push_str(&to_string_canonical(&parse(document).unwrap()));
        joined.push('\n');
    }

    let digest = fnv1a(joined.as_bytes());
    println!("corpus canonical digest (FNV-1a 64): {digest:#018x}");
    println!("corpus canonical bytes: {}", joined.len());

    // Pinned, and MEASURED — the first draft of this line was a placeholder,
    // which is the only honest way to write a golden you have not run yet.
    // A change here is a change in what every consumer serialises.
    assert_eq!(joined.len(), 299, "the canonical corpus changed size");
    assert_eq!(
        digest, 0x91bf_02f1_3e6d_8ab3,
        "the canonical corpus changed bytes"
    );
}

// -- what is NOT guaranteed ----------------------------------------------

#[test]
fn input_bytes_are_not_reproduced_and_here_is_exactly_why() {
    // Measured, so the gap is a list rather than a caveat. Every row is a case
    // where output != input, and every one is deliberate: the parser keeps the
    // VALUE, not the spelling.
    let lossy: &[(&str, &str, &str)] = &[
        (r#"{ "a" : 1 }"#, r#"{"a":1}"#, "whitespace is dropped"),
        (r#""\u0041""#, r#""A""#, "\\u escapes are decoded"),
        (r#""\/""#, r#""/""#, "the escaped solidus is decoded"),
        ("1e3", "1000.0", "float text is re-rendered, not preserved"),
        ("1.50", "1.5", "trailing zeros are not preserved"),
        (
            "1.0E3",
            "1000.0",
            "exponent case and form are not preserved",
        ),
        (
            r#"{"a":1,"a":2}"#,
            r#"{"a":2}"#,
            "a duplicate key collapses",
        ),
    ];

    for (input, expected_output, why) in lossy {
        let rendered = to_string(&parse(input).unwrap());
        assert_eq!(&rendered, expected_output, "{why}");
        assert_ne!(&rendered, input, "expected {input} NOT to survive verbatim");
        println!("{input:>18}  ->  {rendered:<12}  ({why})");
    }

    // Integers ARE byte-preserved, because there is exactly one spelling of an
    // integer that this parser accepts.
    for exact in ["0", "-1", "18446744073709551615", "-9223372036854775808"] {
        assert_eq!(to_string(&parse(exact).unwrap()), exact);
    }
}

#[test]
fn unicode_is_not_normalised_and_that_is_deliberate() {
    // "é" as one scalar and as e + combining acute are DIFFERENT JSON strings.
    // Normalising would make the bytes depend on a Unicode version, which is
    // exactly the kind of moving dependency a chain must not have.
    let composed = "\"\u{00e9}\"";
    let decomposed = "\"e\u{0301}\"";

    assert_ne!(parse(composed).unwrap(), parse(decomposed).unwrap());
    assert_ne!(
        to_string_canonical(&parse(composed).unwrap()),
        to_string_canonical(&parse(decomposed).unwrap())
    );
}

/// The significant digits in one of this crate's decimal renderings.
///
/// Leading and trailing zeros are positional, not significant: `1000.0` and
/// `0.0000001` each carry exactly one.
fn significant_digits(text: &str) -> usize {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let trimmed = digits.trim_start_matches('0').trim_end_matches('0');
    if trimmed.is_empty() {
        1
    } else {
        trimmed.len()
    }
}

/// The fewest significant digits that still round-trip to `value`, found by
/// asking Rust for scientific notation at increasing precision.
fn minimum_significant_digits(value: f64) -> usize {
    for precision in 0..=17 {
        let candidate = format!("{value:.precision$e}");
        if candidate.parse::<f64>().map(f64::to_bits) == Ok(value.to_bits()) {
            return precision + 1;
        }
    }
    18
}

#[test]
fn float_rendering_is_shortest_round_trip_not_merely_round_trip() {
    // Bit-for-bit round-trip proves the rendering is SUFFICIENT. It does not
    // prove it is MINIMAL -- a writer emitting 17 digits for every float would
    // pass that test and still produce different bytes from one emitting the
    // shortest form. Two implementations only agree if both are shortest, so
    // this is the assertion that makes the rendering a function of the VALUE
    // rather than of whichever algorithm happened to be linked.
    let mut rng = Xorshift(0x2026_0823_0000_0003);
    let mut checked = 0_u32;

    for _ in 0..50_000 {
        let value = f64::from_bits(rng.next());
        if !value.is_finite() {
            continue;
        }

        let rendered = to_string(&Value::from(value));
        let ours = significant_digits(&rendered);
        let minimum = minimum_significant_digits(value);

        assert_eq!(
            ours, minimum,
            "{value:e} rendered as {rendered} with {ours} significant digits; \
             {minimum} round-trips"
        );
        checked += 1;
    }

    assert!(checked > 45_000);
    println!("floats verified SHORTEST-round-trip: {checked}");

    // And the landmarks, spelled out, because a sample is not a statement.
    for (value, expected) in [(0.1_f64, 1), (1.5, 2), (1000.0, 1), (0.000_000_1, 1)] {
        assert_eq!(
            significant_digits(&to_string(&Value::from(value))),
            expected
        );
    }
}

#[test]
fn a_canonical_document_round_trips_byte_for_byte() {
    // The POSITIVE half of the byte-round-trip question, and the class where
    // input bytes ARE reproduced exactly: a document already in canonical form.
    //
    // That is the whole contract behind "canonicalise once". Anything else --
    // whitespace, `\u` escapes, `1e3`, trailing zeros, duplicate keys -- is
    // listed in `input_bytes_are_not_reproduced_and_here_is_exactly_why`, and
    // every one of those differences disappears after one canonical pass.
    for document in corpus() {
        let canonical = to_string_canonical(&parse(document).unwrap());

        // Byte-for-byte, through BOTH writers, in both directions.
        assert_eq!(
            to_string_canonical(&parse(&canonical).unwrap()),
            canonical,
            "canonical writer moved a canonical document"
        );
        assert_eq!(
            to_string(&parse(&canonical).unwrap()),
            canonical,
            "plain writer moved a canonical document"
        );
    }

    // Including the float extremes, where a writer that was not shortest would
    // gain or lose a digit on the second pass.
    for text in ["5e-324", "1.7976931348623157e308", "1e300", "0.1", "-0.0"] {
        let canonical = to_string_canonical(&parse(text).unwrap());
        assert_eq!(to_string_canonical(&parse(&canonical).unwrap()), canonical);
    }
}

#[test]
fn nothing_in_the_output_can_depend_on_the_target() {
    // The audit, as far as a test can carry it. The rest is a CI gate that
    // greps `src/` for the constructs this crate must never contain --
    // `HashMap` above all, whose per-process randomised iteration order would
    // make key order differ between two runs of the SAME binary.
    //
    // What reaches the output is: decimal integers, `core`'s shortest-round-trip
    // float rendering, `\u00XX` for control bytes (a fixed four hex digits from
    // a u8), and literal UTF-8. No pointer-width value, no byte-order-dependent
    // encoding, and no locale-aware formatting is written anywhere.

    // Pointer width cannot reach the bytes: the only `usize` in the writer is
    // indent depth, which controls how many spaces a PRETTY document carries
    // and is absent from compact and canonical output entirely.
    let value = parse(r#"{"a":[1,2],"b":{}}"#).unwrap();
    assert_eq!(to_string(&value), r#"{"a":[1,2],"b":{}}"#);
    assert_eq!(to_string_canonical(&value), r#"{"a":[1,2],"b":{}}"#);

    // Integers render in decimal at every width boundary a 32-bit target would
    // expose. wasm32 (a 32-bit-pointer target) is built in CI for the same
    // reason.
    for text in [
        "0",
        "4294967295",
        "4294967296",
        "9223372036854775807",
        "-9223372036854775808",
        "18446744073709551615",
    ] {
        assert_eq!(to_string(&parse(text).unwrap()), text);
    }

    // Control bytes escape to a FIXED four hex digits, lowercase, regardless of
    // anything ambient.
    let control = Value::from("\u{0}\u{1}\u{1f}");
    assert_eq!(to_string(&control), r#""\u0000\u0001\u001f""#);
}
