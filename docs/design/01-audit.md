# dops audit for dops-next

Audit of the Go product against a Rust rewrite. Codename for the new product is **dops-next**. This document does not propose names.

**Source tree:** `~/origin/dops` at commit `795d2d2` (tag `v0.13.1`). Feature set is **v0.12.0** (execution history) plus a Go 1.26.2 stdlib bump and history file-permission harden (`v0.12.0..HEAD`). The in-tree fallback string in `cmd/version.go:9` is still `"0.10.0"`; releases inject the tag via `-ldflags` (`Makefile:4`).

**Reference workload:** `~/Bitbucket/sdo-dops-catalog` — 32 `src/*/runbook.yaml` files (counted on disk). 29 of those are Jenkins-trigger wrappers sharing three global params; three are not Jenkins (`clone-ses-repos`, `device-log-metrics`, `ses-argocd-sync`).

**Method:** read-only inspection of origin dops and the catalog. Token counts for MCP `tools/list` were produced by replaying `RunbookToInputSchema` / `RunbookToDescription` (`internal/mcp/schema.go`) against the 32 YAML files. Catalog name assumed `src` (what `dops catalog add …/src` stores; `cmd/catalog.go:96-97`). No secrets are quoted; the catalog secret is named `jenkins_token` only.

---

## 1. What dops is

dops is a runbook runner with four interfaces over one engine: TUI (default `dops`), CLI (`dops run`), web (`dops open`), MCP (`dops mcp serve`). Architecture: `docs/architecture.md:1-8`, `cmd/root.go:21-41`.

Bootstrap is shared: `loadDeps()` loads `config.json`, decrypts `vault.json` into `Config.Vars`, loads theme, then `DiskCatalogLoader.LoadAll` (`cmd/deps.go:34-78`, `docs/architecture.md:122-136`). MCP has a parallel `loadMCPDeps` (`cmd/mcp.go:114-141`) that skips theme.

The product that operators actually run is **YAML + POSIX script catalogs**, not the TUI. The Sesami catalog is almost entirely “fill params, exec `script.sh`, which calls Jenkins.” That is the compatibility target.

---

## 2. What is best and must carry over

These are the pieces a rewrite should treat as the product, not as Go accidents.

### 2.1 One engine, four faces

TUI, CLI, web, and MCP share catalog load, var resolution, risk ceilings, executor, and history (`docs/architecture.md:363-364`, `cmd/deps.go:34-37`). Duplicating business logic per interface is how the current tree already drifts (MCP confirm ≠ TUI confirm; MCP `--allow-risk` cannot un-hide load-filtered runbooks). Carry the *rule*; do not copy the drift.

### 2.2 Directory contract: `runbook.yaml` + script

A runbook is a directory containing `runbook.yaml` and a script named by `script:` (`docs/architecture.md:282-310`, `docs/guides/runbooks.md:9-15`). Parameters become **uppercase env vars** in the child process (`internal/executor/script.go:27-30`, `internal/mcp/tools.go:93-96`). POSIX `#!/bin/sh` + `set -eu` is the documented script style (`docs/guides/runbooks.md:80-110`).

This is the migratable unit. dops-next should ingest today’s catalogs without rewriting 32 `script.sh` files.

### 2.3 Risk as a first-class, load-time gate

Every runbook declares `risk_level` (`internal/domain/runbook.go:42`). Four levels with a total order (`internal/domain/risk.go:6-23`). Two ceilings:

1. Global: `defaults.max_risk_level` in `config.json` (default **medium**, `internal/config/store.go:86-91`).
2. Catalog: `catalog.policy.max_risk_level`, used when set, else the global default (`internal/catalog/loader.go:55-58`).

Runbooks that **exceed** the ceiling are omitted at load — not listed in TUI, CLI, or MCP (`internal/catalog/loader.go:184-186`, `docs/architecture.md:245-252`). High/critical that pass the ceiling still need a confirm step (TUI: `internal/tui/confirm/model.go`; MCP: synthetic fields in `internal/mcp/schema.go:35-48`).

This is the safety model operators already rely on. Keep it. Unify the confirm UX (see §9).

### 2.4 Encrypted vault, separate from config

`config.json` holds theme, defaults, catalog registry — no parameter values (`docs/architecture.md:189-202`). `vault.json` is an age envelope (`version` + `data`) with 0600 atomic write (`internal/vault/vault.go:13-19, 76-136`). Keys live in `~/.dops/keys/keys.txt` (`internal/crypto/age.go:20-37`). Spec: `specs/vault-v0.3.0.md`.

The Sesami catalog’s `jenkins_token` is `type: string` + `secret: true` + `scope: global` (e.g. `~/Bitbucket/sdo-dops-catalog/src/cc4-aaa/runbook.yaml:20-25`). Secret is a **flag**, not a ninth type. MCP strips secret params from tool schemas and warns in the description (`internal/mcp/schema.go:18-23, 133-144`). History masks them as `****` (`internal/domain/execution.go:82-88`).

Carry: encrypted-at-rest parameter store, secret flag, never put values in `config.json`, never put secret values in MCP schemas or history JSON.

### 2.5 Scoped parameters with a 3-layer merge

