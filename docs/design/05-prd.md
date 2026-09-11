# dops-next PRD and architecture

**Date:** 2026-09-11
**Status:** design (phase 5)
**Codename:** dops-next. This document does not propose a product name. The binary and crate names stay `dops` until the naming phase (`docs/design/04-naming.md`) lands.

This is the product and architecture contract for the Rust rewrite. It obeys `docs/design/03-principles.md`. Every numbered decision cites `01-audit.md`, `02-competitors.md`, or `03-principles.md`. Session-mining rows also cite `06-session-mining.md`.

**Inputs (read-only):**

- `docs/design/01-audit.md` — Go product at `~/origin/dops` `795d2d2` (tag `v0.13.1`, feature-complete at v0.12.0)
- `docs/design/02-competitors.md` — adjacent MCP / task-runner / skills products
- `docs/design/03-principles.md` — charter and non-goals
- `docs/design/06-session-mining.md` — pipeline, redaction, review gate (engine packaging in `06` §5 is Go `cmd/` + `internal/`; **ignore that layout** — dops-next is Rust)
- Reference catalog `~/Bitbucket/sdo-dops-catalog` — 32 `src/*/runbook.yaml` (disk count 2026-09-11)
- crates.io versions and licenses, retrieved 2026-09-11 (see §10). Anything not retrieved is marked **unverified**.

No secrets appear in this document. The Sesami catalog secret is named `jenkins_token` only.

---

## 1. Product summary, users, and success metrics

### 1.1 What it is

dops-next is a **script library** that is also an **MCP server for AI agents**. The job is to cut model tokens by preferring reviewed POSIX scripts over free-form reasoning (`03` charter 8–9; `02` §7.3).

One binary exposes three interfaces over one engine (`03` charter 13; `01` §2.1):

| Interface | Entry | Who |
|---|---|---|
| TUI | `dops` (no args) | DevOps operators |
| CLI | `dops run`, `dops list`, `dops info`, `dops catalog`, `dops mine`, … | Operators, scripts, CI, scheduled mining |
| MCP | `dops mcp serve` (stdio default) | Mason's agents (Claude Code, Cursor, Codex, Grok, …) |

It is not a general agent harness, not a web app, and not a hosted control plane (`03` non-goals).

The migratable unit is today's catalog: a directory of `runbook.yaml` + `script.sh`, parameters as `UPPER_SNAKE` env vars, four risk words, age-encrypted vault, git-installable catalogs (`01` §2.2–2.6). The rewrite **ports that unit** and **does not port** one-MCP-tool-per-runbook or agent-default-`critical` (`03` scoreboard; `01` §2.9, §8 rank 7).

### 1.2 Users

**Mason's agents.** Local stdio MCP clients. They list, describe, and run runbooks. They may draft new runbooks. They never raise their own risk ceiling, never see secret values, and never accept their own proposals (`03` §10–11).

