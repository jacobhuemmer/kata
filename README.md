# Kata

Rust rewrite of [dops](https://github.com/rundops/dops) as **Kata**:
a script library that serves as an MCP server for AI agents, built to cut
tokens and prefer scripts and automation over model reasoning. The unit is a
**kata** — one script with a closed-grammar header — kept in a **catalog**
on disk with no registry; there is no full-screen TUI, only a styled CLI with a
built-in picker and inline prompts.

Binary `kata`, crates `kata-cli` / `kata-core` / …, config `~/.config/kata`,
catalog `~/.config/kata/catalog/`, env `KATA_*`, MCP `kata mcp serve`.
crates.io `kata` is a different product — we publish as `kata-cli` if at all.
See `docs/design/13-kata-rename.md`. The old name **kadou** is retired.

## Design docs

| Phase | Doc |
|---|---|
| 1 | `docs/design/01-audit.md` |
| 2 | `docs/design/02-competitors.md` |
| 3 | `docs/design/03-principles.md` |
| 4 | `docs/design/04-naming.md` — naming research (historical; product is now Kata). |
| 13 | `docs/design/13-kata-rename.md` — kadou → Kata; catalog as the collection. |
| 5 | `docs/design/05-prd.md` |
| 6 | `docs/design/06-session-mining.md` |
| 7 | `docs/design/07-review.md` — independent review of the phase-5 PRD draft; its blocking items are resolved in `05-prd.md`'s Revision log (§13). |
| 8 | `docs/design/08-shape-review.md` — single-file kata, folders as namespaces, no registry; adopted in full. |
| 9 | `docs/design/09-tui-decision.md` — drops the full-screen TUI for a styled CLI with a built-in picker; adopted in full (decisions D1–D7). |

## Gates

No pushes, remote repos, crate or package publishing, domain registration,
or renaming of rundops without Mason's explicit approval. No secrets or raw
agent session content in this repo.
