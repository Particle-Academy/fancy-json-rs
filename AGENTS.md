# AGENTS.md — fancy-json

A dependency-free JSON reader and writer in Rust. `CLAUDE.md` symlinks here.

This file describes **this crate's code**. Process rules — publishing, kit
versioning, backports, the third-party approval bar — live in the envelope's
`AGENTS.md` and are deliberately not repeated.

## What this crate is, and the one rule

It exists because Rust has no JSON in its standard library — the one thing the
PHP and Python runtimes in this suite get for free — and because its first
consumer compiles it **into a blockchain node**. There, every transitive
dependency is audit surface and a panic is an abort rather than something the
caller catches.

> **`cargo tree` must stay one line.** Not "few dependencies" — none. CI fails
> the build if the tree grows, because nothing else would notice one appear.

Adding one needs the owner's approval like any third-party code, and the
`no-dependencies` CI job is what makes that a gate rather than a hope.

## Architecture

- `value.rs` — `Value`, `Number`, `Map`.
- `parse.rs` — the reader, `ParseOptions`, `DEFAULT_MAX_DEPTH`.
- `write.rs` — the three writers.
- `error.rs` — `Error` + `ErrorKind`.

Four invariants hold the crate together. Each one exists because the
alternative fails **silently**, which is why each has a test naming it.

### 1. An integer literal never touches an `f64`

`Number` is `PosInt(u64) | NegInt(i64) | Float(f64)`, and the **shape** of the
literal picks the variant, not its value: `1.0` is a `Float` because a document
that says `1.0` said "measurement", not "count".

`2^53 + 1` is the shape of a balance in minor units. Through a double it comes
back as `9007199254740992` and nothing reports it. An integer outside
`u64`/`i64` is therefore an **error**, not a demotion —
`with_lossy_big_integers` opts in, and `as_u64`/`as_i64` *still* return `None`
for the result, so a lossy read can never be mistaken for an exact one.

`as_f64` succeeds for every number, because asking for a float **is** the
request to accept float behaviour. The reverse conversion is the implicit one
that must not exist, and does not.

### 2. Nothing recurses without a bound

Three places, and only the first is obvious:

- **Parsing** is capped at `DEFAULT_MAX_DEPTH`. A recursive-descent reader with
  no cap dies by stack overflow on a document an attacker writes in a few
  hundred bytes.
- **Writing** is iterative — an explicit `Task` stack. The parser's cap does not
  protect it, because a value built in code was never parsed, and a loop is the
  natural way to build a deep one.
- **`Drop for Value`** is iterative too, for the same reason. This is why
  `Value` **cannot be destructured by move**; use `as_array` / `into_array` /
  `take` instead. That ergonomic cost is deliberate and is the whole reason the
  impl exists — do not remove it to make a match arm nicer.

### 3. `-0` is the integer zero

`integer_from` normalises it. A separate `NegInt(0)` would give one number two
representations, and `PosInt(0) != NegInt(0)` would make equality depend on how
the value was *spelled* — which breaks round-trip tests in a way that looks like
a writer bug.

### 4. Key order survives, but does not decide equality

`Map` preserves insertion order, because every peer runtime does (a PHP array,
a Python dict, a JS object). Sorting lives in `to_string_canonical`, not in the
container.

`PartialEq for Map` is order-**insensitive**: two documents listing the same
pairs in different orders are the same JSON value. The conformance loaders in
every other language compare objects the same way.

Lookup goes through a `BTreeMap<String, usize>` side index rather than a linear
scan. A scan is fine for the small objects JSON usually carries and quadratic on
a hostile document with a hundred thousand keys — which is exactly the input
this crate is meant to survive.

## Traps

**Rust's `Display` for `f64` prints `3` for `3.0`.** Writing that loses the fact
that the value was a float and the next reader gets an integer, so
`write_number` appends `.0` when the rendering has no `.`, `e` or `E`. It also
never uses exponent notation, so `1e308` writes as 309 digits — verbose,
deterministic, and the trade this crate wants.

**`sorted_entries()` returns a `Vec` of references into the MAP, not into the
Vec.** That is what makes it safe to push those pairs onto the writer's task
stack and let the `Vec` drop. If you change it to return owned pairs, the
canonical writer stops compiling for a reason that will look unrelated.

**`0x7F` is not a JSON control character.** Only `U+0000..=U+001F` must be
escaped. Rejecting `DEL` would refuse documents the grammar allows.

**Escape `/`? No.** PHP's `json_encode` escapes it by default and every URL in a
shared fixture was written without those escapes.

**A leading zero ends the number.** `01` is `0` followed by `TrailingData`, not
`InvalidNumber` — which is what stops a zero-padded id being read as a different
number. The error-kind tests pin the distinction.

## Testing

```bash
cargo test --all-features          # unit + integration + doctests (README included)
cargo test --release --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
cargo build --no-default-features --target thumbv7em-none-eabi
```

The release run matters on its own: optimisation changes stack-frame size, so
it is the honest test of the deep-nesting cases.

`#[cfg(doctest)] #[doc = include_str!("../README.md")] struct Readme;` compiles
the README's examples. A README that does not compile is one that stopped being
true, and nothing else in the build would notice.

## Status

**0.1.0 — built and green, unpublished.** 52 tests. The crate name `fancy-json`
was verified free on crates.io on 2026-08-23 (send a User-Agent and keep a
known-published control — a UA-less request 403s and every name looks taken).

Consumers: `fancy-flow-rs` (the Rust runtime twin of the fancy-flow engine) and,
through it, the Impactium blockchain agent.