Four scopes in YAML: `global`, `catalog`, `runbook`, `local` (`docs/architecture.md:230-241`). Persistence key paths (`internal/vars/keypath.go:7-16`):

| Scope | Vault key |
|---|---|
| `global` | `vars.global.<param>` |
| `catalog` | `vars.catalog.<cat>.<param>` |
| `runbook` | `vars.catalog.<cat>.runbooks.<rb>.<param>` |
| `local` or empty | not saved (`cmd/run.go:173-176`) |

Resolution is **always** global < catalog < runbook, then user input (`internal/vars/resolver.go:19-49`, `docs/architecture.md:241`). Scope does **not** filter reads — a `local` param named `jenkins_url` still picks up the global value if one exists. That is how 29 Jenkins runbooks share three globals without a catalog-level include file.

Carry the merge semantics. Document them. The Sesami catalog is unusable without global scope.

### 2.6 Catalog registry + git install

Catalogs are entries in `config.json`, not a scan of `~/.dops/catalogs` (`internal/domain/config.go:21-29`). Commands: `list`, `add` (local path), `remove`, `install` (git clone), `update` (pull / checkout ref / policy / display name) (`cmd/catalog.go:26-36`).

`install` supports `--name`, `--ref`, `--path` (sub-directory inside the clone, validated against escape), `--risk`, `--display-name` (`cmd/catalog.go:204-274`). `Catalog.RunbookRoot()` joins `Path` + `SubPath` (`internal/domain/config.go:53-60`). Inactive catalogs are skipped (`internal/catalog/loader.go:51-53`).

Carry the registry model (named catalog, path, optional git URL, optional sub-path, active flag, policy ceiling, display name). Display names are cosmetic; IDs and vault paths use canonical `name` (`specs/catalog-aliases.md:11-12`).

### 2.7 Aliases as lookup, not identity

`aliases: [scale, scale-deploy]` on the runbook (`internal/domain/runbook.go:38`, `specs/runbook-aliases.md`). Validation: lowercase alnum / hyphen / dot (`internal/domain/runbook.go:50-61`). Collisions with IDs or other aliases are warnings, first-loaded wins (`internal/catalog/loader.go:79-117`). CLI `dops run` tries ID then alias (`cmd/run.go:122-137`). MCP puts aliases in the tool **description**, not the tool name (`internal/mcp/schema.go:126-128`).

Carry aliases. They are cheap and already specified.

### 2.8 Execution history as an audit trail

v0.12.0 added `ExecutionRecord` (`internal/domain/execution.go:29-45`) written by TUI, CLI, web, and MCP. Store: JSON files under `~/.dops/history/` (`internal/history/store.go:31-62`), logs in size-capped `.log.gz` tar archives (10MB per archive, `internal/history/archive.go:16-37`). CLI: `dops history` (`cmd/history.go`). Web: `/api/history` (`internal/web/api.go:78-80`). Secrets masked before persist.

Carry structured execution records. The 50MB / 90-day eviction in `specs/VERSION_0_12_0.md:22-26` is **not** what the code does (`NewFileStore` ignores its size-cap argument, `internal/history/store.go:41-43`). Rewrite to one documented policy.

### 2.9 MCP as a real interface — not as one-tool-per-runbook

Exposing runbooks to agents is the right product. The **registration shape** is wrong: each runbook becomes a full MCP tool with a full JSON Schema (`internal/mcp/server.go:98-126`, `docs/architecture.md:260-268`). For the 32-runbook catalog that costs ~8.5k tokens on connect, ~37× a four-tool dispatcher (see §7). Carry MCP. Change the tool surface.

---

## 3. What to drop or defer

### 3.1 Drop (do not rebuild)

| Item | Why |
|---|---|
| **One MCP tool per runbook** | Linear schema cost; 21 CC4 tools share a ~965-byte schema. See §7. |
| **`internal/mcp/watcher.go`** | File watcher exists and is never called (`NewWatcher` only defined, never wired). Dead code. |
| **`DecryptingVarResolver`** | Comment: “not yet wired into the default resolver chain” (`internal/vars/decrypting_resolver.go:10-13`). Vault already decrypts the whole blob on load (`internal/vault/vault.go:39-73`). |
| **Catalog switcher tab bar** | Built then reverted (`plans/TODO.md:17`, `specs/VERSION_0_11_0.md:103-105`). |
| **MCP progress as currently stubbed** | `OnProgress` is rate-limited then discarded (`internal/mcp/server.go:137-149`: “MCP SDK notification support can be wired here when available”). Do not ship a no-op. |
| **`dops://history` as a promised resource** | Spec checkbox still open (`specs/VERSION_0_12_0.md:42`). History exists; the MCP resource does not. |
| **Integer vs number as two types** | `integer` = any whole, `number` = non-negative whole (`internal/domain/runbook.go:14-15`). The Sesami catalog uses `number` once (`ses-argocd-sync` timeout) and `integer` never. Collapse in the new schema; accept both on import. |
| **Demo runner / demo history seed as product** | `internal/executor/demo.go`, `internal/web/api.go:38-67`. Fine for marketing; not a runtime requirement. |

### 3.2 Defer (after a compatible runner exists)

