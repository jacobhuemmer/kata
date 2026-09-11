---
name: clean-code-naming
description: Rust naming conventions and Clean Code naming principles for kadou. Use when creating, renaming, or reviewing names of functions, variables, types, traits, modules, or crates in Rust code. Triggers on new type definitions, function signatures, variable declarations, module naming, and crate naming.
user-invocable: false
---

# Clean Code Naming for kadou (Rust)

Apply these naming principles to all Rust code. Rust idioms take precedence
over generic Clean Code advice. Use the product vocabulary from the repo
`CLAUDE.md` (kata, folder, header, args, needs, vault, risk, grant, draft,
history) — never the legacy Go dops nouns banned in `CLAUDE.md`'s
vocabulary rule.

## Crate and Module Names

- Short, lowercase, `snake_case`, single concept: `kadou-core`, `kadou-exec`,
  `kadou-mcp`, `kadou-mine`
- A module names what it *provides*: `header`, `risk`, `vault`, `visibility`
  — not `util`, `common`, `helpers`, `misc`
- The module name is part of the call site: `header::parse_header`, not
  `helpers::parse_the_header_thing`

## Trait Names

- Single-method traits use the verb + `-er`/behavior suffix when idiomatic:
  `Runner` (`kadou_exec::Runner`)
- Multi-method traits describe the capability, not the implementer:
  `VaultStore`, not `VaultImpl`
- Do not prefix with `I` — not Rust idiom
- Define a trait where it is *consumed* (`clean-code-boundaries`), not where
  it happens to be implemented first

## Function Names

- `snake_case`, start with a verb: `parse_header`, `resolve_needs`,
  `build_mcp_env`, `redact_all`
- Getters omit `get`: `id()` not `get_id()` (Rust convention)
- Constructors: `new` for the obvious case, `from_*`/`try_from_*` for
  fallible or converting construction, matching `From`/`TryFrom` where a
  trait impl fits
- Boolean-returning functions read as questions: `is_visible_risk`,
  `is_trusted`, `looks_like_kata_candidate`
- If the name needs a comment to explain what it does, rename it instead

## Variable Names

- Short names in small scopes are fine and idiomatic: `id`, `cfg`, `ctx`,
  `err`
- Longer, descriptive names once scope grows or the type isn't obvious from
  context
- No Hungarian notation, no type-in-name (`str_name`, `int_count`)
- Boolean variables read as predicates: `trusted`, `visible`, `truncated`
- Avoid `data`, `info`, `temp`, `val` — they say nothing about the domain

## Constants

- `SCREAMING_SNAKE_CASE`: `KILL_GRACE`, `MAX_HEADER_LINES`
- Group related constants in the module they govern, not a shared
  `constants.rs` grab-bag

## Type Names

- Nouns describing what the type *is*: `Kata`, `ParsedHeader`, `RiskLevel`,
  `ServerState`
- No `Type`/`Struct` suffix: `Config` not `ConfigStruct`
- Avoid stutter with the module name: `header::ParsedHeader` is fine,
  `header::HeaderParsedHeader` stutters

## Error Variant Names

- `thiserror` enum variants name the failure, not the mechanism:
  `ExecError::Spawn { program, source }`, not `ExecError::IoError1`
- See `clean-code-errors` for the full error-type contract

## Spec Traceability

- Names mirror `docs/design/05-prd.md` vocabulary exactly: if the PRD says
  "need", the field is `needs`, never `secrets` or `secret_params`
- Consistent vocabulary between the PRD and code eliminates translation
  overhead when tracing a bug back to a section number

## What NOT to Do

- Do not rename established Rust conventions (`err`, `ctx`, `self`, `_`)
- Do not add comments to compensate for a bad name — fix the name
- Do not abbreviate unless the abbreviation is already in the PRD
  (`cfg`, `mcp`) or the standard library (`fmt`, `io`)
- Do not shadow a builtin or a workspace-wide type name
