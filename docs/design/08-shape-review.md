# kadou shape review: single-file kata, folders as namespaces, no registry

**Date:** 2026-09-11
**Reviewed:** the planner's "omakase shape" proposal (`~/Documents/Sessions/dops-rust-rewrite/briefs/08-shape-proposal.md`) against `05-prd.md` (revised after `07-review.md`), `03-principles.md`, and `01-audit.md` §7 (Sesami workload)
**Role:** creative design review before the shape is written into the PRD. Opinionated. Defaults are picked, not offered.
**Reference workload read-only:** `~/Bitbucket/sdo-dops-catalog` at its current checkout (32 `src/*/runbook.yaml`, 1 793 YAML lines total; 29 wrappers share one `scripts/trigger-pipeline.sh`). No secrets, no internal hostnames appear here; Jenkins examples use `ci.example.com`.

Mason's words that steer this review: "This seems a bit confusing to people." "I want it to be beautiful." "I like the simplicity of a single file." "Is kata a good name, alternatives?"

---

## 1. Verdict

**Adopt the shape.** One unit, one file, one folder tree, no registry. It removes the four things people found confusing (runbook vs catalog, v1 vs v2, `catalog.yaml` groups, `catalog add` vs `install`) and replaces them with things people already know: a script, a folder, `git clone`.

**Single-file kata: yes, and it is the best idea in the proposal.** A kata being a file, not a directory, does more than look simple:

- `e` in the TUI opens the kata in `$EDITOR`. There is nothing else to open.
- `describe_kata` returns the file. The header is the schema, so the first 300 bytes an agent reads *are* the contract.
- The grant pin (`07` B4) is the file's sha256. No "which files count" question.
- `propose_kata` takes one string. No `yaml` + `script` pair that can disagree (`07` B7 spent a paragraph on that).
- `kadou new` writes one file and opens it. `kadou check` points at a line in it.
- A folder of kata looks like a folder of scripts, because it is one. `ls` is the catalog listing.

Five amendments the PRD must carry when it takes the shape:

1. **The header is not YAML.** It looks like YAML and that is the point, but it is a closed grammar with six keys, parsed by a ~200-line parser that gives cargo-style errors. Calling it YAML invites anchors, multi-line strings, and `serde` error messages. Section 3 specifies it.
2. **`needs:` replaces scopes and closes `07` B2 structurally.** Args are what an agent may set. Needs are what only the vault may supply. There is no `agent_settable` flag and no scope lock to test, because the two sets never overlap.
3. **Project-local `./kata/` needs a trust gate**, or a cloned repo can ship a `risk: low` kata that `needs: jenkins_token` and an agent will run it. Section 4 gives the direnv-style rule.
4. **The shebang is the runtime.** No shebang means `/bin/sh`. A `#` header works in sh, bash, python, ruby, perl unchanged; refusing them would be a check for its own sake. This widens `03` §1 rule 5 and is open question 2.
5. **`kadou import` rewrites the one idiom Sesami depends on.** All 29 wrappers contain the identical two lines `REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"` and `TRIGGER="${REPO_ROOT}/scripts/trigger-pipeline.sh"`. Single files sit one level shallower than `src/<name>/script.sh`, so the importer replaces that pair with `$KADOU_ROOT` and prints the diff. Verified against the tree: 29 of 29 identical.

What the shape retires from `05-prd.md` and what it keeps:

| Retired | Replaced by |
|---|---|
| `runbook.yaml` + `script.sh` directory | one `<name>.sh` with a header; `<name>/kata.sh` when helpers are needed |
| catalog registry in config, `catalog add/install/update/remove` | folders under `~/.config/kadou/kata/`; `kadou get <git-url>`, `kadou update` |
| v1 / v2 / `format_version` / v1 compatibility loader | `kadou import <dops-catalog>` once; the header has one version, which is "this one" |
| `catalog.yaml` parameter groups, `uses:` | `needs:` line naming vault values |
| four scopes (`global`, `catalog`, `runbook`, `local`), `agent_settable` | args (settable by anyone) and needs (vault only) |
| nine / six parameter types | four: `text`, `int`, `bool`, `select` |
| `catalog.runbook` ids | `folder/name` ids, paths on disk |
| reserved staging catalogs `mined`, `proposed` as loader roots | drafts under state dir, addressed as `proposed/<folder>/<name>` and `mined/<name>` |
| `propose_runbook(catalog, name, description, risk_level, yaml, script)` | `propose_kata(id, source)` |

Unchanged: four MCP tools, agent ceiling `low`, human ceiling `medium`, grants and pending records, age vault with the Go envelope import, stream redaction, history tiers, MCP env allowlist, timeout and cancel, session mining as a crate. The safety model survives intact and gets simpler (section 6).

---

## 2. Omarchy applied

Color notes for the mocks: the product theme `doop` (`~/origin/dops/internal/theme/doop.json`) is Tokyo-Night-derived. Risk colors are low `#9ece6a` green, medium `#e0af68` yellow, high `#ff9e64` orange, critical `#f7768e` red on `#1a1b26`. Primary is `#7aa2f7` blue. Muted text is `#565f89`. In the mocks `●` carries the risk color, `▸` is the selection in primary, and `✓` / `✗` are success / error. Everything else is plain foreground.

### 2.1 Omakase defaults

**Ideal:** the product arrives decided. One tree, one file shape, one id shape, one theme, one runtime rule, one place for secrets. The first question kadou asks a human is "what do you want to run?" and the answer is a keystroke.

Decided, not configurable:

| Decision | Value |
|---|---|
| Where kata live | `~/.config/kadou/kata/<folder>/…` and `./kata/` in a project |
| Unit shape | `<name>.sh` with a `# ---` header; `<name>/kata.sh` for multi-file |
| Id | path under `kata/` without `.sh`: `sesami/cc4-aaa`, `starter/hello`, `./deploy` |
| Env contract | args and needs become `UPPER_SNAKE`; kadou adds `KADOU_ID`, `KADOU_FILE`, `KADOU_DIR`, `KADOU_ROOT` |
| Runtime | the shebang; none means `/bin/sh` |
| Risk words | `low`, `medium`, `high`, `critical` |
| Ceilings | human `medium`, agent `low` |
| Config | `~/.config/kadou/kadou.toml`, may be empty |
| Secrets | vault only, via `kadou vault set <name>` |
| MCP | `list_kata`, `describe_kata`, `run_kata`, `propose_kata`, stdio |

### 2.2 Batteries included

**Ideal:** after install, `kadou` shows five kata you can run, and `kadou new` writes one you can edit. The starter folder is on disk, not only in the binary, because "the operator can read every script the product will run" (`03` charter 12) means being able to `cat` it.

