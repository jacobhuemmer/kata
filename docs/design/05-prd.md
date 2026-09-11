# kadou PRD and architecture

**Date:** 2026-09-11
**Status:** design (phase 5, revised after independent review)
**Product name:** **kadou** (稼働), chosen by Mason. This revision retires the prior codename (see the Revision log, §13, for the exact string) and applies the name throughout: binary `kadou`, crates `kadou-*`, config/data/state dirs `~/.config/kadou` / `~/.local/share/kadou` / `~/.local/state/kadou`, env prefix `KADOU_*`, MCP server name `kadou`, keyring service `kadou`, LaunchAgent label `dev.kadou.mine`. The legacy Go product's on-disk paths (`~/.dops/`) are named **literally** wherever this document imports from them — that product is not renamed.

This is the product and architecture contract for the Rust rewrite. It obeys `docs/design/03-principles.md`. Every numbered decision cites `01-audit.md`, `02-competitors.md`, or `03-principles.md`. Session-mining rows also cite `06-session-mining.md`. This revision resolves every blocking item raised by the independent review in `07-review.md` §8.1 and adopts its concrete proposals; see the **Revision log** (§13).

**Inputs (read-only):**

- `docs/design/01-audit.md` — Go product at `~/origin/dops` `795d2d2` (tag `v0.13.1`, feature-complete at v0.12.0)
- `docs/design/02-competitors.md` — adjacent MCP / task-runner / skills products
- `docs/design/03-principles.md` — charter and non-goals
- `docs/design/06-session-mining.md` — pipeline, redaction, review gate (engine packaging in `06` §5 is Go `cmd/` + `internal/`; **ignore that layout** — kadou is Rust)
- `docs/design/07-review.md` — independent review of the prior draft (`sd/dops/prd` at `72d7e49`). Verdict: revise before implementation, with 15 ranked blocking items. This PRD is that revision.
- Reference catalog `~/Bitbucket/sdo-dops-catalog` — 32 `src/*/runbook.yaml` (disk count 2026-09-11)
- crates.io versions and licenses, retrieved 2026-09-11 (see §10). Anything not retrieved is marked **unverified**.

No secrets appear in this document. The Sesami catalog secret is named `jenkins_token` only. No internal hostnames or Bitbucket workspace identifiers appear in examples (`07` B15) — Jenkins and git remote examples below use placeholder domains.

---

## 1. Product summary, users, and success metrics

### 1.1 What it is

kadou is a **script library** that is also an **MCP server for AI agents**. The job is to cut model tokens by preferring reviewed POSIX scripts over free-form reasoning (`03` charter 8–9; `02` §7.3).

One binary exposes three interfaces over one engine (`03` charter 13; `01` §2.1):

| Interface | Entry | Who |
|---|---|---|
| TUI | `kadou` (no args) | DevOps operators |
| CLI | `kadou run`, `kadou list`, `kadou info`, `kadou catalog`, `kadou mine`, … | Operators, scripts, CI, scheduled mining |
| MCP | `kadou mcp serve` (stdio default) | Mason's agents (Claude Code, Cursor, Codex, Grok, …) |

It is not a general agent harness, not a web app, and not a hosted control plane (`03` non-goals).

The migratable unit is today's catalog: a directory of `runbook.yaml` + `script.sh`, parameters as `UPPER_SNAKE` env vars, four risk words, age-encrypted vault, git-installable catalogs (`01` §2.2–2.6). The rewrite **ports that unit** and **does not port** one-MCP-tool-per-runbook or agent-default-`critical` (`03` scoreboard; `01` §2.9, §8 rank 7).

### 1.2 Users

**Mason's agents.** Local stdio MCP clients. They list, describe, and run runbooks. They may draft new runbooks. They never raise their own risk ceiling, never see secret values, never write the vault (`07` B2; §4.6, §6.5), and never accept their own proposals (`03` §10–11).