| Item | Why defer |
|---|---|
| **Vue 3 web UI** | Full SPA + SSE (`web/`, `internal/web/`). Valuable, not blocking catalog execution. |
| **TUI feature parity** | 20 embedded themes (`internal/theme/`), text selection, OSC 52 clipboard, VHS tapes. Rewrite the TUI later against the same engine. |
| **`type: skill` MCP prompts** | Implemented (`internal/domain/skill.go`, `internal/mcp/server.go:68-96`). **Zero** `skill.md` files in origin catalogs or sdo-dops-catalog. Keep the YAML field for compatibility; do not prioritize. |
| **`create-runbook` prompt** | Useful scaffolding (`internal/mcp/prompts.go:227-329`). Not needed to run existing catalogs. |
| **Windows / PowerShell** | `script.ps1` path in `dops init` (`cmd/init.go:35-68`) and `generateParamVars` (`internal/mcp/prompts.go:193-207`). Sesami workload is POSIX. |
| **Hot-reload of catalogs** | Watcher was the intended path; unused. Restart-to-reload is enough for v1. |
| **Per-field “save for future runs?” wizard UX** | Concept (skip saved fields, `specs/skip-saved-fields.md`) is good. The Bubble Tea wizard (`internal/tui/wizard/`) need not be cloned. |

### 3.3 Do not treat as the format

Architecture docs and MCP docs disagree with code in places that will bite a migrator:

- `docs/guides/mcp.md:95` says resources are `dops://runbook/<id>`; code registers `dops://catalog/{id}` (`internal/mcp/server.go:215-217`).
- `docs/architecture.md:55` says the wizard has “8 field types”; the domain has **9** parameter types (`internal/domain/runbook.go:11-21`). The wizard uses four *modes* covering those nine (`internal/tui/wizard/model.go:16-23`).
- `docs/architecture.md:256` and `internal/mcp/prompts.go:66` say critical confirmation is “type the runbook ID”; MCP critical uses `_confirm_word=CONFIRM` (`internal/mcp/tools.go:143-147`). TUI critical **does** require the runbook ID (`internal/tui/confirm/model.go:105-113`). Documented table in `docs/guides/runbooks.md:56-63` matches the split; architecture.md does not.
- History lifecycle in the v0.12 spec (7/90 day, 50MB) ≠ 10MB archive rotation in code.

---

## 4. Catalog / spec / runbook model (migratable format)

This section is the contract dops-next should import.

### 4.1 On-disk layout

```
<catalog-root>/                 # Catalog.RunbookRoot()
  <entry-dir>/                  # directory name becomes ID suffix
    runbook.yaml
    script.sh                   # or whatever `script:` names
    skill.md                    # only when type: skill
```

Loader: scan immediate subdirectories of each **active** catalog root; require `runbook.yaml`; skip non-dirs (`internal/catalog/loader.go:140-158`). Nested catalogs are not supported (one level). `dops init` scaffolds `~/.dops/catalogs/default/hello-world/` (`cmd/init.go:104-141`).

Docs say `name:` must match the directory (`docs/guides/runbooks.md:22`). The loader does **not** enforce that. Execution joins `RunbookRoot / Runbook.Name / Script` (`internal/tui/exec.go:35`, `internal/mcp/tools.go:90-91`). If YAML `name` ≠ directory, the script path is wrong. The Sesami catalog keeps them equal.

### 4.2 `runbook.yaml` fields

From `internal/domain/runbook.go:34-45` and the MCP schema guide (`internal/mcp/prompts.go:26-43`):

| Field | Required in practice | Notes |
|---|---|---|
| `id` | no | If empty, set to `<catalog>.<dirname>` (`internal/catalog/loader.go:160-163`). Must be `<catalog>.<runbook>` if set (`ValidateRunbookID`, `internal/domain/runbook.go:64-76`). |
| `name` | yes (docs) | Used as directory segment for the script. |
| `type` | no | Empty/`runbook` = executable. `skill` = injectable context (`IsSkill()`, `internal/domain/runbook.go:47-48`). |
| `aliases` | no | List of strings; see §2.7. |
| `description` | yes (docs) | Becomes MCP tool description. |
| `trigger` | skills only | Comma-separated keywords; appended to prompt description (`internal/mcp/server.go:73-76`). |
| `version` | no | Opaque string; listed in MCP catalog resource. |
| `risk_level` | yes (docs) | `low` \| `medium` \| `high` \| `critical`. Empty is not a valid `ParseRiskLevel` (`internal/domain/risk.go:25-31`); `Exceeds` treats unknown as rank 0. |
| `script` | yes for runbooks | Filename relative to the runbook dir. Skills have no script. |
| `parameters` | no | List of `Parameter`. |

`Parameter` (`internal/domain/runbook.go:23-32`):

| Field | Notes |
|---|---|
| `name` | Env var is `strings.ToUpper(name)`. |
| `type` | One of the nine; unknown falls through to string in MCP schema (`internal/mcp/schema.go:98-99`). |
| `required` | MCP: required unless a vault value is already resolved (`internal/mcp/schema.go:27-32`). |
| `scope` | `local` \| `global` \| `catalog` \| `runbook`. Empty is treated as local for **save**, global for **keypath default** (`internal/vars/keypath.go:13-15` default branch is `vars.global.*` — only reached if save is not skipped). |
| `secret` | bool. Masked; excluded from MCP input schema. |
| `default` | YAML any; copied into MCP JSON Schema `default`. |
| `description` | MCP property description. |
| `options` | Required for `select` / `multi_select` (docs); MCP emits `enum` if present. |