**DevOps operators.** Humans at a keyboard (TUI) or in a shell (CLI). They install team catalogs (Sesami's 32 Jenkins-trigger runbooks first), save `jenkins_url` / `jenkins_user` / `jenkins_token` once in the vault, raise ceilings, grant specific high/critical ids to agents, and accept proposed or **mined** runbooks (`06` §2.9).

Same engine, same catalogs, same vault, same risk policy. A fourth interface (Vue web UI) is deferred and is not a v1 driver (`01` §3.2, §9 Q10; `03` non-goals).

### 1.3 Success metrics (numbers)

Token figures for *today* are schema-replay, not a live MCP wire trace (`01` §6, §10). Targets below are product constraints (`03` §8 rule 6).

| Metric | Today (Sesami 32-runbook catalog) | dops-next target | Why |
|---|---|---|---|
| **Tokens per connect** (`tools/list` + `resources/list` + `prompts/list`) | ~8 800 (`01` §6.1: 33 940-byte `tools/list` ≈ 8 500 tokens @ 4 chars) | **≤ 800 tokens**, stretch **≤ 500** | Four constant meta-tools. Audit's 4-tool dispatcher was 917 bytes ≈ 229 tokens (`01` §6.2 item 5). Budget includes real descriptions. GitHub MCP's 93-tool ~55k dump is the failure mode (`02` §5, §7.1). |
| **Tokens per run** (tool result body) | Last 50 lines, unstructured-ish JSON (`01` §4.11, `internal/mcp/tools.go`) | **≤ 1 500 tokens** typical (last **50** lines + exit code + duration_ms + log_path + history_id). Hard cap **8 192** output UTF-8 bytes before truncation notice | Server-side truncate; full log on disk (`02` §7.4; `03` §8 rule 4). |
| **Time to first runbook after install** | curl installer, then `dops init`, then empty-or-hello-world (`03` §3 dops-today) | **≤ 60 s** wall clock from `curl \| sh` to `dops` showing the starter catalog on a warm network. **≤ 10 s** from a completed install to first TUI frame / first `list_runbooks` on a local SSD | Install = ready (`03` §3). Empty catalog is a product bug (`03` §9 rule 6). |
| **Connect cost vs catalog size** | Linear: 32 runbooks ~8.5k tokens; SPEC's 370 pipelines extrapolate ~98k (`01` §6.2 item 4) | **O(1)** in runbook count. 32 and 370 pay the same `tools/list` | Meta-tools (`02` §1.5, mise MCP in `02` §2.3). |
| **Sesami import** | n/a | All **32** `runbook.yaml` files load; `dops info` / `describe_runbook` round-trip; `dops run --dry-run` resolves env without calling Jenkins | `01` §9 Q11. |

A change that grows the default `tools/list` past 800 tokens is a principles violation, not a feature (`03` §8 rule 6).

---

## 2. Decision table (01 §9 plus mining)

Each row is a closed decision. One-line reason plus citations. Rows 1–14 close `01` §9. Row 15 closes how `06` §5.3 fits the `03` §8 tool budget.

| # | Question | Decision | Reason | Cite |
|---|---|---|---|---|
| 1 | MCP tool shape | **Four meta-tools only:** `list_runbooks`, `describe_runbook`, `run_runbook`, `propose_runbook`. No tool per runbook. No resource-only catalog. No `select_catalog` that then registers tools. No history tool. **No `mine_*` tools** (decision 15). | Constant-size surface is ~37× cheaper than 32 eager schemas and stays flat at 370 pipelines; a second four-tool mine surface would double connect cost. | `01` §6.2, §9 Q1; `02` §7.1, §7.8, mise in `02` §2.3; `03` §8 rules 1–2, convention table |
| 2 | Format versioning | **`runbook.yaml` v2** with a **v1 compatibility loader**. v1 files (no `format_version`) load without rewrite. v2 adds optional `format_version: 2`, omittable `script:` (defaults to `script.sh`), omittable `name:` (defaults to directory), and optional catalog-root `catalog.yaml` shared `parameters:`. `integer` and `number` both import; v2 authors write `integer` (any whole) or `float`. | Must ingest 32 Sesami files as-is; catalog-level params kill 29 copies of `jenkins_*` for *new* catalogs without breaking old ones. | `01` §4.2–4.3, §7, §9 Q2; `03` §7 rules 1–3 |
| 3 | Catalog identity | **`name` is the stable id.** `display_name` is cosmetic. Recommended Sesami register: `--name jenkins-pipelines` with `sub_path = "src"` (matches SPEC.md comment, not `filepath.Base` → `src`). Renames are operator-explicit and break vault keys / history ids by design. | IDs are `catalog.runbook`; silent rename is data loss. | `01` §2.6, §4.9, §9 Q3; `03` §7 rule 2 |
| 4 | Confirm protocol | **One human-attest rule, three faces.** low/medium: none. high: human must affirm (TUI: y/N, default No; CLI: `--confirm <id>`). critical: human must type the runbook id (TUI input; CLI `--confirm <id>`). **MCP: no confirm strings in schemas.** high/critical from an agent without a prior grant → `pending_grant`, does not run. | Schema `_confirm_id` / `_confirm_word` is copyable theater; Sesami `ses-deploy` is production-impacting. | `01` §4.6, §9 Q4; `02` §7.7; `03` §10 rules 2–3, §11 rule 4 |
| 5 | Default risk ceiling | **Human `max_risk_level = "medium"`.** **Agent `allow_risk = "low"`.** `--allow-risk` on `mcp serve` cannot exceed config. Effective MCP ceiling is `min(agent, catalog policy, human global)`. High/critical hidden at list/describe/run for that interface. | Today's MCP default `critical` is inverted omakase-safe and is undermined by `LoadAll` at medium. | `01` §4.6, §9 Q5; `03` §1 rule 4, §10 rules 1 and 3 |
| 6 | Vault portability | **Age X25519 identity, local-first (default).** Identity file `0600` under the data dir. **Optional** OS keyring wrap of that identity (`keyring` crate). **Optional** passphrase-wrapped identity. Import Go `vault.json` v1 when present. No required 1Password / cloud KMS. | Own the machine; secrets never in config; per-machine keys are the safe default; keyring is the portable overlay. | `01` §2.4, §4.7, §9 Q6; `02` §7.11; `03` §6 rules 4–5, charter 12 |
| 7 | Script contract | **Exec a file with env vars.** `script:` is a path relative to the runbook directory (default `script.sh`). Shared files **outside** runbook dirs (e.g. `scripts/trigger-pipeline.sh`) are first-class as *catalog tree files*, not as a second `script:` URI scheme. Loader must not assume a two-file directory is the whole program. POSIX `/bin/sh`. No `run_shell` tool. | 29 Sesami wrappers `dirname "$0"` up to repo `scripts/`; rewriting 32 scripts is out of scope. | `01` §2.2, §7, §9 Q7; `03` §1 rule 5, §7 rule 6, §9 rules 1–2 |
| 8 | History policy | **Implement the documented policy, not the ignored size-cap.** JSON records + 10 MB gzip tar log archives. **90-day TTL** and **50 MB total cap** (oldest records+archives first). Secrets masked `****`. `interface` is `tui` \| `cli` \| `mcp`. | Audit of Jenkins triggers needs retention; v0.12 spec and code currently disagree. | `01` §2.8, §4.10, §9 Q8; `03` §11 rule 5 |
| 9 | Skills | **Parse `type: skill` so load does not break.** Do **not** register skill prompts or dump skill bodies on the default MCP surface in MVP. Product later. | Zero `skill.md` in origin catalogs or sdo-dops-catalog; registering all skills violates lazy MCP. | `01` §3.2, §4.8, §9 Q9; `02` §4.1; `03` §8 rule 3 |
| 10 | Web / TUI in v1 | **CLI + MCP ship first** (slices 1–5). **TUI is in MVP** (slice 8) on the same engine. **Web UI is a non-goal.** No `dops open`. | Charter is three interfaces; four on day one is how today's MCP/TUI confirm drifted. Vue is extra surface. | `01` §3.1–3.2, §9 Q10; `03` charter 13, non-goals |
| 11 | Compatibility tests | **Yes.** 32 Sesami YAML files are the acceptance suite: parse, strict-load, schema round-trip, `describe`, `run --dry-run` env map. **Do not execute Jenkins in CI.** Optional live dry-run behind `DOPS_TEST_CATALOG`. | The catalog is the compatibility target (`01` §1). | `01` §7, §9 Q11 |
| 12 | Loader strictness | **Fail closed.** `name` (after defaulting) **must** equal the directory. `risk_level` required and valid. `select` / `multi_select` require `options`. `script` file must exist for non-skill entries. Unknown `type` is a load error (v1 `number`/`integer`/`file_path`/`resource_id` are known aliases — §4.3). YAML that does not unmarshal is a load error. | Today's "unmarshal and hope" makes `name` ≠ dirname a silent wrong script path. | `01` §4.1–4.2, §9 Q12; `03` §7 rule 1 |
| 13 | Secret parameter vs type | **Keep `secret: true` on any type.** Do not add `type: secret`. | Catalog uses the flag (e.g. `jenkins_token`); a new type would not exist in today's YAML. | `01` §2.4, §9 Q13; `03` §10 rule 4 |
| 14 | Inactive catalogs and multi-catalog MCP | Inactive catalogs are skipped for **execution**. Addressing is the **id** `catalog.runbook` (or an alias). `list_runbooks.catalog` is an optional filter. `run_runbook` / `describe_runbook` take `id`, not a separate required catalog argument. **Staging exception (decision 15):** catalog `mined` (and `proposed`) may be listed/described when `include_staging=true` or `catalog=mined`; `run_runbook` still refuses them. | Dispatcher still needs a stable id; mined drafts must be reviewable without becoming executable tools. | `01` §2.6–2.7, §9 Q14; `03` §7 rule 2; `06` §2.9, §5.3 |
| 15 | Session mining MCP (`mine_list` / `mine_get` / `mine_run` / `mine_review`) | **Do not add those tools, resources, or prompts.** Engine is crate `dops-mine` + CLI `dops mine` (not Go `cmd/mine.go`). Map: `mine_list` → `list_runbooks` (`catalog=mined` or `include_staging=true`); `mine_get` → `describe_runbook` on `mined.<slug>`; `mine_run` → **CLI/LaunchAgent only** (`dops mine run --once`); `mine_review` approve → human `dops mine approve` / `dops catalog accept`; reject/skip → human `dops mine reject\|skip`. No `dops://mine/*` on `resources/list`. No MCP approve. | Four extra tools would break the ≤4 budget; inactive `mined` catalog is already the review gate; schema `_confirm_id` on mine_review is the same theater decision 4 rejected. | `06` §5 (ignore Go layout), §5.3, §2.9; `03` §8 rules 1 and 6, §11 rules 1–2; `02` §7.13 |

---

## 3. Rust workspace layout

Edition 2024. **MSRV 1.88** (required by `rmcp` 3.3.0). One binary named `dops`. Workspace at repo root.

```
dops-next/
  Cargo.toml                 # workspace
  crates/
    dops/                    # bin: clap CLI, wires TUI + MCP
    dops-core/               # domain, config, catalog loader, vault, vars, history, risk
    dops-exec/               # process group, POSIX sh, env injection, dry-run
    dops-mcp/                # rmcp server, 4 tools, result truncation
    dops-tui/                # ratatui app
    dops-mine/               # session mining engine (ingest/parse/cluster/redact/propose)
  catalogs/starter/          # embedded at compile time (also present as files for humans)
  docs/design/
  tests/fixtures/            # v1 YAML, v2 YAML, Sesami-shaped stubs (no tokens)
```

| Crate | Responsibility | Depends on |
|---|---|---|
| `dops-core` | `Runbook`, `Parameter`, `RiskLevel`, XDG paths, TOML config, catalog registry, v1/v2 loader, vault, 3-layer var merge, history store | serde, toml, serde-yaml-ng, age, directories, uuid, thiserror, zeroize |
| `dops-exec` | `Runner::run(ctx, script_path, env)`, cwd = runbook dir, parent env + `UPPER_SNAKE`, cancel via process group | dops-core, tokio |
| `dops-mcp` | stdio (default) + loopback HTTP (opt-in); the four tools; no resources/prompts on the default list | dops-core, dops-exec, rmcp, tokio, serde_json |
| `dops-tui` | keyboard-first catalog / wizard / confirm / output / palette / `?` | dops-core, dops-exec, ratatui, crossterm |
| `dops-mine` | index.jsonl ingest, per-agent parsers, normalize/cluster/rank/redact/propose; **no MCP types** | dops-core, serde_json, regex, sha2, tokio |
| `dops` | `main`, clap command tree including `dops mine`, rust-embed starter, install-time version | all of the above, clap, clap_complete, rust-embed |

**Why split this way:** one engine (`dops-core` + `dops-exec`) shared by CLI, TUI, and MCP so confirm/risk/vault cannot drift again (`01` §2.1). Mining is a local batch job (`06` §3) that writes drafts; it must not sit inside `dops-mcp`. Ignore `06` §5.1 Go `cmd/mine.go` + `internal/mine/` — that layout is not this repo.

### 3.1 Key dependencies (crates.io 2026-09-11)

Versions are **max stable on crates.io on 2026-09-11**. Pin in `Cargo.toml` with `^` of the major.minor recorded here; bump only with a design note if a major moves.

| Crate | Version | License | Why |
|---|---|---|---|
| **clap** | 4.6.6 | MIT OR Apache-2.0 | CLI tree, derive, completions. Standard, permissive. |
| **clap_complete** | 4.6.9 | MIT OR Apache-2.0 | Shell completions. |
| **ratatui** | 0.30.2 | MIT | TUI. Replaces Bubble Tea / Lip Gloss. |
| **crossterm** | 0.29.0 | MIT | ratatui default backend; keyboard + terminal. |
| **rmcp** | 3.3.0 | Apache-2.0 | **Official** Rust MCP SDK (`github.com/modelcontextprotocol/rust-sdk`). Features: `server`, `transport-io` (stdio), `transport-streamable-http-server` (opt-in HTTP). |
| **rmcp-macros** | 3.3.0 | Apache-2.0 | Tool impl macros; same repo. |
| **tokio** | 1.53.1 | MIT | Async runtime required by rmcp. |
| **serde** | 1.0.229 | MIT OR Apache-2.0 | Serde. |
| **serde_json** | 1.0.151 | MIT OR Apache-2.0 | MCP JSON + history records. |
| **serde-yaml-ng** | 0.10.0 | MIT | YAML for `runbook.yaml`. **Not** `serde_yaml` 0.9.34+deprecated. **Not** `serde_yml` 0.0.13 (unmaintained shim). |
| **toml** | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 | Config parse. |
| **toml_edit** | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 | Comment-preserving `dops config set`. |
| **age** | 0.12.1 | MIT OR Apache-2.0 | Vault. crates.io description still says **[BETA]**; it is the Rust port of the same age (X25519 + ChaCha20-Poly1305) Go uses (`01` §4.7). Accept the beta label; do not invent a second envelope. |
| **directories** | 6.0.0 | MIT OR Apache-2.0 | XDG paths. |
| **thiserror** | 2.0.20 | MIT OR Apache-2.0 | Typed errors in libraries. |
| **uuid** | 1.26.1 | Apache-2.0 OR MIT | Execution / pending / history ids (v4). |
| **schemars** | 1.2.2 | MIT | JSON Schema for the four tool input types (rmcp `server` already pulls this). |
| **rust-embed** | 8.12.0 | MIT | Embed starter catalog + bundled themes. |
| **zeroize** | 1.9.0 | Apache-2.0 OR MIT | Wipe decrypted vault buffers. |
| **keyring** | 4.2.0 | MIT OR Apache-2.0 | **Optional** feature `keyring`: store age-identity passphrase in the OS keyring. |
| **tracing** / **tracing-subscriber** | 0.1.44 / 0.3.23 | MIT | Structured logs to stderr (MCP must not write on stdout). |
| **camino** | 1.2.5 | MIT OR Apache-2.0 | UTF-8 paths. |
| **fs-err** | 3.3.1 | MIT OR Apache-2.0 | IO errors with paths. |
| **sha2** | 0.11.0 | MIT OR Apache-2.0 | Script digest in `describe_runbook`. |
| **humantime** | 2.4.0 | MIT OR Apache-2.0 | Duration display. |
| **regex** | 1.13.1 | MIT OR Apache-2.0 | Mining redaction rules (`06` §4.2). Already listed in the 2026-09-11 crates.io pull. |

**Dev / test:** `assert_cmd` 2.2.2, `predicates` 3.1.4, `tempfile` 3.27.0, `insta` 1.48.0 (snapshot `tools/list` payload size), `proptest` 1.11.0 — all MIT OR Apache-2.0 except insta (Apache-2.0).

**Explicitly not used**

| Crate | Why not |
|---|---|
| `serde_yaml` 0.9.34+deprecated | Deprecated on crates.io. |
| `git2` / `gix` | Catalog install shells out to `git` (same as today, `01` §4.9). No libgit2 in the binary. |
| `axum` / `hyper` as first-party HTTP | rmcp's streamable HTTP feature is enough for opt-in loopback. |
| Any GPL-3.0 runner (e.g. taking mcp-shell as a library) | Copyleft is a product constraint (`02` §7.15). |

**Unverified:** whether `age` 0.12.1's on-disk identity format is byte-compatible with Go `filippo.io/age` keys in `~/.dops/keys/keys.txt`. MVP imports Go `vault.json` **if** the identity file converts; otherwise operators re-save globals. Marked as a slice-4 test, not a guess.

---

## 4. Catalog format

### 4.1 On-disk layout (unchanged unit)

```
<catalog-root>/                    # Catalog.RunbookRoot() = path + optional sub_path
  catalog.yaml                     # optional; v2 shared params / catalog metadata
  <entry-dir>/                     # directory name = runbook name
    runbook.yaml
    script.sh                      # default; or whatever `script:` names
    skill.md                       # only when type: skill (parsed, not executed)
  scripts/                         # optional extra files; not scanned as runbooks
```

Loader scans **immediate subdirectories** of each catalog root that contain `runbook.yaml`. Nested catalogs are not supported (`01` §4.1). Extra files (Sesami `scripts/trigger-pipeline.sh`, `device-log-metrics/lib/`, tests) are preserved and not loaded as runbooks (`01` §7; decision 7).

**Active vs staging.** Active catalogs are listed, described, and runnable (subject to risk). Inactive catalogs are skipped for execution (`01` §2.6). The **staging** catalogs `mined` and `proposed` are inactive by default and are the only inactive catalogs MCP may *list/describe* (decision 15). They are never runnable until a human `dops catalog accept` copies a runbook into an active catalog, or the operator sets `active = true` on `mined` after reading the scripts (`06` §2.9–2.10).

### 4.2 Identity

- Catalog **`name`**: `[a-z0-9][a-z0-9-]*` recommended; stored in config; **stable**.
- Runbook **id**: `<catalog>.<dirname>` if YAML `id` omitted. If YAML `id` is set, it must equal that.
- **Aliases:** optional list, lowercase alnum / hyphen / dot, unique across loaded catalogs; first-loaded wins with a warning (`01` §2.7). CLI/MCP resolve id then alias. Sidebar shows `name`.
- **Display name:** catalog-level, max 50 printable chars, never used in vault keys or history ids (`01` §4.5, §4.9).

Sesami import (recommended):

```sh
dops catalog add --name jenkins-pipelines --path src ~/Bitbucket/sdo-dops-catalog
```

IDs become `jenkins-pipelines.cc4-aaa`, not `src.cc4-aaa`. Operators who already registered as `src` keep `src.*` until they choose to re-add (`01` §4.9, decision 3). `sub_path` is how a monorepo installs without flattening.

### 4.3 `runbook.yaml` v1 compatibility rule

A file **without** `format_version` is v1. The v1→internal mapping:

| v1 field | v2 internal | Rule |
|---|---|---|
| missing `format_version` | treat as 1 | Load; do not rewrite the file |
| `name` missing | directory name | Then enforce name == dirname |
| `name` present ≠ dirname | **load error** | Fixes the script-path footgun (`01` §4.1) |
| `script` missing | `script.sh` | v1 files in Sesami all set `script: script.sh` |
| `script` present | as written, relative to runbook dir | Must exist on disk |
| `type` empty / `runbook` | executable | |
| `type: skill` | parse, skip exec, require `skill.md` or warn-and-skip | Decision 9 |
| `id` empty | `<catalog>.<dirname>` | |
| `risk_level` empty / unknown | **load error** | Fail closed (decision 12). Today's `Exceeds` treated unknown as rank 0 (`01` §4.2) — do not copy that. |
| `type: number` | integer, minimum 0 | Sesami `ses-argocd-sync.timeout` (`01` §4.3) |
| `type: integer` | integer (negative ok) | Unused in Sesami; keep |
| `type: file_path` / `resource_id` | string + original type recorded | Unused in Sesami; accept |
| `secret: true` | flag | Never a type (decision 13) |
| `scope` empty | save=local; resolve still global < catalog < runbook < input | `01` §4.4 |
| unknown keys | **load error** | Strict. |

v1 files are **not** rewritten on load. `dops catalog migrate <name>` (slice 7, optional) can emit v2 + `catalog.yaml` shared params; it is not required to run Sesami.

### 4.4 `runbook.yaml` v2

```yaml
format_version: 2          # required for v2
name: disk-usage           # optional; defaults to directory
version: "1.0.0"           # optional opaque string
description: Show disk usage for a path
risk_level: low            # required: low | medium | high | critical
aliases: [du]
# script omitted → script.sh
parameters:
  - name: path
    type: string           # string | boolean | integer | float | select | multi_select
    required: true
    scope: local           # local | global | catalog | runbook
    secret: false
    default: "."
    description: Path to measure
    options: []            # required when type is select or multi_select
```

Author-facing types in v2: **six** (`string`, `boolean`, `integer`, `float`, `select`, `multi_select`). Import aliases `number`, `file_path`, `resource_id` exist only on the v1 path (`01` §3.1 "integer vs number", `03` §7 "too many knobs").

`script:` if present must be a relative path **without** `..` escape from the runbook dir (reject absolute paths and `..`). Shared helpers live beside the catalog and are invoked from `script.sh` via `dirname "$0"` — that is the Sesami pattern and it keeps the exec contract one file (`01` §7).

### 4.5 Optional `catalog.yaml` (v2 only)

At `<catalog-root>/catalog.yaml`:

```yaml
format_version: 2
parameters:
  - name: jenkins_url
    type: string
    required: true
    scope: global
    default: "https://ci.sesami.io"
    secret: false
    description: Jenkins server URL
  - name: jenkins_user
    type: string
    required: true
    scope: global
    secret: false
    description: Jenkins username or service account ID
  - name: jenkins_token
    type: string
    required: true
    scope: global
    secret: true
    description: Jenkins API token
```

Merge: catalog.yaml parameters, then runbook parameters of the same `name` **override**. This is how 29 Jenkins wrappers become one shared block **when an operator migrates**. Until then, 29 copies still load (`01` §2.5, §7).

`catalog.yaml` does not change the catalog's stable `name` (that lives in config).

### 4.6 Parameter resolution (carry as-is)

Order: vault global < vault catalog < vault runbook < CLI `--param` / TUI input / MCP `args` (`01` §2.5, §4.4). Then **filter to declared parameter names** so extra vault keys do not leak into the child env. Env var is `name.to_ascii_uppercase()`.

Scopes (`01` §4.4):

| Scope | Saved? | Vault key |
|---|---|---|
| `global` | yes | `global.<param>` |
| `catalog` | yes | `catalog.<cat>.<param>` |
| `runbook` | yes | `catalog.<cat>.runbooks.<rb>.<param>` |
| `local` | no | — |

Sesami: 87 `jenkins_*` global declarations, 146 runbook-scoped Jenkins toggles, 1 local (`ses-argocd-sync.app_name`). The catalog is unusable without global scope (`01` §2.5).

### 4.7 Loader strictness (decision 12)

A catalog load **fails** (CLI/TUI/MCP all refuse to start with that catalog active) if any **active** runbook in it fails the checks in §4.3. Inactive non-staging catalogs are skipped entirely (`01` §2.6). Staging load errors fail that draft only (drop + audit), not the whole server. Warnings (non-fatal): alias collisions, skill missing `skill.md`.

Starter catalog is always loaded from embed and is guaranteed valid at compile time (tests).

---

## 5. MCP surface

### 5.1 Default surface

`tools/list` returns **exactly these four tools**, in this order. No other tools. No per-runbook tools, including as an opt-in — the charter closed that list (`03` convention table; `02` §7.1 said opt-in for tiny catalogs, **`03` wins**). `06` §5.3's `mine_list` / `mine_get` / `mine_run` / `mine_review` are **not** registered (decision 15).

`resources/list` is **empty**. Catalog JSON, schema markdown, and `dops://mine/*` are not registered (`01` §5.2; `02` §7.11; `03` §8; `06` §5.3 rejected as a connect-time dump). Agents that need a schema call `describe_runbook`.

`prompts/list` is **empty**. `create-runbook` becomes the `propose_runbook` **tool**. `review-mined-runbook` is not a prompt (`06` §5.3). Skills are not prompts (`01` §9 Q9; `03` §8 rule 3). The session-mining **skill** is a `SKILL.md` that execs `dops mine`, not an MCP prompt (`06` §5.2).

Server name: `dops`. Version: ldflags / `CARGO_PKG_VERSION`. Transport: **stdio default**. HTTP is `--transport http --bind 127.0.0.1:8808` (loopback only; refuse `0.0.0.0`) (`03` §1 rule 6, §10 rule 5).

### 5.2 Progressive disclosure

1. **Connect:** four tool schemas (budget ≤ 800 tokens).
2. **`list_runbooks`:** id, name, catalog, one-line description, risk_level, aliases. Filterable. No parameter schemas (`03` §8 rules 2 and 5; Agent Skills layer 1, `02` §7.2).
3. **`describe_runbook`:** full parameter JSON Schema (secrets omitted, names listed), script path, sha256, script body (default on so agents can read before run — `03` §9 rule 3). This is layer 2.
4. **`run_runbook`:** execute; result is last N lines + metadata. Script source is **not** in the result (`02` §7.2 layer 3; `03` §8 rule 4).

Eager-loading clients (Cursor without tool search, naive CI) only ever see four schemas (`02` §7.8). There is nothing to defer.

### 5.3 Multi-catalog addressing

`id` is `catalog.runbook` or an alias. Optional `catalog` filter on list. Inactive catalogs absent from list **except** staging (`mined`, `proposed`) when `include_staging=true` or `catalog` is that name (decision 14–15). A second catalog does not add tools (`01` §9 Q14). `run_runbook` on a staging id returns `error=staging` and does not execute.

### 5.4 Tool input schemas (JSON Schema draft 2020-12)

These objects are the `inputSchema` of each tool. `additionalProperties` is false on the root of every tool.

#### `list_runbooks`

Description: `List runbooks the current agent ceiling can see. Returns names and one-line descriptions only.`

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "dops://schema/tools/list_runbooks",
  "title": "list_runbooks",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "query": {
      "type": "string",
      "description": "Case-insensitive substring match against id, name, aliases, and description."
    },
    "catalog": {
      "type": "string",
      "description": "Canonical catalog name. When set, only that catalog is listed."
    },
    "risk": {
      "type": "string",
      "enum": ["low", "medium", "high", "critical"],
      "description": "Exact risk_level filter. Cannot reveal runbooks above the agent ceiling."
    },
    "limit": {
      "type": "integer",
      "minimum": 1,
      "maximum": 200,
      "default": 50,
      "description": "Maximum entries to return."
    },
    "offset": {
      "type": "integer",
      "minimum": 0,
      "default": 0
    },
    "include_staging": {
      "type": "boolean",
      "default": false,
      "description": "When true, also list inactive staging catalogs (mined, proposed). Staging ids are not executable. Equivalent to catalog=mined for the mine_list use case (06 §5.3) without a fifth tool."
    }
  }
}
```

#### `describe_runbook`

Description: `Return the full parameter schema, risk, and script for one runbook id. Secret parameter values are never included.`

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "dops://schema/tools/describe_runbook",
  "title": "describe_runbook",
  "type": "object",
  "additionalProperties": false,
  "required": ["id"],
  "properties": {
    "id": {
      "type": "string",
      "minLength": 1,
      "description": "Runbook id (catalog.runbook) or a unique alias."
    },
    "include_script": {
      "type": "boolean",
      "default": true,
      "description": "When true (default), include the POSIX script body so the agent can read it before run_runbook."
    }
  }
}
```

