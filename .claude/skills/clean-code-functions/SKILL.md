---
name: clean-code-functions
description: Clean Code function design principles for kadou's Rust codebase. Use when creating, editing, or reviewing Rust functions and methods. Triggers on new function definitions, refactoring existing functions, adding parameters, or when a function is growing in size or complexity.
user-invocable: false
---

# Clean Code Functions for kadou (Rust)

Apply these principles when writing or modifying functions across
`kata-core`, `kata-exec`, `kata-mcp`, `kata-mine`, and the `kadou` bin.

## Size

- Functions should be small — **under 30 lines** of logic. `header.rs` and
  `tools.rs` are already large files; keep each function inside them small
  even when the file is not
- If a function is hard to name, it's doing too much — split it
- Extract a block that needs a `// this handles X` comment into its own
  function named `X` — the comment becomes the name

## Single Responsibility, Single Level of Abstraction

- A function does ONE thing at ONE level of abstraction
- Top-level orchestration reads like the PRD section it implements:
  ```rust
  pub fn run_kata(state: &ServerState, args: RunArgs) -> (Value, bool) {
      let kata = match resolve_visible(state, &args.id) { Ok(k) => k, Err(e) => return e };
      let env = build_mcp_env(&kata, &args)?;
      let outcome = finish_run(state, &kata, env)?;
      truncate_output(outcome)
  }
  ```
- Lower-level functions (`resolve_visible`, `build_mcp_env`, `finish_run`,
  `truncate_output`) hold the detail. Don't inline detail into the
  orchestrator "just this once."

## Command/Query Separation

- A function either **does** something (a command — returns `()`,
  `Result<(), E>`, or nothing meaningful) or **returns** something (a query
  — returns data), never both
- `kadou vault set` is a command; `describe_kata` is a query. Don't make
  `describe_kata` write a last-used-args cache as a side effect — that's a
  second, hidden responsibility
- Exception: a query that also returns whether it had to do work (Rust's
  `Option`/`Result` idiom) is fine — `resolve_needs` returning
  `Result<ResolvedNeed, PathsError>` is a query, not a command

## Arguments

- Zero, one, or two arguments are ideal; three or more — reach for a
  config struct, matching the existing pattern:
  ```rust
  pub struct ListArgs { pub query: Option<String>, pub folder: Option<String>, ... }
  pub fn list_kata(state: &ServerState, config: &Config, args: ListArgs) -> Value
  ```
- `&ServerState` / `&Config` context references come first, by convention
  in this codebase (see `tools.rs`)
- Prefer borrowing (`&str`, `&Path`) over owned types in function
  signatures unless ownership must transfer

## No Hidden Side Effects

- If named `parse_header`, it must not touch the filesystem
- If named `resolve_visible`, it must not execute the kata
- Side effects that are necessary (writing history, redacting a log) must
  be named so the effect is obvious: `finish_run`, not `process`

## Return Values

- Return early on error — avoid deep nesting; this is idiomatic Rust with
  `?` and matches the existing style in `header.rs`/`tools.rs`
- Return concrete types from constructors (`ParsedHeader`, `KataContext`),
  accept trait bounds or `&dyn Trait` / generics as parameters when a
  function needs to be swappable (`clean-code-solid`)

## Spec Traceability

- Each PRD acceptance criterion (`docs/design/05-prd.md` §9 slice
  descriptions) should map to one or a small cluster of functions
- If a function can't be traced to a PRD requirement, question whether it
  should exist (YAGNI, `clean-code-design`)
- Function names should make the PRD section connection findable by
  `grep`, e.g. `is_visible_risk` for the §6.2 visibility formula