There is **no** YAML schema validation at load beyond unmarshal. Invalid `type` or missing `options` is a wizard/runtime problem, not a load error (`internal/catalog/loader.go:194-206`).

### 4.3 The nine parameter types

Declared in `internal/domain/runbook.go:11-21`. Validation (TUI wizard, `internal/tui/wizard/model.go:345-363`) and MCP JSON Schema (`internal/mcp/schema.go:70-99`):

| Type | Meaning | Wizard | MCP schema |
|---|---|---|---|
| `string` | Free text | text / password if `secret` | `{type:string}` |
| `boolean` | Toggle | yes/no | `{type:boolean}` |
| `integer` | Whole, negative OK | `strconv.Atoi` | `{type:integer}` |
| `number` | Whole ≥ 0 | `Atoi` + `n < 0` reject | `{type:integer, minimum:0}` |
| `float` | Decimal | `ParseFloat` | `{type:number}` |
| `select` | One of `options` | list | `{type:string, enum:[…]}` |
| `multi_select` | Many of `options` | checklist | `{type:array, items:{type:string, enum}}` |
| `file_path` | Path | text | `{type:string, description:"file path"}` |
| `resource_id` | ARN/URI-ish | text | `{type:string, description:"resource identifier"}` |

Sesami catalog usage (32 files): `string` 142, `boolean` 86, `select` 5, `number` 1. Unused: `integer`, `float`, `multi_select`, `file_path`, `resource_id`. Built-in example `catalogs/complex/deploy-app/runbook.yaml` does use `select`, `multi_select`, `boolean`, and `secret`.

**Import rule:** accept all nine. dops-next may map `file_path`/`resource_id` to string and `number` to non-negative integer without breaking catalogs.

### 4.4 Scopes

| Scope | Saved? | Typical use in Sesami catalog |
|---|---|---|
| `global` | yes | `jenkins_url`, `jenkins_user`, `jenkins_token` (87 param declarations) |
| `catalog` | yes | unused in sdo-dops-catalog |
| `runbook` | yes | `branch`, `version`, Jenkins toggles (146) |
| `local` | no | `ses-argocd-sync.app_name` (1) |

Precedence: global < catalog < runbook < CLI `--param` / wizard / MCP args (`internal/vars/resolver.go:22-38`, `internal/mcp/tools.go:79-87`). Resolver then **filters to declared parameter names** (`internal/vars/resolver.go:41-47`) so extra vault keys do not leak into the env.

CLI `dops run` saves resolved values unless `--no-save` (`cmd/run.go:89-99`). Local/empty scope is skipped. MCP and TUI persist through their own paths (TUI: per-field `SaveFieldMsg`, `docs/architecture.md:167`).

### 4.5 Aliases (runbook and catalog)

- **Runbook aliases:** YAML list; global uniqueness with warnings; CLI/MCP lookup; sidebar still shows `name` (`specs/runbook-aliases.md:14`).
- **Catalog display names:** `display_name` on the config entry, max 50 printable chars (`internal/domain/config.go:39-51`). `dops catalog add|install|update --display-name`. Not an identifier.

sdo-dops-catalog runbooks currently define **no** aliases.

### 4.6 Risk levels and policy ceilings

Order: low(0) < medium(1) < high(2) < critical(3) (`internal/domain/risk.go:14-19`).

`Exceeds(ceiling)` is strict greater-than, so a runbook **at** the ceiling is allowed (`internal/domain/risk.go:21-23`).

Default global ceiling **medium** (`internal/config/store.go:89`) hides `ses-release-build` (high) and `ses-deploy` (critical) unless the catalog policy or defaults are raised. `dops catalog add` does not set policy (`cmd/catalog.go:114-119`). `dops catalog install --risk` / `update --risk` does (`cmd/catalog.go:261-262, 377-386`).

MCP `--allow-risk` (default `critical`, `cmd/mcp.go:81`) is a **second** filter at tool registration (`internal/mcp/server.go:105-108`). It cannot restore runbooks already dropped by `LoadAll`. A default-config user who only `catalog add`s the Sesami `src/` tree will not see the two production-impacting runbooks on MCP.

Confirm gates after the ceiling:

| Level | TUI (`internal/tui/confirm/model.go`) | MCP (`internal/mcp/schema.go:35-48`, `tools.go:136-149`) |
|---|---|---|
| low / medium | none | none |
| high | y/N, default No | required `_confirm_id` == runbook ID |
| critical | type the runbook ID | required `_confirm_word` == `"CONFIRM"` |

Web uses the same high/critical split as TUI conceptually (`docs/guides/runbooks.md:56-63`). Rewrite should pick **one** confirm protocol and use it everywhere.

Sesami risk mix: medium 25, low 5, high 1 (`ses-release-build`), critical 1 (`ses-deploy`).

### 4.7 Vault / vars resolution