- First run materializes `kata/starter/` from the embed **only if `kata/` does not exist**. If you delete `starter/`, it stays deleted. `kadou get starter` brings it back.
- `kadou new` ships a template with every header key and one example arg.
- `kadou check` ships the linter. `kadou import` ships the converter for old dops catalogs.
- Themes are embedded; `theme = "…"` in `kadou.toml` switches.

### 2.3 One-line install

```
$ curl -fsSL https://<stable-install-url>/install.sh | sh
  kadou 0.1.0 → /usr/local/bin/kadou   sha256 ok
  next:  kadou            open the library
         kadou mcp serve  for agents

$ kadou
```

Two lines of output. The next command is `kadou`, not `kadou init`.

### 2.4 Beautiful by default

**Ideal:** the first frame is a finished product. The header pane shows the *file*, because the file is short enough to be the documentation.

First run, empty config, starter materialized:

```
 kadou 稼働                                                                ? help
┌ kata ───────────────────┐┌ starter/hello ────────────────────────────────────┐
│ starter                 ││ Print a greeting                                  │
│ ▸ hello         ● low   ││                                                   │
│   disk-usage    ● low   ││ risk   ● low                                      │
│   git-status    ● low   ││ args   name   text  = world     Who to greet      │
│   health        ● low   ││                                                   │
│   list-path     ● low   ││ file   ~/.config/kadou/kata/starter/hello.sh      │
│                         ││                                                   │
│                         ││ #!/bin/sh                                         │
│                         ││ # ---                                             │
│                         ││ # about: Print a greeting                         │
│                         ││ # risk:  low                                      │
│                         ││ # args:                                           │
│                         ││ #   name: text = world   # Who to greet           │
│                         ││ # ---                                             │
│                         ││ echo "hello, $NAME"                               │
└─────────────────────────┘└───────────────────────────────────────────────────┘
 ↵ run   / find   e edit   n new   c check   g get   q quit
   tip  kadou get <git-url>  adds your team's kata as a folder
```

The same frame after `kadou get` of the converted Sesami folder, cursor on the critical kata:

```
 kadou 稼働                                          2 folders · 37 kata   ? help
┌ kata ───────────────────┐┌ sesami/ses-deploy ────────────────────────────────┐
│ starter                 ││ Trigger the SES Deploy pipeline                   │
│   hello         ● low   ││ Deploys SES modules to a target OKE cluster.      │
│   …                     ││                                                   │
│ sesami          git ✓   ││ risk   ● critical   confirm by typing the id      │
│   cc4-aaa       ● med   ││ needs  jenkins_url  jenkins_user  jenkins_token   │
│   cc4-admin     ● med   ││        ✓ vault                                    │
│   …                     ││ args   version    text                  required  │
│   device-log-m… ● low   ││        oke_cluster text                  required  │
│   ses-argocd-s… ● med   ││        namespace  text  = ""                      │
│ ▸ ses-deploy    ● crit  ││        modules    text  = "aaa,admin,…"           │
│   ses-release…  ● high  ││                                                   │
│                         ││ file   kata/sesami/ses-deploy.sh   sha 4b1c…      │
│                         ││ last   2026-09-10 14:02  ✓ 1m12s  mason (cli)     │
└─────────────────────────┘└───────────────────────────────────────────────────┘
 ↵ run   / find   e edit   n new   c check   g pull   p pending 1   q quit
```

Rules the frame obeys:

- Risk is a colored dot plus a word, never only a color.
- The right pane is *file first*: about, risk, needs, args, file, last run, then the source below the fold (`j`/`k` scroll it).
- `p pending 1` appears in the footer only when a grant is waiting. Badges that are always present are noise.
- The Japanese mark `稼働` is the only decoration. It is the name, not a logo.

CLI output shares the palette. Success is one line. Errors are a sentence and a fix.

### 2.5 Keyboard-first

Every human flow has a key, and the single-file shape earns three new ones:

| Key | Action |
|---|---|
| `Enter` | run (wizard if args are needed) |
| `/` | find across id, about, alias |
| `e` | edit the selected kata in `$EDITOR`, re-check on return |
| `n` | new kata in the selected folder |
| `c` | check the selected folder, show diagnostics in the output pane |
| `g` | `git pull` the selected folder (only offered when it has `.git`) |
| `p` | pending grants |
| `y` | yank the id to the clipboard |
| `Ctrl+p` | palette: theme, trust this project, vault, history, help, quit |
| `?` | help overlay |

`e` and `n` are the omakase moves: editing a kata is editing a file, so the TUI should drop you into your editor instead of growing a form builder.

### 2.6 Dotfile-native config

```
~/.config/kadou/
  kadou.toml              # may be empty; missing keys mean defaults
  themes/                 # drop-in *.toml themes
  kata/
    starter/hello.sh      # written on first run; yours after that
    sesami/               # `kadou get <git-url> --as sesami`; a git checkout
      cc4-aaa.sh
      ses-deploy.sh
      device-log-metrics/
        kata.sh
        lib/  tests/  envs/
      scripts/trigger-pipeline.sh
~/.local/share/kadou/     # vault, keys
~/.local/state/kadou/     # history, pending, proposed, mined, last-used args
./kata/                   # project-local; found from cwd
```

`kadou.toml`, complete:

```toml
# Missing keys mean defaults. This file may be empty.
theme    = "doop"
max_risk = "medium"           # human ceiling for every folder

[agent]
max_risk = "low"
allow    = []                 # ids the agent may run above low, e.g. "sesami/ses-deploy"

[folder.sesami]               # optional per-folder policy; nothing to "register"
max_risk = "critical"

[trust]
paths = []                    # project-local kata folders that may use the vault and be seen by agents

[exec]
timeout  = "30m"
pass_env = []

[mcp]
max_output_lines = 50
max_wait = "50s"

[vault]
keyring = false
```

No `[[catalogs]]`. A folder exists because it is on disk. Policy is optional and keyed by folder name.

### 2.7 Convention over configuration

**`kadou new`:**

```
$ kadou new sesami/argocd-sync
  wrote    ~/.config/kadou/kata/sesami/argocd-sync.sh
  opening  nvim
```

The file it wrote:

```sh
#!/bin/sh
# ---
# about: <what this does, to what>
# risk:  medium                  # low | medium | high | critical
# needs:                         # vault names, e.g. jenkins_url jenkins_token
# args:
#   example: text = hello        # delete or replace
# ---
set -eu

echo "argocd-sync: $EXAMPLE"
```

When the editor exits, `kadou new` runs the check and prints the one-line result. Leaving `<what this does…>` in place is an error, not a warning.

**`kadou run`:**