#### `run_runbook`

Description: `Execute one runbook by id with a parameter map. Secret parameters are injected from the local vault and must be omitted from args. High/critical without a human grant does not run; it returns status pending_grant.`

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "dops://schema/tools/run_runbook",
  "title": "run_runbook",
  "type": "object",
  "additionalProperties": false,
  "required": ["id"],
  "properties": {
    "id": {
      "type": "string",
      "minLength": 1,
      "description": "Runbook id (catalog.runbook) or a unique alias."
    },
    "args": {
      "type": "object",
      "default": {},
      "additionalProperties": true,
      "description": "Map of parameter name to value. Names must be declared on the runbook. Secret names are rejected if present. Do not send _confirm_id or _confirm_word; those fields do not exist."
    },
    "dry_run": {
      "type": "boolean",
      "default": false,
      "description": "When true, resolve env and return the would-be command without executing."
    }
  }
}
```

There are **no** `_confirm_id` or `_confirm_word` properties (`03` §10 rule 2; `01` §4.6). Wrong args: `status=error`, `error=invalid_args`, plus the parameter schema from describe (so the model can retry without a round-trip to describe if it skipped it).

#### `propose_runbook`

Description: `Write a draft runbook (yaml + script) under the proposed/ state dir and return a diff. Does not register, accept, or execute. A human runs dops catalog accept.`

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "dops://schema/tools/propose_runbook",
  "title": "propose_runbook",
  "type": "object",
  "additionalProperties": false,
  "required": ["catalog", "name", "description"],
  "properties": {
    "catalog": {
      "type": "string",
      "description": "Canonical catalog name that would own the runbook after accept. Use user for the default user catalog."
    },
    "name": {
      "type": "string",
      "pattern": "^[a-z0-9][a-z0-9-]*$",
      "description": "Directory name / runbook name."
    },
    "description": {
      "type": "string",
      "minLength": 1
    },
    "risk_level": {
      "type": "string",
      "enum": ["low", "medium", "high", "critical"],
      "default": "low"
    },
    "yaml": {
      "type": "string",
      "description": "Full runbook.yaml body. If omitted, the server scaffolds a v2 file from name/description/risk_level."
    },
    "script": {
      "type": "string",
      "description": "POSIX script.sh body. If omitted, the server scaffolds a set -eu stub."
    }
  }
}
```

