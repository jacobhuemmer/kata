---
name: spec-workflow
description: Spec-driven development workflow for kadou, treating docs/design/05-prd.md as the contract. Use when implementing a new feature, planning a PRD slice, mapping PRD sections to tests, or when the user references the PRD or a decision row. Triggers on spec/PRD mentions, slice planning, or acceptance-criteria mapping.
user-invocable: false
---

# Spec-Driven Workflow for kadou

`docs/design/05-prd.md` is the source of truth for what gets built. This
is not a metaphor: PRD section numbers appear in commits, tests, and code
comments so any line of code can be traced back to the decision that
required it.

## The Document Set, in Precedence Order on Conflict

1. `docs/design/08-shape-review.md` and `docs/design/09-tui-decision.md`
   — win over the PRD itself (its own header says so)
2. `docs/design/05-prd.md` — the contract: format (§4), MCP surface (§5),
   exec/safety (§6), CLI UX (§7), non-goals/risks (§8), slice plan (§9)
3. `docs/design/07-review.md` — resolved findings, historical context only
4. `docs/design/03-principles.md` — charter and non-goals; consult before
   adding anything not already in the PRD
5. `docs/design/06-session-mining.md` — `kata-mine` pipeline contract,
   with packaging overridden by PRD §6.8 (Rust crate, not Go `cmd/`)

`docs/design/tools-list.json` is not prose — it is the literal byte
contract for `tools/list`, enforced by an `insta` snapshot.

## Workflow

1. **Find the governing section** before writing code. Grep the PRD for
   the feature's noun (`needs`, `pending_grant`, `redact`) rather than
   guessing at behavior from the code alone.
2. **Check the decision table (§2)** and **non-goals (§8.1)** — if the
   PRD explicitly declined a feature (a fifth arg type, a `run_shell`
   tool, an MCP vault-write path), do not build it even if it looks like a
   small addition.
3. **Write tests from the PRD's acceptance criteria** — a decision row's
   "Reason" and a §9 slice's "Done" line are the acceptance criteria;
   each becomes one or more test cases (`tdd-workflow`).
4. **Implement to pass the tests** — the PRD section is the boundary of
   the work; don't expand scope past what it specifies (YAGNI,
   `clean-code-design`).
5. **Verify** — all forced-verification commands pass (`CLAUDE.md`), and
   the specific acceptance criteria are checked, not just "tests are
   green" in general.
6. **Cite the section** — in the commit message or a `//` comment only
   when the "why" is non-obvious from the code (repo-wide comment policy),
   e.g. `// §6.1: stdin is always /dev/null, never the MCP server's own`.

## Spec-to-Code Traceability

- **Names** mirror PRD vocabulary exactly — see `CLAUDE.md`'s Vocabulary
  section and `clean-code-naming`
- **Tests** name the PRD behavior, not the function under test
  (`clean-code-testing`)
- **Functions** map to one PRD requirement each — if a function can't be
  traced to one, question whether it's needed
- **Error handling** matches the PRD's documented error cases exactly
  (`no_such_kata`, `invalid_args`, `missing_needs`, `draft`, `timeout` —
  §5.5) — don't invent a sixth error kind for a case the PRD already
  covers with one of these

## When the PRD Is Ambiguous or Silent

- Do not guess and code around it. Flag the ambiguity and ask, or check
  whether `08-shape-review.md`/`09-tui-decision.md` already resolved it
  (they frequently did — check there before treating something as open)
- If implementation reveals a real gap the PRD doesn't cover, that's a
  principles/PRD question for Mason, not a place to improvise a silent
  default — see safety invariant 2 in `CLAUDE.md` (agent ceilings) for why
  a silently-chosen default in this product is a safety question, not a
  style choice

## Slice Plan (§9)

The PRD's §9 slice plan sequences work (starter kata → exec → vault →
MCP tools → safety hardening → mining). When picking up a task, confirm
which slice it belongs to and that its prerequisites (an earlier slice's
"Done" line) are actually met before building on top of them.

## Review Checklist Before Implementing

- [ ] The PRD section or decision row is identified and read in full
- [ ] `08-shape-review.md`/`09-tui-decision.md` don't override it
- [ ] §8.1 non-goals doesn't already rule this out
- [ ] Acceptance criteria are concrete enough to become test names
- [ ] Vocabulary matches (`CLAUDE.md`) — no legacy Go dops nouns creeping
      in from the source material during a port