```
$ kadou run sesami/cc4-aaa branch=release/26.4.1.0
 ▶ sesami/cc4-aaa  ● medium   Trigger a SES/CC4/cc4-aaa branch pipeline
   branch=release/26.4.1.0  version=""  send_email=true  publish_image=true
   publish_api=true  allow_image_override=false
   needs  jenkins_url jenkins_user jenkins_token   ✓ vault
 ───────────────────────────────────────────────────────────────────────
 ==> Stage 1/3: Validate
 Pipeline             : SES/CC4/cc4-aaa
 Branch               : release/26.4.1.0
 …
 ✓ Done
 ───────────────────────────────────────────────────────────────────────
 ✓ sesami/cc4-aaa  exit 0  2m41s   kadou history 8f2c
```

Args are positional `key=value`. `--param` is gone. `--dry-run` prints the header block and the env names and stops. `--confirm <id>` is unchanged for high and critical.

**`kadou check`:**

```
$ kadou check
error: unknown arg type `string`
  --> kata/sesami/ses-argocd-sync.sh:8:14
   |
 8 | #   app_name: string                  # ArgoCD application name
   |               ^^^^^^ use `text`
   = arg types are text, int, bool, select

error: `about` is required
  --> kata/sesami/argocd-sync.sh:2
   = add a line like:  # about: Force-refresh an ArgoCD application

warning: `jenkins_url` is needed by 29 kata and is not in the vault
   = kadou vault set jenkins_url --plain

checked 37 kata in 2 folders   2 errors  1 warning
```

Cargo's diagnostic shape, because operators already read it. Every error names a line and ends with a fix line.

**An error at run time:**

```
$ kadou run sesami/ses-deploy version=25.6.1.2 oke_cluster=uat
error: sesami/ses-deploy is critical and your ceiling for sesami is medium
  = raise it in ~/.config/kadou/kadou.toml
        [folder.sesami]
        max_risk = "critical"
  = then:  kadou run sesami/ses-deploy version=25.6.1.2 oke_cluster=uat --confirm sesami/ses-deploy
```

```
$ kadou run sesami/cc4-aaa
error: sesami/cc4-aaa needs `jenkins_token`, which is not in the vault
  = kadou vault set jenkins_token
```

### 2.8 What an agent sees

`tools/list` names: `list_kata`, `describe_kata`, `run_kata`, `propose_kata`. Descriptions:

- `list_kata`: `Search kata (reviewed scripts) visible to this agent. Returns id, about, risk. No schemas.`
- `describe_kata`: `One kata: args schema, needs, risk, source. Read it before run_kata.`
- `run_kata`: `Run one kata with args. Secrets come from the vault as needs; never pass them. Above your grant it returns pending_grant.`
- `propose_kata`: `Draft a kata (one file with a header) for human review. Never registers or runs it.`

`list_kata` result (compact on the wire; spaced here):

```json
{"kata":[
  {"id":"starter/hello","about":"Print a greeting","risk":"low"},
  {"id":"sesami/device-log-metrics","about":"Collect ES write volume metrics for SES device logs","risk":"low"},
  {"id":"sesami/helm-package","about":"Trigger the helm-package pipeline","risk":"low"}
 ],"total":8,"offset":0,"limit":50}
```

`describe_kata` result:

```json
{"id":"sesami/cc4-aaa","folder":"sesami","about":"Trigger a SES/CC4/cc4-aaa branch pipeline","risk":"medium",
 "file":"/Users/mason/.config/kadou/kata/sesami/cc4-aaa.sh","sha256":"sha256:4b1c…",
 "needs":["jenkins_url","jenkins_user","jenkins_token"],"needs_missing":[],
 "args":{"type":"object","additionalProperties":false,"properties":{
   "branch":{"type":"string","default":"dev","description":"Branch, tag, or PR to trigger"},
   "version":{"type":"string","default":"","description":"Image tag; blank falls back to branch"},
   "send_email":{"type":"boolean","default":true},
   "publish_image":{"type":"boolean","default":true},
   "publish_api":{"type":"boolean","default":true},
   "allow_image_override":{"type":"boolean","default":false}},"required":[]},
 "source":"#!/bin/sh\n# ---\n# about: Trigger a SES/CC4/cc4-aaa branch pipeline\n# risk:  medium\n# needs: jenkins_url=https://ci.example.com jenkins_user jenkins_token\n# args:\n#   branch: text = dev …",
 "source_truncated":false,
 "files":["scripts/trigger-pipeline.sh"]}
```

`needs` lists names only, never values. `needs_missing` tells the agent the run will fail before it tries, and the fix is a human command. Args carry no secrets by construction, so `secret_param_names` and `resolved` from the PRD are gone.

`run_kata` result is the PRD's shape with `runbook_id` → `id`: `status`, `id`, `exit_code`, `duration_ms`, `output_lines`, `output`, `truncated`, `summary`, `log_path`, `history_id`. `pending_grant`, `dry_run`, `invalid_args`, `running`, and `draft` (was `staging`) keep their meaning.

`propose_kata` input and result:

```json
{"id":"sesami/argocd-sync","source":"#!/bin/sh\n# ---\n# about: Force-refresh an ArgoCD application\n# risk:  medium\n# needs: kubeconfig_context\n# args:\n#   app: text\n# ---\nset -eu\n…"}
```

```json
{"status":"proposed","id":"proposed/sesami/argocd-sync",
 "path":"/Users/mason/.local/state/kadou/proposed/sesami/argocd-sync.sh",
 "diff":"--- /dev/null\n+++ sesami/argocd-sync.sh\n…",
 "accept":"kadou accept sesami/argocd-sync"}
```

The server parses the header from `source` before writing; a bad header is `invalid_args` with the same diagnostic text `kadou check` prints. One string in, one file out.

---

## 3. Header spec

### 3.1 Grammar choice

**YAML-in-comment, as proposed, with one correction: it is a closed grammar that kadou parses itself.** Alternatives considered and rejected:

- Front matter above the shebang (`---` on line 1): breaks the shebang, so `sh file.sh` still works but `./file.sh` does not. Rejected.
- Real YAML extracted from the comment and fed to `serde-yaml-ng`: gives YAML's error messages, invites anchors and block scalars, and still needs a second parser for the arg one-liner. Rejected.
- Shell-native (`# @arg env select dev|uat`, like argbash/docopt): reads worse and is not indentable into a block. Rejected.
- TOML in comment: `[args]` tables are noisier than the two-space list. Rejected.

The header is the run of `#` lines between two `# ---` lines. It looks like YAML so that eyes and models parse it for free; it is small enough that a purpose-built parser is safer than a general one.

### 3.2 Placement and framing