No `accept` tool (`03` §11 rule 2).

### 5.5 Result shapes

All tool results are a **single JSON text content block** (pretty-printed, UTF-8). Secrets in vault values are redacted from `output` if they appear (`02` §7.11; Atuin `secrets_filter` lesson).

#### `list_runbooks` result

```json
{
  "runbooks": [
    {
      "id": "starter.disk-usage",
      "name": "disk-usage",
      "catalog": "starter",
      "description": "Show disk usage for a path",
      "risk_level": "low",
      "aliases": ["du"],
      "staging": false
    }
  ],
  "total": 6,
  "offset": 0,
  "limit": 50,
  "truncated": false
}
```

`truncated` is true when `offset+len < total`. Hidden (above-ceiling) runbooks are absent, not listed as denied (`03` §10 rule 3). Staging entries set `staging: true` and never appear unless `include_staging` or `catalog` names a staging catalog. List payloads still have **no scripts** (`06` §5.3 mine_list).

#### `describe_runbook` result

```json
{
  "id": "jenkins-pipelines.cc4-aaa",
  "name": "cc4-aaa",
  "catalog": "jenkins-pipelines",
  "description": "Trigger a SES/CC4/cc4-aaa branch pipeline",
  "risk_level": "medium",
  "version": "1.0.0",
  "aliases": [],
  "script_path": "/abs/path/src/cc4-aaa/script.sh",
  "script_sha256": "sha256:…",
  "script": "#!/bin/sh\nset -eu\n…",
  "parameters": [
    {
      "name": "branch",
      "type": "string",
      "required": true,
      "scope": "runbook",
      "secret": false,
      "default": "dev",
      "description": "Branch, release tag, or PR to trigger",
      "options": null,
      "json_schema": { "type": "string", "description": "Branch, release tag, or PR to trigger", "default": "dev" }
    }
  ],
  "secret_param_names": ["jenkins_token"],
  "grant_required": false,
  "visible_to_agent": true
}
```