**DevOps operators.** Humans at a keyboard (TUI) or in a shell (CLI). They install team catalogs (Sesami's 32 Jenkins-trigger runbooks first), save `jenkins_url` / `jenkins_user` / `jenkins_token` once in the vault with **`kadou vault set <key>`** (`07` B13; §6.5 — not `kadou config set`, which refuses vault-scoped keys), raise ceilings, grant specific high/critical ids to agents, and accept proposed or **mined** runbooks (`06` §2.9).

Same engine, same catalogs, same vault, same risk policy. A fourth interface (Vue web UI) is deferred and is not a v1 driver (`01` §3.2, §9 Q10; `03` non-goals).

### 1.3 Success metrics (numbers)

Token figures for *today* are schema-replay, not a live MCP wire trace (`01` §6, §10). Targets below are product constraints (`03` §8 rule 6).

| Metric | Today (Sesami 32-runbook catalog) | kadou target | Why |
|---|---|---|---|
| **Tokens per connect** (`tools/list` only — `resources/list` and `prompts/list` are not called; §5.1) | ~8 800 (`01` §6.1: 33 940-byte `tools/list` ≈ 8 500 tokens @ 4 chars) | **≤ 800 tokens**, stretch **≤ 500**. CI gate: compact `tools/list` ≤ **2 800 bytes** (`07` B1; not 3 200 — that gate is loose at any tokenizer stricter than 4 B/token). The measured payload is `docs/design/tools-list.json`: **2 330 compact bytes ≈ 583 tokens @ 4 B/token, ≈ 666 @ 3.5 B/token.** Both under budget with headroom for tool annotations. | Four constant meta-tools, hand-authored `inputSchema` (Appendix A of `07`, adopted verbatim with two additions). GitHub MCP's 93-tool ~55k dump is the failure mode (`02` §5, §7.1). |
| **Tokens per run** (tool result body) | Last 50 lines, unstructured-ish JSON (`01` §4.11, `internal/mcp/tools.go`) | **≤ 1 500 tokens** typical (last **50** lines + exit code + duration_ms + log_path + history_id), **compact JSON, not pretty-printed** (`07` §3.3 — pretty adds ~28%). Hard cap **8 192** output UTF-8 bytes before truncation notice | Server-side truncate; full log on disk, and the fresh-tier log is a plain-text file an agent's own tools can read (`07` B3). |
| **Time to first runbook after install** | curl installer, then `dops init`, then empty-or-hello-world (`03` §3 dops-today) | **≤ 60 s** wall clock from `curl \| sh` to `kadou` showing the starter catalog on a warm network. **≤ 10 s** from a completed install to first TUI frame / first `list_runbooks` on a local SSD | Install = ready (`03` §3). Empty catalog is a product bug (`03` §9 rule 6). |
| **Connect cost vs catalog size** | Linear: 32 runbooks ~8.5k tokens; SPEC's 370 pipelines extrapolate ~98k (`01` §6.2 item 4) | **O(1)** in runbook count. 32 and 370 pay the same `tools/list` | Meta-tools (`02` §1.5, mise MCP in `02` §2.3). |
| **Sesami import** | n/a | All **32** `runbook.yaml` files load. `kadou info` / `describe_runbook` round-trip is tested with **catalog policy `max_risk_level = critical`** — the compatibility-suite ceiling, not the shipped default. At the default agent ceiling (`low`) only 5 of 32 runbooks are visible over MCP (`helm-package`, `sdo-k8s-ses`, `ses-automation`, `clone-ses-repos`, `device-log-metrics`; §6.2) — that is by design, not an import failure. `kadou run --dry-run` resolves env for all 32 without calling Jenkins. | `01` §9 Q11; `07` §1 row "1.3", B1. |

A change that grows the default `tools/list` past the 2 800-byte gate is a principles violation, not a feature (`03` §8 rule 6).

---

## 2. Decision table (01 §9 plus mining, revised)

Each row is a closed decision. One-line reason plus citations. Rows 1–14 close `01` §9. Row 15 closes how `06` §5.3 fits the `03` §8 tool budget. Changes from the prior draft are marked **[rev]** with the blocking id from `07` §8.1.

| # | Question | Decision | Reason | Cite |
|---|---|---|---|---|
| 1 | MCP tool shape | **Four meta-tools only:** `list_runbooks`, `describe_runbook`, `run_runbook`, `propose_runbook`. No tool per runbook. No resource-only catalog. No `select_catalog` that then registers tools. No history tool. **No `mine_*` tools** (decision 15). | Constant-size surface is ~37× cheaper than 32 eager schemas and stays flat at 370 pipelines; a second four-tool mine surface would double connect cost. | `01` §6.2, §9 Q1; `02` §7.1, §7.8, mise in `02` §2.3; `03` §8 rules 1–2, convention table |
| 2 | Format versioning | **`runbook.yaml` v2** with a **v1 compatibility loader**. v1 files (no `format_version`) load without rewrite. v2 adds optional `format_version: 2`, omittable `script:` (defaults to `script.sh`), omittable `name:` (defaults to directory), and optional catalog-root `catalog.yaml` **opt-in** shared `parameters:` groups (`uses:`). **[rev, B9]** String-typed defaults (`"true"`, `"60"`) coerce to the declared type; uncoercible values are a load error. `integer` and `number` both import; v2 authors write `integer` (any whole) or `float`. | Must ingest 32 Sesami files as-is (all 176 declared defaults are YAML strings, including 86 booleans); catalog-level params kill 29 copies of `jenkins_*` for *new* catalogs without breaking old ones, and must not silently push `JENKINS_TOKEN` into non-Jenkins runbooks. | `01` §4.2–4.3, §7, §9 Q2; `03` §7 rules 1–3; `07` §6, B9 |
| 3 | Catalog identity | **`name` is the stable id.** `display_name` is cosmetic. Recommended Sesami register: `--name jenkins-pipelines` with `sub_path = "src"` (matches SPEC.md comment, not `filepath.Base` → `src`). Renames are operator-explicit and break vault keys / history ids by design. | IDs are `catalog.runbook`; silent rename is data loss. | `01` §2.6, §4.9, §9 Q3; `03` §7 rule 2 |
| 4 | Confirm protocol | **One visibility-and-grant state machine, three faces.** low/medium: none. high: human must affirm (TUI: y/N, default No; CLI: `--confirm <id>`). critical: human must type the runbook id (TUI input; CLI `--confirm <id>`). **MCP: no confirm strings in schemas.** **[rev, B4]** `pending_grant` happens **only** when a runbook is *visible* to the agent (§6.2 formula) **and** is high/critical **and** its id is not in `[agent].allowed_runbooks` at a sufficient ceiling. A runbook the agent cannot see returns `no_such_runbook`, never `pending_grant` (a `pending_grant` reply would itself leak that the id exists). The pending record is pinned to a script digest and catalog HEAD, expires after 24h, and dedupes on `(runbook_id, args_hash)` (§6.4). | Schema `_confirm_id` / `_confirm_word` is copyable theater; Sesami `ses-deploy` is production-impacting; the prior draft's §5.5 example and §6.4 step 1 contradicted the §6.3 table and the §6.2 visibility rule. | `01` §4.6, §9 Q4; `02` §7.7; `03` §10 rules 2–3, §11 rule 4; `07` C3, B4 |
| 5 | Default risk ceiling | **Human `max_risk_level = "medium"` per catalog, overridable per catalog.** **Agent `allow_risk = "low"`.** `--allow-risk` on `mcp serve` cannot exceed config. **[rev, B4]** Ceiling formula, stated precisely (matches Go semantics — catalog policy *replaces* the global default, it does not intersect with it): `human_ceiling(c) = c.policy.max_risk_level ?? defaults.max_risk_level`; `agent_ceiling(c) = min(agent.allow_risk, --allow-risk, human_ceiling(c))`; a runbook is visible to the agent iff `rank(risk_level) ≤ agent_ceiling(c)`. `allowed_runbooks` never raises visibility — it only decides run-vs-`pending_grant` for a runbook that is already visible and high/critical (§6.3 table). High/critical hidden at list/describe/run above that ceiling. | Today's MCP default `critical` is inverted omakase-safe; the prior draft's `min(agent, catalog, human)` formula silently capped the human ceiling at the global default, contradicting Go (`internal/catalog/loader.go:55-58`) and the PRD's own §7.5 comment ("raise to critical to even see ses-deploy"). | `01` §4.6, §9 Q5; `03` §1 rule 4, §10 rules 1 and 3; `07` C2 (row 2.5), B4 |
| 6 | Vault portability | **Age X25519 identity, local-first (default).** Identity file `0600` under the data dir. **[rev, B12]** **Optional** keyring wrap of the identity's **passphrase** (not the identity itself — the two prior drafts of this decision disagreed) via `keyring-core` + `apple-native-keyring-store` on macOS. **Verified**: import of Go `vault.json` v1 works; the envelope is specified byte-for-byte in §6.5. No required 1Password / cloud KMS. | Own the machine; secrets never in config; per-machine keys are the safe default; keyring is the portable overlay; the bare `keyring` crate is wrong per its own docs and breaks Linux MSRV. | `01` §2.4, §4.7, §9 Q6; `02` §7.11; `03` §6 rules 4–5, charter 12; `07` §4.5, B12 |
| 7 | Script contract | **Exec a file with env vars.** `script:` is a path relative to the runbook directory (default `script.sh`). Shared files **outside** runbook dirs (e.g. `scripts/trigger-pipeline.sh`) are first-class as *catalog tree files*, not as a second `script:` URI scheme. Loader must not assume a two-file directory is the whole program. POSIX `/bin/sh`. No `run_shell` tool. cwd is the runbook directory — a documented change from Go, which inherits the caller's cwd; harmless for Sesami. | 29 Sesami wrappers `dirname "$0"` up to repo `scripts/`; rewriting 32 scripts is out of scope. | `01` §2.2, §7, §9 Q7; `03` §1 rule 5, §7 rule 6, §9 rules 1–2; `07` C16 |
| 8 | History policy | **Implement the documented policy, not the ignored size-cap.** JSON records + gzip-tar log archives. **[rev, B3]** Keep the v0.12 spec's **fresh tier**: the last **7 days** of logs are plain `0600` text files (not gzip-tar), so a running or recent run's `log_path` is a file an agent's own tools can open; older logs compress into 10 MB gzip-tar archives. **90-day TTL** and **50 MB total cap** (oldest records+archives first) unchanged. Secrets masked `****` in parameter maps, and the raw output stream is redacted **before** it is written to the log or returned to MCP (§6.6). `interface` is `tui` \| `cli` \| `mcp`. Pending and proposed records: 30-day retention. | Audit of Jenkins triggers needs retention; the prior draft dropped the fresh plain-text tier without saying so, which made `log_path` a tar entry no agent file tool can open. | `01` §2.8, §4.10, §9 Q8; `03` §11 rule 5; `07` C15, B3 |
| 9 | Skills | **Parse `type: skill` so load does not break.** Do **not** register skill prompts or dump skill bodies on the default MCP surface in MVP. Product later. | Zero `skill.md` in origin catalogs or sdo-dops-catalog; registering all skills violates lazy MCP. | `01` §3.2, §4.8, §9 Q9; `02` §4.1; `03` §8 rule 3 |
| 10 | Web / TUI in v1 | **CLI + MCP ship first** (slices 1–5, re-cut — §9). **TUI is in MVP** on the same engine. **Web UI is a non-goal.** No `kadou open`. | Charter is three interfaces; four on day one is how today's MCP/TUI confirm drifted. Vue is extra surface. | `01` §3.1–3.2, §9 Q10; `03` charter 13, non-goals |
| 11 | Compatibility tests | **Yes.** 32 Sesami YAML files are the acceptance suite: parse, strict-load, schema round-trip, `describe`, `run --dry-run` env map. **Do not execute Jenkins in CI.** Optional live dry-run behind `KADOU_TEST_CATALOG`. **[rev, B15]** CI cannot read `~/Bitbucket/sdo-dops-catalog` (no such path in the runner). Vendor **sanitized, shape-preserving fixtures** under `tests/fixtures/sesami-shaped/` (32 stub runbooks with the same key set, type mix, and default-string shapes, no real hostnames or Bitbucket workspace names) for CI, and run the real tree locally / on a self-hosted runner via `KADOU_TEST_CATALOG=~/Bitbucket/sdo-dops-catalog`. | The catalog is the compatibility target (`01` §1), but CI must not depend on a path that only exists on Mason's machine. | `01` §7, §9 Q11; `07` §2 row "2 row 11", B15 |
| 12 | Loader strictness | **Fail closed, per catalog.** `name` (after defaulting) **must** equal the directory. `risk_level` required and valid. `select` / `multi_select` require `options`. `script` file must exist for non-skill entries. Unknown `type` is a load error (v1 `number`/`integer`/`file_path`/`resource_id` are known aliases — §4.3). YAML that does not unmarshal is a load error. Unknown top-level or parameter keys are load errors against the **complete** known-key list (§4.3). **[rev, B9]** A load failure in one catalog does not take down another catalog, the starter catalog, or the server — it fails that catalog only and is reported per catalog. A `required: true` parameter with an empty-string `default` does not satisfy "required" (Sesami's `ses-argocd-sync.app_name`). | Today's "unmarshal and hope" makes `name` ≠ dirname a silent wrong script path; per-process failure would let one bad team catalog brick the starter catalog and MCP. | `01` §4.1–4.2, §9 Q12; `03` §7 rule 1; `07` §2 rows "2 row 12"/"4.7", B9 |
| 13 | Secret parameter vs type | **Keep `secret: true` on any type.** Do not add `type: secret`. | Catalog uses the flag (e.g. `jenkins_token`); a new type would not exist in today's YAML. | `01` §2.4, §9 Q13; `03` §10 rule 4 |
| 14 | Inactive catalogs and multi-catalog MCP | Inactive catalogs are skipped for **execution**. Addressing is the **id** `catalog.runbook` (or an alias). `list_runbooks.catalog` is an optional filter. `run_runbook` / `describe_runbook` take `id`, not a separate required catalog argument. **[rev, B14]** **Staging exception (decision 15):** `mined` and `proposed` are **reserved** catalog names (not configurable — `[mine] catalog` is removed from config). Staging entries are listed/described **regardless of the risk ceiling**, because they are never executable — the miner never assigns `low`, so ceiling-filtering staging would hide everything from a default agent. `run_runbook` always refuses staging ids. Drafts live at `~/.local/share/kadou/catalogs/proposed/<catalog>--<name>/` (flattened, one level, so the one-level loader can scan it) and are addressed as `proposed.<catalog>--<name>`. The only path from staging to executable is `kadou catalog accept <staging-id> --into <user-catalog>` (default `user`); there is no `active = true` shortcut on `mined`. | Dispatcher still needs a stable id; mined drafts must be reviewable without becoming executable tools; the prior draft's `proposed/<catalog>/<name>/` was two levels deep (the loader only scans one) and its `active = true` path bypassed `kadou catalog accept` entirely. | `01` §2.6–2.7, §9 Q14; `03` §7 rule 2; `06` §2.9, §5.3; `07` §3.6, C7, C8, B14 |
| 15 | Session mining MCP (`mine_list` / `mine_get` / `mine_run` / `mine_review`) | **Do not add those tools, resources, or prompts.** Engine is crate `kadou-mine` + CLI `kadou mine` (not Go `cmd/mine.go`). Map: `mine_list` → `list_runbooks` (`catalog=mined` or `include_staging=true`); `mine_get` → `describe_runbook` on `mined.<slug>`; `mine_run` → **CLI/LaunchAgent only** (`kadou mine run --once`); `mine_review` approve → human `kadou mine approve` / `kadou catalog accept … --into <user-catalog>`; reject/skip → human `kadou mine reject\|skip`. No `kadou://mine/*` on `resources/list`. No MCP approve. **[rev, B14]** The miner writes `format_version: 2` drafts (the prior `06` §6.4 draft example was v1-shaped with `type: number`). | Four extra tools would break the ≤4 budget; inactive `mined` catalog is already the review gate; schema `_confirm_id` on mine_review is the same theater decision 4 rejected. | `06` §5 (ignore Go layout), §5.3, §2.9; `03` §8 rules 1 and 6, §11 rules 1–2; `02` §7.13; `07` §6.8, B14 |

---

## 3. Rust workspace layout

Edition 2024. **MSRV 1.88** (required by `rmcp` 3.3.0 and `ratatui`). One binary named `kadou`. Workspace at repo root.

```
kadou/
  Cargo.toml                 # workspace
  crates/
    kadou/                   # bin: clap CLI, wires TUI + MCP
      starter/                # embedded starter catalog, inside the bin crate (07 §5.5) so `cargo package` works for rust-embed
    kadou-core/              # domain, config, catalog loader, vault, vars, history, risk
    kadou-exec/              # process group, POSIX sh, env injection, dry-run
    kadou-mcp/               # rmcp server, 4 tools, result truncation
    kadou-tui/               # ratatui app
    kadou-mine/              # session mining engine (ingest/parse/cluster/redact/propose)
  docs/design/
  tests/fixtures/            # v1 YAML, v2 YAML, sesami-shaped stub catalog for CI (decision 11)
```

| Crate | Responsibility | Depends on |
|---|---|---|
| `kadou-core` | `Runbook`, `Parameter`, `RiskLevel`, XDG paths, TOML config, catalog registry, v1/v2 loader, vault, 3-layer var merge, history store | serde, toml, serde-yaml-ng, age, **etcetera** (XDG, not `directories` — B10), uuid, thiserror, zeroize, base64 |
| `kadou-exec` | `Runner::run(ctx, script_path, env)`, cwd = runbook dir, **MCP env allowlist**, `exec.timeout`, cancel via process group (SIGTERM then SIGKILL) | kadou-core, tokio, **libc/nix/rustix** (process-group kill) |
| `kadou-mcp` | stdio (default) + loopback HTTP (opt-in, gated — §5.1); the four tools with **hand-authored** `inputSchema`; no resources/prompts on the default list | kadou-core, kadou-exec, rmcp, tokio, serde_json |
| `kadou-tui` | keyboard-first catalog / wizard / confirm / output / palette / `?` | kadou-core, kadou-exec, ratatui, crossterm |
| `kadou-mine` | index.jsonl ingest, per-agent parsers, normalize/cluster/rank/redact/propose; **no MCP types**; writes `format_version: 2` drafts | kadou-core, serde_json, regex, sha2, tokio, **similar** (diff) |
| `kadou` | `main`, clap command tree including `kadou mine`, rust-embed starter (embedded under this crate — see workspace layout above), install-time version | all of the above, clap, clap_complete, rust-embed |

**Why split this way:** one engine (`kadou-core` + `kadou-exec`) shared by CLI, TUI, and MCP so confirm/risk/vault cannot drift again (`01` §2.1). Mining is a local batch job (`06` §3) that writes drafts; it must not sit inside `kadou-mcp`. Ignore `06` §5.1 Go `cmd/mine.go` + `internal/mine/` — that layout is not this repo.

### 3.1 Key dependencies (crates.io 2026-09-11, re-verified per `07` §5)

Versions are **max stable on crates.io on 2026-09-11**. Pin in `Cargo.toml` with `^` of the major.minor recorded here; bump only with a design note if a major moves. Changes from the prior draft are marked **[rev]**.

| Crate | Version | License | Why |
|---|---|---|---|
| **clap** | 4.6.6 | MIT OR Apache-2.0 | CLI tree, derive, completions. |
| **clap_complete** | 4.6.9 | MIT OR Apache-2.0 | Shell completions. |
| **ratatui** | 0.30.2 | MIT | TUI. Replaces Bubble Tea / Lip Gloss. |
| **crossterm** | 0.29.0 | MIT | ratatui default backend; keyboard + terminal. |
| **rmcp** | 3.3.0 | Apache-2.0 | **Official** Rust MCP SDK. Build with `default-features = false`, features `["server", "macros", "transport-io"]`; HTTP (`transport-streamable-http-server`) behind a `kadou` cargo feature so stdio builds carry no HTTP stack (`07` §5.3). |
| **rmcp-macros** | 3.3.0 | Apache-2.0 | Tool impl macros; same repo. |
| **tokio** | 1.53.1 | MIT | Async runtime. Features: `rt-multi-thread`, `process`, `signal`, `io-util`, `macros` (`07` §5.1). |
| **serde** | 1.0.229 | MIT OR Apache-2.0 | Serde. |
| **serde_json** | 1.0.151 | MIT OR Apache-2.0 | MCP JSON (compact, not pretty — `07` §3.3) + history records. |
| **serde-yaml-ng** | 0.10.0 | MIT | YAML for `runbook.yaml`. **Not** `serde_yaml` 0.9.34+deprecated. **Not** `serde_yml` 0.0.13. Verified 32/32 on the real Sesami tree; watch maintenance (last release 2024-05-26), keep the 32-file round-trip as the swap guard for `serde-saphyr` or `serde_norway` if needed. |
| **toml** | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 | Config parse. |
| **toml_edit** | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 | Comment-preserving `kadou config set`. |
| **age** | 0.12.1 | MIT OR Apache-2.0 | Vault. crates.io description still says **[BETA]**; it is the Rust port of the same age (X25519 + ChaCha20-Poly1305) Go uses. **Go import verified** (`07` Appendix B): `age::IdentityFile::from_buffer` reads a dops-written `keys.txt`; decrypting the Go `vault.json` envelope requires stripping the literal `age1` prefix and unpadded standard base64 (§6.5). |
| **etcetera** | 0.11.0 | MIT OR Apache-2.0 | **[rev, B10]** XDG base directories on macOS/Linux. **Replaces `directories` 6.0.0**, which maps config *and* data to `~/Library/Application Support` on macOS and returns no state dir at all — wrong for a product whose convention table names `~/.config/kadou` as canonical (`03` §6 rule 2). |
| **thiserror** | 2.0.20 | MIT OR Apache-2.0 | Typed errors in libraries. |
| **uuid** | 1.26.1 | Apache-2.0 OR MIT | Execution / pending / history ids (v4). |
| **schemars** | 1.2.2 | MIT | Used only to **deserialize** tool arguments into typed Rust structs. `inputSchema` on the wire is **hand-authored JSON** (`docs/design/tools-list.json`), not schema-derived — rmcp/schemars derivation adds `$schema`, renders `Option<T>` as a two-element type array, and expands optional enums into `$defs`/`anyOf`, none of which fits the budget (`07` §3.1). |
| **rust-embed** | 8.12.0 | MIT | Embed starter catalog + bundled themes. |
| **zeroize** | 1.9.0 | Apache-2.0 OR MIT | Wipe decrypted vault buffers. |
| **keyring-core** | 1.0.0 | MIT OR Apache-2.0 | **[rev, B12]** **Replaces bare `keyring` 4.2.0.** The `keyring` crate's own docs say applications should link `keyring-core` plus a specific store, not the umbrella crate; the umbrella's default `v1` feature pulls in a dependency chain (`aes` 0.9.3 via `zbus-secret-service-keyring-store` → `secret-service`) whose `rust-version` is 1.89, above this workspace's declared 1.88 MSRV. |
| **apple-native-keyring-store** | 1.0.2 | MIT OR Apache-2.0 | **[new, B12]** macOS Keychain backend for `keyring-core`. Mason's target platform. A Linux `zbus-secret-service-keyring-store` 1.0.1 backend goes behind its own opt-in feature if Linux support is ever needed; it is not required for MVP and keeps the 1.89 MSRV bump out of the default build. |
| **tracing** / **tracing-subscriber** | 0.1.44 / 0.3.23 | MIT | Structured logs to stderr (MCP must not write on stdout — a stray byte on stdout corrupts the stdio transport). |
| **camino** | 1.2.5 | MIT OR Apache-2.0 | UTF-8 paths. |
| **fs-err** | 3.3.1 | MIT OR Apache-2.0 | IO errors with paths. |
| **sha2** | 0.11.0 | MIT OR Apache-2.0 | Script digest in `describe_runbook`; grant-approval pin (§6.4). |
| **humantime** | 2.4.0 | MIT OR Apache-2.0 | Duration display. |
| **regex** | 1.13.1 | MIT OR Apache-2.0 | Mining redaction rules (`06` §4.2); history stream redaction (§6.6). |
| **flate2** | 1.1.10 | MIT OR Apache-2.0 | **[new, B3]** History `.log.gz` tar archives (aged-out tier). |
| **tar** | 0.4.46 | MIT OR Apache-2.0 | **[new, B3]** Same. |
| **similar** | 3.2.0 | Apache-2.0 | **[new, B7]** `propose_runbook` diff generation, and the accept-time diff shown to a human (§6.7). |
| **libc** | 0.2.189 | MIT | **[new, B5]** `killpg` for process-group SIGTERM/SIGKILL. |
| **nix** | 0.31.3 | MIT | **[new, B5]** Higher-level process-group / signal wrapper over `libc` where it saves hand-rolled `unsafe`. |
| **base64** | 0.23.1 | MIT OR Apache-2.0 | **[new, B12]** Go vault import: the envelope is `"age1" + base64::STANDARD_NO_PAD(<ciphertext>)` (§6.5), not plain age armor. |
| **tempfile** | 3.27.0 | MIT OR Apache-2.0 | **[rev]** Also a **runtime** dependency now, not dev-only: atomic config/vault/proposal writes (write-to-temp, fsync, rename). |

**Dev / test:** `assert_cmd` 2.2.2, `predicates` 3.1.4, `insta` 1.48.0 (snapshot the exact `tools/list` bytes against `docs/design/tools-list.json`), `proptest` 1.11.0 — all MIT OR Apache-2.0 except insta (Apache-2.0). `cargo-deny` 0.20.2 (rust-version 1.88) in CI for license/advisory gating (`07` §5.4, suggestion 6).

**Explicitly not used**

| Crate | Why not |
|---|---|
| `serde_yaml` 0.9.34+deprecated | Deprecated on crates.io. |
| `directories` 6.0.0 | **[rev, B10]** Wrong macOS paths, no state dir. See `etcetera` above. |
| `keyring` 4.2.0 (bare) | **[rev, B12]** Upstream says link `keyring-core` + a store instead; breaks Linux MSRV. See `keyring-core` above. |
| `git2` / `gix` | Catalog install shells out to `git` (same as today, `01` §4.9). No libgit2 in the binary. |
| `axum` / `hyper` as first-party HTTP | rmcp's streamable HTTP feature is enough for opt-in loopback. |
| Any GPL-3.0 runner (e.g. taking mcp-shell as a library) | Copyleft is a product constraint (`02` §7.15). |

### 3.2 MSRV

Declared **1.88**. The keyring-core swap (§3.1) removes the only dependency chain that forced 1.89 on Linux under the prior draft's `keyring` 4.2.0 choice. Add an MSRV CI job: `cargo +1.88 check --workspace --all-targets`, run on **macOS and Linux** (`07` §5.2, cross-cutting note in §7).

### 3.3 Licenses

Add `cargo-deny` with a license allowlist plus advisories to CI (`07` §5.4). Every dependency in this workspace offers a permissive option (MIT / Apache-2.0 / MPL-2.0 / BSD-family / Unicode). rmcp is Apache-2.0-only, which needs NOTICE handling in release archives (gated on Mason, §7.4).

---

## 4. Catalog format

### 4.1 On-disk layout (unchanged unit)

```
<catalog-root>/                    # Catalog.RunbookRoot() = path + optional sub_path
  catalog.yaml                     # optional; v2 shared parameter groups (opt-in — §4.5)
  <entry-dir>/                     # directory name = runbook name
    runbook.yaml
    script.sh                      # default; or whatever `script:` names
    skill.md                       # only when type: skill (parsed, not executed)
  scripts/                         # optional extra files; not scanned as runbooks
```

Loader scans **immediate subdirectories** of each catalog root that contain `runbook.yaml`. Nested catalogs are not supported (`01` §4.1). Extra files (Sesami `scripts/trigger-pipeline.sh`, `device-log-metrics/lib/`, tests) are preserved and not loaded as runbooks (`01` §7; decision 7).

**Active vs staging.** Active catalogs are listed, described, and runnable (subject to risk). Inactive (non-staging) catalogs are skipped entirely, including for listing (`01` §2.6). **[rev, B14]** The **staging** catalogs `mined` and `proposed` are **reserved names** — not configurable, not renamable — and are the only inactive catalogs kadou will list/describe. They are listed/described **regardless of the risk ceiling** (§2 row 14) and are **never** runnable. The **only** way a staging draft becomes executable is:

```sh
kadou catalog accept mined.<name> --into <user-catalog>   # --into defaults to "user"
```

which validates with the strict loader, shows a diff, prompts `y/N` (or `--yes`), and copies the runbook into an **existing user-owned catalog** under `~/.config/kadou/catalogs/<user-catalog>/`. There is **no** `active = true` shortcut on `mined` — flipping that flag would make every future `kadou mine approve` executable without a per-runbook review, which defeats the review gate (`06` §2.9–2.10; `07` C7).

Drafts under `proposed/` are stored **flattened**, one level deep, so the same loader that scans catalog roots can scan the staging root too: `~/.local/share/kadou/catalogs/proposed/<catalog>--<name>/`, addressed as `proposed.<catalog>--<name>` (`07` §3.6 item 2, B14).

### 4.2 Identity

- Catalog **`name`**: `[a-z0-9][a-z0-9-]*` recommended; stored in config; **stable**. `mined` and `proposed` are reserved and cannot be registered by an operator.
- Runbook **id**: `<catalog>.<dirname>` if YAML `id` omitted. If YAML `id` is set, it must equal that.
- **Aliases:** optional list, lowercase alnum / hyphen / dot, unique across loaded catalogs; first-loaded wins with a warning (`01` §2.7). CLI/MCP resolve id then alias. Sidebar shows `name`.
- **Display name:** catalog-level, max 50 printable chars, never used in vault keys or history ids (`01` §4.5, §4.9).

Sesami import (recommended):

```sh
kadou catalog add --name jenkins-pipelines --path src ~/Bitbucket/sdo-dops-catalog
```

IDs become `jenkins-pipelines.cc4-aaa`, not `src.cc4-aaa`. Operators who already registered as `src` keep `src.*` until they choose to re-add (`01` §4.9, decision 3). `sub_path` is how a monorepo installs without flattening. A later `kadou catalog rename --migrate-vault` (deferred; §13) would let an operator move `src` → `jenkins-pipelines` without losing saved runbook-scope values.

### 4.3 `runbook.yaml` v1 compatibility rule

A file **without** `format_version` is v1. The loader's known-key set is the **complete** union of every key that appears in the real Sesami catalog plus the Go product's schema — verified against 32/32 files on disk (`07` §6):

- **Top-level:** `name`, `id`, `version`, `description`, `risk_level`, `type`, `trigger`, `aliases`, `script`, `format_version`, `parameters`.
- **Parameter:** `name`, `type`, `required`, `description`, `scope`, `default`, `secret`, `options`.

Any other key is an unknown-key load error (decision 12). The v1→internal mapping:

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
| **`default` present as a YAML string but the declared `type` is `boolean`, `integer`, `float`, or `number`** | **coerce**: `"true"`/`"false"` → boolean; numeric string → the declared numeric type | **[rev, B9]** All 176 Sesami defaults are YAML strings, including the 86 boolean defaults (`"false"`/`"true"`) and the `number` default `"60"`. A typed default field with no coercion fails 87 of them. An uncoercible string is a load error. |
| **`required: true` with `default: ""`** | **does not satisfy required** | `ses-argocd-sync.app_name` is `required: true, default: "", scope: local` — an empty default is not a value. |
| unknown keys | **load error** | Against the complete key list above. |

v1 files are **not** rewritten on load. `kadou catalog migrate <name>` (slice, optional) can emit v2 + `catalog.yaml` shared params; it is not required to run Sesami.

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
    scope: local            # local | global | catalog | runbook
    secret: false
    default: "."
    description: Path to measure
    options: []            # required when type is select or multi_select
```

Author-facing types in v2: **six** (`string`, `boolean`, `integer`, `float`, `select`, `multi_select`). Import aliases `number`, `file_path`, `resource_id` exist only on the v1 path (`01` §3.1 "integer vs number", `03` §7 "too many knobs").

`script:` if present must be a relative path **without** `..` escape from the runbook dir (reject absolute paths and `..`). Shared helpers live beside the catalog and are invoked from `script.sh` via `dirname "$0"` — that is the Sesami pattern and it keeps the exec contract one file (`01` §7).

**Parameter name rule [new, B6]:** `^[a-z][a-z0-9_]*$`. The loader rejects any name whose ASCII-uppercase form is one of the reserved child-process names: `PATH`, `HOME`, `PWD`, `OLDPWD`, `IFS`, `SHELL`, `CDPATH`, `BASH_ENV`, `PS4`, or any name starting `LD_`, `DYLD_`, or `KADOU_`. All 24 distinct Sesami parameter names pass this rule, including `env` → `ENV` (non-interactive `sh` does not read `$ENV`, so that one is safe to keep).

### 4.5 Optional `catalog.yaml` (v2 only)

At `<catalog-root>/catalog.yaml`, shared parameters are grouped and **opt-in per runbook** — a runbook must declare `uses: [<group>]` to pull a group in (`07` B9). This prevents a Jenkins-credentials group from silently attaching to a non-Jenkins runbook:

```yaml
format_version: 2
parameter_groups:
  jenkins:
    - name: jenkins_url
      type: string
      required: true
      scope: global
      default: "https://ci.example.com"
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

and in a runbook's `runbook.yaml`:

```yaml
format_version: 2
uses: [jenkins]
parameters:
  - name: branch
    type: string
    default: dev
```

Merge rule: shared parameters named in `uses:` (in the listed order) load first; a runbook's own declaration of the same `name` **replaces the whole shared parameter**, not a field-by-field patch. v1 and v2 runbook files may coexist in one catalog, so a migration to `catalog.yaml` is incremental — a Sesami migration would add `uses: [jenkins]` to the 29 Jenkins-trigger runbooks and leave `clone-ses-repos`, `device-log-metrics`, and `ses-argocd-sync` without it, so they never require Jenkins credentials (`07` §6, C-list item on `catalog.yaml`).

`catalog.yaml` does not change the catalog's stable `name` (that lives in config).

### 4.6 Parameter resolution (carry as-is, with a scope lock for MCP)

Order: vault global < vault catalog < vault runbook < CLI `--param` / TUI input / MCP `args` (`01` §2.5, §4.4). Then **filter to declared parameter names** so extra vault keys do not leak into the child env. Env var is `name.to_ascii_uppercase()` (subject to the reserved-name rejection in §4.4).

**[rev, B2] MCP scope lock:** `args` sent through `run_runbook` may set only `local`- and `runbook`-scope parameter values. A `global`- or `catalog`-scope parameter can be supplied by an agent **only** as a per-parameter opt-in the runbook author sets (`agent_settable: false` is the default for `global`/`catalog` scope); attempting to set a locked-scope name in `args` is `invalid_args`. This closes the path where an agent sends `args: {"jenkins_url": "https://attacker.example"}` and the vault's `jenkins_token` is sent to that host under `-u user:token` (`07` C5). **MCP never writes the vault, in any scope** — persisting a resolved value is a human-CLI/TUI-only action (`kadou config set` / `kadou vault set`, or TUI save), tested explicitly in slice 5 (`07` C17, B2).

Scopes (`01` §4.4):

| Scope | Saved? | Vault key | MCP `args` may set it? |
|---|---|---|---|
| `global` | yes | `global.<param>` | No (locked; §4.6) |
| `catalog` | yes | `catalog.<cat>.<param>` | No (locked; §4.6) |
| `runbook` | yes | `catalog.<cat>.runbooks.<rb>.<param>` | Yes |
| `local` | no | — | Yes |

Sesami: 87 `jenkins_*` global declarations, 146 runbook-scoped Jenkins toggles, 1 local (`ses-argocd-sync.app_name`). The catalog is unusable without global scope for a human operator (`01` §2.5); an agent still runs Sesami runbooks because the vault fills the global/catalog values it cannot set itself.

### 4.7 Loader strictness (decision 12)

A catalog load **fails for that catalog only** (CLI/TUI/MCP refuse to activate it, but other active catalogs — including the embedded starter catalog — keep working) if any **active** runbook in it fails the checks in §4.3 (`07` B9, fixing the prior draft's "CLI/TUI/MCP all refuse to start" wording, which would have bricked the starter catalog too on an unrelated team catalog's bad file). Inactive non-staging catalogs are skipped entirely (`01` §2.6). Staging load errors fail that draft only (drop + audit). Warnings (non-fatal): alias collisions, skill missing `skill.md`.

Starter catalog is always loaded from embed and is guaranteed valid at compile time (tests).

---

## 5. MCP surface

### 5.1 Default surface

`tools/list` returns **exactly these four tools**, in this order. No other tools. No per-runbook tools, including as an opt-in — the charter closed that list (`03` convention table; `02` §7.1 said opt-in for tiny catalogs, **`03` wins**). `06` §5.3's `mine_list` / `mine_get` / `mine_run` / `mine_review` are **not** registered (decision 15).

**[rev, B1]** The server advertises **only the `tools` capability**. It does not declare `resources` or `prompts` at all, so a spec-compliant host never calls `resources/list` or `prompts/list` — cheaper than advertising those capabilities with empty lists. Catalog JSON, schema markdown, and `kadou://mine/*` are not registered (`01` §5.2; `02` §7.11; `03` §8; `06` §5.3 rejected as a connect-time dump). Agents that need a schema call `describe_runbook`.

`prompts` is not advertised. `create-runbook` becomes the `propose_runbook` **tool**. `review-mined-runbook` is not a prompt (`06` §5.3). Skills are not prompts (`01` §9 Q9; `03` §8 rule 3). The session-mining **skill** is a `SKILL.md` that execs `kadou mine`, not an MCP prompt (`06` §5.2).

`initialize` sends **no `instructions` field** — several hosts inject `instructions` into the system prompt, and any bytes spent there count against the budget just as much as `tools/list` (`07` §3.1). If a future revision adds `instructions`, it must be counted in the CI byte gate.

Server name: `kadou`. Version: ldflags / `CARGO_PKG_VERSION`. Transport: **stdio default**. **[rev, B11]** HTTP is `--transport http --bind 127.0.0.1:8808` (loopback only; refuse `0.0.0.0`), and additionally: `allowed_origins` (rmcp's Origin allowlist) must be **non-empty** — rmcp 3.3.0 defaults it to empty, which *disables* Origin validation rather than enabling a safe default — and every HTTP request must present a per-launch random bearer token printed once at `kadou mcp serve --transport http` startup. Both are required before HTTP ships; until they are implemented, HTTP stays behind a build-time cargo feature and is not part of slice 5 (`03` §1 rule 6, §10 rule 5; `07` C14, B11).

### 5.2 Progressive disclosure

1. **Connect:** four tool schemas (budget ≤ 2 800 bytes compact / ≤ 800 tokens).
2. **`list_runbooks`:** id, name, catalog, one-line description, risk_level, aliases. Filterable. No parameter schemas (`03` §8 rules 2 and 5; Agent Skills layer 1, `02` §7.2).
3. **`describe_runbook`:** args JSON Schema (secret values omitted, names listed separately), script path, sha256, script body (default on so agents can read before run — `03` §9 rule 3), capped at **16 KiB** with `script_truncated: true` past the cap, plus `catalog_root` and a list of sibling files the script references so an agent can read a shared helper like Sesami's 12.8 KB `trigger-pipeline.sh` with its own file tools (`07` C13, suggestion 2). This is layer 2.
4. **`run_runbook`:** execute; result is last N lines + metadata. Script source is **not** in the result (`02` §7.2 layer 3; `03` §8 rule 4).

Eager-loading clients (Cursor without tool search, naive CI) only ever see four schemas (`02` §7.8). There is nothing to defer.

**[new, suggestion 1]** MCP tool annotations (`readOnlyHint` on `list_runbooks`/`describe_runbook`; `destructiveHint`/`openWorldHint` on `run_runbook`; `readOnlyHint: false`/`destructiveHint: false` on `propose_runbook`) are included on the wire — see `docs/design/tools-list.json`. Cost is folded into the 2 330-byte measured payload. Hosts use them to auto-approve reads.

### 5.3 Multi-catalog addressing

`id` is `catalog.runbook` or an alias. Optional `catalog` filter on list. Inactive catalogs absent from list **except** staging (`mined`, `proposed`) which are always listable/describable regardless of ceiling (decision 14–15; §4.1). A second catalog does not add tools (`01` §9 Q14). `run_runbook` on a staging id returns `error=staging` and does not execute.

### 5.4 Tool input schemas (JSON Schema draft 2020-12)

**[rev, B1]** These are the **exact, hand-authored** `inputSchema` objects served on the wire — not a derive-macro rendering. The full `tools/list` payload, byte-for-byte, is checked in at `docs/design/tools-list.json` and covered by an `insta` snapshot test (§3.1). Compact size: **2 330 bytes** (≈ 583 tokens @ 4 B/token, ≈ 666 @ 3.5 B/token) — under the 2 800-byte CI gate with room to spare. Changes from the prior draft, applied to every tool: dropped `$schema` (MCP 2025-11-25 already defaults to 2020-12), dropped `$id` (no consumer, and a `dops://` URI scheme would have been a rename touchpoint), dropped `title` (duplicates `name`). Kept `additionalProperties: false` on every root.

#### `list_runbooks`

Description: `Search runbooks visible to this agent. Returns id, one-line description, risk. No schemas.`

```json
{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "query": { "type": "string", "maxLength": 200, "description": "Substring of id, alias, or description." },
    "catalog": { "type": "string" },
    "risk": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 },
    "offset": { "type": "integer", "minimum": 0, "default": 0 },
    "include_staging": { "type": "boolean", "default": false, "description": "Also list non-runnable drafts (mined, proposed)." }
  }
}
```

`[rev, B1]` `include_staging`'s description no longer cites internal doc sections or claims equivalence to `catalog=mined` (the prior wording was wrong: `include_staging` *adds* staging entries to the active list; `catalog=mined` lists *only* `mined`). `query` gained `maxLength: 200`.

#### `describe_runbook`

Description: `Get one runbook: args schema, risk, script. Read it before run_runbook.`

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["id"],
  "properties": {
    "id": { "type": "string", "description": "catalog.runbook or alias." },
    "include_script": { "type": "boolean", "default": true }
  }
}
```

#### `run_runbook`

Description: `Run one runbook with args. Secrets come from the local vault; never pass them. Above your grant it does not run and returns pending_grant.`

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["id"],
  "properties": {
    "id": { "type": "string" },
    "args": { "type": "object", "default": {}, "additionalProperties": true, "description": "Parameter name to value, per describe_runbook." },
    "dry_run": { "type": "boolean", "default": false, "description": "Resolve args and env names without executing." }
  }
}
```

**[rev, B1, B6]** The prior `args` description's sentence "Do not send `_confirm_id` or `_confirm_word`; those fields do not exist" is removed — it primed the model with tokens for fields that were never in the schema and cost ~20 tokens on every connect for no benefit. There are still **no** `_confirm_id`/`_confirm_word` properties (`03` §10 rule 2; `01` §4.6). `dry_run`'s description now matches its actual behavior ("resolve args and env names", not "return the would-be command" — the result has no command field). Argument-to-environment **serialization** (not part of the wire schema, but part of the contract) is specified per type in §6.1. No execution-time-bound property is added to the schema; the timeout is server-side config (§6.1, B5).

Wrong args: `status=error`, `error=invalid_args`, `isError: true`, plus the parameter schema from describe (so the model can retry without a round-trip to describe if it skipped it).

#### `propose_runbook`

Description: `Draft a new runbook for human review. Writes files and returns a diff; never registers or runs it.`

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["catalog", "name", "description", "risk_level"],
  "properties": {
    "catalog": { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]*$" },
    "name": { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]*$", "maxLength": 64 },
    "description": { "type": "string", "minLength": 1, "maxLength": 200 },
    "risk_level": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
    "yaml": { "type": "string", "maxLength": 16384 },
    "script": { "type": "string", "maxLength": 65536 }
  }
}
```

**[rev, B7]** Changes from the prior draft:
- `catalog` gained a `pattern`. The server additionally canonicalizes the resolved path and refuses anything outside `~/.local/share/kadou/catalogs/proposed/`, closing a path-traversal write via `catalog: "../../../.ssh"`.
- `name` gained `maxLength: 64`; `description` gained `maxLength: 200`.
- `risk_level` is now **required with no default** — the prior silent `default: "low"` hid an unreviewed agent-authored script's risk choice from the human reviewer, the same reasoning `06` §4.4 applies to mined drafts.
- If `yaml` is supplied and conflicts with `name`/`description`/`risk_level`, the server rejects the call rather than silently picking one; `yaml` must itself pass the strict loader at propose time.
- Re-proposing the same `catalog`/`name` overwrites the prior draft, and the diff is computed against the currently **accepted** runbook when one exists, otherwise against `/dev/null`.
- The result field `pending_id` is renamed **`proposal_id`** — the prior draft reused `pending_id` for both a grant request (a UUID) and a proposal (`catalog.name`), which is two different meanings on one field name (§5.5).

No `accept` tool (`03` §11 rule 2).

### 5.5 Result shapes

**[rev, B1]** All tool results are a **single JSON text content block**, **compact** (not pretty-printed — pretty adds ~28% for no benefit to a model), UTF-8. Every result that represents an input error, a lookup failure, or a runtime failure sets `isError: true` (MCP 2025-11-25 guidance), with `error` one of `no_such_runbook`, `invalid_args`, `staging`, `timeout`. A non-zero script exit is `status: failed` with `isError: true`. Secrets in vault values are redacted from `output` if they appear (`02` §7.11; Atuin `secrets_filter` lesson; see also §6.6 for the fuller redaction rule).

#### `list_runbooks` result

```json
{
  "runbooks": [
    { "id": "starter.disk-usage", "name": "disk-usage", "catalog": "starter", "description": "Show disk usage for a path", "risk_level": "low" }
  ],
  "total": 5,
  "offset": 0,
  "limit": 50,
  "truncated": false
}
```

`truncated` is true when `offset+len < total`. Hidden (above-ceiling) runbooks are absent, not listed as denied (`03` §10 rule 3). Staging entries set `staging: true` and never appear unless `include_staging` or `catalog` names a staging catalog. `aliases: []` and `staging: false` are **omitted**, not printed, on entries that don't need them (`07` §3.3). List payloads still have **no scripts** (`06` §5.3 mine_list). `total: 5` matches the five starter runbooks in §7.6 (the prior draft's example said 6 — fixed).

#### `describe_runbook` result

```json
{
  "id": "jenkins-pipelines.cc4-aaa",
  "name": "cc4-aaa",
  "catalog": "jenkins-pipelines",
  "description": "Trigger a SES/CC4/cc4-aaa branch pipeline",
  "risk_level": "medium",
  "version": "1.0.0",
  "script_path": "/abs/path/src/cc4-aaa/script.sh",
  "script_sha256": "sha256:…",
  "script": "#!/bin/sh\nset -eu\n…",
  "script_truncated": false,
  "catalog_root": "/abs/path/src",
  "helper_files": ["scripts/trigger-pipeline.sh"],
  "args_schema": {
    "type": "object",
    "properties": {
      "branch": { "type": "string", "description": "Branch, release tag, or PR to trigger", "default": "dev" }
    },
    "required": []
  },
  "secret_param_names": ["jenkins_token"],
  "resolved": ["jenkins_url", "jenkins_user"]
}
```

**[rev, B1]** Changes from the prior draft, which duplicated every parameter as both a flat field and a `json_schema` object (6 071 pretty bytes / ~1 518 tokens for one describe):
- Parameters collapse into **one** `args_schema` JSON Schema object (compact: **1 408 bytes** for this example) instead of a flat-field-plus-`json_schema` list per parameter.
- `secret_param_names` lists secret parameter names (never values); `resolved` lists **names only** (never values) of parameters that already have a saved vault value — those names are marked **not required** in `args_schema` even if the runbook YAML says `required: true`, so the agent does not try to resend a global credential it cannot see.
- `visible_to_agent` is dropped — a describable id is definitionally visible (an above-ceiling or unknown id returns `no_such_runbook`, never a describe result with `visible_to_agent: false`).
- `parameters` (the old flat array) is dropped in favor of `args_schema`.

`include_script: false` drops `script` but keeps `script_path` and `script_sha256`.

Unknown id or above-ceiling: `isError: true`, `error: no_such_runbook` (do not distinguish hidden vs missing — same as load-time hide). Staging ids **are** describable (redacted yaml + script) so agents can review drafts (`06` §5.3 mine_get). `describe_runbook` on staging must not include source session bodies, cwd, or raw commands (`06` §4.1).

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
  "log_path": "/Users/…/.local/state/kadou/history/logs/2026-09-11/<uuid>.log",
  "history_id": "<uuid>"
}
```

`status` is `success` when exit_code is 0, `failed` otherwise, `cancelled` on ctx cancel, or **`running`** when `mcp.max_wait` elapses before the process exits (§6.1) — in that case `history_id` and `log_path` are still returned so the agent can tail the (plain-text, fresh-tier) log with its own file tools. `summary` is defined as the **last non-empty output line, truncated to 200 chars**. **[rev, B3]** `log_path` for a fresh (≤7-day) run is a **plain text file**, not a path into a gzip tar archive — the prior draft's `….log.gz#<uuid>.log` pointed at a tar entry no agent file tool can open (`07` C15).

Pending grant (high/critical, visible, no allow-list):

```json
{
  "status": "pending_grant",
  "runbook_id": "jenkins-pipelines.ses-deploy",
  "risk_level": "critical",
  "pending_id": "<uuid>",
  "pending_path": "/Users/…/.local/state/kadou/pending/<uuid>.json",
  "reason": "high risk; not in [agent].allowed_runbooks"
}
```

**[rev, B4]** `reason` no longer describes a state that §6.2's visibility formula makes unreachable (the prior draft's "critical exceeds agent allow_risk low; queued" could never actually be returned, because an agent whose ceiling is below `critical` never sees the runbook in the first place — see decision 4 / `07` C3). `pending_path` is new: it lets the agent (or a human, via the CLI) inspect the pending record's outcome after approval without a fifth MCP tool (§6.4).

