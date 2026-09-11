# kadou

Rust rewrite of [dops](https://github.com/rundops/dops) as **kadou** (稼働):
a script library that serves as an MCP server for AI agents, built to cut
tokens and prefer scripts and automation over model reasoning. The unit is a
**kata** — one script with a closed-grammar header — kept in **folders** on
disk with no registry; there is no full-screen TUI, only a styled CLI with a
built-in picker and inline prompts.

`kadou` is the product name, chosen by Mason. The prior working codename is
retired; binary, crates, config/data/state directories, env prefix, MCP
server name, and keyring service are all named `kadou` (see
`docs/design/05-prd.md` §10/§13 for the full rename map).

## Design docs

| Phase | Doc |
|---|---|
| 1 | `docs/design/01-audit.md` |
| 2 | `docs/design/02-competitors.md` |
| 3 | `docs/design/03-principles.md` |
| 4 | `docs/design/04-naming.md` — naming research for the product name `kadou`. |
| 5 | `docs/design/05-prd.md` |
| 6 | `docs/design/06-session-mining.md` |
| 7 | `docs/design/07-review.md` — independent review of the phase-5 PRD draft; its blocking items are resolved in `05-prd.md`'s Revision log (§13). |
| 8 | `docs/design/08-shape-review.md` — single-file kata, folders as namespaces, no registry; adopted in full. |
| 9 | `docs/design/09-tui-decision.md` — drops the full-screen TUI for a styled CLI with a built-in picker; adopted in full (decisions D1–D7). |

## Gates

No pushes, remote repos, crate or package publishing, domain registration,
or renaming of rundops without Mason's explicit approval. No secrets or raw
agent session content in this repo.