`parameters` **omits** secret fields' values and defaults. `secret_param_names` tells the agent the vault will inject them. `json_schema` per param uses the same mapping as today's `paramToSchemaProperty` minus confirm fields (`01` §5.1, `internal/mcp/schema.go`). `include_script: false` drops `script` but keeps path and sha256.

Unknown id or above-ceiling: JSON-RPC tool error, message `no such runbook` (do not distinguish hidden vs missing — same as load-time hide). Staging ids **are** describable (redacted yaml + script) so agents can review drafts (`06` §5.3 mine_get). `describe_runbook` on staging must not include source session bodies, cwd, or raw commands (`06` §4.1).

#### `run_runbook` results

Success / failure after exec:

```json
{
  "status": "success",
  "runbook_id": "starter.disk-usage",
  "exit_code": 0,
  "duration_ms": 42,
  "output_lines": 8,
  "output": "…last ≤50 lines…",
  "truncated": false,
  "summary": "8.1G /",
  "log_path": "/Users/…/.local/state/dops/history/logs/….log.gz#<uuid>.log",
  "history_id": "<uuid>"
}
```

`status` is `success` when exit_code is 0, `failed` otherwise, `cancelled` on ctx cancel.

Pending grant (high/critical, no allow-list):

```json
{
  "status": "pending_grant",
  "runbook_id": "jenkins-pipelines.ses-deploy",
  "risk_level": "critical",
  "pending_id": "<uuid>",
  "reason": "risk_level critical exceeds agent allow_risk low; queued for a human grant"
}
```

Dry-run:

```json
{
  "status": "dry_run",
  "runbook_id": "jenkins-pipelines.cc4-aaa",
  "script_path": "/abs/path/src/cc4-aaa/script.sh",
  "env_names": ["JENKINS_URL", "JENKINS_USER", "JENKINS_TOKEN", "BRANCH", "VERSION", "SEND_EMAIL", "PUBLISH_IMAGE", "PUBLISH_API", "ALLOW_IMAGE_OVERRIDE"],
  "env_public": { "BRANCH": "dev", "VERSION": "" },
  "secret_env_names": ["JENKINS_TOKEN"]
}
```

`env_public` never includes secret values.

Invalid args:

```json
{
  "status": "error",
  "error": "invalid_args",
  "message": "unknown arg \"cmd\"; runbooks do not take a shell string",
  "expected": { "type": "object", "properties": { "…": {} }, "required": ["branch"] }
}
```

Staging (mined/proposed) — **does not run**, even with a grant (`06` §2.9, decision 15):

```json
{
  "status": "error",
  "error": "staging",
  "runbook_id": "mined.k8s-pod-logs",
  "message": "staging catalog; a human must run: dops catalog accept mined.k8s-pod-logs"
}
```

Truncation: last **50** lines (`mcp.max_output_lines` in config, default 50, max 200). If UTF-8 bytes of `output` would exceed 8192, cut to the last whole lines that fit and set `truncated: true` (`01` §4.11; `02` §7.4; `03` §8 rule 4).

#### `propose_runbook` result

```json
{
  "status": "proposed",
  "pending_id": "user.my-check",
  "path": "/Users/…/.local/state/dops/proposed/user/my-check/",
  "diff": "--- /dev/null\n+++ runbook.yaml\n…",
  "accept": "dops catalog accept user.my-check"
}
```

### 5.6 Agent config snippet (docs, not a tool)

```json
{
  "mcpServers": {
    "dops": {
      "command": "dops",
      "args": ["mcp", "serve"]
    }
  }
}
```

(`03` §3 rule 5.)

---

## 6. Script execution and safety

### 6.1 Exec contract

```
argv:  /bin/sh <abs-runbook-dir>/<script>
cwd:   <abs-runbook-dir>
env:   parent environ + declared params as NAME=value (NAME = param name, ASCII upper)
stdin: closed
stdout/stderr: merged line stream to history log + interface
cancel: context cancel → process group (Unix)
```

- No flags-to-script adapter. No JSON blob default input (`03` §7 rule 3).
- `script:` cannot escape the runbook directory (`..` rejected). Shared helpers are reached from inside `script.sh` (`01` §7; Sesami `REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"` in `src/cc4-aaa/script.sh`).
- Windows / `script.ps1` deferred (`01` §3.2; `03` non-goals).
- `dry_run` does not spawn. It returns script path + env names.
- Child gets the **parent** environment plus params (same as Go `os.Environ()` + overlay, `01` §4.11). Document that operators should not rely on leaking secrets via parent env; vault injection is the supported path.

### 6.2 Risk levels and ceilings

Order: `low < medium < high < critical`. A runbook **at** the ceiling is allowed (`01` §4.6 `Exceeds` is strict greater-than).

| Ceiling | Default | Where |
|---|---|---|
| Human global | `medium` | `config.toml` `[defaults] max_risk_level` |
| Catalog | unset → human global | `[[catalogs]] policy.max_risk_level` |
| Agent | `low` | `[agent] allow_risk` |

`dops mcp serve --allow-risk` may only **narrow**. It cannot exceed `[agent].allow_risk` or the catalog/human ceilings (`03` §10 rule 1; `01` §4.6 MCP vs LoadAll bug).

Load-time hide is **per interface**: TUI/CLI use the human/catalog ceiling; MCP uses `min(agent, catalog, human)`. `ses-release-build` (high) and `ses-deploy` (critical) are invisible to a default agent and visible to a human only after the catalog policy or global ceiling is raised (`01` §4.6, Sesami mix: medium 25, low 5, high 1, critical 1).

Starter catalog contains only `low` (`03` §10 rule 6).

### 6.3 Unified confirm protocol (decision 4)

| Level | TUI | CLI | MCP |
|---|---|---|---|
| low / medium | none | none | none |
| high | Overlay: Yes / **No** (default No). `y` accepts, `n`/`Esc` cancels | Requires `--confirm <runbook-id>` | If id ∈ `[agent].allowed_runbooks` **and** ceiling ≥ high: run. Else `pending_grant`. |
| critical | Overlay: type the runbook id | Requires `--confirm <runbook-id>` | If id ∈ `[agent].allowed_runbooks` **and** ceiling ≥ critical: run. Else `pending_grant`. |

The model cannot mint a grant. There is no confirm field in any tool schema (`03` §10 rule 2).

CLI without `--confirm` on high/critical prints the summary and exits 2 with the exact `--confirm` line to copy — that is a human, not an agent, path.

### 6.4 Human grant flow (agents)

1. Agent `run_runbook` on a high/critical id without grant → write `~/.local/state/dops/pending/<pending_id>.json` (runbook id, args with secrets stripped, requester `mcp`, timestamp). Return `pending_grant`.
2. Human sees pending in TUI (badge + palette "Pending grants") or `dops grant list`.
3. `dops grant approve <pending_id>` **one-shot executes** that pending record (still subject to human ceiling + TUI/CLI confirm for critical). `dops grant allow <runbook-id>` appends to `[agent].allowed_runbooks` (config edit, human-owned).
4. `dops grant deny <pending_id>` deletes the record.
5. MCP has no approve tool (`03` §11).

Raising `[agent].allow_risk` or `allowed_runbooks` is a **config file edit** or `dops config set` (human CLI), never an MCP tool (`03` §6 rule 6, §11 rule 3).

### 6.5 Secret handling

- Flag `secret: true` on any type (decision 13).
- Vault on-disk: age envelope, `0600`, atomic write. Payload shape compatible with Go v1 (`01` §4.7):

```json
{
  "global": { "jenkins_url": "https://ci.sesami.io", "jenkins_user": "…" },
  "catalog": {
    "<catalog-name>": {
      "runbooks": { "<runbook-name>": { "branch": "dev" } }
    }
  }
}
```