Dry-run:

```json
{
  "status": "dry_run",
  "runbook_id": "jenkins-pipelines.cc4-aaa",
  "env_names": ["JENKINS_URL", "JENKINS_USER", "JENKINS_TOKEN", "BRANCH", "VERSION", "SEND_EMAIL", "PUBLISH_IMAGE", "PUBLISH_API", "ALLOW_IMAGE_OVERRIDE"],
  "env_public": { "BRANCH": "dev", "VERSION": "" },
  "secret_env_names": ["JENKINS_TOKEN"]
}
```

`env_public` never includes secret values; it **does** include vault-resolved non-secret globals like `JENKINS_USER`, stated explicitly here so that is not a surprise (`07` §3.3).

Invalid args:

```json
{
  "status": "error",
  "error": "invalid_args",
  "isError": true,
  "message": "unknown arg \"cmd\"; runbooks do not take a shell string",
  "expected": { "type": "object", "properties": { "…": {} }, "required": ["branch"] }
}
```

Staging (mined/proposed) — **does not run**, even with a grant (`06` §2.9, decision 15):

```json
{
  "status": "error",
  "error": "staging",
  "isError": true,
  "runbook_id": "mined.k8s-pod-logs",
  "message": "staging catalog; a human must run: kadou catalog accept mined.k8s-pod-logs --into <catalog>"
}
```

