---
name: clean-code-testing
description: Clean Code testing principles for kadou's Rust test suites. Use when writing, editing, or reviewing Rust tests (unit tests, integration tests in crates/*/tests/, or insta snapshots). Triggers on test function creation, table-driven test setup, fixture design, snapshot tests, or test refactoring.
user-invocable: false
---

# Clean Code Testing for kadou (Rust)

Apply these principles when writing or modifying tests anywhere in the
workspace.

## Tests Derive from the PRD

- Every acceptance criterion in `docs/design/05-prd.md` becomes one or more
  test cases
- Name tests after the behavior, not the implementation:
  ```rust
  // Bad
  #[test]
  fn test_run_kata() { /* ... */ }

  // Good
  #[test]
  fn run_kata_on_draft_returns_error_without_executing() { /* ... */ }
  ```
- If a test can't be traced to a PRD requirement, question whether it (or
  the code it tests) is needed

## FIRST Principles

- **Fast** — no real network, no sleeping; a kata subprocess test uses the
  embedded starter kata or a `tempfile` fixture, not a live Jenkins call
- **Independent** — no shared mutable state between tests; use
  `tempfile::tempdir()` per test, as `sesami_shaped.rs` and `mcp_server.rs`
  already do, not a shared static directory
- **Repeatable** — control time, randomness, and environment explicitly;
  never depend on the real `$HOME` (see "`KATA_HOME` Isolation" below)
- **Self-validating** — assert, don't `println!` and eyeball the output
- **Timely** — write the test before or alongside the code (`tdd-workflow`)

## Table-Driven Tests: rstest or a Plain Loop

- Use `rstest`'s `#[case]` attribute for parameterized cases where the
  crate already depends on it, or a plain `for` loop over a local slice of
  cases when it doesn't — either is acceptable; don't add `rstest` to a
  crate for a single table
  ```rust
  #[rstest]
  #[case("low", RiskLevel::Low)]
  #[case("critical", RiskLevel::Critical)]
  fn parses_risk_level(#[case] input: &str, #[case] want: RiskLevel) {
      assert_eq!(RiskLevel::parse(input).unwrap(), want);
  }
  ```
  ```rust
  #[test]
  fn parses_risk_level() {
      let cases = [("low", RiskLevel::Low), ("critical", RiskLevel::Critical)];
      for (input, want) in cases {
          assert_eq!(RiskLevel::parse(input).unwrap(), want, "input: {input}");
      }
  }
  ```
- Each case needs a name or an inline description in the assertion failure
  message — a bare `assert_eq!` inside a loop with no context is a
  debugging tax on the next person

## insta Snapshots for Parser Output and the Wire Contract

- The header parser's diagnostics and the `tools/list` byte payload are
  snapshot-tested with `insta`, matching the existing pattern in
  `crates/kata-core/tests/header_fixtures.rs`:
  ```rust
  insta::assert_debug_snapshot!(format!("bad_{name}"), diags);
  ```
- New bad-header fixtures go in `tests/fixtures/headers/bad/`; new
  good-header fixtures in `tests/fixtures/headers/good/` — the fixture
  corpus *is* the parser's contract (§4.7, §6.2 risk 1)
- Review `cargo insta review` output before accepting a snapshot change —
  an accepted snapshot is a claim that the new output is correct, not just
  different

## `tempfile` Isolation with `KATA_HOME` — Never the Real `$HOME`

- Every test that touches paths, config, the vault, or history sets
  `KATA_HOME` to a `tempfile::tempdir()` root, matching `paths.rs`'s
  `discover()`/`resolve()` contract — never read or write the developer's
  real `~/.config/kata` from a test
  ```rust
  let home = tempfile::tempdir().unwrap();
  let paths = kata_core::paths::resolve(Some(home.path().to_path_buf()), None).unwrap();
  ```
- Do not rely on `XDG_CONFIG_HOME`/`XDG_DATA_HOME`/`XDG_STATE_HOME` being
  unset or set to anything in particular — pin `KATA_HOME` explicitly, as
  the existing `paths.rs` tests do
- A test that skips this isolation is not repeatable across machines or CI
  and can corrupt a developer's real kadou state — treat this as a hard
  rule, not a preference

## Test Doubles

- Prefer a small fake behind a trait (`clean-code-boundaries`) over a
  mocking framework — a fake `Runner` that records what it was asked to
  run is easier to read than a mock-expectation DSL
- Only fake at real boundaries: the process runner, the clock, the
  filesystem when a fixture won't do — never fake `kata-core` domain
  logic itself

## One Concept Per Test

- A test verifies one behavior; multiple assertions are fine if they all
  check that one behavior
- If the test name needs "and" to describe it, split it into two tests

## Test Readability

- Arrange-Act-Assert, with each section visually distinct
- Some duplication across tests is fine — don't build a shared harness
  that makes any single test's setup unreadable in isolation
- Use `#[test]` in a trailing `mod tests` for unit tests (matches
  `risk.rs`, `header.rs`), and `crates/*/tests/*.rs` for integration tests
  that exercise a public API end to end (matches `header_fixtures.rs`,
  `sesami_shaped.rs`, `mcp_server.rs`)

## What NOT to Do

- Do not test private functions directly through `#[cfg(test)]` hacks —
  test through the crate's public API
- Do not depend on the real `$HOME`, real network, or wall-clock sleeps
- Do not skip a flaky test with `#[ignore]` — find and fix the flakiness
- Do not chase 100% line coverage as a goal — see `mutation-testing` for a
  better signal of whether tests actually assert anything
