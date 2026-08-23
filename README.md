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

## What this is not

Not a `serde` replacement. There are no derive macros and no `Serialize` /
`Deserialize` traits: you read a document by key, the way the PHP and Python
runtimes do. If you want a struct mapped to JSON with an attribute, you want
[`serde`](https://serde.rs) and you should use it — this crate is for callers
who would rather have an empty dependency tree.

## License

MIT