Truncation: last **50** lines (`mcp.max_output_lines` in config, default 50, max 200; kept configurable per `07` suggestion 5). If UTF-8 bytes of `output` would exceed 8192, cut to the last whole lines that fit and set `truncated: true`; lines are lossy-UTF-8 decoded and individually capped at 4 KiB (`01` §4.11; `02` §7.4; `03` §8 rule 4; `07` §4.1).

#### `propose_runbook` result

```json
{
  "status": "proposed",
  "proposal_id": "user.my-check",
  "path": "/Users/…/.local/share/kadou/catalogs/proposed/user--my-check/",
  "diff": "--- /dev/null\n+++ runbook.yaml\n…",
  "accept": "kadou catalog accept user.my-check --into user"
}
```

`pending_id` → **`proposal_id`** (B7); `path` moved under the flattened staging layout (§4.1, B14).

### 5.6 Agent config snippet (docs, not a tool)

```json
{
  "mcpServers": {
    "kadou": {
      "command": "kadou",
      "args": ["mcp", "serve"]
    }
  }
}
```

(`03` §3 rule 5.) Host-side tool ids follow the server name, e.g. `mcp__kadou__run_runbook` in Claude Code — see the rename note in §10 of the review, folded into this document throughout.

---

## 6. Script execution and safety

### 6.1 Exec contract

```
argv:  /bin/sh <abs-runbook-dir>/<script>
cwd:   <abs-runbook-dir>
env:   TUI/CLI: parent environ + declared params. MCP: an allowlist + declared params (below).
stdin: closed (/dev/null; never the MCP server's own stdin)
stdout/stderr: piped (never inherited), merged line stream to history log + interface
cancel: notifications/cancelled, stdio EOF, or exec.timeout → SIGTERM the process group, SIGKILL after 5s
```

