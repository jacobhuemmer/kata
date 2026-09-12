---
name: create-kata
description: "Create a new kadou kata — a single script file with a closed six-key header. Use when the user asks to create a new kata, add automation, scaffold a script for the kadou library, or convert an idea into a kata. Triggers on: 'create a kata', 'add a kata', 'new kata', 'scaffold a script for kadou'. Replaces dops's create-runbook skill — kadou has no runbook.yaml, no catalog, no registry."
user-invocable: true
---

# Create a kadou Kata

**Migration note:** this replaces dops's `create-runbook` skill. There is
no `runbook.yaml`, no `script.sh` pair, no catalog registry, and no
`catalog add` step in kadou — a kata is **one file**, and a folder exists
because it's on disk (`docs/design/05-prd.md` §1.1, decision 16).

## Where a Kata Lives

```
~/.config/kadou/kata/<folder>/<name>.sh          # single-file form
~/.config/kadou/kata/<folder>/<name>/kata.sh      # multi-file form, plus lib/ tests/ etc.
./kata/<name>.sh                                  # project-local (agent-invisible until `kadou trust`)
```

The id is the path under `kata/` without the extension: `sesami/cc4-aaa`,
`starter/hello`, `./deploy`. The folder name is the first id segment —
nothing "registers" it.

## The Six-Key Header Grammar (§4.3)

YAML-in-comment, closed grammar, parsed by kadou's own bespoke parser (not
a general YAML library — `08-shape-review.md` §3.1). All six keys are
lowercase, any order; unknown keys are a `kadou check` error.

| Key | Required | Value | Notes |
|---|---|---|---|
| `about` | yes | one line, 1–120 chars | the description everywhere: list, `kadou show`, MCP |
| `risk` | yes | `low` `medium` `high` `critical` | four words, no scores |
| `needs` | no | space-separated vault names; `name=default` allowed | secrets and vault-backed values only — never settable by an agent |
| `args` | no | block of arg lines, two-space indent | what an agent *may* set via `run_kata` |
| `alias` | no | space-separated short names | unique across all folders |
| `timeout` | no | `30s` `10m` `2h`, never above 24h | overrides `[exec] timeout` for this kata |

Not keys: `name`, `id`, `version`, `script`, `type`, `format_version`,
`scope`, `secret` — none of these exist in kadou's grammar.

**Placement:** optional shebang on line 1; opening `# ---` within the
first 3 lines; every header line starts with `#`; closing `# ---`; max 64
header lines; no tabs, no CRLF.

## Arg Line Grammar

```
<name>: <type>[ <options>][ = <default>][  # <help>]
```

Four types only: `text`, `int`, `bool`, `select`. `name` matches
`^[a-z][a-z0-9_]*$` and must not be a reserved shell name (`path`, `home`,
`pwd`, `shell`, or anything that would become `LD_*`/`DYLD_*`/`KADOU_*`
uppercased). No default means required — that is the whole rule.

## Needs vs Args — the One Rule That Matters

**A secret is never an arg; it is a need.** There is no `secret: true`
flag on an arg, and there never will be — the split is structural (§4.4,
safety invariant 3 in `CLAUDE.md`). If the value must come from the vault,
put it in `needs:`, never in `args:`.

```
# needs: jenkins_url=https://ci.example.com jenkins_user jenkins_token
```

`name=default` gives a non-secret fallback; a bare name with no default is
resolved from the vault or is `missing_needs` at run time.

## Template

```sh
#!/bin/sh
# ---
# about: One-line description of what this kata does
# risk:  low
# needs: jenkins_url=https://ci.example.com jenkins_token
# args:
#   branch: text = main   # branch to deploy
# ---
# Optional notes paragraph shown by `kadou show`, not required.
set -eu

BRANCH="${BRANCH:-main}"
JENKINS_TOKEN="${JENKINS_TOKEN:?jenkins_token is required}"

main() {
  echo "Deploying ${BRANCH}..."
  # TODO: implement
  echo "done"
}

main "$@"
```

Follow `shell-scripts` for the body: POSIX `sh`, `set -eu`, quote
everything, `main()` at the bottom.

## Token budget

A kata must cost fewer tokens than the one-off commands it replaces.
This is the same gate as `session-mining` — run that skill's **Token
budget** procedure (isolated `KADOU_HOME`, `kadou --plain run` vs the
concatenated one-off stdout+stderr, UTF-8 bytes / 4). Include the kadou
frame / MCP envelope. Report counts only.

**PASS only when kata bytes < one-off bytes.** A thin wrapper around
one `kubectl`/`git`/`helm` invocation fails (the run frame is ~80–90
tokens on top of the same stdout). Collapse steps, filter, and cap
until it passes. Do not land the file, `propose_kata`, or call the
work done on FAIL.

## Workflow

1. Pick the folder and name: `<folder>/<name>` (or `./name` for a
   project-local kata not yet meant for the shared library)
2. Write `<folder>/<name>.sh` (single-file) from the template above, or
   `<folder>/<name>/kata.sh` plus siblings for a multi-file kata (reach
   them via `$KADOU_ROOT`, never a second `script:` field)
3. `chmod +x` the file
4. `kadou check <folder>` — must show `0 errors` before anything else
5. `kadou show <folder>/<name>` to confirm `about`, `risk`, and the args
   schema look right
6. `kadou run <folder>/<name> --dry-run` to confirm the env var names
   match what the script reads
7. Token budget: measure against the one-off sequence this kata
   replaces (`session-mining` Token budget). Must print `PASS`.
8. If proposing from an agent session instead of writing the file
   directly, use the `propose_kata` MCP tool (`id`, `source`) — it never
   registers or runs the kata; a human runs `kadou accept <id>` to
   promote it into a real folder (§6.7)