- Optional shebang on line 1.
- The opening `# ---` must appear within the first 3 lines (allows a shebang plus one `# shellcheck` line).
- Every line until the closing `# ---` starts with `#`. `#` alone is a blank line. A line that does not start with `#` before the close is an error ("header not closed").
- Max 64 header lines. Tabs are an error with a fix. CRLF is an error with a fix. UTF-8 only.
- The header is stripped by nothing; the interpreter sees comments.
- The first comment paragraph *after* the closing `# ---` is `notes`: shown by `kadou show` and the TUI as the long description. Not required.

### 3.3 Keys

Six keys, all lowercase, in any order. Unknown keys are errors.

| Key | Required | Value | Notes |
|---|---|---|---|
| `about` | yes | one line, 1–120 chars | the description everywhere: list, TUI, MCP |
| `risk` | yes | `low` `medium` `high` `critical` | four words, no scores |
| `needs` | no | space-separated vault names; `name=default` allowed | see 3.6 |
| `args` | no | block of arg lines, two-space indent | see 3.4 |
| `alias` | no | space-separated short names | unique across all folders, checked |
| `timeout` | no | `30s` `10m` `2h` | overrides `[exec] timeout` for this kata; never above 24h |

Not keys: `name` (the filename), `id` (derived), `version` (git), `script` (the file), `type`, `format_version`, `scope`, `secret`.

### 3.4 Arg lines

```
<name>: <type>[ <options>][ = <default>][  # <help>]
```

| Part | Rule |
|---|---|
| `name` | `^[a-z][a-z0-9_]*$`; env is its uppercase; rejects `path`, `home`, `pwd`, `ifs`, `shell`, `oldpwd`, `cdpath`, `bash_env`, `ps4`, anything becoming `LD_*`, `DYLD_*`, `KADOU_*` (from `05` §4.4, unchanged) |
| `type` | `text` `int` `bool` `select` |
| `options` | only for `select`: `a\|b\|c`, words matching `^[A-Za-z0-9_.:/-]+$` |
| `default` | bare token, `true`/`false`, integer, or `"quoted"`; `= ""` means optional and empty |
| required | no default → required. That is the whole rule. |
| `help` | free text to end of line after two spaces and `#` |

Examples, every form:

```
#   version: text                              # Image tag to deploy
#   branch:  text = dev                        # Branch, tag, or PR
#   note:    text = ""                         # optional, empty when unset
#   count:   int = 3
#   force:   bool = false
#   env:     select dev|uat|prod = uat         # Target environment
#   mode:    select report|compare             # required select
```

Env serialization is unchanged from `05` §6.1: text as-is, int as decimal, bool `true`/`false`, select the chosen option. Anything else in MCP `args` is `invalid_args`.

**Why four types.** Sesami uses `string` 142, `boolean` 86, `select` 5, `number` 1. `float`, `file_path`, `resource_id`, `multi_select` are unused in every real catalog on disk. A path is text with help. A float in an env var is text. A multi-select is a `text` with a comma convention until someone needs the checklist, and then it is a `select` modifier (`select+`) added in a later revision, not now.

**Why no `secret` on args.** An arg is settable by an agent through `run_kata`. A secret must never be. Therefore a secret is never an arg; it is a need. This is the whole of `07` B2 expressed as a grammar.

### 3.5 Short form for the common case

Most kata have no args, or one. The block form is fine at one arg; there is no inline form. One way.

### 3.6 `needs`

```
# needs: jenkins_url=https://ci.example.com jenkins_user jenkins_token
```

- Each token is a vault name matching the arg name rule; env is the uppercase.
- `name=default` gives a plain (non-secret) fallback used when the vault has no entry. A default in a git-tracked file is by definition not a secret, so a need with a default is always plain.
- Resolution: vault entry, else default, else **missing**. Missing needs: CLI and TUI prompt once and save (`kadou vault set` inline); MCP returns `error: missing_needs` with the names and the human command. Env of the agent host is never consulted.
- Each vault entry carries a secret bit. `kadou vault set <name>` is secret by default; `--plain` for URLs and usernames. Secret entries are redacted from output (`05` §6.6 unchanged); plain entries are not, so `jenkins_url` printing in "Server: …" does not shred the log.
- Needs are one flat namespace across folders. Two folders that both want `token` collide on purpose: prefix (`jenkins_token`, `argocd_token`). `kadou check` errors when two folders declare the same need with different defaults.
- `describe_kata` returns names and `needs_missing`. Never values.

### 3.7 Multi-file detection

A kata is either:

1. `<folder>/<name>.sh` (or any extension; the header decides), or
2. `<folder>/<name>/kata.sh` plus anything else in that directory.

Detection is by presence of `kata.sh`; nothing declares it. Rules:

- A directory containing `kata.sh` is a leaf. Nothing inside it is scanned further; helpers, `lib/`, `tests/`, `envs/` are opaque.
- A directory without `kata.sh` is a namespace and is scanned.
- A file with a `# ---` header is a kata. A file without one is a helper and is ignored by `list`. `kadou check -v` names ignored files so a forgotten header is findable.
- A file whose first comment block *opens* `# ---` and fails to parse is an error, not a helper.
- Names starting with `_` or `.` are never scanned. `README.md`, `SPEC.md`, `.git`, `.gitignore` sit in folders untouched.
- `cwd` for a run is the kata's directory (folder form) or the folder containing the file. `KADOU_DIR` equals cwd. `KADOU_ROOT` is the top-level folder under `kata/` (the git checkout root for a `kadou get` folder). `KADOU_FILE` is the kata's absolute path. `KADOU_ID` is the id.

### 3.8 Readable to humans, cheap for agents

- A typical header is 6–12 lines, 150–400 bytes. `describe_kata` returns it inside `source`, so an agent pays for the schema once, as prose it can read, and once as `args` JSON Schema it can validate against. The JSON Schema is generated from the header, so the two cannot drift.
- `about` is the only free-text field on `list_kata`. Lists stay at roughly 60 bytes per kata.
- The header uses no quoting in the common case. A model writing a kata for `propose_kata` writes shell it already knows plus six keys it saw in `describe_kata`. There is no schema to fetch.
- `kadou check` diagnostics are the same text the MCP `invalid_args` carries, so an agent that proposes a bad header gets the fix line a human would.

### 3.9 The converted `cc4-aaa`

Today's `runbook.yaml` is 62 lines. After `kadou import`:

