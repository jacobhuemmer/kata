# CLAUDE.md — kadou engineering directives

kadou is a Rust script library that is also an MCP server for AI agents
(`docs/design/05-prd.md` §1.1). These directives are how work on this repo
gets done. They adapt Sesami's dops-practices for Rust; read the relevant
`.claude/skills/*/SKILL.md` before applying it — load only what the task
needs.

## The governing loop

**Read the spec → write a failing test → make it pass → verify → commit →
repeat.** Every rule below serves one of these phases.

1. **Gather context.** Find the PRD section that governs the change
   (`docs/design/05-prd.md`, see "Spec is the contract" below). Read the
   crate you're touching before editing it — don't guess at a module's
   shape from its name.
2. **Take action.** Red commit, then green commit (see TDD below). One
   logical change per commit, scoped to a single crate where possible.
3. **Verify work.** Run the full forced-verification list before calling
   anything done. Never report success on a static read of the diff alone.
4. **Repeat.** Small steps, not batch rewrites.

## Spec is the contract

`docs/design/05-prd.md` is the product and architecture contract. Section
numbers in commit messages, code comments, and skill files should trace
back to it. On conflict, `docs/design/08-shape-review.md` and
`docs/design/09-tui-decision.md` win over the PRD, which wins over
`docs/design/07-review.md` (the PRD's own header states this precedence).
`docs/design/03-principles.md` is the charter and non-goals list —
consult it before adding anything the PRD doesn't already specify.
`docs/design/06-session-mining.md` governs `kadou-mine`; `docs/design/
tools-list.json` is the byte-for-byte MCP wire contract. `docs/design/
README.md` indexes all nine design docs.

If a change isn't traceable to a PRD section or a principles rule, it's
scope creep — flag it instead of building it (`docs/design/03-principles.md`
non-goals, §8).

## TDD is the default workflow

Every feature and bug fix is Red → Green → Refactor, one behavior at a
time. Use the `tdd-workflow` skill. In this repo that means:

- A commit containing only a new or extended failing test lands *before*
  the commit that makes it pass. Two commits minimum per behavior, not
  one that includes both.
- The failing test must actually fail (or fail to compile) before you
  write implementation. If it passes immediately, it isn't testing new
  behavior.
- Refactor only under green tests, in its own commit when it's more than
  a one-line cleanup.

Exceptions: exploratory spikes you intend to throw away, and pure CLI
frame/output-formatting code you verify by running `kadou` directly — see
`clean-code-testing`.

## Forced verification

Nothing is "done" until all of these pass locally. State explicitly which
you ran; never claim a behavioral fix from a green `cargo check` alone.

```sh
cargo +stable fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo +1.88 check --workspace --all-targets   # MSRV gate, docs/design/05-prd.md §3.2
cargo deny check                               # license/advisory gate, §3.3
cargo mutants                                   # where a crate has a mutants config; see mutation-testing skill
```

These mirror `.github/workflows/ci.yml` (`fmt`, `clippy`, `test` on macOS +
Linux, `msrv`, `deny`); `cargo mutants` is not yet a CI job, run it
locally per the `mutation-testing` skill when a slice touches
security-relevant logic (visibility, redaction, vault, header parsing).

## Small conventional commits

Match the existing history: `<type>(<crate>): <imperative summary>`, e.g.
`feat(kadou-core): add kadou check diagnostics`,
`test(kadou-mcp): drive the real server end to end over an in-process duplex`.
Common types: `feat`, `fix`, `test`, `refactor`, `docs`, `chore`. Scope by
crate name (`kadou`, `kadou-core`, `kadou-exec`, `kadou-mcp`, `kadou-mine`)
or `kadou-mcp/tools` style sub-scope when it disambiguates. One logical
change per commit — a red-test commit and its green-implementation commit
are two commits, not a squash.

## One worktree per task

Each task (slice, bugfix, refactor) gets its own git worktree and branch.
Do not share a worktree across unrelated tasks, and do not touch files
outside your task's authorized scope — when multiple workers run in
parallel, respect the crate/directory boundaries your assignment states
(e.g. "don't touch `crates/**`" for a docs-only task).

## Vocabulary

Use exactly these words in code, comments, commit messages, and skills —
they are the PRD's own nouns (`docs/design/05-prd.md` §1.1, §4):

**kadou** (the product/binary), **kata** (one script with a header — the
unit), **folder** (a directory under `kata/`, a namespace, not a
registry), **header** (the closed six-key comment grammar, §4.3), **args**
(what an agent may set via `run_kata`), **needs** (what only the vault may
supply), **vault** (the age-encrypted secret store), **risk** (one of
`low`/`medium`/`high`/`critical`), **grant** (human approval of a pending
high/critical run), **draft** (an unaccepted `proposed/` or `mined/`
kata), **history** (the run-record + log store).

Never write **runbook** or **catalog** in code, comments, or new
`.claude/skills/*` content — those are the legacy Go dops nouns this
rewrite replaces (`docs/design/05-prd.md` §1.1 migration table). The two
sanctioned exceptions: this vocabulary rule itself, and the migration note
in `create-kata/SKILL.md` explaining what it replaces.

## Safety invariants — never regress these

These are tested invariants (`docs/design/05-prd.md` §9), not conventions.
A change that weakens any of them is a correctness bug, not a
simplification, regardless of how it affects test coverage:

1. **The four MCP tools are byte-identical to `docs/design/tools-list.json`.**
   `list_kata`, `describe_kata`, `run_kata`, `propose_kata`, in that order,
   nothing else — hand-authored `inputSchema`, checked by an `insta` byte
   snapshot (§5.4, §9 slice 5). `crates/kadou-mcp/src/schema.rs` is the
   only place this JSON is built.
2. **Agents never see high/critical by default.** Default `agent.max_risk`
   is `low`; the visibility formula lives in
   `crates/kadou-mcp/src/visibility.rs` (`human_ceiling`, `agent_ceiling`,
   `is_visible_risk`). `--allow-risk` / `--max-risk` may only narrow, never
   raise, an agent's ceiling above config (§6.2).
3. **Needs are never args.** The split is structural, not a runtime scope
   check — a secret is a need, never an arg, by construction (§4.4). A
   `run_kata` call naming a need in `args` is `invalid_args`, never a
   lookup.
4. **MCP never writes the vault, in any scope.** Vault mutation
   (`kadou vault set`) is human-CLI-only (§6.5). If you find yourself
   wiring a vault write path through `kadou-mcp`, stop — that path
   doesn't exist in this product.
5. **Secrets are redacted in results and logs, before either is
   written.** `crates/kadou-mcp/src/redact.rs` (`redact_all`) runs on the
   line stream before it hits the history log or the MCP result — never
   redact-on-read (§6.6).
6. **MCP children get an explicit env allowlist, never the agent host's
   full environment.** `crates/kadou-mcp/src/env.rs` (`build_mcp_env`)
   builds the allowlist; the CLI path keeps the full parent environment
   because that's a human's own shell, not an agent's (§6.1). Do not let
   an MCP-spawned child inherit ambient env "to fix" a missing variable —
   add it to `[exec] pass_env` or the allowlist instead.

## Design doc pointers

| Doc | What it governs |
|---|---|
| `docs/design/03-principles.md` | Charter, non-goals — check before adding scope |
| `docs/design/04-naming.md` | Why `kadou`/`kata` were chosen (reference only, not a rename) |
| `docs/design/05-prd.md` | The contract: format, MCP surface, exec safety, CLI UX, slice plan |
| `docs/design/06-session-mining.md` | `kadou-mine` pipeline, redaction, review gate |
| `docs/design/08-shape-review.md` | Single-file kata, folders-not-registry — wins over the PRD |
| `docs/design/09-tui-decision.md` | No full-screen TUI in v1 — wins over the PRD |
| `docs/design/tools-list.json` | Byte-for-byte `tools/list` payload |

## Gates (repo-level, from `README.md`)

No pushes, remote repos, crate/package publishing, domain registration, or
renaming of `rundops` without Mason's explicit approval. No secrets or raw
agent session content in this repo.