On-disk (`internal/vault/vault.go:15-19`, `specs/vault-v0.3.0.md:26-38`):

```json
{ "version": 1, "data": "<age1… base64>" }
```

Decrypted payload (`internal/domain/config.go:73-81`, `docs/architecture.md:215-227`):

```json
{
  "global": { "jenkins_url": "https://ci.sesami.io", "jenkins_user": "…" },
  "catalog": {
    "<catalog-name>": {
      "<catalog-scoped-keys>": "…",
      "runbooks": {
        "<runbook-name>": { "branch": "dev" }
      }
    }
  }
}
```

`CatalogVars` JSON is a flat object with an optional nested `"runbooks"` key (`internal/domain/config.go:83-117`). Age: X25519 + ChaCha20-Poly1305 (`internal/crypto/age.go`, `docs/architecture.md:357-359`). Tamper → decrypt error: “vault.json is corrupted or was modified outside dops” (`internal/vault/vault.go:64-65`).

`config.Set`/`Get`/`Unset` use dotted paths; vars writes go to in-memory `Config.Vars` then `Vault.Save` (`internal/config/path.go`). `Vars` is `json:"-"` on `Config` so it cannot round-trip through `config.json` (`internal/domain/config.go:10-15`).

**Migration for dops-next:** import `vault.json` v1 if keys are present; otherwise start empty and let operators re-save globals. Do not log or document `jenkins_token` values.

### 4.8 `type: skill` entries

A skill is `runbook.yaml` with `type: skill` plus `skill.md` in the same directory (`internal/domain/skill.go:1-12`, `specs/VERSION_0_11_0.md:161-176`). Loader:

- Reads `skill.md`; missing file → warning, skip (`internal/catalog/loader.go:165-181`).
- Does **not** add the entry to `Runbooks` (not executable, not TUI-visible).
- Does **not** apply risk filter (`specs/VERSION_0_11_0.md:220`).

MCP: each skill is a **prompt** named with the skill ID; `GetPrompt` returns `skill.md` as a user-role text message; description includes `[triggers: …]` (`internal/mcp/server.go:68-96`). Not a tool.

No skills ship in origin `catalogs/` or sdo-dops-catalog. Compatibility: keep parsing `type: skill` so a future catalog does not break load.

### 4.9 Catalog registration (`config.json`, git install)

`config.json` shape (`internal/domain/config.go:10-29`, `docs/architecture.md:193-201`):

```json
{
  "theme": "github",
  "defaults": { "max_risk_level": "medium" },
  "catalogs": [
    {
      "name": "src",
      "display_name": "Jenkins pipelines",
      "path": "/abs/path/to/sdo-dops-catalog/src",
      "sub_path": "",
      "url": "git@…",
      "active": true,
      "policy": { "max_risk_level": "critical" }
    }
  ]
}
```

| Command | Effect |
|---|---|
| `dops init` | `EnsureDefaults`; if no catalogs, write hello-world under `catalogs/default` and register it (`cmd/init.go:104-141`). |
| `dops catalog add <path>` | Name = `filepath.Base(path)` (so `…/src` → `src`). Absolute path. Active true. No URL, no policy (`cmd/catalog.go:73-122`). |
| `dops catalog install <url>` | Clone into `~/.dops/catalogs/<name>`, optional `--ref`, `--path` (sub-path), `--risk`, `--display-name`, `--name` (`cmd/catalog.go:204-274`). |
| `dops catalog update <name>` | `git pull` or `fetch`+`checkout --ref`; optional policy / display name (`cmd/catalog.go:305-391`). Local-only catalogs (no URL) cannot git-update. |
| `dops catalog remove <name>` | Drop from config; does not delete files (`cmd/catalog.go:129-157`). |
| `dops catalog list` | Table: name, display name, path, url, active, risk policy (`cmd/catalog.go:41-69`). |

SPEC.md for the Sesami catalog (`~/Bitbucket/sdo-dops-catalog/SPEC.md:35-40`) documents:

```sh
dops catalog add ~/src/bitbucket.org/sdo-dops-catalog/src
# catalog name: jenkins-pipelines
```

The comment does not match the command: `add` names the catalog `src`. `install --name jenkins-pipelines --path src` would match the comment. dops-next should accept either; IDs are `<catalog>.<dirname>`, so a rename **breaks** saved runbook-scoped vault keys and history `runbook_id`s. Treat catalog `name` as stable.

`sub_path` is how a monorepo (catalog files under `src/`) installs without flattening.

### 4.10 History

Record fields (`internal/domain/execution.go:29-45`): `id` (UUID v4), `runbook_id`, `runbook_name`, `catalog_name`, `parameters` (secrets `****`), `status` (`running|success|failed|cancelled`), `exit_code`, `start_time`, `end_time`, `duration`, `output_lines`, `output_summary`, `log_path`, `interface` (`tui|cli|web|mcp`).

Persistence:

- Record: `~/.dops/history/<timestamp>-<runbook-id>-<uuid>.json` (`internal/history/store.go:200-204`).
- Log: append into `~/.dops/history/logs/<archive-id>.log.gz` as tar entries named `<uuid>.log`; `LogPath` is `archive#entry` (`internal/history/store.go:65-82`, `archive.go:16-37`).
- List: newest-first, default limit 50 in the store / 20 in the CLI (`internal/history/store.go:170-176`, `cmd/history.go:69`).
- Size cap argument to `NewFileStore` is ignored (`internal/history/store.go:41-43`). Archive rotation is 10MB (`archive.go:16`).

