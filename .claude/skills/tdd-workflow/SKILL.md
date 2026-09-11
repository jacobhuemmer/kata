---
name: tdd-workflow
description: Test Driven Development workflow for kadou's Rust crates. Use when implementing new features, adding new functions, or when the user asks for TDD. Triggers on "write tests first", "TDD", "test driven", new feature implementation where tests should drive the design.
user-invocable: true
---

# TDD Workflow for kadou (Rust)

When implementing a PRD slice test-first, follow this exact cycle and the
commit convention below. Do not skip steps.

## The Cycle: Red — Green — Refactor

### 1. RED — Write a Failing Test, Commit It Alone

Write the test **before** the implementation exists, in the crate's
`tests/` directory or a trailing `mod tests` block, matching the existing
pattern (`risk.rs`, `header.rs`, `header_fixtures.rs`).

```rust
#[test]
fn parse_risk_level_rejects_unknown_word() {
    let (parsed, diags) = parse_header(FIXTURE_UNKNOWN_RISK);
    assert!(parsed.is_none());
    assert_eq!(diags[0].message, "unknown risk level `extreme`");
}
```

Run `cargo test -p kadou-core parse_risk_level_rejects_unknown_word` —
expect a compile error or a failing assertion. If it passes immediately,
either the test is wrong or the behavior already exists.

**Commit the failing test by itself:**
```
test(kadou-core): add failing case for unknown risk word
```

**Rules for Red:**
- One behavior per test function, named after the behavior
  (`clean-code-testing`)
- The compiler is part of Red — a type error IS a failing test
- Define the function signature in the test first, then satisfy the
  compiler in Green

### 2. GREEN — Write the Minimum Code to Pass, Commit It Separately

Write the smallest implementation that makes the test pass. Don't
generalize beyond what this test demands — the next Red step will force
the next behavior.

Run `cargo test -p kadou-core` — expect green, and re-run the full
`cargo test --workspace` before committing.

**Commit the passing implementation as its own commit:**
```
feat(kadou-core): reject unknown risk words in the header parser
```

Two commits minimum per behavior — never squash the failing-test commit
into the implementation commit. This is what makes `git log` a readable
record of what was proven before it was built.

**Rules for Green:**
- Do not add code "because we'll need it later" (YAGNI,
  `clean-code-design`)
- Hardcode if that's the fastest path to green; the next test forces real
  logic
- Do not refactor yet

### 3. REFACTOR — Clean Up Under Green Tests

With passing tests as a safety net: extract functions, rename, remove
duplication, apply `clean-code-functions`/`clean-code-design`. Re-run
`cargo test --workspace` after every change, not just at the end. If the
refactor is more than a one-line cleanup, give it its own commit:
```
refactor(kadou-core): extract risk-word validation from parse_header
```

Never add new behavior during refactor — only restructure.

### 4. Repeat

Back to Red. Add the next case. Drive the next behavior.

## Practical TDD in This Codebase

```
1. Open or create crates/<crate>/tests/<area>.rs or the trailing mod tests
2. Write the first test case (simplest happy path) — RED
3. cargo test -p <crate> — confirm it fails or doesn't compile
4. Commit: test(<crate>): <behavior>
5. Write the minimal implementation — GREEN
6. cargo test --workspace — confirm green
7. Commit: feat(<crate>): <behavior>
8. Refactor if needed, re-run tests, commit separately if non-trivial
9. Repeat for the next case/edge case
```

## Fixing a Bug: Test First, Always

1. Write a test that fails the same way the bug manifests — RED. Commit it
   alone: `test(kadou-mcp): reproduce needs-as-args bypass`
2. Fix the bug — GREEN. Commit: `fix(kadou-mcp): reject needs named in
   run_kata args`
3. The test is now a permanent regression guard

## What NOT to Do

- Don't write all tests first, then implement everything — that's
  waterfall, not TDD. One test at a time
- Don't skip Red — if a test passes immediately, it isn't testing new
  behavior
- Don't write production code without a failing test demanding it
- Don't test implementation details — test through the crate's public API
  (`clean-code-testing`)
- Don't refactor during Red or Green — keep the phases separate
- Don't chase coverage as a goal — high coverage is TDD's side effect, not
  its purpose (see `mutation-testing` for a stronger signal)

## When TDD Is Not Worth It

- Exploratory spikes you intend to throw away
- Pure CLI frame/output-formatting code (`ui` module) — verify by running
  `kadou` directly (`clean-code-testing`)
- Generated code (none currently in this workspace)

For everything else — header parsing, risk/visibility formulas, vault
resolution, redaction, exec/env — TDD is the default, not an option.