Values of `jenkins_token` are never written into this document, logs at info level, MCP schemas, `args` echoes, or history parameter maps (`01` §2.4; `03` §10 rule 4). History stores `****`.

- Default identity: age X25519 at `~/.local/share/dops/keys/identity.txt` `0600`.
- Optional `vault.keyring = true`: passphrase for that identity lives in the OS keyring (`keyring` crate, service `dops`, account `vault-identity`).
- Optional `vault.passphrase_cmd`: operator-defined command that prints a passphrase to stdout (not a 1Password hard dep).
- Import: if `~/.dops/vault.json` + `~/.dops/keys/keys.txt` exist and convert, copy into the XDG data dir once. If convert fails, start empty and tell the operator to re-save globals.

Do not copy mise `mise://env` (`02` §7.11, §8).

### 6.6 History / audit

Record fields (`01` §4.10) plus `initiator` (username if known, else `local`) and `mcp_client` (from MCP initialize `clientInfo.name` when interface is `mcp`):

`id`, `runbook_id`, `runbook_name`, `catalog_name`, `parameters` (secrets `****`), `status` (`running|success|failed|cancelled|pending_grant`), `exit_code`, `start_time`, `end_time`, `duration_ms`, `output_lines`, `output_summary`, `log_path`, `interface` (`tui|cli|mcp`).

Paths: `~/.local/state/dops/history/`. 10 MB log archives. **90-day TTL** and **50 MB** combined cap (decision 8). `dops history` lists newest-first (default 20). No `dops://history` resource (`01` §3.1).

### 6.7 Propose / accept loop

`propose_runbook` writes `~/.local/state/dops/proposed/<catalog>/<name>/{runbook.yaml,script.sh}` and returns a unified diff. It does **not** register the catalog entry and does **not** run (`03` §11 rule 1; `03` §9 rule 5).

`dops catalog accept <catalog>.<name>` validates with the strict loader, copies into `~/.config/dops/catalogs/<catalog>/` (creating a `user` catalog if needed), and registers it if missing. TUI palette: "Accept proposed runbook".

The product never `git commit`, `git push`, or `catalog install`s an agent-invented URL (`03` §11 rule 6).

### 6.8 Session mining (crate + CLI, not extra MCP tools)

Contract: `06-session-mining.md` (pipeline, redaction, bounds, review gate). **Packaging override:** `06` §5.1 assumes Go `cmd/mine.go` and `internal/mine/*.go`. dops-next implements that engine as **`crates/dops-mine`** and **`dops mine …`** on the existing clap tree. One binary (`03` charter 13). The skill (`06` §5.2) stays a `SKILL.md` that execs `dops mine`, never parses `~/Documents/Sessions` itself.

**XDG paths** (not `$DOPS_HOME/mine` as a second hidden dir — `03` §1 rule 1, §6):

| `06` path | dops-next path |
|---|---|
| `$DOPS_HOME/mine/` | `~/.local/state/dops/mine/` |
| `$DOPS_HOME/mine/queue/<fingerprint>/` | `~/.local/state/dops/mine/queue/<fingerprint>/{meta.json,runbook.yaml,script.sh}` |
| `$DOPS_HOME/catalogs/mined/` | `~/.local/share/dops/catalogs/mined/` (registered inactive) |
| `$DOPS_HOME/mine/redact-extra.txt` | `~/.config/dops/mine/redact-extra.txt` |

Pipeline, redaction ids R1–R13, rank cutoff, LaunchAgent `dev.dops.mine`, bounds (20 min / 512 MB RSS / 2 GB scan), and fail-closed secret drop are **as specified in `06` §2–4**. This PRD does not repeat the survey or the synthetic kubectl example.

**MCP mapping (decision 15)** — `06` §5.3 proposed four tools plus two resources plus a prompt. That is a second eager surface. Fold into the existing four:

| `06` §5.3 | dops-next |
|---|---|
| Resource `dops://mine/queue` | **Not registered.** `list_runbooks` with `catalog=mined` or `include_staging=true` |
| Resource `dops://mine/proposal/{fingerprint}` | **Not registered.** `describe_runbook` id `mined.<slug>` (redacted yaml+sh already on disk; no session refs) |
| Tool `mine_list` | `list_runbooks` |
| Tool `mine_get` | `describe_runbook` |
| Tool `mine_run` (`_confirm_id`, hidden unless `--allow-mine-run`) | **CLI only:** `dops mine run --once`. Not a starter runbook (starter is low-risk and must not read transcripts — `03` §10 rule 6). Skill may exec the CLI; MCP does not. |
| Tool `mine_review` (reject/skip; approve off by default, `_confirm_id`) | **Human CLI only:** `dops mine approve\|reject\|skip`. No MCP accept (`03` §11). Schema confirm strings stay forbidden (decision 4). |
| Prompt `review-mined-runbook` | **Not registered.** Operator uses `dops mine review` / `dops info mined.<slug>` |

