---
name: clean-code-solid
description: SOLID design principles applied to kadou's Rust traits and modules. Use when creating or editing Rust structs, traits, or modules, or when making architectural decisions about type relationships, dependency injection, or crate structure. Triggers on new struct/trait definitions, refactoring type relationships, or adding dependencies between crates.
user-invocable: false
---

# SOLID Principles for kadou (Rust traits and modules)

Rust has no classes; SOLID applies to structs, traits, and the module/crate
graph. Apply these when designing kadou's types and crate boundaries.

## Single Responsibility Principle

- A struct has one reason to change. `ParsedHeader` parses; it does not
  also resolve needs or serialize to JSON — those are `resolve.rs` and
  `tools.rs`'s jobs respectively
- A module is also a unit of responsibility: `redact.rs` only redacts,
  `env.rs` only builds the MCP child environment
- Signal a violation: you need "and" to describe what a struct does

```rust
// Bad: mixed concerns
struct Kata { /* ... */ }
impl Kata {
    fn parse(source: &str) -> Self { /* ... */ }
    fn write_to_history(&self) { /* ... */ }
    fn redact_secrets(&self) -> String { /* ... */ }
}

// Good: separated
mod header { pub fn parse_header(source: &str) -> (Option<ParsedHeader>, Vec<Diagnostic>) { /* ... */ } }
mod history { pub fn record(kata: &Kata, outcome: &RunOutcome) { /* ... */ } }
mod redact { pub fn redact_all(text: &str, secrets: &[String]) -> String { /* ... */ } }
```

## Open/Closed Principle

- New PRD-driven variants extend via a new `match` arm or a new trait impl,
  not by rewriting an existing, tested function
- New arg types are added by extending `ArgKind` (§4.3: `text`, `int`,
  `bool`, `select` — closed by the PRD itself; adding a fifth type is a PRD
  change, not a code-only change)
- A genuinely open extension point (e.g. a new risk-ceiling source) should
  be a small trait new callers implement, not a growing `if`/`else if`
  chain in `visibility.rs`

## Liskov Substitution Principle

- Any implementation of a trait must be substitutable without breaking
  correctness — a test-double `Runner` must not silently no-op where the
  real one would execute, or a test proves nothing
- Do not implement a trait partially with `unimplemented!()` for the
  awkward cases — if a type can't fulfill the contract, it shouldn't
  implement the trait

## Interface Segregation Principle

- Keep traits small — a consumer that only needs to read the vault
  shouldn't depend on a trait that also lets it write
- Model this the way the MCP invariant already forces it: the visibility
  and read paths (`list_kata`, `describe_kata`) never see a write
  capability; only the human CLI vault path does (`clean-code-boundaries`,
  safety invariant 4 in the repo `CLAUDE.md`)

## Dependency Inversion Principle

- `kata-mcp` depends on `kata-core` and `kata-exec`'s public types, not
  the reverse — the crate graph is a DAG, `kata-core` knows nothing about
  MCP or exec
- Accept trait bounds or narrow interfaces as parameters, return concrete
  types — the same "accept interfaces, return structs" idiom carries over
  from Go to Rust as "accept `impl Trait`/generics, return concrete types"
- Wire concrete implementations at the edge: `main.rs` constructs the real
  `ServerState`/config and passes it down; library modules never reach for
  a global or read `std::env` directly outside `paths.rs` (`clean-code-boundaries`)

## Rust-Specific Guidance

- No inheritance — use composition (a struct holding another) for reuse,
  traits for polymorphism
- Accept `&impl Trait` / generic bounds, return concrete types — the Rust
  form of DIP
- Keep the crate dependency graph acyclic: if `kata-mine` ever needs
  something from `kata-mcp`, extract a shared type into `kata-core`
  instead of introducing a back-edge (`kata-mine` currently has **no MCP
  types** — keep it that way, per §3 "Why split this way")