MCP writes history on tool completion (`internal/mcp/server.go:163-170`) but does not expose it as a resource.

### 4.11 Executor

`Runner.Run(ctx, scriptPath, env)` streams stdout/stderr lines (`internal/executor/runner.go:5-12`, `script.go:21-75`). Child env = parent env + uppercase param map. Platform shell from script extension (`internal/executor/shell.go`). Cancel via context + platform process group (`proc_unix.go` / `proc_windows.go`). MCP truncates tool output to last **50** lines (`internal/mcp/tools.go:17, 111-115`).

---

## 5. MCP surface (every tool, resource, prompt)

SDK: `github.com/modelcontextprotocol/go-sdk v1.4.1` (`go.mod:12`). Server name `"dops"`, version from ldflags (`internal/mcp/server.go:45-47`). Transports: stdio (default) and HTTP with gzip except SSE (`cmd/mcp.go:66-72`, `internal/mcp/server.go:284-332`).

### 5.1 Tools

**There are no fixed tool names.** `registerTools` loops loaded runbooks and `AddTool`s one tool per runbook (`internal/mcp/server.go:98-126`):

| Property | Value |
|---|---|
| Name | `runbook.ID` (e.g. `src.cc4-aaa`) |
| Description | YAML description + optional `[aliases: …]` + `[risk: …]` + secret warning (`internal/mcp/schema.go:122-144`) |
| Input schema | JSON Schema object from non-secret parameters + confirm fields |

Skills are not tools. Runbooks above `--allow-risk` are not tools. Runbooks already filtered at load are not tools.

For the reference catalog (name `src`, 32 runbooks, no vault defaults applied in the count):

- 32 tools
- Compact `{"tools":[…]}` **33 940 bytes** (~**8 485 tokens** at 4 chars/token; ~**9 697** at 3.5)
- Pretty-printed **51 177 bytes**
- Sum of compact `inputSchema` objects: **25 752 bytes**
- 21 CC4 tools share a **965-byte** schema (same param set; only description/name differ)

High/critical extras: `src.ses-release-build` includes `_confirm_id`; `src.ses-deploy` includes `_confirm_word`.

`dops mcp tools` prints `ID` + description, it does not dump schemas (`cmd/mcp.go:86-111`).

### 5.2 Resources

Registered in `internal/mcp/server.go:188-281`:

| URI | Kind | MIME | Body |
|---|---|---|---|
| `dops://catalog` | resource | `application/json` | Array of `{id,name,catalog,description,risk_level,version}` (`internal/mcp/resources.go:11-41`) |
| `dops://catalog/{id}` | resource **template** | `application/json` | Full runbook including `parameters` (`resources.go:44-62`) |
| `dops://schema/runbook` | resource | `text/markdown` | Embedded YAML schema guide, **2 073 chars** (~518 tokens) (`internal/mcp/prompts.go:14-76`) |
| `dops://schema/shell-style` | resource | `text/markdown` | POSIX style guide, **2 177 chars** (~544 tokens) (`prompts.go:78-176`) |

Not implemented: `dops://history` (`specs/VERSION_0_12_0.md:42`). Docs’ `dops://runbook/<id>` (`docs/guides/mcp.md:95`) is wrong.

`resources/list` metadata (3 resources + 1 template) is ~**644 bytes compact** (~160 tokens). Resource **bodies** are not sent until `resources/read`. A 32-runbook `dops://catalog` body is ~**7 903 bytes** pretty (~1 976 tokens).

### 5.3 Prompts

| Name | Arguments | Result |
|---|---|---|
| `create-runbook` | `catalog` (req), `name` (req), `description` (req), `risk_level` (opt, default `low`) | User message with YAML + `script.sh` / `script.ps1` template (`internal/mcp/prompts.go:227-329`) |
| `<skill-id>` | none | One prompt per loaded skill; body = `skill.md` (`internal/mcp/server.go:68-96`) |

sdo-dops-catalog contributes **zero** skill prompts. `prompts/list` for `create-runbook` only: ~**567 bytes** (~142 tokens).

---

## 6. Token cost today

### 6.1 What a typical MCP client loads on connect

Clients (Claude Code, Cursor, etc.) after `initialize` call `tools/list`, and usually `resources/list` + `prompts/list`. They do **not** fetch resource bodies unless the model asks.

For the 32-runbook reference catalog, one connect pays:

| Handshake | Compact bytes | Tokens @ 4 chars | Notes |
|---|---|---|---|
| `tools/list` | 33 940 | **~8 500** | 32 full JSON Schemas. Dominates. |
| `resources/list` | 644 | ~160 | Names/URIs only |
| `prompts/list` | 567 | ~140 | `create-runbook` only |
| **Total (typical)** | **~35 200** | **~8 800** | Every session, every agent |

Optional later reads:

| Read | Bytes | Tokens |
|---|---|---|
| `dops://catalog` | ~7 900 | ~2 000 |
| `dops://schema/runbook` | 2 073 | ~520 |
| `dops://schema/shell-style` | 2 177 | ~540 |
| `dops://catalog/{id}` × 1 | ~1–2 KB | ~300–500 |

If the client prefetches schema resources (some do for “MCP apps”), add ~1k tokens. Still small next to `tools/list`.

Method: Python replay of `paramToSchemaProperty` + `RunbookToDescription` + confirm fields against `~/Bitbucket/sdo-dops-catalog/src/*/runbook.yaml`, catalog id prefix `src.`. Vault defaults were **not** injected; saved values would add `"default": "…"` keys and drop names from `required` (`internal/mcp/schema.go:27-32, 114-117`), changing size by tens of bytes per field, not order of magnitude. Token estimate is `bytes/4` (JSON is punctuation-heavy; `/3.5` ≈ 9.7k for `tools/list`).

### 6.2 Why one-tool-per-runbook is expensive

1. **MCP `tools/list` inlines every input schema.** There is no “list names, fetch schema on call.” `registerTools` builds the schema at server start (`internal/mcp/server.go:110-121`) and the SDK returns it on list.

2. **Cost scales with runbook count, not with distinct shapes.** 21 CC4 services are the same 8 non-secret params (3 globals minus `jenkins_token`, plus `branch`/`version`/four booleans). Each copy is 965 bytes of schema + ~190 bytes of description. That is ~24 KB of near-duplicates inside a 34 KB list.

3. **Secrets inflate descriptions, not schemas.** `jenkins_token` is omitted from properties (good) but every Jenkins tool description appends `⚠ Sensitive inputs (jenkins_token) are loaded from local config…` (`internal/mcp/schema.go:139-141`). 29 copies of the same warning.

4. **The SPEC’s intended catalog is 370 pipelines** (`~/Bitbucket/sdo-dops-catalog/SPEC.md:1-4`). Linear extrapolation from ~1.06 KB/tool: **~390 KB / ~98k tokens** on connect — larger than many models’ tool budget. Even 32 runbooks is already the size of a small skill file, paid on **every** turn that includes tool defs.

5. **A four-tool dispatcher is ~37× smaller.** Compact payload for `list` / `get` / `run` / `history`: **917 bytes** (~229 tokens). Same catalog, same operations, schemas fetched per id.

6. **Confirm fields duplicate policy.** High/critical tools grow an extra required property. A single `run` tool can apply policy server-side without 32 copies.

7. **Agents cannot search.** With 32 similarly named `src.cc4-*` tools, the model must keep all names in context to pick one. A `list(query=)` tool returns a short page; that is how Datadog/Atlassian lazy-mcp already works in this environment.

dops-next should expose a **small, stable tool set** and treat runbooks as data (resources or `list`/`get` results), not as tool-registry entries.

---

## 7. Reference workload notes (sdo-dops-catalog)

- **32** runbook dirs under `src/` (disk count). SPEC.md still describes a 370-pipeline TODO; the 32 checked items are what exists.
- **29** Jenkins wrappers: each re-declares `jenkins_url` (string, global, default `https://ci.sesami.io`), `jenkins_user` (string, global), `jenkins_token` (string, global, **secret: true**). Scripts call `scripts/trigger-pipeline.sh` with `JENKINS_PASS="${JENKINS_TOKEN}"` (`SPEC.md:52-87`).
- **3** non-Jenkins: `clone-ses-repos` (low, local git), `device-log-metrics` (low, select env), `ses-argocd-sync` (medium, `k8s_context` select + local `app_name` + `number` timeout).
- CC4 runbooks add `branch` default `dev` and four booleans (`send_email`, `publish_image`, `publish_api`, `allow_image_override`) — e.g. `src/cc4-aaa/runbook.yaml`.
- Production-impacting: `ses-deploy` critical (`src/ses-deploy/runbook.yaml:4`), `ses-release-build` high (`src/ses-release-build/runbook.yaml:4`).
- Shared script utility is **outside** the runbook directory (`scripts/trigger-pipeline.sh`). The loader only knows about `script:` inside the runbook dir; extra files work because `script.sh` reaches them via `$(dirname "$0")/..`. A rewrite must not assume a two-file directory is the whole program.

---

## 8. Ranked keep / drop / defer

Rank is rewrite priority (1 = do first). “Keep” means keep the *behavior or format*, not the Go code.