- No flags-to-script adapter. No JSON blob default input (`03` §7 rule 3).
- `script:` cannot escape the runbook directory (`..` rejected). Shared helpers are reached from inside `script.sh` (`01` §7; Sesami `REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"` in `src/cc4-aaa/script.sh`).
- Windows / `script.ps1` deferred (`01` §3.2; `03` non-goals).
- `dry_run` does not spawn. It returns env names, not a command line (§5.4).
- `/bin/sh` is dash on Linux and bash-in-POSIX-mode on macOS; test both where CI runs both platforms (§9 cross-cutting).

**[rev, B5] MCP child environment.** For `interface = mcp`, the child does **not** inherit the agent host's full environment (which routinely carries API keys and cloud/CI tokens that redaction — aimed only at vault values — cannot know about; `07` C4, §4.2). It starts from an explicit allowlist plus declared params:

```
PATH HOME USER LOGNAME SHELL LANG LC_* TZ TMPDIR SSH_AUTH_SOCK KUBECONFIG
```

plus `TERM=dumb`, plus an operator-editable `[exec] pass_env = []` for anything else a team catalog genuinely needs (verified sufficient for Sesami: the cc4/`ses-*` wrappers need only declared params + `PATH`/`HOME`; `clone-ses-repos` needs `SSH_AUTH_SOCK` to clone over SSH; `ses-argocd-sync` needs `KUBECONFIG` or `~/.kube/config` via `HOME`; `device-log-metrics` reads its own `envs/*.env` files, unaffected by parent env). **CLI and TUI keep the full parent environment** — that is a human's own shell context, not an agent's (`07` §4.2).

**[rev, B5] Timeout and lifecycle.** `[exec] timeout` bounds a single run (default 30 min — Sesami's `trigger-pipeline.sh` streams a Jenkins console until the build ends, with only a `QUEUE_TIMEOUT=300` on the queue wait, not the whole run). `[mcp] max_wait` (default 50 s) bounds how long an MCP call blocks before returning `status: running` with `history_id`/`log_path` so the agent can poll the log itself — no fifth tool. A **per-server concurrency limit** (default 2) caps parallel runs; one run at a time per runbook id, or the call returns `busy`. Cancellation — via `notifications/cancelled`, client disconnect (stdio EOF), or `exec.timeout` — sends `SIGTERM` to the process group, then `SIGKILL` after a 5 s grace period (Go's own `WaitDelay` is 2 s; kadou's is slightly longer to give well-behaved scripts a chance to flush). stdin is `/dev/null`, tested as an invariant (a script that runs `cat` must not consume JSON-RPC frames on stdin); stdout/stderr are piped, tested as an invariant (one stray byte on the MCP server's own stdout corrupts the stdio transport).

