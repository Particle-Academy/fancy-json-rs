# Using fancy-json — guide for agents

How an agent installs and uses this crate in a consumer project. What the crate
is, is in [README.md](./README.md). Notes for working ON this repo's code are in
[CONTRIBUTING.md](./CONTRIBUTING.md).

## Install

```toml
[dependencies]
fancy-json = "0.1"
```

For `no_std` + `alloc`:

```toml
fancy-json = { version = "0.1", default-features = false }
```

The crate name is `fancy-json` (the repo is `fancy-json-rs`); the Rust path is
`fancy_json`. It has no dependencies at all.

## Find it on the Fancy MCP server

The catalog server is `https://ui.particle.academy/mcp` (streamable HTTP, no API
key; MCP id `fancy-ui`). This crate is not yet listed there. Server-side
packages are found with `search-backend-packages`, once a crate is listed.

## API essentials

- **Read:** `parse(&str) -> Result<Value, Error>`, or
  `parse_with(&str, ParseOptions)` with `ParseOptions::with_max_depth` /
  `with_lossy_big_integers`.
- **Navigate:** `Value::get(key)`, `as_str`, `as_u64`, `as_i64`, `as_f64`,
  `as_array`, `into_array`, `take`.
- **Write:** `to_string` (compact, insertion order), `to_string_pretty(v, n)`,
  `to_string_canonical` (compact, keys sorted at every depth).
- **Numbers:** `Number` is `PosInt(u64) | NegInt(i64) | Float(f64)`, chosen by
  the literal's shape. Integers never pass through an `f64`.
- **Errors:** `Error` + `ErrorKind`, with 1-based `line` and `column` counted in
  characters.

## Traps

- **To hash, sign or compare, use `to_string_canonical`.** `to_string` keeps
  insertion order, so two writers of the same value can differ.
- **An integer too large for `u64`/`i64` is an error.** `with_lossy_big_integers`
  opts in, and even then `as_u64` / `as_i64` return `None` for it.
- **`as_i64` on a float literal is `None`**, even for `3.0`.
- **Raising `with_max_depth` removes the stack-overflow guarantee.** The reader
  is recursive; the depth cap (`DEFAULT_MAX_DEPTH`, 128) is the guarantee. Raise
  it only for a trusted document and a measured depth. Lowering is always safe.
- **`Value` cannot be destructured by move** (its `Drop` is iterative). Use
  `as_array` / `into_array` / `take`.
- **Strict parsing:** no comments, trailing commas, `NaN`/`Infinity`, leading
  zeros or trailing data.
- **Float rendering can change across Rust releases.** It is fixed per target
  but not promised across rustc versions.
- Not a `serde` replacement: there are no derives; read documents by key.