| Rank | Item | Verdict | Evidence |
|---|---|---|---|
| 1 | `runbook.yaml` + script directory contract, env-var injection | **Keep** | `internal/domain/runbook.go`, `internal/executor/script.go:27-30` |
| 2 | Parameter model: 9 types (import), `secret` flag, 4 scopes, 3-layer resolve | **Keep** | `runbook.go:11-32`, `internal/vars/resolver.go` |
| 3 | Risk levels + dual policy ceiling + load-time hide | **Keep** | `internal/domain/risk.go`, `internal/catalog/loader.go:184-186` |
| 4 | Vault v1 (age envelope, vars payload) + config.json without secrets | **Keep** | `internal/vault/vault.go`, `internal/domain/config.go:10-15` |
| 5 | Catalog registry: name, path, sub_path, url, active, policy, display_name; add/install/update | **Keep** | `cmd/catalog.go`, `internal/domain/config.go:21-29` |
| 6 | MCP as an interface | **Keep** | `cmd/mcp.go`, `internal/mcp/server.go` |
| 7 | One MCP tool per runbook | **Drop** | `internal/mcp/server.go:98-126`; §7 token table |
| 8 | CLI `dops run --param --dry-run --no-save` + ID/alias resolve | **Keep** | `cmd/run.go` |
| 9 | Execution history records + secret masking | **Keep** (fix eviction policy) | `internal/domain/execution.go`, `internal/history/` |
| 10 | High/critical confirm gates | **Keep** (unify TUI vs MCP) | `confirm/model.go` vs `mcp/tools.go:136-149` |
| 11 | Runbook aliases | **Keep** | `specs/runbook-aliases.md`, `loader.go:79-117` |
| 12 | `type: skill` parse + MCP prompts | **Defer** (parse now, product later) | `internal/domain/skill.go`; unused in catalogs |
| 13 | `create-runbook` prompt + schema resources | **Defer** | `internal/mcp/prompts.go`, `server.go:250-281` |
| 14 | Web UI (Vue SPA, SSE, theme API) | **Defer** | `web/`, `internal/web/api.go:69-81` |
| 15 | Full-screen TUI / 20 themes / VHS | **Defer** | `internal/tui/`, `internal/theme/` |
| 16 | `integer` vs `number` distinction | **Drop** as two types; accept both on import | `runbook.go:14-15`; catalog uses `number`×1 |
| 17 | MCP watcher, DecryptingVarResolver, stub progress | **Drop** | unwired / no-op |
| 18 | Windows PowerShell path | **Defer** | `cmd/init.go:35-68` |
| 19 | `dops://history` | **Drop** as specified; add a real history **tool** if needed | spec unchecked |
| 20 | Demo runner / seeded demo history | **Drop** from runtime | `internal/executor/demo.go` |

---

## 9. Open questions for the PRD

1. **MCP tool shape.** Replace one-tool-per-runbook with a small dispatcher (`list`, `get`, `run`, maybe `history`)? Resource-only catalog + one `run`? Dynamic tools after a `select_catalog` call? The token table in §7 is the constraint.

2. **Format versioning.** Import today’s YAML 1:1, or define `runbook.yaml` v2 (collapse `number`/`integer`, optional catalog-level `parameters:` include so 29 copies of `jenkins_*` become one)? Include-files would shrink both git and MCP if tools stay per-runbook.

3. **Catalog identity.** Canonical name `src` vs `jenkins-pipelines` vs git basename. Renames break vault keys and history IDs. Need a stable ID separate from display name (already partly there via `display_name`).

4. **Confirm protocol.** One rule for TUI/CLI/web/MCP: type ID, type `CONFIRM`, or a capability/approval gate? Sesami `ses-deploy` is critical and production-impacting (`src/ses-deploy/runbook.yaml:3-4`).

5. **Default risk ceiling.** Keep default `medium` (hides high/critical until policy is set) or default `critical` with confirms? Current MCP `--allow-risk critical` is undermined by `LoadAll` using `defaults.max_risk_level`.

6. **Vault portability.** Age X25519 per-machine keys (`~/.dops/keys/keys.txt`) are not syncable. Is that desired for dops-next, or should vault wrap a user-held passphrase / 1Password / OS keyring?

7. **Script contract.** Stay on “dops execs a file with env vars,” or allow catalog-relative shared binaries (`scripts/trigger-pipeline.sh`) as a first-class `script:` path? Today sharing is a convention inside `script.sh`.

8. **History policy.** Implement the spec (7/90 day, 50MB) or the code (10MB archives, no TTL)? Retention matters for audit of Jenkins triggers.

9. **Skills.** First-class MCP prompts, resources, or out of v1? No current catalog uses them.

10. **Web/TUI in v1.** Engine + CLI + MCP only, or UI in the first cut? Four-interfaces-one-engine is a keep; shipping four interfaces on day one is not.

11. **Compatibility tests.** Should dops-next run the 32 Sesami runbooks (dry-run / schema round-trip) as the acceptance suite? Recommend yes; do not execute Jenkins in CI.

12. **Loader strictness.** Enforce `name` == directory, validate types/`options` at load, fail closed — or preserve today’s “unmarshal and hope”? Strict load would catch the script-path footgun (`rb.Name` vs dirname).

13. **Secret parameter vs secret type.** Keep `secret: true` on any type (current), or a `type: secret`? Catalog uses the flag. PRD should not introduce a type that existing YAML lacks.

14. **Inactive catalogs and multi-catalog MCP.** Multiple catalogs concatenate into one tool list today. With a dispatcher, is the catalog name a required `run` argument?

---

## 10. Claims this audit does not make

- No runtime MCP session was captured; token numbers are schema-replay, not a wire trace of `go-sdk` framing (JSON-RPC envelope adds tens of bytes, not thousands).
- Origin HEAD is `v0.13.1`; behavior described is v0.12.0 feature-complete unless noted.
- Catalog git remotes and `jenkins_token` values were not read.
- Web UI and TUI were not exercised; they are cited from source for keep/defer only.
)