**[rev, B6] Args → env serialization**, specified per type (previously undefined; Go's `fmt.Sprintf("%v", v)` turns a `multi_select` array into `"[a b]"`, which is not specified anywhere as the contract):

| Parameter type | Env value |
|---|---|
| `string` | as-is |
| `boolean` | `"true"` / `"false"` |
| `integer` / `float` | JSON number's text form |
| `select` | the chosen value, validated against `options` |
| `multi_select` | comma-joined selected values; the loader rejects any `options` entry containing a comma |
| object / null / array (other than `multi_select`'s own encoding) | rejected as `invalid_args` |

Reject `args` entries naming a secret parameter, or a `global`/`catalog`-scope parameter without `agent_settable: true` (§4.6, B2).

### 6.2 Risk levels and ceilings

Order: `low < medium < high < critical`. A runbook **at** the ceiling is allowed (`01` §4.6 `Exceeds` is strict greater-than).

**[rev, B4]** Stated precisely, matching Go semantics (catalog policy *replaces* the global default rather than intersecting with it):

```
human_ceiling(c)  = c.policy.max_risk_level ?? defaults.max_risk_level
agent_ceiling(c)  = min(agent.allow_risk, --allow-risk, human_ceiling(c))
visible(rb, c)    = rank(rb.risk_level) ≤ agent_ceiling(c)     [MCP]
                  = rank(rb.risk_level) ≤ human_ceiling(c)     [TUI/CLI]
```

| Ceiling | Default | Where |
|---|---|---|
| Human global | `medium` | `config.toml` `[defaults] max_risk_level` |
| Catalog | unset → human global (replaces it, does not intersect) | `[[catalogs]] policy.max_risk_level` |
| Agent | `low` | `[agent] allow_risk` |

`kadou mcp serve --allow-risk` may only **narrow**. It cannot exceed `[agent].allow_risk` or `agent_ceiling(c)` above (`03` §10 rule 1; `01` §4.6 MCP vs LoadAll bug). `allow_risk`/`allowed_runbooks` never raise visibility — they only decide run-vs-`pending_grant` for a runbook that is already visible and high/critical (§6.3).

At the **default** ceilings on the Sesami catalog, an agent sees exactly **five** runbooks: `clone-ses-repos`, `device-log-metrics`, `helm-package`, `sdo-k8s-ses`, `ses-automation`. **Three of those trigger Jenkins with the vault token** (`helm-package`, `sdo-k8s-ses`, `ses-automation`). "Agent default low" is therefore not "agent cannot touch CI" — which is why the env allowlist, redaction, and MCP scope lock (§6.1, §6.5, §6.6) must ship no later than the same slice as `run_runbook` (§9). `ses-release-build` (high) and `ses-deploy` (critical) are invisible to a default agent and visible to a human only after the catalog policy or global ceiling is raised (Sesami mix: medium 25, low 5, high 1, critical 1).

Starter catalog contains only `low` (`03` §10 rule 6).

### 6.3 Unified confirm protocol (decision 4)

| Level | TUI | CLI | MCP |
|---|---|---|---|
| low / medium | none | none | none |
| high | Overlay: Yes / **No** (default No). `y` accepts, `n`/`Esc` cancels | Requires `--confirm <runbook-id>` | If **visible** (§6.2), id ∈ `[agent].allowed_runbooks`, **and** ceiling ≥ high: run. Else `pending_grant`. |
| critical | Overlay: type the runbook id | Requires `--confirm <runbook-id>` | If **visible**, id ∈ `[agent].allowed_runbooks`, **and** ceiling ≥ critical: run. Else `pending_grant`. |

The model cannot mint a grant. There is no confirm field in any tool schema (`03` §10 rule 2). Approving a pending high **or** critical record still requires `--confirm <id>` at approval time (§6.4) — the prior draft's §6.4 only mentioned this for critical.

CLI without `--confirm` on high/critical prints the summary and exits 2 with the exact `--confirm` line to copy — that is a human, not an agent, path.

### 6.4 Human grant flow (agents)

**[rev, B4]** The pending record and flow are fully specified (the prior draft left TTL, dedupe, script pinning, and outcome-visibility undefined):

1. Agent `run_runbook` on a **visible** high/critical id (§6.2) without a grant → write `~/.local/state/kadou/pending/<pending_id>.json` with: runbook id, args (secrets stripped), requester `mcp`, `mcp_client` (self-reported `clientInfo.name` — a label, not an identity), timestamp, **`script_sha256`** and the **catalog's git HEAD** (when the catalog is git-backed) pinned at request time. Return `pending_grant` with `pending_id` and `pending_path` (§5.5).
2. **TTL:** the record expires after **24 hours**. `kadou grant list` shows expired records as expired; `kadou grant approve` on an expired record fails.
3. **Dedupe:** a second `run_runbook` call with the same `(runbook_id, args_hash)` while a pending record is outstanding returns the existing `pending_id` rather than creating a duplicate.
4. Human sees pending in TUI (badge + palette "Pending grants") or `kadou grant list`, which shows the **args and a script diff since the request** (in case a `git pull` landed a different script than what was requested).
5. `kadou grant approve <pending_id>` **one-shot executes** that pending record (still subject to human ceiling + TUI/CLI confirm for high or critical), **refusing** if the current on-disk `script_sha256` / catalog HEAD no longer matches the pinned value — a `git pull` between request and approval must not silently swap the program. Approval **runs in the human CLI's environment** (full parent env, not the MCP server's allowlisted one). On success the pending record gains `history_id`, `status`, and `log_path`, all readable via `pending_path` (§5.5) without a fifth MCP tool (`02` §7.9). `kadou grant allow <runbook-id>` appends to `[agent].allowed_runbooks` (config edit, human-owned).
6. `kadou grant deny <pending_id>` deletes the record.
7. MCP has no approve tool (`03` §11).

Suggested, not required: pin `kadou grant allow <id>` to the current script digest, with `--any-version` to opt out of re-pinning on every catalog update (`07` suggestion 3 — adopted as a cheap default; see §13).

Raising `[agent].allow_risk` or `allowed_runbooks` is a **config file edit** or `kadou config set` (human CLI), never an MCP tool (`03` §6 rule 6, §11 rule 3).

### 6.5 Secret handling

- Flag `secret: true` on any type (decision 13).
- **[rev, B12] Vault envelope, specified byte-for-byte to match the Go product exactly** (verified by decrypting a real Go-written envelope with the Rust `age` crate — `07` Appendix B item 5):
  - `vault.json` = `{"version":1,"data":"age1"+base64::STANDARD_NO_PAD(<binary age v1 ciphertext>)}`. The literal 4-character prefix `"age1"` is **not** an age recipient string and is **not** part of the ciphertext — it is a Go-side tag, stripped before decoding. There is no base64 padding.
  - `keys.txt` = a standard age identity file (`AGE-SECRET-KEY-…` with `#`-prefixed comment lines), parsed with `age::IdentityFile::from_buffer`.
  - Import is **copy-once and read-only** against `~/.dops/vault.json` + `~/.dops/keys/keys.txt`: if both exist and convert, kadou copies them into its own XDG data dir once and records an import marker; the source files under `~/.dops/` are never modified. If conversion fails, kadou starts with an empty vault and the operator re-saves globals with `kadou vault set`.
  - Payload shape inside the decrypted plaintext (unchanged from the prior draft):
    ```json
    {
      "global": { "jenkins_url": "https://ci.example.com", "jenkins_user": "…" },
      "catalog": {
        "<catalog-name>": {
          "runbooks": { "<runbook-name>": { "branch": "dev" } }
        }
      }
    }
    ```
- Values of `jenkins_token` are never written into this document, logs at info level, MCP schemas, `args` echoes, or history parameter maps (`01` §2.4; `03` §10 rule 4). History stores `****`.
- Default identity: age X25519 at `~/.local/share/kadou/keys/identity.txt` `0600`. This default (plaintext identity next to the vault) protects only against copying the vault file without the keys directory — the same threat model as Go. Say so plainly to operators.
- **[rev, B12]** Optional `vault.keyring = true`: the identity is age scrypt-encrypted (age's `armor` + `encrypted` module), and the **passphrase** — not the identity file itself — lives in the OS keyring via `keyring-core` + `apple-native-keyring-store`, service `kadou`, account `vault-identity`. (The prior draft stated this decision two different ways: decision 6 said keyring-*wrapped identity*, §6.5 said keyring-wrapped *passphrase*. This is the passphrase form.)
- **[rev, B12]** `vault.passphrase_cmd` (an operator-defined command that prints a passphrase to stdout) is **dropped from MVP**. It turns config into code execution: `03` §6 rule 6 expects agents to propose file diffs for human review, and a changed `passphrase_cmd` would execute the next time the vault opens. If a future revision reintroduces it, require an absolute path owned by the invoking user.
- **[new, B13] Secret entry.** `kadou vault set <key>` reads the value from a TTY prompt or stdin, **never argv** (argv lands in shell history and `ps`). `kadou config set` **refuses** vault-scoped keys outright — this replaces the prior draft's `kadou config set … jenkins_token` example, which contradicted its own §7.5 rule ("No parameter values in TOML") and `03` §6 rule 4. `kadou run --param <name>=…` refuses (does not silently accept) a `secret: true` parameter name.
- **[rev, B2] MCP never writes the vault**, in any scope — stated as a rule and covered by a slice-5 test (§4.6, §9).
- Directory modes: vault and keys directories `0700`, files `0600`.

Do not copy mise `mise://env` (`02` §7.11, §8).

### 6.6 History / audit

Record fields (`01` §4.10) plus `initiator` (username if known, else `local`) and `mcp_client` (from MCP initialize `clientInfo.name` when interface is `mcp` — stated explicitly as **self-reported, a label, not an identity**):

`id`, `runbook_id`, `runbook_name`, `catalog_name`, `parameters` (secrets `****`), `status` (`running|success|failed|cancelled|pending_grant`), `exit_code`, `start_time`, `end_time`, `duration_ms`, `output_lines`, `output_summary`, `log_path`, `interface` (`tui|cli|mcp`).

**[rev, B3] Redaction happens in the line stream, before the log is written**, so the MCP result, the on-disk log, and the TUI all see the same already-redacted text (the prior draft only masked the parameter map, leaving the raw output stream — which can `eval` and print credentials, per Sesami's `trigger-pipeline.sh` `--dry-run` — unredacted). For every secret value injected into the child, redact: the literal value, its standard base64 encoding, base64 of `user:value` (HTTP Basic auth, which `curl -u` sends), and its URL-encoded form. Apply a minimum length (8 chars) so short values do not shred unrelated output. Also redact secret values supplied through CLI `--param` (which §6.5 otherwise refuses — this is defense in depth).

**Fresh tier (decision 8):** the last **7 days** of logs are plain `0600` text files; a running or recently-finished run's `log_path` always points at one of these. After 7 days, logs compress into 10 MB gzip-tar archives. **90-day TTL** and **50 MB** combined cap (decision 8). `kadou history` lists newest-first (default 20). No `kadou://history` resource (`01` §3.1). History and pending directories `0700`, files `0600`. Retention for `pending/` and `proposed/` (staging drafts): **30 days**.

### 6.7 Propose / accept loop

**[rev, B7]** `propose_runbook` writes `~/.local/share/kadou/catalogs/proposed/<catalog>--<name>/{runbook.yaml,script.sh}` (flattened layout, §4.1) and returns a unified diff. It does **not** register the catalog entry and does **not** run (`03` §11 rule 1; `03` §9 rule 5). The server canonicalizes and contains the write path (§5.4), size-caps `yaml`/`script`, and strict-loads the proposed `runbook.yaml` at propose time so a malformed draft is rejected immediately rather than at accept time.

`kadou catalog accept <staging-id> --into <user-catalog>` (default `user`) validates with the strict loader, **prints the diff and prompts `y/N`** (or `--yes` for scripts), and copies into an **existing user-owned catalog** under `~/.config/kadou/catalogs/<user-catalog>/`. **[rev, B7]** Accept **refuses** a target that is a git-backed catalog's working tree (`03` §11 rule 6) — the prior draft's "copies into `~/.config/kadou/catalogs/<catalog>/` … registers it if missing" would have created a second, shadow directory for an already-registered git-backed catalog like `jenkins-pipelines`. TUI palette: "Accept proposed runbook".

The product never `git commit`s, `git push`es, or `catalog install`s an agent-invented URL (`03` §11 rule 6).

### 6.8 Session mining (crate + CLI, not extra MCP tools)

Contract: `06-session-mining.md` (pipeline, redaction, bounds, review gate). **Packaging override:** `06` §5.1 assumes Go `cmd/mine.go` and `internal/mine/*.go`. kadou implements that engine as **`crates/kadou-mine`** and **`kadou mine …`** on the existing clap tree. One binary (`03` charter 13). The skill (`06` §5.2) stays a `SKILL.md` that execs `kadou mine`, never parses `~/Documents/Sessions` itself.

**XDG paths** (not `$KADOU_HOME/mine` as a second hidden dir — `03` §1 rule 1, §6):

| `06` path | kadou path |
|---|---|
| `$DOPS_HOME/mine/` | `~/.local/state/kadou/mine/` |
| `$DOPS_HOME/mine/queue/<fingerprint>/` | `~/.local/state/kadou/mine/queue/<fingerprint>/{meta.json,runbook.yaml,script.sh}` |
| `$DOPS_HOME/catalogs/mined/` | `~/.local/share/kadou/catalogs/mined/` (registered inactive, reserved name — §4.1) |
| `$DOPS_HOME/mine/redact-extra.txt` | `~/.config/kadou/mine/redact-extra.txt` |

Pipeline, redaction ids R1–R13, rank cutoff, LaunchAgent **`dev.kadou.mine`**, bounds (20 min / 512 MB RSS / 2 GB scan), and fail-closed secret drop are **as specified in `06` §2–4**. This PRD does not repeat the survey or the synthetic kubectl example.

**[rev, B14]** The miner writes **`format_version: 2`** drafts (the prior `06` §6.4 example draft was v1-shaped with `type: number`; kadou's own loader only accepts `integer`/`float` as v2 author types — §4.4). `[mine] catalog` is **removed from config** — `mined` is a reserved, non-renamable name (§4.1, §4.2), closing the path where renaming the staging catalog would have defeated a name-keyed staging check.

**MCP mapping (decision 15)** — `06` §5.3 proposed four tools plus two resources plus a prompt. That is a second eager surface. Fold into the existing four:

| `06` §5.3 | kadou |
|---|---|
| Resource `dops://mine/queue` | **Not registered.** `list_runbooks` with `catalog=mined` or `include_staging=true` |
| Resource `dops://mine/proposal/{fingerprint}` | **Not registered.** `describe_runbook` id `mined.<slug>` (redacted yaml+sh already on disk; no session refs) |
| Tool `mine_list` | `list_runbooks` |
| Tool `mine_get` | `describe_runbook` |
| Tool `mine_run` (`_confirm_id`, hidden unless `--allow-mine-run`) | **CLI only:** `kadou mine run --once`. Not a starter runbook (starter is low-risk and must not read transcripts — `03` §10 rule 6). Skill may exec the CLI; MCP does not. |
| Tool `mine_review` (reject/skip; approve off by default, `_confirm_id`) | **Human CLI only:** `kadou mine approve\|reject\|skip`. No MCP accept (`03` §11). Schema confirm strings stay forbidden (decision 4). |
| Prompt `review-mined-runbook` | **Not registered.** Operator uses `kadou mine review` / `kadou info mined.<slug>` |

Approve copies the draft into the inactive `mined` staging catalog (`06` §2.9). `kadou catalog accept mined.<name> --into <user-catalog>` is what makes it executable — there is no `active = true` shortcut (§4.1, B14). Miner never assigns `low` or `critical` (`06` §4.4). Drafts use v2 types (`integer` not `number`; `file_path` on the v1 import path only — `06` §2.8's `file_path`/`number` map to `string`/`integer` at write time).

`kadou-mine` is not on the slice-5 critical path (§9).

---

## 7. TUI / CLI UX

### 7.1 Command tree

No `kadou init`. No `kadou open`. Bare `kadou` launches the TUI (`03` §1 rule 2).

```
kadou                              # TUI
kadou --help
kadou version
kadou list [--catalog NAME] [--query Q] [--risk LEVEL]
kadou info <id>
kadou run <id> [--param k=v]... [--dry-run] [--no-save] [--confirm <id>]
kadou mcp serve [--transport stdio|http] [--bind 127.0.0.1:8808] [--allow-risk LEVEL]
kadou mcp schema [--bytes]        # print the served tools/list JSON and its byte count
kadou catalog list
kadou catalog add [--name NAME] [--path SUB] [--display-name S] [--risk LEVEL] <dir>
kadou catalog install [--name NAME] [--ref REF] [--path SUB] [--risk LEVEL] [--display-name S] <git-url>
kadou catalog update <name> [--ref REF] [--risk LEVEL] [--display-name S]
kadou catalog remove <name>
kadou catalog accept <id> --into <catalog>
kadou catalog migrate <name>       # optional v1→v2 rewrite (never default)
kadou config get [key]
kadou config set <key> <value>    # human CLI; rewrites TOML with comments preserved; refuses vault-scoped keys
kadou vault set <key>              # reads value from TTY/stdin, never argv
kadou history [--runbook ID] [--limit N]
kadou grant list
kadou grant approve <pending_id>
kadou grant deny <pending_id>
kadou grant allow <runbook_id>
kadou mine run [--once | --watch | --since <iso>]
kadou mine status
kadou mine list
kadou mine show <fingerprint>
kadou mine review
kadou mine approve <fingerprint> --into <catalog>
kadou mine reject <fingerprint> --reason …
kadou mine install-schedule
kadou mine install-catalog
kadou completion <shell>
```

**[rev, B13]** `kadou vault set` is new (closes the gap where §1.2 claimed operators "save `jenkins_token` once in the vault" but no command did that securely). `kadou catalog accept` / `kadou mine approve` now take `--into <catalog>` (§4.1, B14). `kadou mcp schema --bytes` is new (`07` suggestion 11 — operator-visible budget check).

`kadou list` / `kadou info` are the CLI projection of the same meta-tools (`02` §7.9: file-shaped + CLI for agents that prefer CLI over MCP).

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
| `q` | Quit (not during confirm type-id, and not while a text input has focus) |
| `Esc` | Back out / clear search / cancel confirm |
| `Ctrl+c` | Quit or cancel execution (documented in `?`) |
| `Ctrl+x` | Stop running execution |
| `Ctrl+p` | Command palette (theme, catalog, grants, help, quit) |

**[rev, suggestion 9]** The palette chord is **`Ctrl+p`** (or `:`), not `Ctrl+Shift+p` — `Ctrl+Shift+p` is only distinguishable from `Ctrl+p` in terminals that speak an enhanced keyboard protocol (kitty/CSI-u); Terminal.app does not, and `03` §5 rule 3 allows a successor chord. `Ctrl+Shift+p` may still work as an alias where the terminal reports the capability. `q` explicitly does not fire while a text input has focus (fixing an ambiguity in the prior table).

Wizard: `Enter` next, `Shift+Tab` prev, arrows on select, `Space` multi-select, `Esc` cancel (`03` §5 rule 5). Mouse may scroll and select text; every action has a key (`03` §5 rule 1).

### 7.3 First-run experience

After install:

1. Missing config → write defaults (empty TOML is valid; missing keys mean defaults — `03` §6 rule 5).
2. `kadou` shows starter catalog in the sidebar, first runbook selected, metadata pane filled, footer key hints. Not an empty panel (`03` §4 rule 3).
3. `kadou mcp serve` speaks stdio immediately.
4. No questionnaire. No "add a catalog" dead end (`03` §1 rules 2–3).

Adding Sesami is **additive**: `kadou catalog add --name jenkins-pipelines --path src ~/Bitbucket/sdo-dops-catalog`. **[rev, B13]** Then `kadou vault set jenkins_user` and `kadou vault set jenkins_token` (TTY-prompted; or TUI save) — not `kadou config set`, which now refuses vault-scoped keys (§6.5). This replaces the prior draft's contradictory "`kadou config set` … `jenkins_token`" example.

### 7.4 Install one-liner

Canonical (`03` §3):

```sh
curl -fsSL https://<stable-install-url>/install.sh | sh
```

POSIX `#!/bin/sh`, `set -eu`, OS/arch detect, **checksum verification** (SHA-256 of the tarball against a published `SHA256SUMS`; today's `install.sh` skips this — `03` §3 dops-today). Install to `/usr/local/bin` or `KADOU_INSTALL_DIR` / `~/.local/bin`. Idempotent: updates the binary, does not clobber config, vault, or extra catalogs.

Homebrew / Nix / cargo-binstall / winget must produce the **same first-run state**. `cargo install` is not the advertised path (`03` §3 rules 2–3). Stable URL and GitHub release publishing are **gated** (repo README Gates); this PRD specifies the shape only.

### 7.5 Config file (TOML, XDG)

| Kind | Path | Override |
|---|---|---|
| User config | `~/.config/kadou/config.toml` | `KADOU_HOME` replaces the config **root** for tests/containers (`03` §1 rule 1). When `KADOU_HOME` is set: `$KADOU_HOME/config.toml`, `$KADOU_HOME/share/`, `$KADOU_HOME/state/`. **`DOPS_HOME` is honored as a documented, deprecated alias for one release** (§13, B-list note on the rename) — if both are set, `KADOU_HOME` wins and a startup warning names the deprecated variable. |
| User catalogs / user themes | `~/.config/kadou/catalogs/`, `~/.config/kadou/themes/` | |
| Product data (vault, keys, cloned catalogs, mined/proposed staging) | `~/.local/share/kadou/` | |
| State (history, pending, mine work queue) | `~/.local/state/kadou/` | |
| Starter catalog / bundled themes | embedded in the binary | |

**[rev, suggestion 15]** `XDG_CONFIG_HOME`/`XDG_DATA_HOME`/`XDG_STATE_HOME` are **not** a second override next to `KADOU_HOME` — `03` §1 rule 1 names `KADOU_HOME` (via its `DOPS_HOME` predecessor) as the *only* override, and `etcetera`'s default strategy (used when `KADOU_HOME` is unset) already reads those XDG variables as part of computing the platform-default paths. There is no separate kadou-specific XDG override layer to specify.

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

[exec]
pass_env = []                     # extra env vars a human allows an MCP-spawned script to see

[mcp]
max_output_lines = 50
max_wait = "50s"

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
# url = "git@bitbucket.example.com:example-org/sdo-dops-catalog.git"
[catalogs.policy]
max_risk_level = "medium"         # raise to critical to even *see* ses-deploy
```

**[rev, B14]** `[mine] catalog = "mined"` is removed — `mined` is a reserved name, not configurable (§6.8). **[rev, suggestion 5]** `mcp.max_output_lines` and `mcp.max_wait` are added to the closed configurable list (`03` §7 rule 5) rather than left as unlisted knobs, since `02` §7.4 and this PRD's own timeout/truncation rules (§6.1, §5.5) depend on operators being able to tune them. `[exec] pass_env` is likewise a named, closed-list configurable (§6.1, B5). Starter is **not** a `[[catalogs]]` row; it is always registered as catalog name `starter` from embed.

Config mode `0600`. Vault `0600`. No parameter values in TOML (`03` §6 rule 4).

### 7.6 Starter catalog contents

Embedded, auto-registered, **all `risk_level: low`**, **no `secret: true`**, no network required (`03` §2 rules 1 and 6, §10 rule 6).

| Dir | Description | Params |
|---|---|---|
| `hello-world` | Print a greeting | `name` string local default `world` |
| `disk-usage` | `df -h` for a directory | `target_dir` string local default `.` |
| `git-status` | `git status -sb` in a repo directory | `target_dir` string local default `.` |
| `health-check` | Resolve a host (`ping -c 1`; skip if no net) | `host` string local default `localhost` |
| `list-path` | List a directory (`ls -la`) | `target_dir` string local default `.` |

**[rev, B6]** Parameter names in `disk-usage`, `git-status`, and `list-path` are renamed from `path` to **`target_dir`** — `name.to_ascii_uppercase()` on `path` produces `PATH`, which clobbers the shell's own `$PATH` and breaks `df`, `git`, and `ls` on first run (§4.4's reserved-name rule now also rejects `path` as a parameter name at load time, so this is enforced, not just documented). `health-check` drops the `getent`-based lookup (`getent` does not exist on macOS) in favor of `ping -c 1` alone.

Five is enough for first paint and for agents to have something to call the same day (`03` §2). Team catalogs (`catalog install` / `add`) grow the library; they are not how the product becomes real (`03` charter 3).

---

## 8. Non-goals and risks

### 8.1 Non-goals (inherited from `03`)

The PRD does not grow these. A later phase that wants one is a principles revision.

- A general agent harness / chat REPL.
- Generic `run_shell` / `exec` MCP tool.
- One MCP tool per runbook as the default (or opt-in) surface.
- Required `kadou init` or a first-run questionnaire.
- Empty-by-default install.
- Web UI / SPA as a core interface (`kadou open`).
- SaaS, accounts, multi-tenant server, hosted control plane.
- Cloud-required features (except install/update and explicit `catalog install`).
- Unattended high/critical by agents without a prior human grant.
- Schema-printed confirmations (`CONFIRM`).
- Secrets in config, git, MCP schemas, or history parameter maps.
- JSON as the human config format.
- Plugin marketplace / extension host / foreign runtimes as the default authoring model.
- Windows-first or PowerShell-default scripts.
- Replacing kubectl / Terraform / CI; kadou packages them as runbooks.
- Auto-commit / auto-push / auto-install of agent-invented catalogs.
- Becoming a distro or theme shop.
- Unopinionated defaults.
- `mine_list` / `mine_get` / `mine_run` / `mine_review` as extra MCP tools, `kadou://mine/*` resources, or a `review-mined-runbook` prompt (`06` §5.3 vs `03` §8).
- Walking `~/Documents/Sessions` artifact dumps, ledger files, or tool stdout as a mining corpus (`06` §1.6, §4.1).
- Wiki-ingest from the miner (`06` non-goals).
- Executing mined scripts before human approve (`06` opening contract).
- **[new]** MCP-spawned child processes inheriting the agent host's full environment (§6.1, B5).
- **[new]** A grant/pending state whose reachable states disagree across sections (§6.2–6.4, B4).

From `01` §3.1 also dropped: MCP file watcher, `DecryptingVarResolver`, stub progress notifications, `dops://history` resource, integer-vs-number as two author types, demo runner as runtime.

### 8.2 Risks

**[rev]** Rows added per `07` §8.2 ("Missing: parent-env leak, arg-override exfiltration, long-running runs, and the byte gate as a weak token proxy") are marked **[new]**.

| Risk | Mitigation |
|---|---|
| Sesami `script.sh` walks `dirname "$0"` two levels to `scripts/trigger-pipeline.sh`; a naive "copy two files" importer breaks Jenkins | Loader treats extra catalog files as opaque; cwd = runbook dir; compatibility tests use the real tree read-only (`01` §7) |
| Catalog rename (`src` → `jenkins-pipelines`) breaks vault keys and history | `name` is stable; docs warn; no auto-rename (decision 3); `catalog rename --migrate-vault` deferred (§13) |
| Age 0.12.1 labeled BETA | Go vault import is **verified** (§6.5), reducing this to an upstream-maintenance watch, not an open question. |
| `serde_yaml` ecosystem churn | Pin `serde-yaml-ng` 0.10.0; fixture round-trip the 32 YAML files; `serde-saphyr`/`serde_norway` as swap candidates |
| Agents ignore kadou and shell out to `kubectl` anyway | Product still must not *offer* a shell tool (`03` §9). Skill docs (later) teach when to call kadou |
| Pending-grant queue ignored by operators | TUI first-class pending list; MCP result tells the agent to wait; 24h TTL surfaces stale requests (§6.4) |
| HTTP MCP accidentally bound to `0.0.0.0`, or reachable by any local process without auth | Refuse non-loopback binds; require non-empty `allowed_origins` and a per-launch bearer token before HTTP ships at all (§5.1, B11) |
| **[new]** MCP-spawned children inherit the agent host's parent environment, which can carry API keys and cloud tokens redaction never sees | Explicit env allowlist + `[exec] pass_env` for MCP; full parent env stays CLI/TUI-only (§6.1, B5) |
| **[new]** An agent overrides a `global`/`catalog`-scope arg (e.g. `jenkins_url`) to redirect where the vault's secret is sent | MCP `args` scope lock: only `local`/`runbook` scope is agent-settable; MCP never writes the vault (§4.6, B2) |
| **[new]** `run_runbook` blocks for an entire long-running build (Jenkins console streaming has no PRD-level bound) | `exec.timeout` + `mcp.max_wait` → `status: running` with a pollable plain-text log; concurrency limit (§6.1, B5) |
| Token budget creep in tool descriptions | `insta` snapshot of the exact `docs/design/tools-list.json` bytes; CI fails > 2 800 bytes. **[new]** The byte gate is a *proxy* for token cost, not a tokenizer measurement — it is loose at tokenizers stricter than 4 B/token (at 3.5 B/token the same payload is larger in tokens); treat it as a ceiling, not a target, and consider a real tokenizer count before GA. |
| Mining adds four MCP tools (`06` §5.3) | Decision 15: reuse list/describe; CLI for run/review. Snapshot still 4 tools after the mining slice |
| Miner copies raw transcript lines into product dirs | Fail closed (`06` §4.1); tests in `kadou-mine` with synthetic `ghp_`-style fixtures; no real Sessions bodies in this repo |
| Publishing install URL / crates / GitHub release | Out of scope here; repo Gates require Mason |

---

## 9. MVP slice plan (re-cut per `07` §7, B8)

**[rev, B8]** The prior nine-slice plan shipped agent execution (slice 5) before its audit trail (slice 6), ran `starter.*` runbooks in slices 3 and 5 before the starter embed existed (slice 8), and left the safety controls in §6.1/§6.4/§6.6 for a later slice even though three Sesami runbooks that trigger Jenkins are visible to a *default* agent from the moment `run_runbook` ships (§6.2). This plan moves the starter embed, history, redaction, the MCP env allowlist, the arg scope lock, execution lifecycle (timeout/cancel/concurrency), and `propose_runbook`/accept into or before the first agent-usable slice, and adds the missing CI job. No product code in *this* phase; these slices are the implementation DAG after this PRD.

### Slice 1 — Workspace, domain, XDG config

**Scope:** Cargo workspace, `kadou-core` types (`RiskLevel`, `Runbook`, `Parameter`, `Catalog`, `Config`), `config.toml` load/save (missing keys = defaults), XDG paths via `etcetera` (B10), `kadou version`, `kadou --help`.
**Tests:** parse empty TOML; parse full example; `KADOU_HOME` isolation; `DOPS_HOME` fallback with a deprecation warning; risk order / `Exceeds`; config file `0600`; macOS paths resolve to `~/.config`/`~/.local/{share,state}`, not Application Support.
**Done:** `cargo test -p kadou-core` green; `kadou --help` lists the command tree stubs.

### Slice 2 — Catalog loader v1/v2 + Sesami round-trip

**Scope:** Disk loader, strictness (§4.7, per-catalog failure isolation), v1 mapping with the complete known-key list and string-default coercion (§4.3), optional opt-in `catalog.yaml` groups (§4.5), aliases, active flag, load-time risk filter parameterized by ceiling. Read-only use of `~/Bitbucket/sdo-dops-catalog/src` locally; CI uses the sanitized fixture catalog (decision 11, B15).
**Tests:** 32 real YAML files (local run) and the sanitized fixture set (CI) parse; `name`≠dirname fails; missing `risk_level` fails; `ses-argocd-sync` `type: number` imports and its empty-default-but-required param fails as required; 87 string-typed defaults coerce (86 booleans + 1 integer); `select` without `options` fails; shared `scripts/` ignored as runbooks; a bad file in one catalog does not break another active catalog; mixed v1/v2 in one catalog; `catalog.yaml` group without `uses:` does not attach to an unrelated runbook.
**Done:** `kadou list --catalog jenkins-pipelines` (after a test config `add`, catalog policy `max_risk_level = critical`) prints 32 ids without executing anything. (At the human-default `medium` ceiling it prints 30 — `ses-release-build` and `ses-deploy` are hidden, matching §6.2.)

### Slice 3 — Executor + CLI run --dry-run

**Scope:** `kadou-exec` POSIX `/bin/sh`, env injection (CLI/TUI: full parent env; the MCP allowlist path lands with slice 5, since MCP itself doesn't exist yet), cwd = runbook dir, cancel, `kadou run --dry-run`, `kadou info`. Uses the **embedded starter catalog** stub from the outset (a minimal subset ships here; the full TUI-facing embed and installer land in the TUI slice) so `starter.hello-world` exists wherever this PRD references it.
**Tests:** fixture `script.sh` echoes `$FOO`; secret names appear in `env_names` not `env_public`; `..` in `script:` rejected; process-group cancel test (Unix, SIGTERM then SIGKILL); stdin is `/dev/null` and a `cat`-running script does not hang; stdout/stderr piped, not inherited; a bash helper invoked via `sh` still runs as bash (Sesami's exec-bit pattern); reserved env names (`path`, `home`, …) rejected at load.
**Done:** `kadou run starter.hello-world --dry-run` and `kadou run jenkins-pipelines.cc4-aaa --dry-run` print env names including `JENKINS_TOKEN` as secret, no Jenkins HTTP.

### Slice 4 — Vault + secret flag + Go import

**Scope:** age envelope per the exact byte-for-byte spec in §6.5, `0700`/`0600` atomic writes, 3-layer resolve, `secret: true` omitted from describe/history, `kadou vault set` (TTY/stdin only), optional `keyring-core` + `apple-native-keyring-store` feature compiled and tested but off by default. Real (not synthetic-only) Go `vault.json`/`keys.txt` import test using the method in `07` Appendix B item 5, against a **synthetic** payload (never a real `jenkins_token`).
**Tests:** round-trip vault; mask in history struct; MCP-schema helper strips secrets; Go-envelope import decrypts correctly; wrong key gives a clean error; import is idempotent and non-destructive to `~/.dops/`; directories `0700`; `keyring-core` feature compiles on the macOS target.
**Done:** `kadou vault set jenkins_user` persists encrypted via TTY/stdin only; files are `0600`; a synthetic Go-written vault imports successfully.

### Slice 5 — MCP list / describe / run / propose (ship, agent-usable, and safe on day one)

**[rev, B8]** This slice absorbs work the prior plan deferred to slices 6–8, so that the first agent-usable ship is also the first *safe* one:

**Scope:**
- `kadou-mcp` + `kadou mcp serve` stdio. All **four** tools, including `propose_runbook` (moved forward from the old slice 7) — without it, the first agent-usable ship has no "no runbook → propose" loop (`03` §9 rule 2), and shipping it here means the four-tool budget snapshot only needs to be taken once.
- Wire agent ceiling default `low`, full visibility formula (§6.2).
- MCP env allowlist + `[exec] pass_env` (§6.1, moved forward from the old slice-6/wherever-it-was-implied).
- `exec.timeout`, `mcp.max_wait` → `status: running`, cancellation wiring (`notifications/cancelled`, stdio EOF), concurrency limit (§6.1).
- History write path with the fresh plain-text tier and stream redaction (§6.6, moved forward from the old slice 6) — every MCP run gets an audit record from the start; there is no window where the three low-risk Jenkins triggers (`helm-package`, `sdo-k8s-ses`, `ses-automation`) run without one.
- MCP args scope lock (only `local`/`runbook` settable) and "MCP never writes the vault" as a tested invariant (§4.6, §6.5).
- The embedded starter catalog is complete here (full 5 runbooks, §7.6), not stubbed.
- Truncation to 50 lines, compact JSON, `isError` rules.
- `insta` snapshot of the exact four-tool `tools/list` against `docs/design/tools-list.json`.
- `pending_grant` itself, TTL, and dedupe may land in the *next* slice (6) as long as high/critical stay invisible to a default agent in the meantime — the visibility formula alone (this slice) already prevents `ses-deploy`-class exposure.

**Tests:** compact `tools/list` ≤ 2 800 bytes and byte-identical to `docs/design/tools-list.json`; list of Sesami at `allow_risk=low` returns **exactly** the five expected ids (not just "hides high/critical"); `describe cc4-aaa` (test config `allow_risk=medium`, since `cc4-aaa` is medium-risk and invisible at the true default) has `secret_param_names: ["jenkins_token"]` and no token value; `run starter.hello-world` succeeds and produces a history record with `interface: mcp`; run above ceiling → `isError: true, error: no_such_runbook`; a script that echoes `$JENKINS_TOKEN` shows `****` in both the MCP result **and** the on-disk log; a parent-env value like `FOO_TOKEN` set on the MCP server's own process is **not** visible inside the child; MCP `args: {"jenkins_url": "…"}` is rejected as `invalid_args` (global scope locked); no code path lets MCP persist to the vault; stdin/stdout purity under MCP (§6.1); `max_wait` returns `status: running` with a pollable `log_path`; HTTP bind `0.0.0.0` refused, and HTTP itself stays behind its build feature unless `allowed_origins` + bearer token are implemented (§5.1, B11).
**Done:** a local MCP client can list / describe / run / propose over the starter catalog and Sesami, with an audit trail, env isolation, and a scope-locked argument surface from day one. **This is the first agent-usable ship, and it ships safe.**

### Slice 6 — Grants, pending, unified confirm

**Scope:** CLI `--confirm <id>`, pending_grant files with sha/HEAD pin, TTL, dedupe, `pending_path` outcome visibility (§6.4), `kadou grant *`. (History itself already exists from slice 5.)
**Tests:** MCP run of a fixture `risk_level: high` runbook, with catalog policy `critical` and `allow_risk: "critical"` so it is **visible** but not allow-listed → `pending_grant`; `grant allow` then run succeeds; without `allow_risk` raised, the same runbook is simply invisible (`no_such_runbook`), exercising both branches of §6.2 vs §6.3; TTL expiry; dedupe on repeated identical args; approve refuses on a changed `script_sha256`; approve runs in the CLI's environment, not MCP's.
**Done:** with catalog policy `critical` and agent `allow_risk: critical`, `ses-deploy` (not allow-listed) returns `pending_grant`; a human `grant approve` executes it with the pinned script; at the *default* config it is simply invisible to the agent (not "cannot run" — it is never seen).

### Slice 7 — Catalog git install + accept hardening

**Scope:** `kadou catalog install/update/remove`, git CLI clone/pull, `sub_path` escape check, `kadou catalog accept --into` targeting only user-owned catalogs with a diff and `y/N` (§6.7), `catalog rename` deferred (§13). `propose_runbook` itself already shipped in slice 5.
**Tests:** install `--path` cannot escape; accept refuses a git-backed catalog target; accept prints the diff; `kadou catalog install <url> --path src --name jenkins-pipelines` matches SPEC.md.
**Done:** a human can accept an agent's proposal from slice 5 into a user catalog; `catalog install` round-trips a git URL.

### Slice 8 — TUI + starter installer

**Scope:** ratatui app (sidebar, metadata, output, wizard, confirm, `?`, palette `Ctrl+p`, `/` search, `j`/`k`), product theme `doop` default, rust-embed starter catalog packaged under `crates/kadou/starter/` (so `cargo package` works — §3), POSIX `install.sh` with SHA-256 verify. Visual check of default `View()` (VHS or ratatui test backend snapshot).
**Tests:** first-run with empty config shows 5 starter runbooks with the renamed (`target_dir`) params; key `?` overlay; `q` does not fire while a text input has focus; no `init` command; installer dry-run on a temp prefix, checksum mismatch aborts.
**Done:** `curl | sh` shape is in-tree; `kadou` after install is a finished TUI; `kadou mcp serve` still the agent path.

### Slice 9 — Session mining (`kadou-mine`)

**Scope:** `crates/kadou-mine` + `kadou mine` subcommands per `06` §2–4 and this PRD §6.8. Staging catalog `mined` (inactive, reserved name). `list_runbooks include_staging` / `catalog=mined` and `describe_runbook` on `mined.*` list/describe regardless of ceiling (§4.1). `run_runbook` on staging → `error=staging`. LaunchAgent `dev.kadou.mine` installer. Table-driven redaction tests (`06` §4.3) with **synthetic** inputs only. No Go `internal/mine`.
**Tests:** `tools/list` still 4 tools and ≤ 2 800 bytes; `mine_list` is not a registered tool; fixture cluster of 3 synthetic sessions proposes one `format_version: 2` draft; `ghp_`-style fixture never appears in mine logs; `kadou mine approve` copies into inactive `mined`; `kadou catalog accept mined.<x> --into user` is the only path to executable; there is no `active = true` shortcut; MCP run of `mined.*` fails staging; missing Claude transcript → `skip: transcript_missing`.
**Done:** scheduled `kadou mine run --once` can queue a redacted draft; agents can list/describe it regardless of ceiling; they cannot execute or approve it.

### Cross-cutting

**[new, B8]** CI runs on **macOS and Linux** (`/bin/sh` differs — dash vs. bash-in-POSIX-mode) and includes: `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace`, the MSRV check (`cargo +1.88 check --workspace --all-targets`, §3.2), `cargo-deny` (§3.3), and the `tools/list` byte-snapshot against `docs/design/tools-list.json`.

Slice order keeps **a safe MCP path over Sesami** at slice 5, before TUI polish, matching `01` rank 1–6 and `03` "8, 10, 11 are load-bearing." Mining is still last because it is not required to run the Sesami catalog (`02` §7.13 is delayed adoption).

---

## 10. Crate verification log

Retrieved from `https://crates.io/api/v1/crates/<name>` on **2026-09-11**, and re-verified by the independent review the same day (`07` §5.1). User-Agent `kadou-prd-research/0.1`.

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
| etcetera | 0.11.0 | MIT OR Apache-2.0 |
| thiserror | 2.0.20 | MIT OR Apache-2.0 |
| uuid | 1.26.1 | Apache-2.0 OR MIT |
| schemars | 1.2.2 | MIT |
| rust-embed | 8.12.0 | MIT |
| zeroize | 1.9.0 | Apache-2.0 OR MIT |
| keyring-core | 1.0.0 | MIT OR Apache-2.0 |
| apple-native-keyring-store | 1.0.2 | MIT OR Apache-2.0 |
| tracing | 0.1.44 | MIT |
| tracing-subscriber | 0.3.23 | MIT |
| camino | 1.2.5 | MIT OR Apache-2.0 |
| fs-err | 3.3.1 | MIT OR Apache-2.0 |
| sha2 | 0.11.0 | MIT OR Apache-2.0 |
| regex | 1.13.1 | MIT OR Apache-2.0 |
| humantime | 2.4.0 | MIT OR Apache-2.0 |
| flate2 | 1.1.10 | MIT OR Apache-2.0 |
| tar | 0.4.46 | MIT OR Apache-2.0 |
| similar | 3.2.0 | Apache-2.0 |
| libc | 0.2.189 | MIT |
| nix | 0.31.3 | MIT |
| base64 | 0.23.1 | MIT OR Apache-2.0 |
| tempfile | 3.27.0 | MIT OR Apache-2.0 |
| insta | 1.48.0 | Apache-2.0 |
| assert_cmd | 2.2.2 | MIT OR Apache-2.0 |
| cargo-deny | 0.20.2 | MIT OR Apache-2.0 |

**[rev]** `directories` 6.0.0 and bare `keyring` 4.2.0 are removed from this table (rejected — B10, B12; see §3.1 "Explicitly not used").

**Verified, not unverified:** age identity file interoperability with Go `filippo.io/age` `keys.txt` (`07` Appendix B item 5 decrypted a real Go-written envelope). **Still unverified:** live MCP JSON-RPC framing overhead beyond the tens-of-bytes estimate in `01` §10, and a real-tokenizer (as opposed to byte-ratio) measurement of `docs/design/tools-list.json`.

---

## 11. Key decisions (index)

1. Four meta-tools, lazy describe, no resources/prompts capability advertised at all — `03` §8, `02` §7.1, `01` §6; `07` B1, B11.
2. v1 YAML loads with string-default coercion and the complete known-key list; v2 is additive; `catalog.yaml` shared params are opt-in per runbook via `uses:` — `01` §4, §9 Q2; `07` B9.
3. Stable catalog `name`; Sesami should be `jenkins-pipelines` + `sub_path=src` — `01` §4.9.
4. One visibility-and-grant state machine: hidden → `no_such_runbook` everywhere; visible + high/critical + not allow-listed → `pending_grant` with a pinned, TTL'd, deduped record — `03` §10–11; `07` B4.
5. Human ceiling medium (per-catalog, replaces not intersects the global), agent ceiling low, stated as an exact formula — `03` §1, §10; `07` B4.
6. Age vault with a byte-for-byte-specified Go-compatible envelope; optional keyring-wrapped *passphrase* via `keyring-core`; `kadou vault set` for entry — `01` §2.4, `03` §6; `07` B12, B13.
7. Exec file + env; extra catalog files allowed; MCP children get an env allowlist, a timeout, and a concurrency limit, not the full parent environment — `01` §7; `07` B5, B6.
8. History 90d / 50MB / 10MB archives, with a 7-day plain-text fresh tier and pre-write stream redaction — `01` §9 Q8; `07` B3.
9. Skills parse-only in MVP — `01` §9 Q9.
10. CLI+MCP first (re-cut slice order, safety folded into the first agent-usable slice), TUI after, no web — `03` charter 13; `07` B8.
11. Strict loader, per-catalog failure isolation — `01` §9 Q12; `07` B9.
12. Official `rmcp` 3.3.0, not a third-party MCP crate; `serde-yaml-ng` not deprecated `serde_yaml`; `etcetera` not `directories`; `keyring-core` not bare `keyring` — `07` B10, B12.
13. Starter catalog embedded with non-`PATH`-colliding parameter names; install = ready — `03` §2–3; `07` B6.
14. Token budgets: ≤2 800 compact bytes / ≤800 tokens connect (measured: 2 330 B / ~583 tok), ≤1 500 typical run, ≤60s to first runbook — `07` B1.
15. Session mining is `kadou-mine` + `kadou mine`, not four MCP tools; drafts live in the reserved, non-configurable staging catalogs `mined`/`proposed` (flattened layout) and are list/describe-only, regardless of ceiling, until a human runs `catalog accept --into` — `06` §5 vs `03` §8; `07` B14.

---

## 12. Deviations and limits

- This document does not implement code. Crate versions will drift after 2026-09-11.
- Token-per-connect target is a budget on **our** `tools/list` payload, measured as compact bytes and a 4 B/token (and 3.5 B/token) estimate — not a billed-token measurement from Claude/Cursor, and not a real tokenizer count (`07` §3.1).
- Go vault import is **verified** (`07` Appendix B), not best-effort as the prior draft stated; the envelope is specified byte-for-byte (§6.5).
- This PRD picks the product name `kadou`; no `04-naming.md` exists in this tree. If one is authored later, it documents the *process*, not a still-open decision.
- Publishing `install.sh` to a stable URL, crates.io, and GitHub Releases is gated on Mason (repo README Gates).
- `06` §5.1 Go layout is explicitly ignored (planner note, inbox 002).
- `kadou catalog rename --migrate-vault` (§4.2, §8.2) is deferred, not designed here.
- Live MCP host behavior (per-call timeouts, tool-search deferral by real hosts) was not run; `07`'s findings are static analysis of `rmcp` 3.3.0 source plus a scratch-crate probe, not a live host trace. The Linux `/bin/sh` (dash) behavior of the Sesami scripts was not run in this review pass.
- Did not wiki-ingest (assignment). Did not push.

---

## 13. Revision log

This revision resolves `docs/design/07-review.md` §8.1 (blocking) and takes the cheap items from §8.2 (suggestions). Every blocking id below is closed somewhere in this document; the "Sections changed" column is where to look.

### Blocking (07 §8.1)

| Rank | ID | Revision | Sections changed |
|---|---|---|---|
| 1 | B4 | One visibility-and-grant state machine; hidden → `no_such_runbook`; pending record gets a sha/HEAD pin, 24h TTL, dedupe, `pending_path` | §2 row 4, row 5; §5.5 (`pending_grant` result); §6.2; §6.3; §6.4 |
| 2 | B2 | No secret exfiltration through MCP `args`; MCP never writes the vault | §1.2; §4.6; §6.1 (args→env); §6.5 |
| 3 | B5 | MCP child environment allowlist, `exec.timeout`, `mcp.max_wait`→`running`, cancel (SIGTERM→SIGKILL), concurrency limit | §3 (kadou-exec deps); §3.1 (libc/nix); §6.1; §6.2; §8.2 (risk row) |
| 4 | B3 | Redact before persisting; 7-day plain-text fresh tier; retention for pending/proposed | §2 row 8; §3.1 (flate2/tar); §5.5 (`log_path`); §6.6 |
| 5 | B8 | Re-cut the slice plan; move starter embed, history, redaction, env allowlist, arg lock, lifecycle, propose/accept into or before the first agent-usable slice; add the CI job | §9 (entire section) |
| 6 | B1 | Meet the token budget; rewrite the four schemas; drop `$schema`/`$id`/`title`; hand-authored `inputSchema`; gate at 2 800 B; compact JSON; `isError` rules; describe returns `args_schema`/`resolved`/capped script; define `summary` | §1.3; §5.1; §5.2; §5.4; §5.5; §8.2 (risk row); `docs/design/tools-list.json` |
| 7 | B6 | Env naming/reserved names; per-type args→env serialization; rename starter `path` params; drop `getent` | §4.4; §5.4 (`args` description); §6.1; §7.6 |
| 8 | B7 | Harden `propose_runbook`/accept: `catalog` pattern + containment, size caps, required `risk_level`, strict load at propose, `proposal_id` rename, accept into user-owned catalogs only with diff + y/N | §5.4 (`propose_runbook`); §5.5 (`propose_runbook` result); §6.7 |
| 9 | B9 | Loader and `catalog.yaml`: string-default coercion, complete known-key list, per-catalog failure isolation, empty default ≠ required, opt-in shared params via `uses:` | §2 row 2, row 12; §4.3; §4.5; §4.7 |
| 10 | B10 | XDG paths: `etcetera` replaces `directories` | §3; §3.1; §7.5 |
| 11 | B11 | HTTP transport: `allowed_origins`, per-launch bearer token, or keep HTTP gated behind a build feature until both exist | §5.1 |
| 12 | B12 | Vault crypto: byte-for-byte Go envelope spec; passphrase-in-keyring semantics; `keyring-core` + `apple-native-keyring-store`; MSRV resolved | §2 row 6; §3.1; §3.2; §6.5 |
| 13 | B13 | Secret entry: `kadou vault set` from TTY/stdin; `config set` refuses vault keys; `--param` refuses secret names | §1.2; §6.5; §7.1; §7.3 |
| 14 | B14 | Staging model: visible regardless of ceiling, reserved `mined`/`proposed` names, flattened `proposed/` layout, `accept --into`, no `active = true`, miner writes `format_version: 2` | §2 row 14, row 15; §4.1; §4.2; §5.3; §6.8; §7.5 |
| 15 | B15 | CI fixtures: sanitized shape-preserving catalog for CI, `KADOU_TEST_CATALOG` for the real tree locally; scrub internal hostnames/workspace names from examples | §2 row 11; §4.5 and §6.5 example URLs/hostnames; §9 (slice 2) |

### Suggestions taken (07 §8.2, cheap)

| # | Suggestion | Where |
|---|---|---|
| 1 | MCP tool annotations (`readOnlyHint`, `destructiveHint`, `openWorldHint`) | `docs/design/tools-list.json`; §5.2; §5.4 |
| 2 | `describe_runbook` returns `catalog_root` + helper file list | §5.2; §5.5 |
| 3 | Pin `grant allow` to a script digest, `--any-version` to opt out | §6.4 |
| 4 | Drop `vault.passphrase_cmd` from MVP | §6.5 |
| 5 | Add `mcp.max_output_lines`/`mcp.max_wait` to the closed configurable list | §7.5 |
| 6 | `cargo-deny` in CI | §3.3; §9 (cross-cutting) |
| 7 | Watch `serde-yaml-ng` maintenance; keep the round-trip test as the swap guard | §3.1 |
| 8 | Move the starter catalog embed under the `kadou` bin crate | §3 (workspace layout) |
| 9 | Palette chord `Ctrl+p` instead of `Ctrl+Shift+p` | §7.2 |
| 10 | Document the cwd-vs-Go change and SIGTERM-before-SIGKILL | §2 row 7; §6.1 |
| 11 | `kadou mcp schema --bytes` for operator-visible budget checks | §7.1 |
| 14 | State that `mcp_client` is self-reported | §6.4; §6.6 |
| 15 | Decide `XDG_*_HOME` is not a second override next to `KADOU_HOME` | §7.5 |

### Deferred (not applied in this revision)

| # | Suggestion | Why deferred |
|---|---|---|
| 12 | `kadou catalog rename --migrate-vault` | Requires implementation-phase design of a vault-key migration, not a documentation-only fix; noted in §4.2, §8.2, §12. |
| 13 | Sesami catalog hygiene (drop the `~/.bashrc` credential `eval`, stop printing `-u user:pass` in `--dry-run`, review `curl -k`) | Lives in `~/Bitbucket/sdo-dops-catalog`, which this assignment's authorized scope holds read-only; it is a catalog-content fix, not a kadou product fix. Flagged for a separate, catalog-owning task. |

### Naming (07 §10, applied)

The prior draft's codename ("dops" hyphen "next", retired) is replaced by `kadou` (binary, crates, config/data/state dirs, env prefix, MCP server name, keyring service, LaunchAgent label, and all strings in tool descriptions and CLI examples) throughout this document. `~/.dops/` import paths and the reference catalog path `~/Bitbucket/sdo-dops-catalog` stay literal, since they name the pre-existing Go product and its data, not kadou's own layout. The theme name `doop` is left as-is — `07` §10 calls that a naming-adjacent decision for a later phase, not implied by the binary rename.
