---
name: shell-scripts
description: "POSIX-compatible shell scripting guide for kata bodies, based on Google Shell Style Guide. Use when writing, editing, or reviewing kata script bodies (.sh files, kata.sh in multi-file kata), the install one-liner, or any other shell script in this repo. Triggers on #!/bin/sh, #!/bin/bash, .sh file creation, or shell scripting questions."
user-invocable: false
---

# Shell Script Style Guide for kadou kata

Unchanged from dops: kata bodies are POSIX shell, based on
https://google.github.io/styleguide/shellguide.html. kadou execs the kata
file directly — the shebang is the runtime (`docs/design/05-prd.md` §6.1,
§7 D2) — so the script itself still needs to be portable POSIX, exactly as
it was for dops's `script.sh`.

## Shell Selection

**Default to POSIX sh** (`#!/bin/sh`) for cross-platform kata (Linux +
macOS; `/bin/sh` is dash on Linux, bash-in-POSIX-mode on macOS — test kata
against both where CI runs both, per §3.2). All five starter kata are
`#!/bin/sh` and stay that way (§7.7).

Only use bash when a kata genuinely needs a bash-only feature, and say why:
```sh
#!/bin/bash
# Requires bash for: associative arrays, process substitution
```

| Shell | Shebang | Use when |
|---|---|---|
| POSIX sh | `#!/bin/sh` | Default for every kata |
| Bash | `#!/bin/bash` | Arrays, `[[ ]]`, `${var//pat/rep}`, process substitution |

`kadou check` warns when the declared interpreter isn't on `PATH` — that's
a load-time signal, not a substitute for choosing POSIX by default.

## Error Handling

```sh
#!/bin/sh
set -eu
```

**Do NOT use `set -o pipefail`** — not POSIX. Switch to bash and document
why if a kata truly needs it.

## Variables, Quoting

- Kata read parameters as `UPPER_SNAKE` env vars, set by kadou from `args`/
  `needs` (§4.3/§4.4) — read them with a default or required-check:
  ```sh
  BRANCH="${BRANCH:-dev}"
  JENKINS_TOKEN="${JENKINS_TOKEN:?jenkins_token is required}"
  ```
- Always quote: `"${var}"`, not `$var`
- Lowercase for local script variables, uppercase only for the kadou-
  provided env vars (`KADOU_ID`, `KADOU_FILE`, `KADOU_DIR`, `KADOU_ROOT`,
  and the kata's own declared args/needs)

## Shared Helpers via `$KADOU_ROOT`

A single-file kata reaches a sibling helper through `$KADOU_ROOT`, not a
second `script:` field (§4.6):
```sh
TRIGGER="${KADOU_ROOT}/scripts/trigger-pipeline.sh"
```
`KADOU_ROOT` is the top-level folder under `kata/`, stable regardless of
how deeply the kata itself is nested — this is what `kadou import` rewrites
the old dops `REPO_ROOT`/`TRIGGER` idiom into.

## Conditionals, Loops, Output — Unchanged POSIX Rules

- `[ ]`, not `[[ ]]`; `$(command)`, not backticks; `command -v`, not
  `which`; `printf`, not `echo -e`
- Errors to stderr: `echo "error: ..." >&2`
- Put `main()` at the bottom, call it at the end: `main "$@"`

```sh
if [ -f "${path}" ]; then echo "file exists"; fi
if [ "${status}" -eq 0 ]; then echo "success"; fi
for arg in "$@"; do echo "Processing: ${arg}"; done
```

## Temporary Files

```sh
TMPFILE=$(mktemp)
trap 'rm -f "${TMPFILE}"' EXIT
```

## Things to Avoid in a Kata Body

| Avoid | Use instead | Why |
|---|---|---|
| `#!/bin/bash` (default) | `#!/bin/sh` | POSIX portability across the interpreters kadou execs |
| `[[ ]]` | `[ ]` | Not POSIX |
| `echo -e` | `printf` | Not portable |
| `which` | `command -v` | Not POSIX guaranteed |
| unquoted `$var` | `"${var}"` | Word splitting, globbing |
| `` `cmd` `` backticks | `$(cmd)` | Nesting, readability |
| `set -o pipefail` | explicit per-pipe check | Not POSIX |
| reading `$PATH`/`$HOME` as an arg name | rename the arg (e.g. `target_dir`) | header rule rejects reserved names that clobber shell env (§4.3) |

## The Install Script

The `curl \| sh` installer (§7.5) is also POSIX `#!/bin/sh`, `set -eu`,
with checksum verification — same rules as above, no exception for being
"just an installer."
