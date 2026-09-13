---
name: clean-code-boundaries
description: Clean Code boundary and encapsulation principles for kadou's Rust crates. Use when integrating third-party crates (rmcp, age, inquire, crossterm), wrapping SDK clients, defining module/crate visibility, or managing dependencies at the edges of the workspace. Triggers on new external-crate usage, new pub/pub(crate) visibility decisions, or new inter-crate dependencies.
user-invocable: false
---

# Clean Code Boundaries for kadou (Rust)

Apply these principles wherever kadou code touches a third-party crate or
defines a visibility boundary between its own crates.

## Wrap External Dependencies Behind Small Modules

- Never let a third-party type leak into `kata-core` domain code:
  - **rmcp** types stay inside `kata-mcp` (`server.rs`, `schema.rs`,
    `tools.rs`); `kata-core` and `kata-exec` have zero rmcp dependency
    and must stay that way — mining and CLI-only paths never need to know
    what an MCP tool call looks like
  - **age** (vault crypto) stays behind `kata-core::vault` — callers ask
    for a resolved need or write a vault entry through that module's
    functions, never construct an `age::Identity` or handle ciphertext
    bytes themselves
  - **inquire** (interactive prompts) and **crossterm** (picker, styled
    output) stay inside the `kadou` bin's `ui` module (§3 crate table) —
    `kata-core`/`kata-exec`/`kata-mcp` must never prompt a TTY or touch
    a terminal
- When a wrapped crate changes (an `age` major bump, a `rmcp` API change),
  only the owning module changes — callers are unaffected

## Define Traits at the Consumer

- The module that *uses* a capability defines the trait/function signature
  it needs; the module that provides it satisfies that shape
- Example: `kata-exec::Runner` defines what "run a kata" means; the
  process-group/shebang implementation lives behind it. A future test
  double implements the same shape without `kata-exec` importing test
  code

## Encapsulation via `pub`/`pub(crate)`

- Default to private; export only what another crate genuinely needs
- Use `pub(crate)` for cross-module-same-crate visibility instead of `pub`
  when nothing outside the crate should see it (matches `kata-core/src/
  lib.rs`'s pattern of `mod header;` — parser internals are not `pub`)
- A crate's exported API is its contract — keep `kata-core`'s public
  surface (`Kata`, `RiskLevel`, `ParsedHeader`, `Config`, ...) minimal and
  deliberate

## Principle of Least Privilege

- Pass the specific data or capability a function needs, not the whole
  `ServerState`/`Config` when a narrower borrow would do
- The MCP env allowlist (`kata-mcp/src/env.rs::build_mcp_env`) *is* least
  privilege as a safety invariant, not just a style preference — a kata
  process gets exactly `PATH HOME USER LOGNAME SHELL LANG LC_* TZ TMPDIR
  SSH_AUTH_SOCK KUBECONFIG` plus declared args/needs, never the agent
  host's full environment (§6.1)

## Anti-Corruption Layer

- Translate at the boundary when an external format doesn't match kadou's
  domain language: `kata-core::import` converts the legacy Go dops
  per-item YAML-plus-script pair into a kata header — that translation
  lives in one module, and nothing downstream of it ever sees the old
  shape
- MCP wire JSON (`schema.rs`, `tools.rs`) is the anti-corruption layer
  between `kata-core` domain types and the exact bytes in
  `docs/design/tools-list.json` — domain types don't grow
  `#[serde(rename)]` contortions to match the wire; a small translation
  function does

## Learning Tests for New Third-Party Crates

- When adopting or upgrading a boundary crate (`age`, `rmcp`, `serde-yaml-ng`),
  write a test that verifies your understanding of its behavior — the
  existing Go-vault-import round-trip test against a real `age`-encrypted
  envelope is exactly this: it proves the assumption and catches a
  breaking upgrade immediately
- Learning tests live next to the module that owns the boundary
  (`kata-core/src/vault.rs`'s tests, not a separate crate)

## Configuration Boundaries

- Env vars, `kata.toml`, and CLI flags are external input — parse and
  validate them at the boundary (`config.rs`, `paths.rs`, `commands.rs`)
  into typed values; domain code never calls `std::env::var` directly
  except inside `paths.rs`, which exists precisely to own that boundary
  (`KATA_HOME`/`DOPS_HOME` resolution)

## Spec Traceability

- The PRD describes behavior in kata/folder/need language; boundary
  wrappers exist so implementation code can say `vault::resolve_need(name)`
  instead of leaking an `age::scrypt::Recipient` into a function that's
  supposed to be about resolving a kata's needs
