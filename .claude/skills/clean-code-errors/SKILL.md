---
name: clean-code-errors
description: Clean Code error handling principles for kadou's Rust codebase. Use when writing, editing, or reviewing error handling — Result returns, error enums, thiserror types, unwrap/expect usage, and error messages surfaced to a human or an agent.
user-invocable: false
---

# Clean Code Error Handling for kadou (Rust)

Apply these principles when writing or modifying error handling anywhere in
the workspace.

## thiserror in Libraries

- Every library crate (`kadou-core`, `kadou-exec`, `kadou-mcp`,
  `kadou-mine`) defines its own `thiserror`-derived error enum, matching
  the existing pattern:
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum ExecError {
      #[error("failed to spawn {program}: {source}")]
      Spawn { program: String, #[source] source: std::io::Error },
      #[error("failed to wait for child: {0}")]
      Wait(#[source] std::io::Error),
  }
  ```
- Variants name the failure in domain terms (`Spawn`, `Wait`), not the
  mechanism (`IoError1`, `Err2`)
- Use `#[source]` / `#[from]` so `?` composes across crate boundaries
  without losing the chain

## Typed Errors to the Bin Edge — anyhow Is Not a Workspace Dependency

- This workspace has **no `anyhow` dependency today**. Typed `thiserror`
  errors propagate all the way to `kadou`'s `main.rs`, which matches on
  them (or converts via `Display`) to produce a `std::process::ExitCode`
  and a human-facing message — see the existing `fn main() ->
  std::process::ExitCode` pattern in `crates/kadou/src/main.rs`
- If a bin-only code path genuinely needs to bag heterogeneous errors with
  no caller that inspects the variant (a true one-off CLI glue path),
  `anyhow` is an acceptable *addition*, scoped to the `kadou` bin crate
  only — never in `kadou-core`/`kadou-exec`/`kadou-mcp`/`kadou-mine`. Note
  the addition in the commit message; don't add it silently

## Sentence Errors With a Fix Line

- Every error a human or an agent sees follows the PRD's own error shape
  (`docs/design/05-prd.md` §4.7, §5.5): a one-sentence problem statement,
  then a line starting with `=` (or the MCP `message` field) that says
  exactly what to run to fix it:
  ```
  error: sesami/ses-deploy is critical and needs confirmation
    = kadou run sesami/ses-deploy version=25.6.1.2 oke_cluster=uat --confirm sesami/ses-deploy
  ```
- MCP error results follow the same shape in JSON: `error: "missing_needs"`,
  `message: "kadou vault set jenkins_token"` — the fix line is the
  copy-pasteable command, not a restated diagnosis
- Never surface a bare `Debug` dump of an error enum to a human or an
  agent result; format through `Display` (the `#[error(...)]` message)

## No `unwrap()`/`expect()` Outside Tests

- Library and bin code paths that can be reached at runtime must not
  `unwrap()` or `expect()` on a `Result`/`Option` that can genuinely be
  `Err`/`None` in production — propagate with `?` or match and return a
  typed error instead
- `unwrap()`/`expect()` are fine in `#[test]`/`#[cfg(test)]` code, where a
  panic is the correct failure signal, and in truly-impossible-by-
  construction cases with an `expect("<why this can't happen>")` message
  that documents the invariant (e.g. the existing
  `.expect("DOPS_HOME fallback must warn")` in a test)
- Prefer proving the impossible case can't happen via the type system
  (a non-empty `Vec` type, an enum without the bad state) over an `expect`

## Error Wrapping

- Add context describing what *this* function was doing, not the
  underlying error restated: `format!("resolve needs for {id}")`, not
  `format!("error: {e}")`
- Preserve the chain with `#[source]`/`#[from]` so `anyhow::Error`
  (bin-only) or `Display` on a `thiserror` type can print the full chain
  when it helps diagnosis

## Fail Fast at Boundaries

- Validate inputs where they cross a boundary: CLI args in `commands.rs`,
  MCP tool args in `tools.rs`, config in `config.rs` — not deep inside
  `kadou-core` domain logic that assumes already-validated input
- A malformed header is an error at `kadou check`/load time, not a
  silent default (§4.7: "a bad header is loud, not silently treated as a
  helper file")

## Spec Traceability

- Every documented PRD error case (`no_such_kata`, `invalid_args`,
  `missing_needs`, `draft`, `timeout`) has a test asserting its exact
  `error` string and `message` shape
- If you add an error path the PRD doesn't name, question whether it
  should surface as one of the existing five error kinds instead of a new
  one (YAGNI, `clean-code-design`)
