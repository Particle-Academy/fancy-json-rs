//! The writers: compact, pretty, and canonical.
//!
//! All three are **iterative**. A recursive writer dies by stack overflow on a
//! value the parser's depth cap never saw, because a value built in code is not
//! capped — and the natural way to build one is a loop.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use crate::value::{Number, Value};

/// Serialize to compact JSON: no spaces, keys in insertion order.
///
/// Separator-free, matching `JSON.stringify` and `json_encode`, so an
/// interpolated object reads the same whichever runtime sent it.
#[must_use]
pub fn to_string(value: &Value) -> String {
    write_value(
        value,
        Style {
            indent: None,
            sorted: false,
        },
    )
}

/// Serialize to indented JSON with `indent` spaces per level.
#[must_use]
pub fn to_string_pretty(value: &Value, indent: usize) -> String {
    write_value(
        value,
        Style {
            indent: Some(indent),
            sorted: false,
        },
    )
}

/// Serialize to compact JSON with **object keys sorted** at every depth.
///
/// The form to hash, sign or compare. Insertion order is authored data, so it
/// survives a round trip; canonical order is a function of the value alone, so
/// two writers that built the same value agree byte for byte.
///
/// Keys sort by Unicode scalar value. Arrays are **not** sorted — their order
/// is the value, not a presentation of it.
#[must_use]
pub fn to_string_canonical(value: &Value) -> String {
    write_value(
        value,
        Style {
            indent: None,
            sorted: true,
        },
    )
}

#[derive(Clone, Copy)]
struct Style {
    indent: Option<usize>,
    sorted: bool,
}

enum Task<'a> {
    /// Render a value at a nesting depth.
    Val(&'a Value, usize),
    /// Literal punctuation.
    Text(&'static str),
    /// A quoted, escaped string — an object key.
    Key(&'a str),
    /// A line break plus indentation. Nothing at all when compact.
    Break(usize),
}

fn write_value(root: &Value, style: Style) -> String {
    let mut out = String::new();
    let mut stack: Vec<Task<'_>> = alloc::vec![Task::Val(root, 0)];

    while let Some(task) = stack.pop() {
        match task {
            Task::Text(text) => out.push_str(text),
            Task::Key(key) => {
                write_escaped(&mut out, key);
                out.push(':');
                if style.indent.is_some() {
                    out.push(' ');
                }
            }
            Task::Break(depth) => {
                if let Some(width) = style.indent {
                    out.push('\n');
                    for _ in 0..depth * width {
                        out.push(' ');
                    }
                }
            }
            Task::Val(value, depth) => match value {
                Value::Null => out.push_str("null"),
                Value::Bool(true) => out.push_str("true"),
                Value::Bool(false) => out.push_str("false"),
                Value::Number(number) => write_number(&mut out, *number),
                Value::String(text) => write_escaped(&mut out, text),

                Value::Array(items) if items.is_empty() => out.push_str("[]"),
                Value::Array(items) => {
                    out.push('[');
                    stack.push(Task::Text("]"));
                    stack.push(Task::Break(depth));
                    // Pushed in reverse: the stack pops last-in first.
                    for (index, item) in items.iter().enumerate().rev() {
                        stack.push(Task::Val(item, depth + 1));
                        stack.push(Task::Break(depth + 1));
                        if index > 0 {
                            stack.push(Task::Text(","));
                        }
                    }
                }

                Value::Object(map) if map.is_empty() => out.push_str("{}"),
                Value::Object(map) => {
                    out.push('{');
                    stack.push(Task::Text("}"));
                    stack.push(Task::Break(depth));

                    // `sorted_entries` allocates a Vec, but every reference in
                    // it borrows the MAP, not the Vec — so the pairs stay valid
                    // on the stack after it is dropped.
                    let entries: Vec<(&str, &Value)> = if style.sorted {
                        map.sorted_entries()
                    } else {
                        map.iter().collect()
                    };

                    for (index, (key, item)) in entries.into_iter().enumerate().rev() {
                        stack.push(Task::Val(item, depth + 1));
                        stack.push(Task::Key(key));
                        stack.push(Task::Break(depth + 1));
                        if index > 0 {
                            stack.push(Task::Text(","));
                        }
                    }
                }
            },
        }
    }

    out
}

fn write_number(out: &mut String, number: Number) {
    match number {
        Number::PosInt(value) => {
            let _ = write!(out, "{value}");
        }
        Number::NegInt(value) => {
            let _ = write!(out, "{value}");
        }
        Number::Float(value) => {
            // Rust's `Display` for f64 is shortest-round-trip and lives in
            // `core`, so this is exact and stays no_std. It never uses
            // exponent notation, which is verbose for extreme magnitudes and
            // deterministic — the trade this crate wants.
            let start = out.len();
            let _ = write!(out, "{value}");
            // ...but it prints `3` for 3.0. Writing that loses the fact that
            // the value was a float, and the next reader gets an integer.
            let written = &out[start..];
            if !written.contains(['.', 'e', 'E']) {
                out.push_str(".0");
            }
        }
    }
}

/// Write a JSON string literal, escaping exactly what the grammar requires.
///
/// `/` is **not** escaped: PHP's `json_encode` escapes it by default and every
/// URL in a shared fixture was written without those escapes. Non-ASCII is
/// **not** escaped either — the output is UTF-8, and `\u`-escaping it triples
/// the size of every CJK document for no gain.
fn write_escaped(out: &mut String, text: &str) {
    out.push('"');
    let bytes = text.as_bytes();
    let mut run_start = 0;

    for (index, &byte) in bytes.iter().enumerate() {
        let escape: &str = match byte {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            0x08 => "\\b",
            0x0C => "\\f",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0x00..=0x1F => {
                out.push_str(&text[run_start..index]);
                let _ = write!(out, "\\u{byte:04x}");
                run_start = index + 1;
                continue;
            }
            _ => continue,
        };
        out.push_str(&text[run_start..index]);
        out.push_str(escape);
        run_start = index + 1;
    }

    out.push_str(&text[run_start..]);
    out.push('"');
}