```sh
#!/bin/sh
# ---
# about: Trigger a SES/CC4/cc4-aaa branch pipeline (CC4 AAA service build)
# risk:  medium
# needs: jenkins_url=https://ci.example.com jenkins_user jenkins_token
# args:
#   branch:               text = dev     # Branch, release tag, or PR to trigger
#   version:              text = ""      # Custom image tag; falls back to branch
#   send_email:           bool = true    # Send email on build completion
#   publish_image:        bool = true    # Publish Docker image to the registry
#   publish_api:          bool = true    # Publish API to the Maven repo
#   allow_image_override: bool = false   # Override an existing image tag
# ---
set -eu

JENKINS_URL="${JENKINS_URL:?jenkins_url is required}"
…
TRIGGER="${KADOU_ROOT}/scripts/trigger-pipeline.sh"      # was: REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
…
```

Thirteen header lines carry what 62 YAML lines carried. Across the 32 runbooks, 1 793 YAML lines become roughly 330 header lines and 32 fewer files.

---

## 4. Namespace spec

### 4.1 Folders and ids

- `~/.config/kadou/kata/` is the library. Every immediate subdirectory is a **folder**. The folder name is the first id segment.
- Ids are the path under `kata/` without the extension: `sesami/cc4-aaa`, `sesami/device-log-metrics`, `starter/hello`. Nesting is allowed to any depth: `sesami/cc4/aaa` is legal, and a team may reorganize the 20 `cc4-*` kata into `cc4/` without kadou caring. The TUI groups by top folder and shows the rest of the path as the name.
- Id segments match `^[a-z0-9][a-z0-9-]*$`. Uppercase or underscore in a filename is a check error with a rename suggestion.
- `/` is the separator, not `.`, because ids are paths and `kadou run sesami/deploy` reads as one. Aliases have no slash.
- Reserved top-level names: `starter`, `proposed`, `mined`. `kadou get --as mined` is refused.

### 4.2 Project-local discovery and trust

- From cwd, kadou walks up to the nearest `kata/` directory, stopping at the git root or `$HOME`. Kata found there have ids prefixed `./`: `./deploy`, `./db/migrate`. The `./` prefix can never collide with a library id, and it tells the reader "relative to here".
- A **human** can list, show, and run project-local kata immediately. Running one that has `needs:` is refused with "untrusted folder cannot use the vault; run `kadou trust`".
- An **agent** does not see project-local kata at all until the folder is trusted. The threat is a cloned repository shipping `kata/tidy.sh` with `risk: low` and `needs: jenkins_token`.
- `kadou trust` (in the project) appends the folder's absolute path to `[trust] paths` in `kadou.toml`. `kadou trust --forget` removes it. This is direnv's `allow` model keyed by path, not by content hash, because a folder of scripts changes constantly and re-trusting on every commit would train people to type `trust` reflexively.
- MCP servers launched by an agent host inherit the host's cwd, so the same walk-up and the same trust gate apply.

### 4.3 Git install without a registry

```
$ kadou get git@git.example.com:team/ops-kata.git --as sesami
  cloning  → ~/.config/kadou/kata/sesami   (main @ 3f9e1c2)
  checked  32 kata   0 errors  1 warning
  warning: `jenkins_token` is needed by 29 kata and is not in the vault
     = kadou vault set jenkins_token
```

- `kadou get <url> [--as <folder>] [--ref <ref>]` is `git clone` into `kata/<folder>/`. Default folder name is the repo basename with a trailing `-kata`, `-catalog`, or `.git` stripped. The check runs immediately and its warnings are the onboarding checklist.
- `kadou update [<folder>]` is `git pull --ff-only` in every folder that has `.git`, then a check. A folder that fails the check after pull is **still on disk** and its kata stay listed with a `✗` badge; kadou does not roll back, and it does not run a kata whose header fails.
- Removing a folder is `rm -rf`. `kadou remove <folder>` exists for discoverability and asks `y/N`.
- The git remote lives in `.git/config`, not in kadou's config. Nothing is registered. Moving a folder in Finder works.
- Monorepos: `kadou get <url> --root src --as sesami` clones to `~/.local/share/kadou/repos/<folder>/` and symlinks `kata/sesami → repos/sesami/src`. This is the one place a symlink is allowed, and it exists only so an existing repository layout can be adopted without moving files. New repositories put kata at the root and do not need it.

### 4.4 Policy per folder

`[folder.<name>]` in `kadou.toml` is optional and holds exactly two keys: `max_risk` (human ceiling, replaces the global for that folder) and `agent_max_risk` (agent ceiling, may only lower the global `[agent] max_risk`). Absent sections mean defaults. There is no `active` flag: a folder you do not want is a folder you delete or rename with a leading `_`.

The `05` §6.2 formula is unchanged with `folder` substituted for catalog:

```
human_ceiling(f) = folder[f].max_risk ?? max_risk
agent_ceiling(f) = min(agent.max_risk, folder[f].agent_max_risk ?? agent.max_risk, --max-risk, human_ceiling(f))
```

### 4.5 Collisions

| Case | Rule |
|---|---|
| Same kata name in two folders | no collision; ids differ (`a/deploy`, `b/deploy`) |
| Same alias in two kata | check error; the second loses and is listed without the alias until fixed |
| `x.sh` and `x/kata.sh` in one folder | check error; the file wins for running, so the error is loud |
| Project-local id equals a library id | no collision; `./deploy` vs `sesami/deploy` |
| Folder named `starter` cloned by `kadou get` | refused: reserved |
| Need declared with two different defaults | check error naming both files |

---

## 5. Naming

### 5.1 Scores

Each criterion scored 1–5. Clarity is for a DevOps operator hearing the word once, and for an agent reading `run_<x>` in a tool list among fifty others. Collision counts real products and reserved meanings in the same operator's toolchain.

