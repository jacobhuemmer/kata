# Contributing to kadou

Start with [`CLAUDE.md`](../CLAUDE.md) at the repo root — it's the single
source for how work on this repo gets done: the governing loop, forced
verification commands, commit conventions, the product vocabulary, and the
safety invariants that must never regress.

## Before you write code

1. Read the PRD section that governs the change:
   [`docs/design/05-prd.md`](design/05-prd.md) is the contract. On
   conflict, [`08-shape-review.md`](design/08-shape-review.md) and
   [`09-tui-decision.md`](design/09-tui-decision.md) win over it.
   [`03-principles.md`](design/03-principles.md) has the charter and
   non-goals — check it before adding anything the PRD doesn't already
   specify.
2. Read the crate you're about to touch. Don't guess a module's shape from
   its name — `kadou-core`, `kadou-exec`, `kadou-mcp`, and `kadou-mine`
   each own a distinct part of the engine (see `05-prd.md` §3).
3. Load the relevant skill under [`.claude/skills/`](../.claude/skills/)
   before applying it — `spec-workflow` for slice planning,
   `tdd-workflow` for the red/green/refactor commit convention, the
   `clean-code-*` family for Rust style, `create-kata` for writing a new
   kata, `mutation-testing` for test-quality checks, `shell-scripts` for
   kata script bodies.

## The slice plan

Work is sequenced by `docs/design/05-prd.md` §9: starter kata and CLI
frame → exec engine → vault and needs resolution → the four MCP tools →
safety hardening (grant flow, redaction, env allowlist) → session mining.
Confirm which slice a task belongs to, and that its prerequisites are
actually landed, before building on top of them.

## Workflow

1. **TDD, always** (`tdd-workflow`): a commit with only a failing test
   lands before the commit that makes it pass. Two commits minimum per
   behavior.
2. **Small conventional commits**: `<type>(<crate>): <imperative summary>`
   — `feat`, `fix`, `test`, `refactor`, `docs`, `chore`, scoped to the
   crate you changed. Match the existing `git log`.
3. **One worktree per task.** Don't share a worktree across unrelated
   work, and respect crate/directory ownership boundaries when multiple
   people or agents are working in parallel.
4. **Forced verification before calling anything done:**
   ```sh
   cargo +stable fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cargo +1.88 check --workspace --all-targets
   cargo deny check
   cargo mutants   # where a crate has a mutants config, or per mutation-testing on security-relevant changes
   ```
   State explicitly which of these you ran. A green `cargo check` alone is
   not a claim of behavioral correctness.

## Vocabulary

Use **kadou, kata, folder, header, args, needs, vault, risk, grant, draft,
history** — never **runbook** or **catalog** — in code, comments, commits,
and skills. See `CLAUDE.md`'s Vocabulary section for the full mapping from
the legacy Go dops nouns.

## Safety invariants

Six invariants must never regress, listed in full in `CLAUDE.md`: the four
MCP tools stay byte-identical to `docs/design/tools-list.json`; agents
never see high/critical risk by default; needs can never be passed as
args; MCP never writes the vault; secrets are redacted before a result or
log is written; MCP-spawned children get an explicit env allowlist, never
the agent host's full environment. A change that weakens any of these is a
correctness bug, not a simplification.

## Gates

No pushes, remote repos, crate/package publishing, domain registration, or
renaming of `rundops` without Mason's explicit approval. No secrets or raw
agent session content in this repo (see the repo [`README.md`](../README.md)).