Approve copies the draft into the inactive `mined` staging catalog (`06` §2.9). `dops catalog accept mined.<name>` (or flipping `active = true` after a human read) is what makes it executable. Miner never assigns `low` or `critical` (`06` §4.4). Drafts use v2 types (`integer` not `number`; `file_path` on the v1 import path only — `06` §2.8's `file_path`/`number` map to `string`/`integer` at write time).

`dops-mine` is not on the slice-5 critical path. Slice 9.

---

## 7. TUI / CLI UX

### 7.1 Command tree

No `dops init`. No `dops open`. Bare `dops` launches the TUI (`03` §1 rule 2).

```
dops                              # TUI
dops --help
dops version
dops list [--catalog NAME] [--query Q] [--risk LEVEL]
dops info <id>
dops run <id> [--param k=v]... [--dry-run] [--no-save] [--confirm <id>]
dops mcp serve [--transport stdio|http] [--bind 127.0.0.1:8808] [--allow-risk LEVEL]
dops catalog list
dops catalog add [--name NAME] [--path SUB] [--display-name S] [--risk LEVEL] <dir>
dops catalog install [--name NAME] [--ref REF] [--path SUB] [--risk LEVEL] [--display-name S] <git-url>
dops catalog update <name> [--ref REF] [--risk LEVEL] [--display-name S]
dops catalog remove <name>
dops catalog accept <id>
dops catalog migrate <name>       # optional v1→v2 rewrite (never default)
dops config get [key]
dops config set <key> <value>     # human CLI; rewrites TOML with comments preserved
dops history [--runbook ID] [--limit N]
dops grant list
dops grant approve <pending_id>
dops grant deny <pending_id>
dops grant allow <runbook_id>
dops mine run [--once | --watch | --since <iso>]
dops mine status
dops mine list
dops mine show <fingerprint>
dops mine review
dops mine approve <fingerprint>
dops mine reject <fingerprint> --reason …
dops mine install-schedule
dops mine install-catalog
dops completion <shell>
```

`dops list` / `dops info` are the CLI projection of the same meta-tools (`02` §7.9: file-shaped + CLI for agents that prefer CLI over MCP).

### 7.2 Keybindings (TUI)

Defaults are the product (`03` §5). Overrides: `[keys]` map in config.toml.

| Key | Action |
|---|---|
| `j` / `k` / `↑` / `↓` | Move in the focused list |
| `h` / `l` / `←` / `→` | Collapse / expand catalog (sidebar) |
| `Enter` | Run selected / submit wizard / accept confirm when valid |
| `/` | Search |
| `n` / `N` | Next / previous search match (output) |
| `g` / `G` | Top / bottom |
| `Tab` | Cycle panes |
| `?` | Help overlay |
| `q` | Quit (not during confirm type-id) |
| `Esc` | Back out / clear search / cancel confirm |
| `Ctrl+c` | Quit or cancel execution (documented in `?`) |
| `Ctrl+x` | Stop running execution |
| `Ctrl+Shift+p` | Command palette (theme, catalog, grants, help, quit) |

Wizard: `Enter` next, `Shift+Tab` prev, arrows on select, `Space` multi-select, `Esc` cancel (`03` §5 rule 5). Mouse may scroll and select text; every action has a key (`03` §5 rule 1).

### 7.3 First-run experience

After install:

1. Missing config → write defaults (empty TOML is valid; missing keys mean defaults — `03` §6 rule 5).
2. `dops` shows starter catalog in the sidebar, first runbook selected, metadata pane filled, footer key hints. Not an empty panel (`03` §4 rule 3).
3. `dops mcp serve` speaks stdio immediately.
4. No questionnaire. No "add a catalog" dead end (`03` §1 rules 2–3).

Adding Sesami is **additive**: `dops catalog add --name jenkins-pipelines --path src ~/Bitbucket/sdo-dops-catalog`. Then `dops config set` / TUI save for `jenkins_user` and `jenkins_token`.

### 7.4 Install one-liner

Canonical (`03` §3):

```sh
curl -fsSL https://<stable-install-url>/install.sh | sh
```

POSIX `#!/bin/sh`, `set -eu`, OS/arch detect, **checksum verification** (SHA-256 of the tarball against a published `SHA256SUMS`; today's `install.sh` skips this — `03` §3 dops-today). Install to `/usr/local/bin` or `DOPS_INSTALL_DIR` / `~/.local/bin`. Idempotent: updates the binary, does not clobber config, vault, or extra catalogs.

Homebrew / Nix / cargo-binstall / winget must produce the **same first-run state**. `cargo install` is not the advertised path (`03` §3 rules 2–3). Stable URL and GitHub release publishing are **gated** (repo README Gates); this PRD specifies the shape only.

### 7.5 Config file (TOML, XDG)

| Kind | Path | Override |
|---|---|---|
| User config | `~/.config/dops/config.toml` | `DOPS_HOME` replaces the config **root** for tests/containers (`03` §1 rule 1). When `DOPS_HOME` is set: `$DOPS_HOME/config.toml`, `$DOPS_HOME/share/`, `$DOPS_HOME/state/`. |
| User catalogs / user themes | `~/.config/dops/catalogs/`, `~/.config/dops/themes/` | |
| Product data (vault, keys, cloned catalogs) | `~/.local/share/dops/` | |
| State (history, pending, proposed, mine) | `~/.local/state/dops/` | |
| Starter catalog / bundled themes | embedded in the binary | |

Example `config.toml` (comments welcome; this file is the API — `03` §6):

```toml
# Missing keys mean defaults. This whole file may be empty.

theme = "doop"                    # default; not github, not rainbow

[defaults]
max_risk_level = "medium"

[agent]
allow_risk = "low"
allowed_runbooks = []             # catalog.runbook ids the agent may execute above low
# no MCP config-setter

[mcp]
max_output_lines = 50

[mine]
catalog = "mined"                 # staging catalog name (06 open Q3)
# index defaults to ~/Documents/Sessions/index.jsonl

[vault]
keyring = false

[keys]
# optional overrides; unspecified keys keep product defaults
# run = "enter"

[[catalogs]]
name = "jenkins-pipelines"
display_name = "Jenkins pipelines"
path = "/Users/mason/Bitbucket/sdo-dops-catalog"
sub_path = "src"
active = true
# url = "git@bitbucket.org:sesamiio/sdo-dops-catalog.git"
[catalogs.policy]
max_risk_level = "medium"         # raise to critical to even *see* ses-deploy
```

Starter is **not** a `[[catalogs]]` row; it is always registered as catalog name `starter` from embed.

Config mode `0600`. Vault `0600`. No parameter values in TOML (`03` §6 rule 4).

### 7.6 Starter catalog contents

Embedded, auto-registered, **all `risk_level: low`**, **no `secret: true`**, no network required (`03` §2 rules 1 and 6, §10 rule 6).

| Dir | Description | Params |
|---|---|---|
| `hello-world` | Print a greeting | `name` string local default `world` |
| `disk-usage` | `df -h` for a path | `path` string local default `.` |
| `git-status` | `git status -sb` in a repo path | `path` string local default `.` |
| `health-check` | Resolve a host (`getent`/`ping -c 1` POSIX-ish; skip if no net) | `host` string local default `localhost` |
| `list-path` | List a directory (`ls -la`) | `path` string local default `.` |

Five is enough for first paint and for agents to have something to call the same day (`03` §2). Team catalogs (`catalog install` / `add`) grow the library; they are not how the product becomes real (`03` charter 3).

---

## 8. Non-goals and risks

### 8.1 Non-goals (inherited from `03`)

The PRD does not grow these. A later phase that wants one is a principles revision.

- Product naming (phase 4).
- A general agent harness / chat REPL.
- Generic `run_shell` / `exec` MCP tool.
- One MCP tool per runbook as the default (or opt-in) surface.
- Required `dops init` or a first-run questionnaire.
- Empty-by-default install.
- Web UI / SPA as a core interface (`dops open`).
- SaaS, accounts, multi-tenant server, hosted control plane.
- Cloud-required features (except install/update and explicit `catalog install`).
- Unattended high/critical by agents without a prior human grant.
- Schema-printed confirmations (`CONFIRM`).
- Secrets in config, git, MCP schemas, or history parameter maps.
- JSON as the human config format.
- Plugin marketplace / extension host / foreign runtimes as the default authoring model.
- Windows-first or PowerShell-default scripts.
- Replacing kubectl / Terraform / CI; dops-next packages them as runbooks.
- Auto-commit / auto-push / auto-install of agent-invented catalogs.
- Becoming a distro or theme shop.
- Unopinionated defaults.
- `mine_list` / `mine_get` / `mine_run` / `mine_review` as extra MCP tools, `dops://mine/*` resources, or a `review-mined-runbook` prompt (`06` §5.3 vs `03` §8).
- Walking `~/Documents/Sessions` artifact dumps, ledger files, or tool stdout as a mining corpus (`06` §1.6, §4.1).
- Wiki-ingest from the miner (`06` non-goals).
- Executing mined scripts before human approve (`06` opening contract).

From `01` §3.1 also dropped: MCP file watcher, `DecryptingVarResolver`, stub progress notifications, `dops://history` resource, integer-vs-number as two author types, demo runner as runtime.

### 8.2 Risks

| Risk | Mitigation |
|---|---|
| Sesami `script.sh` walks `dirname "$0"` two levels to `scripts/trigger-pipeline.sh`; a naive "copy two files" importer breaks Jenkins | Loader treats extra catalog files as opaque; cwd = runbook dir; compatibility tests use the real tree read-only (`01` §7) |
| Catalog rename (`src` → `jenkins-pipelines`) breaks vault keys and history | `name` is stable; docs warn; no auto-rename (decision 3) |
| Age 0.12.1 labeled BETA; Go vault import may not convert | Slice 4 tests conversion; fallback is empty vault + re-save; no secret values in fixtures |
| `serde_yaml` ecosystem churn | Pin `serde-yaml-ng` 0.10.0; fixture round-trip the 32 YAML files |
| Agents ignore dops and shell out to `kubectl` anyway | Product still must not *offer* a shell tool (`03` §9). Skill docs (later) teach when to call dops |
| Pending-grant queue ignored by operators | TUI first-class pending list; MCP result tells the agent to wait |
| HTTP MCP accidentally bound to `0.0.0.0` | Refuse non-loopback binds |
| Token budget creep in tool descriptions | insta snapshot of compact `tools/list` bytes; CI fails > 3200 bytes (~800 tokens @ 4 chars) |
| Mining adds four MCP tools (`06` §5.3) | Decision 15: reuse list/describe; CLI for run/review. Snapshot still 4 tools after slice 9 |
| Miner copies raw transcript lines into `$DOPS_HOME` | Fail closed (`06` §4.1); tests in `dops-mine` with synthetic `ghp_` fixtures; no Sessions bodies in this repo |
| Publishing install URL / crates / GitHub release | Out of scope here; repo Gates require Mason |

---

## 9. MVP slice plan

Nine slices. **Slice 5 is the first shippable agent path:** `list_runbooks` / `describe_runbook` / `run_runbook` over the Sesami catalog. Mining is slice 9 and must not grow `tools/list`. No product code in *this* phase; these slices are the implementation DAG after this PRD.

### Slice 1 — Workspace, domain, XDG config

**Scope:** Cargo workspace, `dops-core` types (`RiskLevel`, `Runbook`, `Parameter`, `Catalog`, `Config`), `config.toml` load/save (missing keys = defaults), XDG paths, `dops version`, `dops --help`.
**Tests:** parse empty TOML; parse full example; `DOPS_HOME` isolation; risk order / `Exceeds`.
**Done:** `cargo test -p dops-core` green; `dops --help` lists the command tree stubs.

### Slice 2 — Catalog loader v1/v2 + Sesami round-trip

**Scope:** Disk loader, strictness (§4.7), v1 mapping (§4.3), optional `catalog.yaml`, aliases, active flag, load-time risk filter parameterized by ceiling. Read-only use of `~/Bitbucket/sdo-dops-catalog/src`.
**Tests:** 32 YAML files parse; `name`≠dirname fails; missing `risk_level` fails; `ses-argocd-sync` `type: number` imports; `select` without `options` fails; shared `scripts/` ignored as runbooks; fixture v2 with catalog.yaml merge.
**Done:** `dops list --catalog jenkins-pipelines` (after a test config `add`) prints 32 ids without executing anything.

### Slice 3 — Executor + CLI run --dry-run

**Scope:** `dops-exec` POSIX `/bin/sh`, env injection, cwd = runbook dir, cancel, `dops run --dry-run`, `dops info`.
**Tests:** fixture `script.sh` echoes `$FOO`; secret names appear in env_names not env_public; `..` in `script:` rejected; process-group cancel test (Unix).
**Done:** `dops run starter.hello-world --dry-run` and `dops run jenkins-pipelines.cc4-aaa --dry-run` print env names including `JENKINS_TOKEN` as secret, no Jenkins HTTP.

### Slice 4 — Vault + secret flag + Go import attempt

**Scope:** age envelope, 0600 atomic write, 3-layer resolve, `secret: true` omitted from describe/history, optional keyring feature compiled but off by default. Try import of `~/.dops/vault.json` in a unit test with a **synthetic** envelope (never the real `jenkins_token`).
**Tests:** round-trip vault; mask in history struct; MCP-schema helper strips secrets; documented failure path if Go identity convert is incompatible (**unverified** until this slice).
**Done:** saving `jenkins_user` via CLI persists encrypted; files are `0600`.

### Slice 5 — MCP list / describe / run (ship)

**Scope:** `dops-mcp` + `dops mcp serve` stdio. Tools 1–3 only (`propose_runbook` stub returns "not implemented" **or** omit until slice 7 — **omit**: default surface stays 3 tools until propose lands, then 4. Do not ship a dummy tool). Wire agent ceiling default `low`. Truncation 50 lines. `insta` snapshot of `tools/list`.
**Tests:** compact `tools/list` ≤ 3200 bytes; list of Sesami at `allow_risk=low` hides high/critical (`ses-release-build`, `ses-deploy`); describe `cc4-aaa` has `secret_param_names: ["jenkins_token"]` and no token value; run `starter.hello-world`; run above ceiling → `no such runbook`; HTTP bind `0.0.0.0` refused.
**Done:** a local MCP client can list / describe / run the starter catalog, and list / describe the Sesami catalog, without one-tool-per-runbook. **This is the first agent-usable ship.**

### Slice 6 — Grants, pending, unified confirm, history

**Scope:** CLI `--confirm <id>`, TUI confirm overlay (can land with slice 8 if TUI is not up — **CLI path required here**), pending_grant files, `dops grant *`, history records + 90d/50MB policy, `interface` field.
**Tests:** MCP run of a fixture `risk_level: high` without grant → `pending_grant` file; `grant allow` then run succeeds; history masks secrets; eviction unit test with fake clocks/files.
**Done:** Sesami `ses-deploy` cannot run over MCP on a default config; a human can grant the id.

### Slice 7 — propose / accept + catalog git install

**Scope:** `propose_runbook` tool (fourth tool), `dops catalog accept`, `catalog add/install/update/remove`, git CLI clone/pull, `sub_path` escape check. Optional `catalog migrate`.
**Tests:** propose writes diff and does not exec; accept loads via strict loader; install `--path` cannot escape; MCP `tools/list` still 4 tools.
**Done:** an agent can draft a runbook; a human can accept it; `dops catalog install <url> --path src --name jenkins-pipelines` matches SPEC.md.

### Slice 8 — TUI + starter embed + installer

**Scope:** ratatui app (sidebar, metadata, output, wizard, confirm, `?`, palette, `/` search, `j`/`k`), product theme `doop` default, rust-embed starter catalog, POSIX `install.sh` with SHA-256 verify. Visual check of default `View()` (VHS or ratatui test backend snapshot).
**Tests:** first-run with empty config shows 5 starter runbooks; key `?` overlay; no `init` command; installer dry-run on a temp prefix.
**Done:** `curl | sh` shape is in-tree; `dops` after install is a finished TUI; `dops mcp serve` still the agent path.

### Slice 9 — Session mining (`dops-mine`)

**Scope:** `crates/dops-mine` + `dops mine` subcommands per `06` §2–4 and this PRD §6.8. Staging catalog `mined` (inactive). `list_runbooks include_staging` / `catalog=mined` and `describe_runbook` on `mined.*`. `run_runbook` on staging → `error=staging`. LaunchAgent installer. Table-driven redaction tests (`06` §4.3) with **synthetic** inputs only. No Go `internal/mine`.
**Tests:** `tools/list` still 4 tools and ≤ 3200 bytes; `mine_list` is not a registered tool; fixture cluster of 3 synthetic sessions proposes one draft; `ghp_` fixture never appears in mine logs; `dops mine approve` copies into inactive `mined`; MCP run of `mined.*` fails staging; missing Claude transcript → `skip: transcript_missing`.
**Done:** scheduled `dops mine run --once` can queue a redacted draft; agents can list/describe it; they cannot execute or approve it.

Slice order keeps **MCP over Sesami** at slice 5, before TUI polish, matching `01` rank 1–6 and `03` "8, 10, 11 are load-bearing." Mining is later because it is not required to run the Sesami catalog (`02` §7.13 is delayed adoption).

---

## 10. Crate verification log

Retrieved from `https://crates.io/api/v1/crates/<name>` on **2026-09-11**. User-Agent `dops-next-prd-research/0.1`.

| Crate | max_stable_version | license field |
|---|---|---|
| rmcp | 3.3.0 | Apache-2.0 |
| rmcp-macros | 3.3.0 | Apache-2.0 |
| clap | 4.6.6 | MIT OR Apache-2.0 |
| clap_complete | 4.6.9 | MIT OR Apache-2.0 |
| ratatui | 0.30.2 | MIT |
| crossterm | 0.29.0 | MIT |
| tokio | 1.53.1 | MIT |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.151 | MIT OR Apache-2.0 |
| serde-yaml-ng | 0.10.0 | MIT |
| toml | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_edit | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 |
| age | 0.12.1 | MIT OR Apache-2.0 |
| directories | 6.0.0 | MIT OR Apache-2.0 |
| thiserror | 2.0.20 | MIT OR Apache-2.0 |
| uuid | 1.26.1 | Apache-2.0 OR MIT |
| schemars | 1.2.2 | MIT |
| rust-embed | 8.12.0 | MIT |
| zeroize | 1.9.0 | Apache-2.0 OR MIT |
| keyring | 4.2.0 | MIT OR Apache-2.0 |
| tracing | 0.1.44 | MIT |
| tracing-subscriber | 0.3.23 | MIT |
| camino | 1.2.5 | MIT OR Apache-2.0 |
| fs-err | 3.3.1 | MIT OR Apache-2.0 |
| sha2 | 0.11.0 | MIT OR Apache-2.0 |
| regex | 1.13.1 | MIT OR Apache-2.0 |
| humantime | 2.4.0 | MIT OR Apache-2.0 |
| insta | 1.48.0 | Apache-2.0 |
| assert_cmd | 2.2.2 | MIT OR Apache-2.0 |

**Unverified:** age identity file interoperability with Go `filippo.io/age` `keys.txt`; live MCP JSON-RPC framing overhead (audit already notes envelope is tens of bytes, `01` §10).

---

## 11. Key decisions (index)

1. Four meta-tools, lazy describe, no resources/prompts on connect — `03` §8, `02` §7.1, `01` §6.
2. v1 YAML loads; v2 is additive; catalog.yaml shared params optional — `01` §4, §9 Q2.
3. Stable catalog `name`; Sesami should be `jenkins-pipelines` + `sub_path=src` — `01` §4.9.
4. Human attest for high/critical; MCP grant/pending, never schema `CONFIRM` — `03` §10–11.
5. Human ceiling medium, agent ceiling low — `03` §1, §10.
6. Age vault, optional keyring; secrets are a flag — `01` §2.4, `03` §6.
7. Exec file + env; extra catalog files allowed — `01` §7.
8. History 90d / 50MB / 10MB archives — `01` §9 Q8.
9. Skills parse-only in MVP — `01` §9 Q9.
10. CLI+MCP first, TUI in slice 8, no web — `03` charter 13.
11. Strict loader — `01` §9 Q12.
12. Official `rmcp` 3.3.0, not a third-party MCP crate; `serde-yaml-ng` not deprecated `serde_yaml`.
13. Starter catalog embedded; install = ready — `03` §2–3.
14. Token budgets: ≤800 connect, ≤1500 typical run, ≤60s to first runbook.
15. Session mining is `dops-mine` + `dops mine`, not four MCP tools; drafts live in inactive catalog `mined` and are list/info only until a human accepts — `06` §5 vs `03` §8.

---

## 12. Deviations and limits

- This document does not implement code. Crate versions will drift after 2026-09-11.
- Token-per-connect target is a budget on **our** `tools/list` payload, not a billed-token measurement from Claude/Cursor.
- Go vault import is specified as best-effort (`unverified` crypto-format).
- `04-naming.md` may exist on another branch; this PRD still does not pick a product name; binary remains `dops`.
- Publishing `install.sh` to a stable URL, crates.io, and GitHub Releases is gated on Mason (repo README Gates).
- `06` §5.1 Go layout is explicitly ignored (planner note, inbox 002).
- Did not wiki-ingest (assignment). Did not push.
)