| Name | Clarity (ops) | Clarity (agents) | Pronounce | Collision | Fit with kadou | Reads as `run_<x>` | Total /30 |
|---|---|---|---|---|---|---|---|
| **kata** (型) | 3 | 4 | 5 | 3 (Kata Containers, "code kata") | 5 | 4 `run_kata` | **24** |
| runbook | 5 | 4 | 4 | 2 (PagerDuty-style docs; means a document in many orgs) | 2 | 2 `run_runbook` | 19 |
| op | 4 | 4 | 5 | 1 (1Password `op` CLI in Mason's own shell; "op" everywhere) | 5 | 5 `run_op` | 24, disqualified |
| waza (技) | 2 | 3 | 4 | 5 | 4 | 3 `run_waza` | 21 |
| play | 3 | 4 | 5 | 2 (Ansible play, playbook — same domain) | 2 | 4 `run_play` | 20 |
| recipe | 3 | 4 | 4 | 3 (Chef) | 2 | 3 `run_recipe` | 19 |
| task | 3 | 3 | 5 | 1 (Taskfile, Jira, and MCP's own `tasks` primitive) | 2 | 3 `run_task` | 17 |
| script | 3 | 2 | 5 | 1 (an agent reads `run_script` as "run arbitrary script text", the exact tool the charter forbids) | 2 | 1 `run_script` | 14 |

### 5.2 Recommendation

**kata.** Singular and plural. One sentence of gloss, used once in `kadou --help` and once in the README: *a kata is a reviewed script with a header that kadou can list, describe, and run.*

Why it wins:

- It is the right metaphor. A kata is a form drilled until it is executed exactly, which is what a production runbook is supposed to be and what "runbook" (a document) no longer says.
- It pairs with kadou (稼働). Two Japanese words, both starting with *ka*, one product. A mixed vocabulary (`kadou` + `runbook`) is what made the current draft read as two products glued together.
- `run_kata`, `list_kata`, `describe_kata`, `propose_kata` are short, unambiguous, and cheaper on the wire than `_runbook` (the four names save 16 bytes on every connect).
- Collision is survivable. Kata Containers is a container runtime with binaries named `kata-runtime` and `kata-agent`; kadou never ships a binary named `kata`, and the two do not meet on a command line. "Code kata" is a practice exercise; the martial-arts sense is older and is the one `kadou` invokes.

Why not runbook: it is the safest word and the worst tool name. `run_runbook` is a stutter an agent has to read thousands of times, and the word already means "a wiki page with steps" to half the operators who will hear it. If Mason wants zero explanation ever, runbook is the fallback, and the price is the stutter and the mixed vocabulary.

**The folder does not need a name.** Call it a folder in every human-facing string. "Namespace" appears only in this spec and the PRD when describing ids. The word "catalog" is retired with the registry. "Library" is the collective noun for everything under `kata/` and is used in exactly one place: the TUI title tooltip and `kadou --help`.

Vocabulary the product uses, complete: kadou, kata, folder, header, args, needs, vault, risk, grant, draft, history.

---

## 6. Stress test

### 6.1 The Sesami workload

| Item | Today | Under the shape | Breaks? |
|---|---|---|---|
| 32 runbook dirs under `src/` | `src/<name>/{runbook.yaml,script.sh}` | 29 single files `sesami/<name>.sh`; 3 stay folder form: `device-log-metrics/` (lib, tests, envs, two Python files, `collect.sh`), `clone-ses-repos/` and `ses-argocd-sync/` only if they carry helpers (they do not; they become files) | no |
| 29 wrappers, identical `../..` root idiom | works because `script.sh` is two levels below the repo | **breaks** at one level. `kadou import` replaces the verified-identical two lines with `TRIGGER="${KADOU_ROOT}/scripts/trigger-pipeline.sh"` and shows the diff. `KADOU_ROOT` is the folder root regardless of nesting, so a later move into `cc4/` does not break it again | fixed by importer |
| `scripts/trigger-pipeline.sh` (391 lines, bash, arrays) | reached by path | sits at `sesami/scripts/trigger-pipeline.sh`, no header, ignored by `list`, listed by `describe_kata.files` for the kata that reference it. Executed by the wrapper via its own `#!/usr/bin/env bash` shebang, unchanged | no |
| three global params, one secret | 87 declarations across 29 files | one `needs:` line × 29; `jenkins_url` default carried as `jenkins_url=https://…`; `jenkins_token` is a secret vault entry; `jenkins_user` plain | no |
| `JENKINS_PASS="${JENKINS_TOKEN}"` | env rename inside the wrapper | unchanged; needs become `JENKINS_TOKEN` exactly as before | no |
| `device-log-metrics` with `lib/`, `tests/`, `envs/*.env` | folder | `sesami/device-log-metrics/kata.sh` (renamed from `script.sh`; `SCRIPT_DIR` idiom still works because cwd and `$0` are the folder); `tests/` and `lib/` opaque | no |
| `ses-argocd-sync`: `type: number` timeout, `app_name` required with `default: ""`, `k8s_context` select | v1 coercion rules | `timeout: int = 60`; `app_name: text` (required by having no default, which is what "required with empty default" meant); `k8s_context: select dev\|qa\|uat\|prep\|prod = dev` | no |
| CC4 booleans as strings `"true"` | coercion at load | `bool = true`; the importer coerces once, at conversion, and errors on anything uncoercible | no |
| Risk mix: medium 25, low 5, high 1, critical 1 | ceilings per catalog | ceilings per folder; at defaults an agent sees the same five low kata, a human sees 30, `[folder.sesami] max_risk = "critical"` reveals `ses-deploy` | no |
| Runbook-scope saved values (last branch used) | vault `catalog.*.runbooks.*` | **changes**: kadou remembers last-used args per kata in `~/.local/state/kadou/last/`, plain, non-secret, TUI-prefilled, never read by CLI or MCP. Scopes are gone; what the file says is what runs | behavior change, documented |
| `version: 1.0.0` on every runbook | opaque string | dropped; git is the version | no |
| `SPEC.md` "370 pipelines" future | linear file growth | same folder, 370 files, or `cc4/`, `ses/` subfolders; `tools/list` is still O(1) | no |
| Catalog hygiene items from `07` suggestion 13 (`.bashrc` eval, `-u user:pass` in dry-run, `curl -k`) | out of scope | still out of scope; stream redaction still masks the token in the dry-run line; the `.bashrc` eval never fires because kadou always supplies `JENKINS_USER`/`JENKINS_PASS` | no |

**Conversion is a one-time, reviewable diff.** `kadou import ~/Bitbucket/sdo-dops-catalog/src --as sesami` writes the new folder, prints per-file diffs, and refuses to overwrite an existing folder. The 32-file compatibility suite in `05` decision 11 becomes: import all 32, check all 32 with zero errors, `describe` all 32, `run --dry-run` all 32 with the same env names as today. The CI fixture set stays sanitized and shape-preserving (`07` B15); it is now 32 stub `.sh` files.

### 6.2 The PRD safety model

| Control (`05`) | Under the shape | Effect |
|---|---|---|
| Four tools, ≤ 2 800 B (§5.1, §5.4) | renamed `*_kata`; `propose_kata` schema shrinks from six properties to two | payload drops by an estimated 250 B; regenerate `tools-list.json` and re-snapshot. Estimate, not measured |
| Visibility formula and ceilings (§6.2) | per folder instead of per catalog; project-local folders are invisible to agents until trusted | one new term (`trust`) in the formula; otherwise identical |
| Confirm protocol (§6.3) | unchanged; ids are `folder/name` | none |
| Grants and pending records (§6.4) | pin is the kata file's sha256 (folder form: `kata.sh` sha256 plus the folder's git HEAD when present); `[agent] allow` lists ids | simpler pin; `kadou grant allow` unchanged |
| Vault (§6.5) | payload becomes flat `{ "<name>": { "value", "secret" } }`; Go import maps `global.*` to entries (secret bit set for keys the Go catalog marked secret, which the importer knows from the YAML), reports and drops `catalog.*` runbook-scope values or writes them as last-used args | envelope bytes unchanged; payload shape is a §6.5 delta |
| Redaction (§6.6) | keyed by secret vault entries actually injected into the run | plain entries (URL, user) no longer risk shredding output; the token still masks in literal, base64, `user:token` base64, and URL-encoded forms |
| Arg scope lock (§4.6, B2) | **structural**: needs are never in `args`; `args: {"jenkins_url": …}` is `invalid_args` because `jenkins_url` is not an arg | one test replaces a scope table; `agent_settable` is deleted |
| MCP env allowlist, timeout, cancel, concurrency (§6.1) | unchanged; `timeout:` header key may lower or raise the per-run bound up to 24h | none |
| MCP never writes the vault (§4.6) | unchanged; `missing_needs` returns names and a human command | none |
| Propose containment (§5.4 B7) | `id` pattern `^[a-z0-9][a-z0-9-]*(/[a-z0-9][a-z0-9-]*)+$` and a canonical path check under the `proposed/` state dir | same guarantee, one regex |
| Staging catalogs `mined` / `proposed` (§4.1, B14) | drafts are files under `~/.local/state/kadou/{proposed,mined}/…`, addressed as `proposed/<folder>/<name>` and `mined/<name>`, listable with `include_drafts`, never runnable; `kadou accept <id>` moves the file into `kata/<folder>/` after a diff and `y/N`; accept refuses a target folder that is a git checkout | same gate, no loader special case for "inactive reserved catalogs" |
| History (§6.6) | `catalog_name` → `folder`; `runbook_id` → `id`; `runbook_name` dropped | field rename |
| Miner (§6.8) | writes single-file drafts with a header instead of `format_version: 2` YAML pairs | the draft writer changes; pipeline and redaction do not |

**What gets harder.** Two things, honestly:

1. **A bespoke parser is a new surface.** Its error messages are its user interface. Budget for a fixture corpus of bad headers and snapshot the diagnostics the way `tools/list` is snapshotted.
2. **Nothing distinguishes "a folder that failed check" from "a folder that is fine" except the check.** With no registry there is no `active = false` to park a broken folder. The TUI badge and the run refusal carry that weight, and `kadou update` must never leave a folder half-pulled.

---

## 7. PRD deltas

Sections of `05-prd.md` by number. "Sketch" is the new text's shape, not final wording.

| § | Change | Sketch |
|---|---|---|
| Title block, §1.1 | vocabulary | "The unit is a **kata**: one script with a header. Kata live in **folders** under `~/.config/kadou/kata/`. There is no registry." CLI row lists `kadou run`, `list`, `show`, `new`, `check`, `get`, `update`, `import`, `accept`, `trust`, `vault`, `history`, `grant`, `mcp`, `mine` |
| §1.2 | users | agents "list, describe, and run kata; may propose one as a single file". Operators "`kadou get` the team folder, `kadou vault set` the three Jenkins values once" |
| §1.3 | metrics | "Sesami import" row → "all 32 runbooks convert with `kadou import`, check with 0 errors, and `run --dry-run` yields the same env names". Tokens-per-connect row: re-measure after renaming tools and shrinking `propose_kata` |
| §2 row 1 | tool names | `list_kata`, `describe_kata`, `run_kata`, `propose_kata` |
| §2 row 2 | format versioning | **replaced**: "One header grammar (§4.4). No `format_version`. Old dops catalogs are converted once by `kadou import`; the product never loads `runbook.yaml`." |
| §2 row 3 | catalog identity | "Folder name is the id prefix and is the directory name. Rename by renaming the directory; history ids and last-used args follow the old id and are not migrated." |
| §2 row 7 | script contract | "Exec the kata file with its shebang (none → `/bin/sh`), cwd = the kata's directory, env = args + needs + `KADOU_ID/FILE/DIR/ROOT`. Shared helpers reached via `$KADOU_ROOT`." |
| §2 row 9 | skills | "`*.md` beside kata are ignored by the loader. Skills are not a header type." |
| §2 row 11 | compatibility tests | "Import, check, describe, and dry-run all 32; CI uses 32 sanitized `.sh` fixtures." |
| §2 row 12 | loader strictness | "`kadou check` is the loader. A folder with any header error is listed with `✗` and none of its kata run; other folders unaffected. Unknown keys, unknown types, missing `about`/`risk`, tabs, unclosed header are errors with a fix line." |
| §2 row 13 | secret flag | **replaced**: "Secrets are needs. Args cannot be secret. The vault entry carries the secret bit; `kadou vault set` defaults to secret, `--plain` opts out." |
| §2 row 14 | inactive catalogs, staging | **replaced**: "No active flag. Project-local `./kata/` is human-runnable without the vault and agent-invisible until `kadou trust`. Drafts live in the state dir under `proposed/` and `mined/`, are listable with `include_drafts`, never runnable; `kadou accept <id>` is the only path in." |
| §2 row 15 | mining | "Miner writes single-file drafts with a header." |
| §3 | workspace | `kadou-core`: "header parser, folder scanner, `check` diagnostics, vault, last-used args, history" replaces "catalog registry, v1/v2 loader, 3-layer var merge". `serde-yaml-ng` moves to the `kadou import` path only. Add `tests/fixtures/headers/{good,bad}/` |
| §4 (all) | catalog format | **rewrite as "Kata format"**: 4.1 layout (section 2.6 and 3.7 here), 4.2 identity (section 4.1 here), 4.3 header spec (section 3 here), 4.4 needs and args resolution (3.4, 3.6), 4.5 project-local and trust (4.2), 4.6 `kadou import` conversion rules incl. the `../..` rewrite and boolean/number coercion, 4.7 check strictness. Delete `catalog.yaml` and `uses:` entirely |
| §5.2 | progressive disclosure | describe returns `source` (header first, 16 KiB cap), `args` JSON Schema generated from the header, `needs`, `needs_missing`, `files` |
| §5.4 | schemas | rename tools; `describe_kata` and `run_kata` `id` description "folder/name, ./name, or alias"; `list_kata` gains `folder` filter replacing `catalog`, `include_drafts` replacing `include_staging`; `propose_kata` becomes `{id, source}` with the id pattern and `source` maxLength 65536; regenerate `docs/design/tools-list.json` and the `insta` snapshot |
| §5.5 | results | `runbooks` → `kata`, `runbook_id` → `id`, `catalog` → `folder`; describe as in section 2.8; add `missing_needs` error; `staging` error → `draft`; propose result `{status, id, path, diff, accept}` |
| §5.6 | snippet | unchanged except tool ids in the prose: `mcp__kadou__run_kata` |
| §6.1 | exec contract | argv is `<interpreter-from-shebang> <file>` or `/bin/sh <file>`; add `KADOU_*` vars to the allowlist; needs injected as env; per-kata `timeout:` bound; delete the `agent_settable` sentence |
| §6.2 | ceilings | per folder; add project-local trust as a visibility term; keep the five-kata default example |
| §6.4 | grants | pin = kata file sha256 (+ folder git HEAD); `[agent] allow` ids; `kadou grant allow <id>` |
| §6.5 | vault | flat payload with secret bit; `kadou vault set [--plain] <name>`; Go import maps `global.*` and reports `catalog.*`; "MCP never writes the vault" unchanged |
| §6.6 | history | field renames; last-used args dir documented as non-secret state |
| §6.7 | propose / accept | path under state `proposed/<folder>/<name>.sh`; `kadou accept <id> [--into <folder>]` defaulting to the id's folder; refuses a git-checkout target |
| §6.8 | mining | draft writer emits a header file; `mined/<name>` ids |
| §7.1 | command tree | replace: `kadou` · `run <id> [k=v…] [--dry-run] [--confirm <id>]` · `list [--folder F] [--risk R] [query]` · `show <id>` · `new <id> [--from <id>]` · `edit <id>` · `check [folder\|path] [-v]` · `get <url> [--as F] [--ref R] [--root SUB]` · `update [F]` · `remove <F>` · `import <dir> --as <F>` · `accept <id>` · `trust [--forget]` · `vault set [--plain] <name>` / `list` / `rm <name>` · `history` · `grant list/approve/deny/allow` · `mcp serve` / `mcp schema --bytes` · `mine …` · `completion`. Delete `catalog *`, `config get/set`, `info`, `--param` |
| §7.2 | keys | add `e`, `n`, `c`, `g`, `p`, `y` (section 2.5) |
| §7.3 | first run | "`kata/starter/` is written from the embed if `kata/` does not exist. Adding Sesami: `kadou import … --as sesami` (existing dops catalog) or `kadou get <url> --as sesami` (converted repo), then `kadou vault set jenkins_user --plain`, `kadou vault set jenkins_token`." |
| §7.5 | config | file is `kadou.toml`; `[[catalogs]]` deleted; add `[folder.<name>]`, `[trust]`, `[agent] allow`; example as in section 2.6 |
| §7.6 | starter | five files, on disk, names `hello`, `disk-usage`, `git-status`, `health`, `list-path`; `target_dir` args kept |
| §8.1 | non-goals | add "a registry, a `catalog.yaml`, a format version, scopes on args, `secret` on args" |
| §8.2 | risks | rewrite the `../..` row to "importer rewrites the idiom; `KADOU_ROOT` stable under nesting"; add "bespoke header parser: snapshot its diagnostics"; add "project-local folder from an untrusted repo: trust gate"; add "shebang runtime widening: check warns when a non-`sh` interpreter is missing from `PATH`" |
| §9 slice 2 | loader | "Header parser + folder scanner + `kadou check` + `kadou import` with the 32-file conversion test" |
| §9 slice 4 | vault | flat payload; `--plain`; last-used args store |
| §9 slice 5 | MCP | tool renames; `missing_needs`; drafts; trust gate test: an untrusted `./kata` is absent from `list_kata` |
| §9 slice 7 | git | "`kadou get` / `update` / `remove`, `--root` symlink, `kadou accept` refuses git targets" |
| §9 slice 8 | TUI | `e`, `n`, `c`, `g` keys; file-first detail pane; starter materialization |
| §9 slice 9 | mining | header drafts |
| §11 | decisions index | rows 2, 3, 7, 9, 11, 13, 15 reworded per the table above; add "16. Kata is one file with a closed-grammar header; folders are namespaces; no registry" |
| §13 | revision log | add a "Shape (08)" table: this document's five amendments and the table in section 1 |
| `docs/design/tools-list.json` | regenerate | four `*_kata` tools; measure and record bytes |

---

## 8. Open questions for Mason

1. **kata or runbook.** Recommendation is kata (section 5). This is a taste call and it is yours. Everything else in this document works with either word; only the tool names and the file `kata.sh` change.
2. **Shebang as runtime.** The shape makes `#!/usr/bin/env python3` kata natural because the header is a comment in any `#` language. `03` §1 rule 5 says POSIX `sh` is the runtime. Recommendation: honor the shebang, default `/bin/sh`, and let `kadou check` warn when the interpreter is not on `PATH`. Say no and the rule stays "sh only, check errors on any other shebang".
3. **Project-local trust by path.** Recommendation: `kadou trust` pins the folder path (direnv `allow` without the hash). The alternative, hash pinning, re-prompts on every commit. Either way agents never see an untrusted `./kata`.
4. **Sesami repository layout after conversion.** Recommendation: the converted repository has kata at the root (`cc4-aaa.sh`, `scripts/`, `device-log-metrics/`), so `kadou get` needs no `--root`. The alternative keeps `src/` and uses the `--root` symlink. You own that repository; the importer supports both.
5. **Last-used args.** Recommendation: TUI prefills from the last run, CLI and MCP always use header defaults. Go's runbook-scope saving is dropped. If you want CLI to remember too, it is one flag (`--last`), but "what the file says is what runs" is the cleaner rule.
6. **Starter on disk.** Recommendation: materialize `kata/starter/` on first run so it is editable and deletable. The alternative keeps it embedded-only and read-only, which is simpler to guarantee but violates "read every script the product will run" in spirit.

---

## Method and limits

- Read: the proposal, `05-prd.md` in full, `03-principles.md`, `07-review.md` §0, §8, §10, Appendix A, `01-audit.md` §4 and §7. Read-only inspection of `~/Bitbucket/sdo-dops-catalog` (`src/*/runbook.yaml` key inventory, `cc4-aaa`, `device-log-metrics`, `ses-argocd-sync`, `ses-deploy`, the `trigger-pipeline.sh` head) and `~/origin/dops/internal/theme/doop.json`.
- Verified on disk: 29 of 29 Jenkins wrappers contain the identical `REPO_ROOT` / `TRIGGER` pair; 1 793 total YAML lines; parameter-name frequency (29 × three `jenkins_*`, 24 `version`, 22 `branch`, 22 `send_email`, 22 `allow_image_override`, 21 `publish_image`, 21 `publish_api`, 13 singletons).
- Not measured: the byte size of the renamed `tools/list` (estimated, section 6.2); tokenizer counts; any live MCP host behavior. No Rust was written. The mocks are hand-drawn and are a target, not a screenshot.
- No secrets, internal hostnames, or workspace identifiers appear in this document.
