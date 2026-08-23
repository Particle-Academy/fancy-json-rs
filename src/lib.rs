//! A small, dependency-free JSON reader and writer.
//!
//! ```
//! use fancy_json::{parse, to_string, Value};
//!
//! let doc = parse(r#"{"kind":"branch","balance":9007199254740993}"#)?;
//! assert_eq!(doc.get("kind").and_then(Value::as_str), Some("branch"));
//!
//! // Exact. Through an f64 this is 9007199254740992, silently.
//! assert_eq!(doc.get("balance").and_then(Value::as_u64), Some(9_007_199_254_740_993));
//!
//! assert_eq!(to_string(&doc), r#"{"kind":"branch","balance":9007199254740993}"#);
//! # Ok::<(), fancy_json::Error>(())
//! ```
//!
//! # Why this exists
//!
//! Rust has no JSON in its standard library — the one thing the PHP and Python
//! ports of this suite get for free. This crate's consumer compiles it into a
//! blockchain node, where every transitive dependency is audit surface and a
//! panic is not something the caller catches. So it has **no dependencies at
//! all**, and the three properties below are the reason it is worth owning
//! rather than depending on.
//!
//! ## Integers stay integers
//!
//! [`Number`] is `PosInt(u64) | NegInt(i64) | Float(f64)`, and the literal's
//! *shape* picks the variant. An integer literal never routes through an `f64`,
//! so `2^53 + 1` — the shape of a balance in minor units — survives being read
//! back. An integer too large for `u64`/`i64` is an **error**, not a quiet
//! demotion to float; [`ParseOptions::with_lossy_big_integers`] opts in to the
//! lossy reading, and even then [`Value::as_u64`] declines to return it, so a
//! lossy read can never be mistaken for an exact one.
//!
//! ## Nothing recurses without a bound
//!
//! Parsing is capped at [`DEFAULT_MAX_DEPTH`]; past it the parser returns
//! [`ErrorKind::DepthLimitExceeded`] rather than walking off the end of the
//! stack. Both writers and [`Value`]'s own `Drop` are **iterative**, so a value
//! built in code — which the cap never sees — cannot overflow either.
//!
//! ## Strict means strict
//!
//! No comments, no trailing commas, no unquoted keys, no single quotes, no
//! `NaN` or `Infinity`, no leading `+`, no leading zeros, no unescaped control
//! characters, no lone surrogates, and no trailing data after the top-level
//! value. Every one of those is something the sender did not write.
//!
//! # Canonical output
//!
//! [`to_string`] preserves the key order the document was authored with,
//! because every peer runtime does. [`to_string_canonical`] sorts keys at every
//! depth — the form to hash, sign or compare, where two writers that built the
//! same value must agree byte for byte.
//!
//! # `no_std`
//!
//! Default features pull in `std` only for [`std::error::Error`]. Build with
//! `default-features = false` for `no_std` + `alloc`; everything here —
//! parsing, the value tree, all three writers — works there.
//!
//! [`std::error::Error`]: https://doc.rust-lang.org/std/error/trait.Error.html

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

/// The README's examples, compiled and run as doctests.
///
/// A README that does not compile is a README that stopped being true, and
/// nothing else in the build would notice.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct Readme;

mod error;
mod parse;
mod value;
mod write;

pub use error::{Error, ErrorKind};
pub use parse::{parse, parse_with, ParseOptions, DEFAULT_MAX_DEPTH};
pub use value::{Map, Number, Value};
pub use write::{to_string, to_string_canonical, to_string_pretty};
