---
name: clean-code-design
description: Clean Code design maxims for kadou's Rust workspace — DRY, KISS, YAGNI, separation of concerns, composition, Law of Demeter. Use when making architectural decisions, creating new modules or crates, refactoring code structure, or reviewing overall code organization. Triggers on new module/crate proposals, structural refactors, or scope-creep questions.
user-invocable: false
---

# Clean Code Design Principles for kadou (Rust)

Apply these principles when making structural or architectural decisions
across the `kadou-core` / `kadou-exec` / `kadou-mcp` / `kadou-mine` / `kadou`
workspace.

## YAGNI — You Aren't Gonna Need It

- Implement only what `docs/design/05-prd.md` requires for the current
  slice — nothing more
- Do not add config keys, feature flags, or extension points the PRD
  doesn't specify "in case someone needs them later" — §8.1 non-goals is
  explicit about this (no plugin marketplace, no unopinionated defaults)
- If it's not in the PRD or a decision row, it doesn't get built — raise it
  as a principles question instead

## KISS — Keep It Simple

- The simplest solution that satisfies the PRD is the best solution
- Three similar match arms are better than a premature trait abstraction
- The bespoke header parser (`header.rs`) is the model: ~900 lines of
  direct parsing beats pulling in a general YAML library for a closed
  six-key grammar (`08-shape-review.md` §3.1)
- Boring, direct code over clever generics

## DRY — Don't Repeat Yourself

- Every piece of domain knowledge has one authoritative representation:
  the risk order lives in `risk.rs`, not re-encoded ad hoc in `tools.rs`
- DRY applies to knowledge, not to identical-looking code serving
  different purposes — two `match` arms that happen to look similar but
  answer different questions are not duplicates
- Do NOT apply DRY to tests — table-driven coverage beats collapsing two
  test cases into one parameterized abstraction that obscures intent

## Separation of Concerns

- I/O and domain logic stay separate: `kadou-core` parses and resolves,
  `kadou-exec` runs processes, `kadou-mcp` speaks the wire protocol, the
  `kadou` bin wires CLI + config + calls into all three
- Don't mix wire-format concerns (JSON shaping for `tools/list`) into
  `kadou-core` domain types — `kadou-mcp/src/schema.rs` and `tools.rs` own
  that translation

## Composition Over Inheritance

- Rust has no inheritance — build behavior from small traits and structs
  that compose, and reuse via a `Runner`/`Store`-shaped trait rather than a
  shared base struct
- Favor free functions over methods when the operation doesn't need state
  (`redact_all`, `build_mcp_env`, `is_visible_risk` are all free functions,
  not methods on a god-object)

## Law of Demeter

- A function calls methods on: its own parameters, values it creates, and
  its own fields — not a chain reached through someone else's field
- No train wrecks: `state.config.folder.policy.max_risk` — if you need data
  buried in a structure, expose an accessor at the right level
  (`visibility::human_ceiling(config, folder)`)

## Boy Scout Rule

- When implementing a PRD slice, clean up the code you touch: rename a
  confusing local, extract a function, remove a dead branch
- Don't refactor unrelated modules while implementing an unrelated slice —
  scope cleanup to what you're already touching

## Kent Beck's Four Rules of Simple Design (priority order)

1. **Passes all tests** — every PRD acceptance criterion in scope is
   covered
2. **No duplication** — shared concepts (risk ordering, id parsing)
   implemented once
3. **Expresses intent** — code reads like the PRD section it implements
4. **Minimizes types/functions** — no trait or module the PRD doesn't need

When rules conflict, a higher-numbered rule yields to a lower-numbered one.
