---
name: mutation-testing
description: Mutation testing with cargo-mutants for kadou's Rust crates. Use when assessing test quality, finding weak tests, or when the user asks about mutation testing. Triggers on "mutation test", "cargo mutants", "test quality", "test effectiveness", or after completing a feature in security-relevant code (header parsing, risk/visibility, redaction, vault) to verify test strength.
user-invocable: true
---

# Mutation Testing with cargo-mutants

cargo-mutants tests your tests: it introduces small changes (mutants) into
kadou's code and checks whether the test suite catches them. A surviving
mutant means a test exists but doesn't actually assert the thing it looks
like it asserts.

**Status in this workspace:** no `.cargo/mutants.toml` exists yet and
`cargo-mutants` is not yet wired into CI (`.github/workflows/ci.yml` has
`fmt`/`clippy`/`test`/`msrv`/`deny`, no `mutants` job). Run it locally per
this skill on any slice touching safety-relevant logic; add a per-crate
config when a crate's baseline is established enough to gate on.

## Install

```sh
cargo install cargo-mutants --locked
```

## Quick Start

```sh
# See what would be mutated, no testing
cargo mutants --list -p kadou-mcp

# Run mutation testing on one crate (start here — full-workspace runs are slow)
cargo mutants -p kadou-mcp

# Only mutate what changed since main (fast, use before opening a PR)
cargo mutants --in-diff <(git diff main)
```

Prefer `-p <crate>` or `--in-diff` over a bare `cargo mutants` on the
whole workspace — the same reasoning as `--diff` in Go's gremlins: full
runs are for a periodic sweep, not every change.

## Where to Run It First

Priority order, matching kadou's own safety invariants (`CLAUDE.md`):

1. `kadou-mcp/src/visibility.rs` — `human_ceiling`/`agent_ceiling`/
   `is_visible_risk` (agents-never-see-high/critical is a tested
   invariant; mutation testing proves the test suite would actually catch
   a broken comparison operator here)
2. `kadou-mcp/src/redact.rs` — `redact_all`/`variants` (a mutant that
   drops one redaction variant and survives means a secret could leak)
3. `kadou-mcp/src/env.rs` — `build_mcp_env` (the allowlist itself)
4. `kadou-core/src/header.rs` — the bespoke header parser (§6.2 risk 1
   already treats its error messages as a tested contract via `insta`;
   mutation testing complements that)
5. `kadou-core/src/vault.rs` — resolution logic (not the `age` crypto
   itself — see `clean-code-boundaries` on learning tests instead)

## Reading Results

| Status | Meaning | Action |
|---|---|---|
| **CAUGHT** | Tests caught the mutation | Good, no action |
| **MISSED** | Tests did NOT catch the mutation | Write a test that kills it |
| **UNVIABLE** | Mutation didn't compile | Ignore |
| **TIMEOUT** | Tests hung on the mutant | Usually fine (broke a loop) |

## Workflow After Completing a Feature

```sh
# 1. Tests must be green first
cargo test -p <crate>

# 2. Mutate what changed
cargo mutants --in-diff <(git diff main) -p <crate>

# 3. For each MISSED mutant, add a test that would catch it, following
#    tdd-workflow's Red/Green commit convention:
#    test(<crate>): assert exact ceiling boundary for is_visible_risk
#    fix(<crate>): (only if the mutant revealed an actual bug, not just a weak test)

# 4. Re-run to confirm CAUGHT
```

## Interpreting MISSED Mutants

- **A boundary mutation survived** (`<=` became `<`): the test suite
  never exercises the exact boundary value (e.g. a kata whose risk is
  *exactly at* the ceiling — §6.2 defines "at the ceiling is allowed" as
  the precise boundary). Add a test for that exact value.
- **An arithmetic mutation survived**: the test doesn't assert an exact
  value, just "doesn't panic" or "returns Ok". Assert the value.
- **A condition negation survived** (`&&` became `||`): both branches
  produce an acceptable result in every existing test — either the test is
  too loose, or the mutant is equivalent (see below).

**Equivalent mutants (false positives):** some mutations produce
semantically identical code (e.g. negating a condition that returns the
same value either way). When you find one, note it and move on rather than
writing a test that can't distinguish real behavior.

## What It Does NOT Catch

- Missing scenarios entirely (it only mutates code that's covered)
- Concurrency bugs (relevant to `kadou-mcp`'s per-server concurrency
  limit, §6.1 — cover those with a dedicated concurrency test instead)
- Incorrect error message text (mutation doesn't touch string literals by
  default) — that's what the `insta` snapshot corpus in
  `clean-code-testing` is for

Mutation testing complements the `insta` snapshot corpus and the PRD
acceptance-criteria tests — it doesn't replace either.
