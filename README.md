# fancy-json

A small, **dependency-free** JSON reader and writer for Rust. `no_std` + `alloc`.

```toml
[dependencies]
fancy-json = "0.1"
```

```rust
use fancy_json::{parse, to_string, Value};

let doc = parse(r#"{"kind":"branch","balance":9007199254740993}"#)?;
assert_eq!(doc.get("kind").and_then(Value::as_str), Some("branch"));

// Exact. Through an f64 this is 9007199254740992, silently.
assert_eq!(doc.get("balance").and_then(Value::as_u64), Some(9_007_199_254_740_993));
# Ok::<(), fancy_json::Error>(())
```

## Why this exists

Rust has no JSON in its standard library — the one thing the PHP and Python
runtimes in the [Fancy](https://ui.particle.academy) suite get for free. This
crate's first consumer compiles it into a **blockchain node**, where every
transitive dependency is audit surface and a panic is not something the caller
catches.

So it has **no dependencies at all**. Not "few" — none. `cargo tree` is one
line. The three properties below are why it is worth owning rather than
depending on.

### Integers stay integers

`Number` is `PosInt(u64) | NegInt(i64) | Float(f64)`, and the literal's *shape*
picks the variant. An integer literal never routes through an `f64`, so
`2^53 + 1` — the shape of a balance in minor units — survives being read back.

An integer too large for `u64`/`i64` is an **error**, not a quiet demotion to
float. `ParseOptions::with_lossy_big_integers` opts in to the lossy reading, and
even then `as_u64` declines to return it, so a lossy read can never be mistaken
for an exact one.

```rust
# use fancy_json::parse;
assert_eq!(parse("9007199254740993")?.as_u64(), Some(9_007_199_254_740_993));
assert_eq!(parse("3.0")?.as_i64(), None);   // a float is never an exact integer
assert!(parse("18446744073709551616").is_err());
# Ok::<(), fancy_json::Error>(())
```

### Nothing recurses without a bound

Parsing is capped at `DEFAULT_MAX_DEPTH` (128); past it you get
`ErrorKind::DepthLimitExceeded` rather than a stack overflow — which is an
abort, not an error a node can handle. Both writers **and `Value`'s own `Drop`**
are iterative, so a value built in code, which the cap never sees, cannot
overflow either.

The reader itself is recursive descent, so **the cap is the guarantee**.
`ParseOptions::with_max_depth` can raise it, and raising it takes the guarantee
with it — do that only for a document you trust and a depth you have measured.
Lowering it is always safe.

### Strict means strict

No comments, no trailing commas, no unquoted keys, no single quotes, no `NaN` or
`Infinity`, no leading `+`, no leading zeros, no unescaped control characters, no
lone surrogates, and no trailing data after the top-level value. Every one of
those is something the sender did not write, and this crate's consumer executes
what it reads.

Errors carry `line` and `column`, 1-based and counted in **characters** — a byte
offset reported as a column points into the middle of a character for anyone
whose data is not ASCII.

## Canonical output

| | |
|---|---|
| `to_string` | compact, keys in **insertion order** |
| `to_string_pretty(v, n)` | indented `n` spaces per level |
| `to_string_canonical` | compact, keys **sorted** at every depth |

Insertion order is authored data, so it survives a round trip — every peer
runtime (a PHP array, a Python dict, a JS object) preserves it too. Canonical
order is a function of the value alone, so two writers that built the same value
agree byte for byte. That is the form to hash, sign or compare.

Arrays are never sorted: their order *is* the value.

## `no_std`

```toml
fancy-json = { version = "0.1", default-features = false }
```

The `std` feature (on by default) adds only `std::error::Error for Error`.
Parsing, the value tree and all three writers work on `no_std` + `alloc`.

## Byte-determinism

**The guarantee: the same value serialises to the same bytes, on every target,
forever.** For a consensus consumer a rendering divergence is a chain halt, not
a bug, so this is a tested guarantee rather than a property that happens to
hold. Everything below is measured in `tests/determinism.rs` and fails the build
if it breaks.

Note what the guarantee is *not*: "output reproduces input bytes". No
canonicalising writer does that and a chain should not want it — `1e3` and
`1000.0` are the same value, and preserving the spelling would make the bytes
depend on who typed them.

| guaranteed | how it is held |
|---|---|
| floats round-trip **bit-for-bit** | 199,892 random bit patterns, incl. subnormals, extremes and `-0.0` |
| float rendering is **shortest** round-trip | 49,979 floats: no fewer significant digits round-trips |
| canonical output depends on the **value alone** | whitespace, key order, `\u` escape spelling and duplicate position all vary; bytes do not |
| a **canonical document round-trips byte-for-byte** | through both writers, both directions |
| canonical output is a **fixed point** after one pass | canonicalise once; it never moves again |
| non-finite floats are **unrepresentable** | so no `NaN`/`Infinity` divergence exists |
| nothing depends on target, endianness or locale | see below |
| corpus digest | pinned at `0x91bf02f13e6d8ab3` — one number two platforms can compare |

**Shortest matters as much as round-tripping.** A writer emitting 17 digits for
every float would round-trip perfectly and still disagree, byte for byte, with
one emitting the shortest form. Shortest is what makes the rendering a function
of the value rather than of whichever algorithm was linked.

### Nothing in the output can vary by target

What reaches the bytes is: decimal integers, `core`'s shortest-round-trip float
rendering, `\u00xx` for control bytes (a fixed four lowercase hex digits from a
`u8`), and literal UTF-8. No pointer-width value, no byte-order-dependent
encoding, no locale-aware formatting. The only `usize` in the writer is indent
depth, which is absent from compact and canonical output entirely.

`HashMap` is the hazard that would break this quietly — its per-process
randomised iteration order makes key order differ between two runs of the *same*
binary. This crate contains none, and a **CI gate greps `src/`** for it and for
`HashSet`, `RandomState`, `to_ne_bytes`, `SystemTime`, `Instant` and the
locale-aware case conversions. CI also builds a 32-bit-pointer target.

Key ordering is a deliberate pair, not an accident: `to_string` preserves
insertion order (every peer runtime does), `to_string_canonical` sorts by UTF-8
byte order at every depth. **For a chain, use `to_string_canonical`** — it is a
function of the value alone.

### NOT guaranteed — the one real gap

**Float rendering across Rust releases.** The renderer is `core`'s
`Display for f64`. It is pure Rust with no platform `printf`, so it *cannot*
vary by target — but Rust does not promise byte-exact output across *releases*.

`the_decimal_landmarks_render_to_exactly_these_bytes` pins the landmarks
exactly and the extremes by length and digest, so a toolchain bump is a red
build **here**. That does not protect a consumer compiling with a rustc we never
tested. Closing it means owning the float formatting rather than borrowing
`core`'s — or refusing floats outright, which costs a consumer whose data has
none exactly nothing.

Unicode is **not** normalised, deliberately. Composed and decomposed `é` are
different JSON strings and stay different; normalising would make the bytes
depend on a Unicode version, which is a moving dependency a chain must not have.

Rust never emits exponent notation, so `5e-324` renders as 326 bytes and
`f64::MAX` as 311. Deterministic and verbose — a Ryū-style writer emits 24.
Recorded as a measured cost, not a defect.

## What this is not

Not a `serde` replacement. There are no derive macros and no `Serialize` /
`Deserialize` traits: you read a document by key, the way the PHP and Python
runtimes do. If you want a struct mapped to JSON with an attribute, you want
[`serde`](https://serde.rs) and you should use it — this crate is for callers
who would rather have an empty dependency tree.

## License

MIT
