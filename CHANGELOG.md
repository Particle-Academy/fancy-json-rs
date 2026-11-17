# Changelog

All notable changes to this crate are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

**Pre-1.0, breaking changes land in MINOR releases.** The version number is not
promising otherwise until 1.0.

## [Unreleased]

### Changed

- Rebranded the display name to Fancy Dev Kit (README and package description). Package names, the npm scope, URLs and the `fancy-ui` MCP id are unchanged.
- **Docs:** `AGENTS.md` is now the guide to using this package (install, how to find it on the Fancy MCP catalog server once it is listed, API essentials, traps). The contributor notes it held moved to `CONTRIBUTING.md`, and `CLAUDE.md` is now a symlink to it. The README points to the MCP server and to `AGENTS.md`. No code changed; nothing to do.

### Security

- **CI workflows now cap the `GITHUB_TOKEN` to `contents: read`.** With no
  `permissions:` block a workflow inherits the repository default, which can be
  read-write for the whole token — so a compromised action or a malicious
  transitive dependency in any step could push to this repo. Nothing in CI here
  needs more than to read the code. Declared at the workflow level so a job
  added later cannot arrive with no cap. **Nothing for a consumer to do and no
  published artifact changes** — recorded because an unwritten cap is one a
  future edit removes without anyone noticing. (CodeQL
  `actions/missing-workflow-permissions`.)

## [0.1.1] - 2026-09-13

The first TAGGED release. 0.1.0 was recorded here but never tagged, so every
consumer depended on `branch = "main"` and got whatever `main` was that day.
**What to do:** replace `branch = "main"` with `tag = "v0.1.1"` in your
`Cargo.toml`. No API changed since 0.1.0.

### Added

- **`tests/determinism.rs`** — the byte-determinism properties a consensus
  consumer needs, measured rather than asserted. Floats round-trip bit-for-bit
  over 199,892 random bit patterns; canonical output is a function of the value
  alone and a fixed point after one pass; the corpus digest is pinned at
  `0x91bf02f13e6d8ab3`.

  What is **not** guaranteed is measured too: `core`'s float `Display` cannot
  vary by target (it is pure Rust, no platform `printf`) but is not a
  byte-stability promise across Rust *releases*. The landmark test turns a
  toolchain bump into a red build here; it cannot protect a consumer compiling
  with a rustc we never tested.

### Changed

- **The tests SHIP.** `exclude` no longer drops `tests/` from the published
  tarball. This crate's pitch is that someone auditing their dependency tree can
  read and check it, and a tarball whose tests are missing fails exactly that
  audience — `cargo test` on a vendored copy would have covered doctests and
  nothing else. 15 files to 18; 19.2KiB to 25.0KiB compressed.

## [0.1.0] - 2026-08-23

### Added

- First release. A JSON reader and writer with **no dependencies at all** —
  `cargo tree` is one line — `no_std` + `alloc`, and `forbid(unsafe_code)`.

- **`Number` keeps integers exact.** `PosInt(u64) | NegInt(i64) | Float(f64)`,
  with the literal's *shape* picking the variant. An integer literal never
  routes through an `f64`, so `2^53 + 1` survives a round trip. An integer
  outside `u64`/`i64` is an **error**; `ParseOptions::with_lossy_big_integers`
  opts in to reading it as a float, and `as_u64`/`as_i64` still decline to
  return it, so a lossy read cannot be mistaken for an exact one.

- **Bounded recursion everywhere.** Parsing is capped at `DEFAULT_MAX_DEPTH`
  (128) and returns `ErrorKind::DepthLimitExceeded` rather than overflowing the
  stack. Both writers and `Value`'s own `Drop` are iterative, so a value built
  in code — which the cap never sees — cannot overflow either.

  The reader itself is recursive descent, so **the cap is the guarantee**.
  `ParseOptions::with_max_depth` can raise it and raising it takes the guarantee
  with it; both the method and the README say so.

- **Strict RFC 8259.** No comments, trailing commas, unquoted keys, single
  quotes, `NaN`, `Infinity`, leading `+`, leading zeros, unescaped control
  characters, lone surrogates, or trailing data after the top-level value.

- **Three writers.** `to_string` (compact, insertion order), `to_string_pretty`,
  and `to_string_canonical` (keys sorted at every depth, by Unicode scalar
  value) — the form to hash, sign or compare.

- Errors carry `line` and `column`, 1-based and counted in **characters**.
