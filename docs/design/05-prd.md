# kadou PRD and architecture

**Date:** 2026-09-11
**Status:** design (phase 5, second revision — single-file kata shape)
**Product name:** **kadou** (稼働), chosen by Mason. Binary `kadou`, crates `kadou-*`, config/data/state dirs `~/.config/kadou` / `~/.local/share/kadou` / `~/.local/state/kadou`, env prefix `KADOU_*`, MCP server name `kadou`, keyring service `kadou`, LaunchAgent label `dev.kadou.mine`. The legacy Go product's on-disk paths (`~/.dops/`) are named **literally** wherever this document imports from them — that product is not renamed.

This is the product and architecture contract for the Rust rewrite. It obeys `docs/design/03-principles.md`, with `docs/design/08-shape-review.md` and `docs/design/09-tui-decision.md` winning over this document and `docs/design/07-review.md` wherever they conflict (Mason's decisions, 2026-09-11). Every numbered decision cites `01-audit.md`, `02-competitors.md`, `03-principles.md`, `08-shape-review.md`, or `09-tui-decision.md`. Session-mining rows also cite `06-session-mining.md`. This revision applies the single-file **kata** shape from `08` and the no-TUI decision from `09`; `07`'s safety findings (B1–B15) are re-verified against the new shape in `08` §6.2 and carried through unchanged in substance (§6, §8.2 below). See the **Revision log** (§13) for the full delta map.

**Inputs (read-only):**

- `docs/design/01-audit.md` — Go product at `~/origin/dops` `795d2d2` (tag `v0.13.1`, feature-complete at v0.12.0)
- `docs/design/02-competitors.md` — adjacent MCP / task-runner / skills products
- `docs/design/03-principles.md` — charter and non-goals, as revised by `09` §6.1
- `docs/design/04-naming.md` — naming research; the product name `kadou` and the unit name `kata` are Mason's choices (`08` §5)
- `docs/design/06-session-mining.md` — pipeline, redaction, review gate (engine packaging in `06` §5 is Go `cmd/` + `internal/`; **ignore that layout** — kadou is Rust)
- `docs/design/07-review.md` — independent review of the prior draft. Its blocking items (B1–B15) are resolved in the first PRD revision and re-verified against the kata shape in `08` §6.2.
- `docs/design/08-shape-review.md` — single-file kata, folders as namespaces, no registry. Adopted in full (§1 verdict).
- `docs/design/09-tui-decision.md` — drops the full-screen TUI for a styled CLI with a built-in picker. Adopted in full (§1 verdict, decision D7).
- Reference workload `~/Bitbucket/sdo-dops-catalog` — 32 `src/*/runbook.yaml` (disk count 2026-09-11), 29 wrappers share `scripts/trigger-pipeline.sh` via an identical `REPO_ROOT` idiom, three global params incl. secret `jenkins_token`, `device-log-metrics` multi-file
- crates.io versions and licenses, retrieved 2026-09-11 (see §10). Anything not retrieved is marked **unverified**.

No secrets appear in this document. The Sesami kata secret is named `jenkins_token` only. No internal hostnames or Bitbucket workspace identifiers appear in examples (`07` B15) — Jenkins and git remote examples below use placeholder domains.

---

## 1. Product summary, users, and success metrics

### 1.1 What it is

kadou is a **script library** that is also an **MCP server for AI agents**. The job is to cut model tokens by preferring reviewed POSIX scripts over free-form reasoning (`03` charter 8–9; `02` §7.3).

The unit is a **kata**: one script with a header. Kata live in **folders** under `~/.config/kadou/kata/`. There is no registry (`08` §1).

One binary exposes two interfaces over one engine (`03` charter 13, as revised by `09` §6.1; `01` §2.1):

| Interface | Entry | Who |
|---|---|---|
| CLI | `kadou` (library frame), `kadou run` (picker when no id), `kadou list/show/new/edit/check/get/update/accept/trust/vault/history/grant/mine` | Operators, scripts, CI, scheduled mining |
| MCP | `kadou mcp serve` (stdio default) | Mason's agents (Claude Code, Cursor, Codex, Grok, …) |

There is no full-screen TUI in v1 (`09` D7). The CLI has a built-in picker and inline prompts on a TTY (`09` §3). It is not a general agent harness, not a web app, and not a hosted control plane (`03` non-goals).

The migratable unit is today's dops catalog: a directory of `runbook.yaml` + `script.sh`, parameters as `UPPER_SNAKE` env vars, four risk words, age-encrypted vault, git-installable catalogs (`01` §2.2–2.6). The rewrite **ports that convention** — one script, one set of env vars, four risk words — and **replaces the container**: a runbook directory becomes a kata file, a catalog registry becomes a folder on disk, and `catalog add/install` becomes `kadou get`/`kadou import` (`08` §1 table). It **does not port** one-MCP-tool-per-runbook, agent-default-`critical`, or the format-version loader (`03` scoreboard; `01` §2.9, §8 rank 7; `08` §1).

### 1.2 Users

**Mason's agents.** Local stdio MCP clients. They list, describe, and run kata; they may propose one as a single file. They never raise their own risk ceiling, never see secret values, never write the vault (`07` B2; §4.4, §6.5), and never accept their own proposals (`03` §10–11).

**DevOps operators.** Humans in a shell. They `kadou get` the team folder (Sesami's 32 Jenkins-trigger kata first), save `jenkins_url` / `jenkins_user` / `jenkins_token` once in the vault with **`kadou vault set [--plain] <name>`** (`07` B13; §6.5), raise ceilings, grant specific high/critical ids to agents, and are told by the agent's own message, a desktop notification, or `kadou`'s `needs you` block that a grant, draft, or proposal is waiting — and act with one command (`09` §4).

Same engine, same folders, same vault, same risk policy. A web UI is deferred and is not a v1 driver (`01` §3.2, §9 Q10; `03` non-goals).

### 1.3 Success metrics (numbers)

Token figures for *today* are schema-replay, not a live MCP wire trace (`01` §6, §10). Targets below are product constraints (`03` §8 rule 6).

| Metric | Today (Sesami 32-kata folder) | kadou target | Why |
|---|---|---|---|
| **Tokens per connect** (`tools/list` only — `resources/list` and `prompts/list` are not called; §5.1) | ~8 800 (`01` §6.1: 33 940-byte `tools/list` ≈ 8 500 tokens @ 4 chars) | **≤ 800 tokens**, stretch **≤ 500**. CI gate: compact `tools/list` ≤ **2 800 bytes** (`07` B1). The measured payload is `docs/design/tools-list.json`: **2 028 compact bytes ≈ 507 tokens @ 4 B/token, ≈ 579 @ 3.5 B/token.** Both under budget with ~772 bytes of headroom — smaller than the first revision's 2 330 bytes because `propose_kata`'s schema shrank from six properties to two (`08` §6.2). | Four constant meta-tools, hand-authored `inputSchema` (§5.4). GitHub MCP's 93-tool ~55k dump is the failure mode (`02` §5, §7.1). |
| **Tokens per run** (tool result body) | Last 50 lines, unstructured-ish JSON (`01` §4.11, `internal/mcp/tools.go`) | **≤ 1 500 tokens** typical (last **50** lines + exit code + duration_ms + log_path + history_id), **compact JSON, not pretty-printed** (`07` §3.3). Hard cap **8 192** output UTF-8 bytes before truncation notice | Server-side truncate; full log on disk, and the fresh-tier log is a plain-text file an agent's own tools can read (`07` B3). |
| **Time to first kata after install** | curl installer, then `dops init`, then empty-or-hello-world (`03` §3 dops-today) | **≤ 60 s** wall clock from `curl \| sh` to `kadou` showing the starter folder on a warm network. **≤ 10 s** from a completed install to the first `kadou` frame / first `list_kata` on a local SSD (`09` §3.2 — there is no TUI frame to wait for) | Install = ready (`03` §3). Empty folder is a product bug (`03` §9 rule 6). |
| **Connect cost vs library size** | Linear: 32 runbooks ~8.5k tokens; SPEC's 370 pipelines extrapolate ~98k (`01` §6.2 item 4) | **O(1)** in kata count. 32 and 370 pay the same `tools/list` | Meta-tools (`02` §1.5, mise MCP in `02` §2.3). |
| **Sesami import** | n/a | All **32** `runbook.yaml` files convert with `kadou import`, check with **0 errors**, `describe_kata` round-trips, and `kadou run --dry-run` yields the same env names as today (`08` §6.1, §6.2). At the default agent ceiling (`low`) only 5 of 32 kata are visible over MCP (`08` §6.1 risk mix: medium 25, low 5, high 1, critical 1) — that is by design, not an import failure. | `01` §9 Q11; `08` §6.1; `07` §1 row "1.3", B1. |

A change that grows the default `tools/list` past the 2 800-byte gate is a principles violation, not a feature (`03` §8 rule 6).

---

## 2. Decision table (revised for the kata shape)

Each row is a closed decision. Rows 1–15 close `01` §9 as before; rows 16–17 are new, closing `08` §8 and `09` §5. Changes from the first PRD revision are marked **[shape]** (from `08`) or **[tui]** (from `09`).

| # | Question | Decision | Reason | Cite |
|---|---|---|---|---|
| 1 | MCP tool shape | **Four meta-tools only:** `list_kata`, `describe_kata`, `run_kata`, `propose_kata`. No tool per kata. No resource-only library. No history tool. **No `mine_*` tools** (decision 15). **[shape]** Renamed from `*_runbook`; `propose_kata`'s schema shrinks from six properties to two. | Constant-size surface stays flat at any folder count; short names save bytes on every connect. | `01` §6.2, §9 Q1; `02` §7.1, §7.8; `03` §8 rules 1–2; `08` §2.8, §6.2 |
| 2 | Format versioning | **[shape, replaced]** One header grammar (§4.3). No `format_version`. Old dops catalogs are converted once by `kadou import`; the product never loads `runbook.yaml`. | A closed six-key comment header is the whole schema; a v1/v2 loader and `catalog.yaml` opt-in groups added surface without adding safety. | `08` §1 items 1–2, §3, §7 |
| 3 | Folder identity | **[shape, replaced]** Folder name is the id prefix and is the directory name. Rename by renaming the directory; history ids and last-used args follow the old id and are not migrated. | Ids are paths on disk; a folder exists because it is on disk, not because it is registered. | `08` §4.1, §7 |
| 4 | Confirm protocol | **One visibility-and-grant state machine, two faces: CLI and MCP.** **[tui]** CLI: high prompts `run? [y/N]` on a TTY (default No), else requires `--confirm <id>`; critical prompts for the typed id on a TTY, else requires `--confirm <id>`. **MCP: no confirm strings in schemas.** `pending_grant` happens **only** when a kata is *visible* to the agent (§6.2 formula) **and** is high/critical **and** its id is not in `[agent].allow` at a sufficient ceiling. A kata the agent cannot see returns `no_such_kata`, never `pending_grant`. The pending record is pinned to the kata file's sha256 (+ folder git HEAD when present), expires after 24h, and dedupes on `(id, args_hash)` (§6.4). | Schema `_confirm_id` is copyable theater; a TUI overlay was a third face that could drift from the CLI/MCP pair (`01` §2.1). Dropping it collapses three faces to two. | `01` §4.6, §9 Q4; `03` §10 rules 2–3; `07` C3, B4; `09` §3.4, §6.2 |
| 5 | Default risk ceiling | **Human `max_risk = "medium"` per folder, overridable.** **Agent `max_risk = "low"`.** `--allow-risk` on `mcp serve` cannot exceed config. **[shape]** Formula with `folder` substituted for catalog (`08` §4.4): `human_ceiling(f) = folder[f].max_risk ?? max_risk`; `agent_ceiling(f) = min(agent.max_risk, folder[f].agent_max_risk ?? agent.max_risk, --max-risk, human_ceiling(f))`. **[tui]** Project-local trust (§6.2) is a visibility term alongside ceiling. | Today's MCP default `critical` is inverted omakase-safe; per-folder policy matches the per-catalog formula it replaces. | `01` §4.6, §9 Q5; `03` §1 rule 4, §10 rules 1 and 3; `07` B4; `08` §4.4 |
| 6 | Vault portability | **Age X25519 identity, local-first (default).** Identity file `0600` under the data dir. Optional keyring wrap of the identity's **passphrase** via `keyring-core` + `apple-native-keyring-store` on macOS. **[shape]** Payload is now a **flat** `{ "<name>": { "value", "secret" } }` map (§6.5) — needs are one namespace, not scoped per folder. Go `vault.json` v1 import maps `global.*` to entries and reports/drops `catalog.*` runbook-scope values (or writes them as last-used args). | Needs replace scopes structurally (`08` §1 item 2), so the vault payload no longer needs a scope tree. | `01` §2.4, §4.7, §9 Q6; `03` §6 rules 4–5, charter 12; `07` §4.5, B12; `08` §6.2 |
| 7 | Script contract | **Exec the kata file; the shebang is the runtime.** **[tui, D2]** `argv` is `<interpreter-from-shebang> <file>` or `/bin/sh <file>` when there is no shebang. `kadou check` warns when the interpreter is not on `PATH`. cwd is the kata's directory. Shared helpers **outside** the kata file (e.g. `scripts/trigger-pipeline.sh`) are reached via `$KADOU_ROOT`, not a second `script:` field. No `run_shell` tool. | 29 Sesami wrappers `dirname "$0"` up to a shared script; a single-file kata sits one level shallower, so `kadou import` rewrites the idiom once instead of the product carrying a two-file contract forever. | `01` §2.2, §7, §9 Q7; `03` §1 rule 5 (widened), §7 rule 6, §9 rules 1–2; `07` C16; `08` §1 item 4, §3.7, §6.1; `09` D2 |
| 8 | History policy | **Implement the documented policy, not the ignored size-cap.** JSON records + gzip-tar log archives. Fresh tier: the last **7 days** of logs are plain `0600` text files, so a running or recent run's `log_path` is a file an agent's own tools can open; older logs compress into 10 MB gzip-tar archives. **90-day TTL** and **50 MB total cap** unchanged. Secrets masked `****`; the raw output stream is redacted **before** it is written to the log or returned to MCP (§6.6). **[shape]** `catalog_name` → `folder`; `runbook_id` → `id`; `runbook_name` dropped. **[tui]** `interface` is `cli` \| `mcp` (no `tui`). | Audit of Jenkins triggers needs retention; `log_path` must stay a file, not a tar entry, for an agent's own file tools. | `01` §2.8, §4.10, §9 Q8; `03` §11 rule 5; `07` C15, B3; `08` §6.2; `09` §6.2 |
| 9 | Skills | **`*.md` beside kata are ignored by the loader. Skills are not a header type.** | Zero `skill.md` in origin catalogs or sdo-dops-catalog; registering all skills violates lazy MCP. | `01` §3.2, §4.8, §9 Q9; `02` §4.1; `03` §8 rule 3; `08` §7 |
| 10 | CLI/MCP in v1, TUI dropped | **[tui, D7]** **CLI + MCP are the product.** No full-screen TUI in v1; the CLI has a built-in picker and inline prompts on a TTY (§7; `09` §3). Web UI is a non-goal. No `kadou open`. | Two interfaces, not three; a fourth interface (a third confirm face) is how today's MCP/TUI confirm drifted (`01` §2.1). An operator who mostly reviews what agents did needs to be told, not to be somewhere (`09` §5, §7 reversal condition). | `01` §3.1–3.2, §9 Q10; `03` charter 13 (revised); `09` §1, §5 |
| 11 | Compatibility tests | **Yes.** 32 Sesami YAML files are the acceptance suite: `kadou import`, check with 0 errors, `describe_kata` round-trip, `run --dry-run` env map. **Do not execute Jenkins in CI.** CI cannot read `~/Bitbucket/sdo-dops-catalog`; it uses **sanitized, shape-preserving fixtures** under `tests/fixtures/sesami-shaped/` (32 stub `.sh` kata, no real hostnames or workspace names), and the real tree is used locally via `KADOU_TEST_CATALOG=~/Bitbucket/sdo-dops-catalog`. | The catalog is the compatibility target (`01` §1), but CI must not depend on a path that only exists on Mason's machine. | `01` §7, §9 Q11; `07` §2 row "2 row 11", B15; `08` §7 |
| 12 | Loader strictness | **`kadou check` is the loader.** A folder with any header error is listed with `✗` and none of its kata run; other folders are unaffected. Unknown keys, unknown types, missing `about`/`risk`, tabs, an unclosed header are errors with a fix line (§4.3, §4.7). | A bad file in one team's folder must not brick the starter folder or the server; cargo-shaped diagnostics are the same text a human and an agent both see. | `01` §4.1–4.2, §9 Q12; `03` §7 rule 1; `07` §2 rows "2 row 12"/"4.7", B9; `08` §2.7, §6.2, §7 |
| 13 | Secret parameter vs need | **[shape, replaced]** Secrets are **needs**. Args cannot be secret. An arg is settable by an agent through `run_kata`; a secret must never be, so a secret is never an arg — it is a need the vault supplies. | This is the whole of `07` B2 expressed as a grammar (`08` §1 item 2), not a runtime check. | `01` §2.4, §9 Q13; `03` §10 rule 4; `08` §1, §3.4, §3.6 |
| 14 | Staging / inactive folders | **[shape, replaced]** No active flag. Project-local `./kata/` is human-runnable without the vault and agent-invisible until `kadou trust` (§4.5; `09` D3). Drafts live in the state dir under `proposed/<folder>/<name>` and `mined/<name>`, are listable with `include_drafts`, and are never runnable; `kadou accept <id>` is the only path in, and it refuses a target folder that is a git checkout (§6.7). | No registry means no `active = true` shortcut to reuse; the trust gate and the accept gate replace it with two narrower rules. | `01` §2.6–2.7, §9 Q14; `03` §7 rule 2; `06` §2.9, §5.3; `07` §3.6, C7, C8, B14; `08` §1, §4.2, §6.2 |
| 15 | Session mining MCP mapping | **Do not add those tools, resources, or prompts.** Engine is crate `kadou-mine` + CLI `kadou mine` (not Go `cmd/mine.go`). `mine_list` → `list_kata` (`folder=mined` or `include_drafts=true`); `mine_get` → `describe_kata` on `mined.<slug>`; `mine_run` → **CLI/LaunchAgent only** (`kadou mine run --once`); `mine_review` approve → human `kadou mine approve` / `kadou accept … --into <folder>`; reject/skip → human `kadou mine reject\|skip`. No `kadou://mine/*` on `resources/list`. No MCP approve. **[shape]** The miner writes a **single-file draft with a header**, not `format_version: 2` YAML. | Four extra tools would break the ≤4 budget; an inactive `mined` folder is already the review gate. | `06` §5 (ignore Go layout), §5.3, §2.9; `03` §8 rules 1 and 6, §11 rules 1–2; `02` §7.13; `07` §6.8, B14; `08` §6.2 |
| 16 | **[new, shape]** Kata is one file | **Kata is one file with a closed-grammar header; folders are namespaces; no registry.** A folder of kata looks like a folder of scripts, because it is one. | Removes the four things people found confusing (runbook vs catalog, v1 vs v2, `catalog.yaml` groups, `catalog add` vs `install`) and replaces them with things people already know: a script, a folder, `git clone`. | `08` §1, §11 |
| 17 | **[new, tui]** Human review is told, not housed | **The agent's MCP result carries the exact human command (`approve`, `accept`), the server posts a desktop notification, and `kadou`'s `needs you` block is the durable copy.** No screen to sit in. | An operator whose main job is reviewing what agents did needs to be told, not to be somewhere; a badge in a TUI footer required being inside kadou to see it. | `09` §4, §5, §6.2 §11 |

---

## 3. Rust workspace layout

Edition 2024. **MSRV 1.88** (required by `rmcp` 3.3.0). One binary named `kadou`. Workspace at repo root.

```
kadou/
  Cargo.toml                 # workspace
  crates/
    kadou/                   # bin: clap CLI, ui module (style, frames, picker, prompts, notify), wires MCP
      starter/                # embedded starter kata, inside the bin crate so `cargo package` works for rust-embed
    kadou-core/              # domain, config, header parser, folder scanner, `check` diagnostics, vault, last-used args, history
    kadou-exec/              # process group, shebang runtime, env injection, dry-run
    kadou-mcp/               # rmcp server, 4 tools, result truncation
    kadou-mine/              # session mining engine (ingest/parse/cluster/redact/propose)
  docs/design/
  tests/fixtures/            # sesami-shaped stub folder for CI (decision 11), headers/{good,bad}/ (bad-header snapshot corpus, 08 §6.2 risk 1)
```

**[shape]** No `kadou-tui/` crate. There is no ratatui app; the picker and prompts live in the `ui` module of the `kadou` bin crate (`09` §3.7).

| Crate | Responsibility | Depends on |
|---|---|---|
| `kadou-core` | header parser, folder scanner, `check` diagnostics, vault, last-used args, history, XDG paths, TOML config | serde, toml, **serde-yaml-ng (import path only — see below)**, age, etcetera (XDG, not `directories` — B10), uuid, thiserror, zeroize, base64 |
| `kadou-exec` | `Runner::run(ctx, kata_path, env)`, cwd = kata dir, **MCP env allowlist**, `[exec] timeout` / per-kata `timeout:` header key, cancel via process group (SIGTERM then SIGKILL) | kadou-core, tokio, libc/nix/rustix (process-group kill) |
| `kadou-mcp` | stdio (default) + loopback HTTP (opt-in, gated — §5.1); the four tools with **hand-authored** `inputSchema`; no resources/prompts on the default list | kadou-core, kadou-exec, rmcp, tokio, serde_json |
| `kadou-mine` | index.jsonl ingest, per-agent parsers, normalize/cluster/rank/redact/propose; **no MCP types**; writes single-file header drafts | kadou-core, serde_json, regex, sha2, tokio, similar (diff) |
| `kadou` | `main`, clap command tree including `kadou mine`, `ui` module (style, frames, picker, prompt, notify), rust-embed starter, install-time version | all of the above, clap, clap_complete, rust-embed |

**Why split this way:** one engine (`kadou-core` + `kadou-exec`) shared by CLI and MCP so confirm/risk/vault cannot drift (`01` §2.1; `09` §6.2 — two interfaces, not three). Mining is a local batch job (`06` §3) that writes drafts; it must not sit inside `kadou-mcp`. `serde-yaml-ng` moves to the `kadou import` conversion path only — the product's own loader never parses YAML again (`08` §7 §3 row).

### 3.1 Key dependencies (crates.io, re-verified 2026-09-11)

Versions are **max stable on crates.io on 2026-09-11**. Pin in `Cargo.toml` with `^` of the major.minor recorded here; bump only with a design note if a major moves. **[shape]** changes from the first PRD revision are marked.

| Crate | Version | License | Why |
|---|---|---|---|
| **clap** | 4.6.6 | MIT OR Apache-2.0 | CLI tree, derive, completions. |
| **clap_complete** | 4.6.9 | MIT OR Apache-2.0 | Shell completions. |
| **crossterm** | 0.29.0 | MIT | **[shape, kept]** Picker raw mode and prompts (`09` §3.7, §6.2 — no longer a ratatui backend, since ratatui is gone). |
| **anstyle** | 1.0.14 | MIT OR Apache-2.0 | **[new, shape]** Styled terminal output that strips itself on a pipe or with `NO_COLOR`; clap already depends on it, so it is free (`09` §3.7). |
| **anstream** | 1.0.0 | MIT OR Apache-2.0 | **[new, shape]** Auto-detecting stream wrapper paired with `anstyle` for the `style` printer module. |
| **nucleo-matcher** | 0.3.1 | MPL-2.0 | **[new, shape]** Helix's fuzzy matcher; scores the inline picker's filter (`09` §3.3, §3.7). |
| **inquire** | 0.9.4 | MIT | **[new, shape]** Text/int/bool/select prompts for `kadou run`'s missing-arg flow (`09` §3.4, §3.7); hand-rolled ~300 lines is the documented fallback if its look cannot follow the theme. |
| **rmcp** | 3.3.0 | Apache-2.0 | **Official** Rust MCP SDK. Build with `default-features = false`, features `["server", "macros", "transport-io"]`; HTTP (`transport-streamable-http-server`) behind a `kadou` cargo feature so stdio builds carry no HTTP stack (`07` §5.3). |
| **rmcp-macros** | 3.3.0 | Apache-2.0 | Tool impl macros; same repo. |
| **tokio** | 1.53.1 | MIT | Async runtime. Features: `rt-multi-thread`, `process`, `signal`, `io-util`, `macros` (`07` §5.1). |
| **serde** | 1.0.229 | MIT OR Apache-2.0 | Serde. |
| **serde_json** | 1.0.151 | MIT OR Apache-2.0 | MCP JSON (compact, not pretty — `07` §3.3) + history records. |
| **serde-yaml-ng** | 0.10.0 | MIT | **[shape]** Used only by `kadou import` to read old dops `runbook.yaml`. **Not** `serde_yaml` 0.9.34+deprecated. **Not** `serde_yml` 0.0.13. Verified 32/32 against the real Sesami tree; watch maintenance (last release 2024-05-26), keep the 32-file import as the swap guard for `serde-saphyr` or `serde_norway` if needed. |
| **toml** | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 | Config parse (`kadou.toml`). |
| **toml_edit** | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 | Comment-preserving `kadou trust` / `kadou grant allow` config edits. |
| **age** | 0.12.1 | MIT OR Apache-2.0 | Vault. crates.io description still says **[BETA]**; it is the Rust port of the same age (X25519 + ChaCha20-Poly1305) Go uses. **Go import verified** (`07` Appendix B). |
| **etcetera** | 0.11.0 | MIT OR Apache-2.0 | **[B10]** XDG base directories on macOS/Linux. Replaces `directories` 6.0.0, which maps config *and* data to `~/Library/Application Support` on macOS and returns no state dir at all. |
| **thiserror** | 2.0.20 | MIT OR Apache-2.0 | Typed errors in libraries. |
| **uuid** | 1.26.1 | Apache-2.0 OR MIT | Pending / history ids (v4). |
| **schemars** | 1.2.2 | MIT | Used only to **deserialize** tool arguments into typed Rust structs. `inputSchema` on the wire is **hand-authored JSON** (`docs/design/tools-list.json`), not schema-derived. |
| **rust-embed** | 8.12.0 | MIT | Embed starter kata + bundled themes. |
| **zeroize** | 1.9.0 | Apache-2.0 OR MIT | Wipe decrypted vault buffers. |
| **keyring-core** | 1.0.0 | MIT OR Apache-2.0 | **[B12]** Replaces bare `keyring` 4.2.0, whose default dependency chain forces a higher MSRV than this workspace declares. |
| **apple-native-keyring-store** | 1.0.2 | MIT OR Apache-2.0 | **[B12]** macOS Keychain backend for `keyring-core`. |
| **tracing** / **tracing-subscriber** | 0.1.44 / 0.3.23 | MIT | Structured logs to stderr (MCP must not write on stdout). |
| **camino** | 1.2.5 | MIT OR Apache-2.0 | UTF-8 paths. |
| **fs-err** | 3.3.1 | MIT OR Apache-2.0 | IO errors with paths. |
| **sha2** | 0.11.0 | MIT OR Apache-2.0 | Kata file digest in `describe_kata`; grant-approval pin (§6.4). |
| **humantime** | 2.4.0 | MIT OR Apache-2.0 | Duration display; header `timeout:` parsing. |
| **regex** | 1.13.1 | MIT OR Apache-2.0 | Mining redaction rules (`06` §4.2); history stream redaction (§6.6); header arg-line parsing helpers. |
| **flate2** | 1.1.10 | MIT OR Apache-2.0 | History `.log.gz` tar archives (aged-out tier). |
| **tar** | 0.4.46 | MIT OR Apache-2.0 | Same. |
| **similar** | 3.2.0 | Apache-2.0 | `propose_kata` diff generation, and the accept-time diff shown to a human (§6.7). |
| **libc** | 0.2.189 | MIT | `killpg` for process-group SIGTERM/SIGKILL. |
| **nix** | 0.31.3 | MIT | Higher-level process-group / signal wrapper over `libc`. |
| **base64** | 0.23.1 | MIT OR Apache-2.0 | Go vault import: the envelope is `"age1" + base64::STANDARD_NO_PAD(<ciphertext>)` (§6.5). |
| **tempfile** | 3.27.0 | MIT OR Apache-2.0 | Atomic config/vault/proposal writes (write-to-temp, fsync, rename). |

**Dev / test:** `assert_cmd` 2.2.2, `predicates` 3.1.4, `insta` 1.48.0 (snapshot the exact `tools/list` bytes against `docs/design/tools-list.json`, plus the `kadou` frame, `kadou run` frame, `kadou check` frame, and the picker preview — styled and plain, `09` §3.7), `proptest` 1.11.0 — all MIT OR Apache-2.0 except insta (Apache-2.0). `cargo-deny` 0.20.2 (rust-version 1.88) in CI for license/advisory gating.

**Explicitly not used**

| Crate | Why not |
|---|---|
| `ratatui` 0.30.2 | **[shape, removed]** No full-screen TUI in v1 (`09` D7). The picker and prompts are inline CLI components over `crossterm`, not a ratatui app. Reversal: adding a TUI later is one new crate `kadou-tui`, one slice, no format or config change (`09` §7). |
| `serde_yaml` 0.9.34+deprecated | Deprecated on crates.io. |
| `directories` 6.0.0 | **[B10]** Wrong macOS paths, no state dir. See `etcetera` above. |
| `keyring` 4.2.0 (bare) | **[B12]** Upstream says link `keyring-core` + a store instead; breaks Linux MSRV. See `keyring-core` above. |
| `git2` / `gix` | Kata folder install shells out to `git` (same as today, `01` §4.9). No libgit2 in the binary. |
| `axum` / `hyper` as first-party HTTP | rmcp's streamable HTTP feature is enough for opt-in loopback. |
| Any GPL-3.0 runner | Copyleft is a product constraint (`02` §7.15). |

### 3.2 MSRV

Declared **1.88**. `anstyle` (1.66.0), `anstream` (1.66.0), and `inquire` (1.80.0) all declare a `rust-version` well below 1.88; `nucleo-matcher` declares none. None of the new picker/prompt/styled-output crates raise the MSRV. Add an MSRV CI job: `cargo +1.88 check --workspace --all-targets`, run on **macOS and Linux**.

### 3.3 Licenses

Add `cargo-deny` with a license allowlist plus advisories to CI. Every dependency in this workspace offers a permissive option (MIT / Apache-2.0 / MPL-2.0 / BSD-family / Unicode) — `nucleo-matcher`'s MPL-2.0 is file-level copyleft and already fits this list. rmcp is Apache-2.0-only, which needs NOTICE handling in release archives (gated on Mason, §7.4).

---

## 4. Kata format

### 4.1 On-disk layout

```
~/.config/kadou/
  kadou.toml              # may be empty; missing keys mean defaults
  themes/                 # drop-in *.toml themes
  kata/
    starter/hello.sh      # written on first run; yours after that
    sesami/                # `kadou get <git-url> --as sesami`; a git checkout
      cc4-aaa.sh
      ses-deploy.sh
      device-log-metrics/
        kata.sh
        lib/  tests/  envs/
      scripts/trigger-pipeline.sh
~/.local/share/kadou/      # vault, keys
~/.local/state/kadou/      # history, pending, proposed, mined, last-used args
./kata/                    # project-local; found from cwd
```

A kata is either:

1. `<folder>/<name>.sh` (or any extension; the header decides), or
2. `<folder>/<name>/kata.sh` plus anything else in that directory.

Detection is by presence of `kata.sh`; nothing declares it (§4.3 has the full multi-file rule).

### 4.2 Identity

- `~/.config/kadou/kata/` is the library. Every immediate subdirectory is a **folder**. The folder name is the first id segment. There is no registry: a folder exists because it is on disk.
- Ids are the path under `kata/` without the extension: `sesami/cc4-aaa`, `starter/hello`, `./deploy`. Nesting is allowed to any depth.
- Id segments match `^[a-z0-9][a-z0-9-]*$`. Uppercase or underscore in a filename is a check error with a rename suggestion.
- `/` is the separator, not `.` — ids are paths. Aliases have no slash and are unique across all folders, checked.
- Reserved top-level names: `starter`, `proposed`, `mined`. `kadou get --as mined` is refused.
- **Display:** `about` is the description everywhere (list, `kadou show`, MCP); there is no separate display-name field.

### 4.3 Header spec

**YAML-in-comment, closed grammar, parsed by kadou's own ~200-line parser (not a general YAML library).** It looks like YAML so eyes and models parse it for free; it is small enough that a purpose-built parser is safer than a general one (`08` §3.1).

**Placement and framing:**

- Optional shebang on line 1.
- The opening `# ---` must appear within the first 3 lines (allows a shebang plus one `# shellcheck` line).
- Every line until the closing `# ---` starts with `#`. `#` alone is a blank line. A line that does not start with `#` before the close is an error ("header not closed").
- Max 64 header lines. Tabs are an error with a fix. CRLF is an error with a fix. UTF-8 only.
- The header is stripped by nothing; the interpreter sees comments.
- The first comment paragraph *after* the closing `# ---` is `notes`: shown by `kadou show`, not required.

**Keys.** Six keys, all lowercase, in any order. Unknown keys are errors.

| Key | Required | Value | Notes |
|---|---|---|---|
| `about` | yes | one line, 1–120 chars | the description everywhere: list, `kadou show`, MCP |
| `risk` | yes | `low` `medium` `high` `critical` | four words, no scores |
| `needs` | no | space-separated vault names; `name=default` allowed | see §4.4 |
| `args` | no | block of arg lines, two-space indent | see §4.4 |
| `alias` | no | space-separated short names | unique across all folders, checked |
| `timeout` | no | `30s` `10m` `2h` | overrides `[exec] timeout` for this kata; never above 24h |

Not keys: `name` (the filename), `id` (derived), `version` (git), `script` (the file), `type`, `format_version`, `scope`, `secret`.

**Arg lines:**

```
<name>: <type>[ <options>][ = <default>][  # <help>]
```

| Part | Rule |
|---|---|
| `name` | `^[a-z][a-z0-9_]*$`; env is its uppercase; rejects `path`, `home`, `pwd`, `ifs`, `shell`, `oldpwd`, `cdpath`, `bash_env`, `ps4`, anything becoming `LD_*`, `DYLD_*`, `KADOU_*` |
| `type` | `text` `int` `bool` `select` |
| `options` | only for `select`: `a\|b\|c`, words matching `^[A-Za-z0-9_.:/-]+$` |
| `default` | bare token, `true`/`false`, integer, or `"quoted"`; `= ""` means optional and empty |
| required | no default → required. That is the whole rule. |
| `help` | free text to end of line after two spaces and `#` |

Four arg types, not nine: `text`, `int`, `bool`, `select`. Sesami's real usage across 32 kata is `string` 142, `boolean` 86, `select` 5, `number` 1 — `float`, `file_path`, `resource_id`, `multi_select` are unused in every real catalog on disk, so they are not header types (`08` §3.4).

Env serialization, per type: `text` as-is, `int` decimal, `bool` `true`/`false`, `select` the chosen option. Anything else in MCP `args` is `invalid_args`.

Most kata have no args, or one; the block form is fine at one arg — there is no inline short form (one way, `08` §3.5).

### 4.4 Needs and args resolution

```
# needs: jenkins_url=https://ci.example.com jenkins_user jenkins_token
```

- Each token is a vault name matching the arg name rule; env is the uppercase.
- `name=default` gives a plain (non-secret) fallback used when the vault has no entry. A default in a git-tracked file is by definition not a secret, so a need with a default is always plain.
- Resolution: vault entry, else default, else **missing**. Missing needs: CLI prompts once and saves (`kadou vault set` inline); MCP returns `error: missing_needs` with the names and the human command. Env of the agent host is never consulted.
- Needs are one flat namespace across folders. Two folders that both want `token` collide on purpose: prefix (`jenkins_token`, `argocd_token`). `kadou check` errors when two folders declare the same need with different defaults.
- `describe_kata` returns names and `needs_missing`. Never values.

**Args are what an agent may set. Needs are what only the vault may supply.** There is no `agent_settable` flag and no scope lock to test, because the two sets never overlap by construction (decision 13; `08` §1 item 2). `run_kata` `args` naming a need is `invalid_args`.

### 4.5 Project-local discovery and trust (decision D3)

- From cwd, kadou walks up to the nearest `kata/` directory, stopping at the git root or `$HOME`. Kata found there have ids prefixed `./`: `./deploy`, `./db/migrate`.
- A **human** can list, show, and run project-local kata immediately. Running one that has `needs:` is refused with "untrusted folder cannot use the vault; run `kadou trust`".
- An **agent** does not see project-local kata at all until the folder is trusted. The threat is a cloned repository shipping `kata/tidy.sh` with `risk: low` and `needs: jenkins_token`.
- `kadou trust` appends the folder's absolute path to `[trust] paths` in `kadou.toml`. `kadou trust --forget` removes it. This is direnv's `allow` model keyed by **path**, not by content hash — a folder of scripts changes constantly and re-trusting on every commit would train people to type `trust` reflexively.
- MCP servers launched by an agent host inherit the host's cwd, so the same walk-up and trust gate apply.

### 4.6 `kadou import` conversion rules

`kadou import <dops-catalog-dir> --as <folder>` converts an old dops catalog once. It writes the new folder, prints per-file diffs, and refuses to overwrite an existing folder.

- **The `REPO_ROOT`/`TRIGGER` idiom.** All 29 Sesami wrappers contain the identical two lines `REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"` and `TRIGGER="${REPO_ROOT}/scripts/trigger-pipeline.sh"` (verified 29 of 29 on disk). A single-file kata sits one level shallower than `src/<name>/script.sh`, so this **breaks** under a naive two-file copy. The importer replaces that pair with `TRIGGER="${KADOU_ROOT}/scripts/trigger-pipeline.sh"` and shows the diff. `KADOU_ROOT` is the folder root regardless of nesting, so a later reorganization (e.g. into `cc4/`) does not break it again.
- **Boolean and numeric coercion.** CC4 booleans as strings (`"true"`) coerce to `bool = true` once, at conversion; an uncoercible value is a conversion error, not a load-time surprise (`type: number` timeout becomes `int = 60`; `required: true, default: ""` becomes a required arg with no default, which is what "required with empty default" meant).
- **Multi-file kata.** `device-log-metrics` (with `lib/`, `tests/`, `envs/*.env`) converts to `sesami/device-log-metrics/kata.sh` (renamed from `script.sh`); `tests/` and `lib/` stay opaque. `clone-ses-repos` and `ses-argocd-sync` carry no helpers and become plain single files.
- **Global params.** Three Sesami globals (`jenkins_url`, `jenkins_user`, `jenkins_token`) become one `needs:` line per kata; `jenkins_url` keeps its default; `jenkins_token` becomes a secret vault entry; `jenkins_user` is plain.
- **Version and scope-saved values.** `version: 1.0.0` is dropped (git is the version). Runbook-scope saved values (Go's `catalog.*.runbooks.*`) are reported and either dropped or written as non-secret last-used args (§6.6, decision D5) — they are never silently kept as a vault scope, because scopes no longer exist.
- **The 32-file compatibility check.** `kadou import ~/Bitbucket/sdo-dops-catalog/src --as sesami` writes the new folder; `kadou check` all 32 with zero errors; `kadou show` all 32; `kadou run --dry-run` all 32 with the same env names as today. This is the acceptance suite (decision 11).

### 4.7 Check strictness (decision 12)

`kadou check [folder|path] [-v]` is the loader. Cargo-shaped diagnostics: a line per error, a fix line per error, one summary line.

```
$ kadou check sesami
error: unknown arg type `string`
  --> kata/sesami/ses-argocd-sync.sh:8:14
   |
 8 | #   app_name: string                  # ArgoCD application name
   |               ^^^^^^ use `text`
   = arg types are text, int, bool, select

checked 32 kata in sesami   1 error  0 warnings
```

- A folder with any header error is listed with `✗` and none of its kata run; other folders — including `starter` — are unaffected.
- Errors: unknown top-level key, unknown arg type, missing `about`/`risk`, tabs in the header, unclosed header, two folders declaring the same need with different defaults, `x.sh` and `x/kata.sh` colliding in one folder, alias collision.
- Warnings: a need with no vault entry and no default (`kadou check` prints the fix: `kadou vault set <name>`), a file whose first comment block opens `# ---` and fails to parse (this is an **error**, not a warning — a bad header is loud, not silently treated as a helper file).
- The bespoke parser's error messages are its user interface: a fixture corpus of bad headers under `tests/fixtures/headers/bad/` is snapshot-tested (`insta`) the way `tools/list` is snapshotted, per `08` §6.2 risk 1.
- Starter is always loaded from embed and is guaranteed valid at compile time (tests).

---

## 5. MCP surface

### 5.1 Default surface

`tools/list` returns **exactly these four tools**, in this order: `list_kata`, `describe_kata`, `run_kata`, `propose_kata`. No other tools, including as an opt-in. `06` §5.3's `mine_list` / `mine_get` / `mine_run` / `mine_review` are **not** registered (decision 15).

The server advertises **only the `tools` capability**. It does not declare `resources` or `prompts` at all, so a spec-compliant host never calls `resources/list` or `prompts/list`. Agents that need a schema call `describe_kata`.

`prompts` is not advertised. `propose_kata` is the create-kata path as a **tool**, not a prompt. Skills are not prompts. The session-mining skill is a `SKILL.md` that execs `kadou mine`, not an MCP prompt (`06` §5.2).

`initialize` sends **no `instructions` field** — bytes spent there count against the budget the same as `tools/list`.

Server name: `kadou`. Version: `CARGO_PKG_VERSION`. Transport: **stdio default**. HTTP is `--transport http --bind 127.0.0.1:8808` (loopback only; refuse `0.0.0.0`), requires a non-empty `allowed_origins` and a per-launch random bearer token printed once at startup. Both are required before HTTP ships; until implemented, HTTP stays behind a build-time cargo feature (`03` §1 rule 6, §10 rule 5; `07` C14, B11).

### 5.2 Progressive disclosure

1. **Connect:** four tool schemas (budget ≤ 2 800 bytes compact / ≤ 800 tokens; measured 2 028 B / ~507 tok).
2. **`list_kata`:** id, about, risk. Filterable by `query`, `folder`, `risk`, `include_drafts`. No schemas (`03` §8 rules 2 and 5).
3. **`describe_kata`:** `args` JSON Schema generated from the header (so the two cannot drift), `needs`, `needs_missing`, `source` (header first, capped at **16 KiB** with `source_truncated: true` past the cap — an agent pays for the schema once, as prose it can read, and once as `args` JSON Schema it can validate against), `file`, `sha256`, `files` (sibling helpers a kata references, e.g. Sesami's `scripts/trigger-pipeline.sh`, so an agent can read a shared helper with its own file tools). This is layer 2.
4. **`run_kata`:** execute; result is last N lines + metadata. Source is **not** in the result (`03` §8 rule 4).

Eager-loading clients (Cursor without tool search, naive CI) only ever see four schemas.

MCP tool annotations (`readOnlyHint` on `list_kata`/`describe_kata`; `destructiveHint`/`openWorldHint` on `run_kata`; `readOnlyHint: false`/`destructiveHint: false` on `propose_kata`) are included on the wire — see `docs/design/tools-list.json`. Cost is folded into the 2 028-byte measured payload.

### 5.3 Folder addressing

`id` is `folder/name`, `./name` (project-local), or an alias. Optional `folder` filter on `list_kata`. Non-trusted project-local folders are absent from an agent's view (§4.5) **except** drafts (`proposed`, `mined`), which are always listable/describable regardless of ceiling (decision 14–15; §4.1). A second folder does not add tools. `run_kata` on a draft id returns `error=draft` and does not execute.

### 5.4 Tool input schemas (JSON Schema draft 2020-12)

These are the **exact, hand-authored** `inputSchema` objects served on the wire. The full `tools/list` payload, byte-for-byte, is checked in at `docs/design/tools-list.json` and covered by an `insta` snapshot test (§3.1). Compact size: **2 028 bytes** (≈ 507 tokens @ 4 B/token, ≈ 579 @ 3.5 B/token) — under the 2 800-byte CI gate with ~772 bytes to spare. `additionalProperties: false` on every root; no `$schema`, `$id`, or `title`.

#### `list_kata`

Description: `Search kata (reviewed scripts) visible to this agent. Returns id, about, risk. No schemas.`

```json
{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "query": { "type": "string", "maxLength": 200, "description": "Substring of id, alias, or about." },
    "folder": { "type": "string" },
    "risk": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 },
    "offset": { "type": "integer", "minimum": 0, "default": 0 },
    "include_drafts": { "type": "boolean", "default": false, "description": "Also list non-runnable drafts (mined, proposed)." }
  }
}
```

`folder` replaces `catalog`; `include_drafts` replaces `include_staging`.

#### `describe_kata`

Description: `One kata: args schema, needs, risk, source. Read it before run_kata.`

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["id"],
  "properties": {
    "id": { "type": "string", "description": "folder/name, ./name, or alias." },
    "include_source": { "type": "boolean", "default": true }
  }
}
```

`include_script` is renamed `include_source` (the field it toggles is now `source`, §5.5).

#### `run_kata`

Description: `Run one kata with args. Secrets come from the vault as needs; never pass them. Above your grant it returns pending_grant.`

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["id"],
  "properties": {
    "id": { "type": "string" },
    "args": { "type": "object", "default": {}, "additionalProperties": true, "description": "Parameter name to value, per describe_kata." },
    "dry_run": { "type": "boolean", "default": false, "description": "Resolve args and env names without executing." }
  }
}
```

There are still **no** `_confirm_id`/`_confirm_word` properties (`03` §10 rule 2; `01` §4.6). Args-to-environment serialization is specified per type in §6.1. Needs can never appear in `args` by construction (§4.4) — reject an entry naming a need as `invalid_args`.

#### `propose_kata`

Description: `Draft a kata (one file with a header) for human review. Never registers or runs it.`

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["id", "source"],
  "properties": {
    "id": { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]*(/[a-z0-9][a-z0-9-]*)+$", "maxLength": 128 },
    "source": { "type": "string", "maxLength": 65536 }
  }
}
```

**[shape]** This schema shrinks from six properties (`catalog`, `name`, `description`, `risk_level`, `yaml`, `script`) to two (`id`, `source`) — one string in, one file out. The server parses the header from `source` before writing; a bad header is `invalid_args` with the same diagnostic text `kadou check` prints. `id`'s `pattern` requires at least one `/` (folder + name) and the server additionally canonicalizes the resolved path and refuses anything outside `~/.local/state/kadou/proposed/`, closing a path-traversal write. Re-proposing the same `id` overwrites the prior draft, and the diff is computed against the currently **accepted** kata when one exists, otherwise against `/dev/null`. `risk` is inside the header (§4.3), so there is no separate `risk_level` property to default silently — an unreviewed agent-authored kata's risk choice is never hidden from the human reviewer.

No `accept` tool (`03` §11 rule 2).

### 5.5 Result shapes

All tool results are a **single JSON text content block**, **compact** (not pretty-printed), UTF-8. Every result that represents an input error, a lookup failure, or a runtime failure sets `isError: true`, with `error` one of `no_such_kata`, `invalid_args`, `missing_needs`, `draft`, `timeout`. A non-zero script exit is `status: failed` with `isError: true`.

#### `list_kata` result

```json
{
  "kata": [
    { "id": "starter/disk-usage", "about": "Disk usage of a directory", "risk": "low" }
  ],
  "total": 5,
  "offset": 0,
  "limit": 50,
  "truncated": false
}
```

`truncated` is true when `offset+len < total`. Hidden (above-ceiling, or untrusted project-local) kata are absent, not listed as denied. Draft entries set `draft: true` and never appear unless `include_drafts` or `folder` names `mined`/`proposed`. `aliases: []` and `draft: false` are **omitted**, not printed, on entries that don't need them.

#### `describe_kata` result

```json
{
  "id": "sesami/cc4-aaa",
  "folder": "sesami",
  "about": "Trigger a SES/CC4/cc4-aaa branch pipeline",
  "risk": "medium",
  "file": "/Users/mason/.config/kadou/kata/sesami/cc4-aaa.sh",
  "sha256": "sha256:4b1c…",
  "needs": ["jenkins_url", "jenkins_user", "jenkins_token"],
  "needs_missing": [],
  "args": {
    "type": "object",
    "additionalProperties": false,
    "properties": {
      "branch": { "type": "string", "default": "dev", "description": "Branch, tag, or PR to trigger" },
      "version": { "type": "string", "default": "", "description": "Image tag; blank falls back to branch" },
      "send_email": { "type": "boolean", "default": true },
      "publish_image": { "type": "boolean", "default": true },
      "publish_api": { "type": "boolean", "default": true },
      "allow_image_override": { "type": "boolean", "default": false }
    },
    "required": []
  },
  "source": "#!/bin/sh\n# ---\n# about: Trigger a SES/CC4/cc4-aaa branch pipeline\n# risk:  medium\n# needs: jenkins_url=https://ci.example.com jenkins_user jenkins_token\n# args:\n#   branch: text = dev …",
  "source_truncated": false,
  "files": ["scripts/trigger-pipeline.sh"]
}
```

`needs` lists names only, never values. `needs_missing` tells the agent the run will fail before it tries, and the fix is a human command (`kadou vault set <name>`). Unknown id or above-ceiling: `isError: true`, `error: no_such_kata` (do not distinguish hidden vs missing). Draft ids **are** describable (redacted source) so agents can review drafts. `include_source: false` drops `source` but keeps `file` and `sha256`.

#### `run_kata` results

Success / failure after exec:

```json
{
  "status": "success",
  "id": "starter/disk-usage",
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

`status` is `success` when exit_code is 0, `failed` otherwise, `cancelled` on ctx cancel, or **`running`** when `mcp.max_wait` elapses before the process exits (§6.1). `summary` is the last non-empty output line, truncated to 200 chars. `log_path` for a fresh (≤7-day) run is a **plain text file**.

Pending grant (high/critical, visible, no allow-list) — **[tui]** carries `approve` and `expires`, new fields so the agent's next message to the human is one line (`09` §4.2):

```json
{
  "status": "pending_grant",
  "id": "sesami/ses-deploy",
  "risk": "critical",
  "pending_id": "7c1e…",
  "pending_path": "/Users/…/.local/state/kadou/pending/7c1e….json",
  "approve": "kadou grant approve 7c1e",
  "expires": "2026-09-12T14:02:00Z",
  "reason": "critical; not in [agent] allow"
}
```

Dry-run:

```json
{
  "status": "dry_run",
  "id": "sesami/cc4-aaa",
  "env_names": ["JENKINS_URL", "JENKINS_USER", "JENKINS_TOKEN", "BRANCH", "VERSION", "SEND_EMAIL", "PUBLISH_IMAGE", "PUBLISH_API", "ALLOW_IMAGE_OVERRIDE"],
  "env_public": { "BRANCH": "dev", "VERSION": "" },
  "secret_env_names": ["JENKINS_TOKEN"]
}
```

`env_public` never includes secret values; it **does** include vault-resolved non-secret needs like `JENKINS_USER`.

Invalid args:

```json
{
  "status": "error",
  "error": "invalid_args",
  "isError": true,
  "message": "unknown arg \"cmd\"; kata do not take a shell string",
  "expected": { "type": "object", "properties": { "…": {} }, "required": ["branch"] }
}
```

Missing needs:

```json
{
  "status": "error",
  "error": "missing_needs",
  "isError": true,
  "id": "sesami/cc4-aaa",
  "needs_missing": ["jenkins_token"],
  "message": "kadou vault set jenkins_token"
}
```

Draft (mined/proposed) — **does not run**, even with a grant:

```json
{
  "status": "error",
  "error": "draft",
  "isError": true,
  "id": "mined/k8s-pod-logs",
  "message": "draft; a human must run: kadou accept mined/k8s-pod-logs"
}
```

Truncation: last **50** lines (`mcp.max_output_lines` in config, default 50, max 200). If UTF-8 bytes of `output` would exceed 8192, cut to the last whole lines that fit and set `truncated: true`.

#### `propose_kata` result

```json
{
  "status": "proposed",
  "id": "proposed/sesami/argocd-sync",
  "path": "/Users/…/.local/state/kadou/proposed/sesami/argocd-sync.sh",
  "diff": "--- /dev/null\n+++ sesami/argocd-sync.sh\n…",
  "accept": "kadou accept sesami/argocd-sync"
}
```

**[shape, decision D]** `path` and `accept` always point at a location under the state dir and, on accept, a **user-owned** folder — `kadou accept` refuses a target that is a git-backed folder's working tree (§6.7). The draft never proposes writing into a `kadou get`-installed checkout.

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

Host-side tool ids follow the server name, e.g. `mcp__kadou__run_kata` in Claude Code.

---

## 6. Script execution and safety

### 6.1 Exec contract

```
argv:  <interpreter-from-shebang> <abs-kata-path>, or /bin/sh <abs-kata-path> with no shebang
cwd:   the kata's directory (folder form) or the folder containing the file
stdin: closed (/dev/null; never the MCP server's own stdin)
stdout/stderr: piped (never inherited), merged line stream to history log + interface
cancel: notifications/cancelled, stdio EOF, or exec.timeout → SIGTERM the process group, SIGKILL after 5s
```

**[tui, D2]** The shebang is the runtime. No shebang means `/bin/sh`. A `#` header works unchanged in sh, bash, python, ruby, perl; `kadou check` warns when the declared interpreter is not on `PATH`. The starter kata stay `#!/bin/sh`.

- No flags-to-script adapter. No JSON blob default input (`03` §7 rule 3).
- Shared helpers are reached from inside the kata file via `$KADOU_ROOT` (§4.6).
- `KADOU_DIR` equals cwd. `KADOU_ROOT` is the top-level folder under `kata/` (the git checkout root for a `kadou get` folder). `KADOU_FILE` is the kata's absolute path. `KADOU_ID` is the id.
- Windows / `.ps1` deferred (`01` §3.2; `03` non-goals).
- `dry_run` does not spawn. It returns env names, not a command line (§5.4).
- `/bin/sh` is dash on Linux and bash-in-POSIX-mode on macOS; test both where CI runs both platforms.

**MCP child environment.** For MCP, the child does **not** inherit the agent host's full environment. It starts from an explicit allowlist plus declared args and needs:

```
PATH HOME USER LOGNAME SHELL LANG LC_* TZ TMPDIR SSH_AUTH_SOCK KUBECONFIG
```

plus `TERM=dumb`, plus `KADOU_ID`/`KADOU_FILE`/`KADOU_DIR`/`KADOU_ROOT`, plus an operator-editable `[exec] pass_env = []` for anything else a team folder genuinely needs (verified sufficient for Sesami: `clone-ses-repos` needs `SSH_AUTH_SOCK`; `ses-argocd-sync` needs `KUBECONFIG` or `~/.kube/config` via `HOME`; `device-log-metrics` reads its own `envs/*.env` files, unaffected by parent env). **CLI keeps the full parent environment** — that is a human's own shell context, not an agent's.

**Timeout and lifecycle.** `[exec] timeout` bounds a single run (default 30 min); a kata's own `timeout:` header key may lower or raise this bound up to 24h. `[mcp] max_wait` (default 50 s) bounds how long an MCP call blocks before returning `status: running` with `history_id`/`log_path` so the agent can poll the log itself. A **per-server concurrency limit** (default 2) caps parallel runs; one run at a time per kata id, or the call returns `busy`. Cancellation sends `SIGTERM` to the process group, then `SIGKILL` after a 5 s grace period. stdin is `/dev/null` (tested invariant); stdout/stderr are piped (tested invariant — one stray byte on the MCP server's own stdout corrupts the stdio transport).

**Args → env serialization**, specified per type (§4.3):

| Arg type | Env value |
|---|---|
| `text` | as-is |
| `int` | decimal text form |
| `bool` | `"true"` / `"false"` |
| `select` | the chosen value, validated against `options` |
| object / null / array | rejected as `invalid_args` |

Reject `args` entries naming a need (§4.4) — the split is structural, not a runtime scope check.

### 6.2 Risk levels and ceilings

Order: `low < medium < high < critical`. A kata **at** the ceiling is allowed (strict greater-than is the "exceeds" test).

```
human_ceiling(f)  = folder[f].max_risk ?? max_risk
agent_ceiling(f)  = min(agent.max_risk, folder[f].agent_max_risk ?? agent.max_risk, --max-risk, human_ceiling(f))
visible(k, f)     = trusted(f) and rank(k.risk) ≤ agent_ceiling(f)     [MCP]
                  = rank(k.risk) ≤ human_ceiling(f)                    [CLI]
```

| Ceiling | Default | Where |
|---|---|---|
| Human global | `medium` | `kadou.toml` `max_risk` |
| Folder | unset → human global (replaces it, does not intersect) | `[folder.<name>] max_risk` |
| Agent | `low` | `[agent] max_risk` |

`kadou mcp serve --max-risk` may only **narrow**. It cannot exceed `[agent].max_risk` or `agent_ceiling(f)` above. `[agent].max_risk`/`[agent].allow` never raise visibility — they only decide run-vs-`pending_grant` for a kata that is already visible and high/critical (§6.3).

**Project-local trust is a visibility term, not a ceiling.** An untrusted `./kata` is invisible to an agent regardless of risk (§4.5); a human can list, show, and run it, just not with the vault.

At the **default** ceilings on the Sesami folder, an agent sees exactly **five** kata (the same five that are `risk: low`). Three of those trigger Jenkins with the vault token — "agent default low" is not "agent cannot touch CI", which is why the env allowlist, redaction, and MCP scope split (§6.1, §6.5, §6.6) must ship no later than the same slice as `run_kata` (§9).

Starter kata contain only `low` (`03` §10 rule 6).

### 6.3 Unified confirm protocol (decision 4)

| Level | CLI | MCP |
|---|---|---|
| low / medium | none | none |
| high | On a TTY: `run? [y/N]`, default No. Otherwise: requires `--confirm <id>`. | If **visible** (§6.2), id ∈ `[agent].allow`, **and** ceiling ≥ high: run. Else `pending_grant`. |
| critical | On a TTY: type the kata id. Otherwise: requires `--confirm <id>`. | If **visible**, id ∈ `[agent].allow`, **and** ceiling ≥ critical: run. Else `pending_grant`. |

**[tui]** There is no TUI face; the TTY prompt *is* the interactive confirm, and `--confirm <id>` is the non-interactive form for both — one protocol, two faces instead of three. `kadou run <id> --ask` prompts for every arg even when all required args are supplied, walking the whole form (§7.2).

The model cannot mint a grant. There is no confirm field in any tool schema (`03` §10 rule 2). Approving a pending high **or** critical record still requires `--confirm <id>` at approval time (§6.4).

CLI without `--confirm` on high/critical, non-TTY, prints the summary and exits 2 with the exact `--confirm` line to copy:

```
$ kadou run sesami/ses-deploy version=25.6.1.2 oke_cluster=uat </dev/null
error: sesami/ses-deploy is critical and needs confirmation
  = kadou run sesami/ses-deploy version=25.6.1.2 oke_cluster=uat --confirm sesami/ses-deploy
```

### 6.4 Human grant flow (agents)

1. Agent `run_kata` on a **visible** high/critical id (§6.2) without a grant → write `~/.local/state/kadou/pending/<pending_id>.json` with: id, args (needs never included), requester `mcp`, `mcp_client` (self-reported `clientInfo.name` — a label, not an identity), timestamp, **`sha256`** and the **folder's git HEAD** (when git-backed) pinned at request time. Return `pending_grant` with `pending_id`, `pending_path`, **`approve`**, and **`expires`** (§5.5).
2. **TTL:** the record expires after **24 hours**. `kadou grant list` shows expired records as expired; `kadou grant approve` on an expired record fails.
3. **Dedupe:** a second `run_kata` call with the same `(id, args_hash)` while a pending record is outstanding returns the existing `pending_id` rather than creating a duplicate.
4. **[tui]** The agent's own result carries `approve`; the server posts a desktop notification (§7 below); `kadou`'s `needs you` block shows it; `kadou grant list` and `kadou grant show <pending_id>` print the record and a **kata diff since request** (in case a `git pull` landed a different script than what was requested).
5. `kadou grant approve <pending_id>` **one-shot executes** that pending record (still subject to the CLI confirm for high or critical), **refusing** if the current on-disk `sha256` / folder git HEAD no longer matches the pinned value. Approval **runs in the human CLI's environment** (full parent env, not the MCP server's allowlisted one). On success the pending record gains `history_id`, `status`, and `log_path`, all readable via `pending_path` without a fifth MCP tool. `kadou grant allow <id>` appends to `[agent].allow` (config edit, human-owned), pinned to the current sha256 unless `--any-version`.
6. `kadou grant deny <pending_id>` deletes the record.
7. MCP has no approve tool.

Raising `[agent].max_risk` or `[agent].allow` is a **config file edit** or `kadou trust`/`kadou grant allow` (human CLI), never an MCP tool.

### 6.5 Secret handling

- **[shape]** A secret is never an arg; it is a **need** (decision 13). There is no `secret: true` flag to set on an arg.
- **Vault envelope, specified byte-for-byte to match the Go product exactly** (verified by decrypting a real Go-written envelope with the Rust `age` crate — `07` Appendix B item 5):
  - `vault.json` = `{"version":1,"data":"age1"+base64::STANDARD_NO_PAD(<binary age v1 ciphertext>)}`. The literal 4-character prefix `"age1"` is a Go-side tag, stripped before decoding. There is no base64 padding.
  - `keys.txt` = a standard age identity file, parsed with `age::IdentityFile::from_buffer`.
  - Import is **copy-once and read-only** against `~/.dops/vault.json` + `~/.dops/keys/keys.txt`: if both exist and convert, kadou copies them into its own XDG data dir once and records an import marker; the source files under `~/.dops/` are never modified.
  - **[shape]** Payload shape inside the decrypted plaintext is now **flat**, keyed by need name:
    ```json
    {
      "jenkins_url":   { "value": "https://ci.example.com", "secret": false },
      "jenkins_user":  { "value": "…", "secret": false },
      "jenkins_token": { "value": "…", "secret": true }
    }
    ```
    Go import maps `global.*` entries directly to this flat map (secret bit from the source YAML's `secret: true` flags); it reports and drops `catalog.*` runbook-scope values, or writes them as non-secret last-used args (§6.6, decision D5) — needs are one flat namespace, so there is no scope tree left to import into.
- Values of `jenkins_token` are never written into this document, logs at info level, MCP schemas, `args`/`source` echoes, or history parameter maps. History stores `****`.
- Default identity: age X25519 at `~/.local/share/kadou/keys/identity.txt` `0600`. This default protects only against copying the vault file without the keys directory — the same threat model as Go.
- Optional `[vault] keyring = true`: the identity is age scrypt-encrypted, and the **passphrase** — not the identity file itself — lives in the OS keyring via `keyring-core` + `apple-native-keyring-store`, service `kadou`, account `vault-identity`.
- `vault.passphrase_cmd` is **not** in MVP — it would turn config into code execution.
- **`kadou vault set [--plain] <name>`** reads the value from a TTY prompt or stdin, **never argv**. Default is secret; `--plain` for URLs and usernames. **MCP never writes the vault**, in any scope — a tested invariant (§9).
- Directory modes: vault and keys directories `0700`, files `0600`.

### 6.6 History / audit

Record fields plus `initiator` (username if known, else `local`) and `mcp_client` (self-reported, a label, not an identity):

`id`, `folder`, `args` (secrets never appear — they were never args), `status` (`running|success|failed|cancelled|pending_grant`), `exit_code`, `start_time`, `end_time`, `duration_ms`, `output_lines`, `output_summary`, `log_path`, `interface` (`cli|mcp` — **[tui]** no `tui` value).

**Redaction happens in the line stream, before the log is written**, so the MCP result, the on-disk log, and the CLI all see the same already-redacted text. For every secret value injected into the child, redact: the literal value, its standard base64 encoding, base64 of `user:value`, and its URL-encoded form. Apply a minimum length (8 chars) so short values do not shred unrelated output. Also redact secret values supplied through CLI prompts (defense in depth).

**Fresh tier:** the last **7 days** of logs are plain `0600` text files; a running or recently-finished run's `log_path` always points at one of these. After 7 days, logs compress into 10 MB gzip-tar archives. **90-day TTL** and **50 MB** combined cap. `kadou history` lists newest-first (default 20). History and pending directories `0700`, files `0600`. Retention for `pending/` and `proposed/` (drafts): **30 days**.

**[tui, D5] Last-used args.** kadou remembers last-used args per kata in `~/.local/state/kadou/last/`, plain, non-secret. This is **interactive-prefill only**: `kadou run <id>` on a TTY, missing a required arg, shows the last-used value as the prompt default. CLI with the arg already on the command line and MCP always use the header default. Go's runbook-scope vault saving is dropped entirely — "what the file says is what runs."

### 6.7 Propose / accept loop

`propose_kata` writes `~/.local/state/kadou/proposed/<folder>/<name>.sh` and returns a unified diff. It does **not** register the kata and does **not** run (`03` §11 rule 1; `03` §9 rule 5). The server canonicalizes and contains the write path (§5.4), size-caps `source`, and strict-loads the header at propose time so a malformed draft is rejected immediately rather than at accept time.

`kadou accept <id> [--into <folder>]` (default: the id's own folder) validates with the strict loader, **prints the diff and prompts `y/N`** (or `--yes`), and copies the file into an existing, **user-owned** folder under `~/.config/kadou/kata/`. **[shape, decision D]** Accept **refuses** a target that is a git-backed folder's working tree — copying an unreviewed draft straight into a `kadou get` checkout would blur "what git tracks" with "what a human approved," and the next `kadou update` could silently overwrite or orphan it. Say this once, here: **`propose_kata` results always point at a location that will land under a user-owned folder, never a git checkout, and `kadou accept` enforces it.**

The product never `git commit`s, `git push`es, or `kadou get`s an agent-invented URL.

### 6.8 Session mining (crate + CLI, not extra MCP tools)

Contract: `06-session-mining.md` (pipeline, redaction, bounds, review gate). **Packaging override:** `06` §5.1 assumes Go `cmd/mine.go` and `internal/mine/*.go`. kadou implements that engine as **`crates/kadou-mine`** and **`kadou mine …`** on the existing clap tree. One binary. The skill (`06` §5.2) stays a `SKILL.md` that execs `kadou mine`.

**XDG paths:**

| `06` path | kadou path |
|---|---|
| `$DOPS_HOME/mine/` | `~/.local/state/kadou/mine/` |
| `$DOPS_HOME/mine/queue/<fingerprint>/` | `~/.local/state/kadou/mine/queue/<fingerprint>/{meta.json,kata.sh}` |
| `$DOPS_HOME/catalogs/mined/` | `~/.local/share/kadou/kata-drafts/mined/` (never in the library path; listed regardless of ceiling — §4.1) |
| `$DOPS_HOME/mine/redact-extra.txt` | `~/.config/kadou/mine/redact-extra.txt` |

Pipeline, redaction ids R1–R13, rank cutoff, LaunchAgent **`dev.kadou.mine`**, bounds (20 min / 512 MB RSS / 2 GB scan), and fail-closed secret drop are **as specified in `06` §2–4**.

**[shape]** The miner writes a **single-file draft with a header**, not `format_version: 2` YAML — the draft writer changes; pipeline and redaction do not. `mined/<name>` ids, listable with `include_drafts`.

**MCP mapping (decision 15):**

| `06` §5.3 | kadou |
|---|---|
| Resource `dops://mine/queue` | **Not registered.** `list_kata` with `folder=mined` or `include_drafts=true` |
| Resource `dops://mine/proposal/{fingerprint}` | **Not registered.** `describe_kata` id `mined/<slug>` |
| Tool `mine_list` | `list_kata` |
| Tool `mine_get` | `describe_kata` |
| Tool `mine_run` | **CLI only:** `kadou mine run --once`. Not a starter kata. Skill may exec the CLI; MCP does not. |
| Tool `mine_review` | **Human CLI only:** `kadou mine approve\|reject\|skip`. No MCP accept. |
| Prompt `review-mined-runbook` | **Not registered.** Operator uses `kadou mine review` / `kadou show mined/<slug>` |

Approve copies the draft into the inactive `mined` staging area. `kadou accept mined/<name> [--into <folder>]` is what makes it executable — there is no shortcut. Miner never assigns `low` or `critical`.

`kadou-mine` is not on the slice-5 critical path (§9).

---

## 7. CLI UX

**[tui, D7]** There is no full-screen TUI in v1. Every human frame is styled CLI output; the one interactive component is an inline picker plus prompts, both on a TTY only. See `09` §3 for the full design; this section carries the PRD-level contract.

Rules every human-facing frame obeys (`09` §3.1):

1. Risk is a colored dot plus a word, never only a color.
2. Success is one line. Errors are a sentence and a fix line the reader can paste.
3. Styled on a TTY only. `NO_COLOR` and `--plain` force plain on a TTY. `TERM=dumb` disables the picker and prompts (they become errors with the non-interactive form).
4. One palette: `theme = "doop"` in `kadou.toml` colors dots, the `▸` marker, `✓`/`✗`, and muted text everywhere.
5. Nothing paginates; nothing is interactive unless the command is missing an id.
6. Interactive means inline: the picker and prompts draw below the shell prompt and erase themselves when done. No alternate screen, no lost scrollback; `Ctrl+c` always exits with a one-line "cancelled".

### 7.1 Command tree

```
kadou                                        # library frame (not a TUI)
kadou --help
kadou version
kadou run <id> [k=v…] [--dry-run] [--confirm <id>] [--ask]
kadou list [--folder F] [--risk R] [query]
kadou show <id>
kadou new <id> [--from <id>]
kadou edit <id>
kadou check [folder|path] [-v]
kadou get <url> [--as F] [--ref R] [--root SUB]
kadou update [F]
kadou remove <F>
kadou import <dir> --as <F>
kadou accept <id> [--into F]
kadou trust [--forget]
kadou vault set [--plain] <name>
kadou vault list
kadou vault rm <name>
kadou history [--limit N]
kadou grant list
kadou grant show <pending_id>
kadou grant approve <pending_id>
kadou grant deny <pending_id>
kadou grant allow <id> [--any-version]
kadou mine run [--once | --watch | --since <iso>]
kadou mine status
kadou mine list
kadou mine show <fingerprint>
kadou mine review
kadou mine approve <fingerprint> [--into F]
kadou mine reject <fingerprint> --reason …
kadou mine install-schedule
kadou mcp serve [--transport stdio|http] [--bind 127.0.0.1:8808] [--max-risk LEVEL]
kadou mcp schema [--bytes]        # print the served tools/list JSON and its byte count
kadou completion <shell>
```

`run`, `show`, `edit`, and `history` with **no id** open the picker on a TTY (§7.2) and error with the non-interactive form otherwise: `error: no kata id given and no terminal to pick one   = kadou run <id>, or kadou list`.

`kadou list` / `kadou show` are the CLI projection of the same meta-tools that MCP serves (`02` §7.9).

### 7.2 `kadou` with no arguments

First run, empty config, starter materialized (`09` §3.2):

```
$ kadou
 kadou 稼働   1 folder · 5 kata

 starter
   hello         ● low   Print a greeting
   disk-usage    ● low   Disk usage of a directory
   git-status    ● low   git status -sb in a repo
   health        ● low   Resolve a host
   list-path     ● low   List a directory

 run     kadou run                 pick one, or:  kadou run starter/hello
 new     kadou new <folder/name>   write a kata and open it
 team    kadou get <git-url>       add your team's kata as a folder
 agents  kadou mcp serve
```

With a grant and drafts waiting, the `needs you` block appears (and disappears when nothing is waiting — badges that are always present are noise):

```
$ kadou
 kadou 稼働   2 folders · 37 kata                  needs you: 1 grant · 2 drafts

 …

 needs you
   grant  sesami/ses-deploy  version=25.6.1.2 oke_cluster=uat   claude-code · 4m ago
          kadou grant approve 7c1e   ·   kadou grant deny 7c1e
   draft  proposed/ops/argocd-sync                kadou accept ops/argocd-sync
   draft  mined/k8s-pod-logs                      kadou mine review

 run  kadou run   ·   help  kadou --help
```

A folder that failed `kadou check` shows `✗ 2 errors` where a healthy folder shows `git ✓`, and its kata are listed dimmed. Piped, `kadou` prints one kata per line, tab-separated.

### 7.3 Picker and prompt keys

Any id-taking command with no id (`run`, `show`, `edit`, `history`) opens the picker on a TTY. Type to filter across id, about, and alias; the highlighted kata's header shows below the list.

| Key | Action |
|---|---|
| printable characters | filter |
| `↑`/`↓`, `Ctrl+k`/`Ctrl+j` | move |
| `↵` | select and continue the command |
| `tab` | print the full `kadou show` for the highlighted kata and return |
| `e` | open it in `$EDITOR` and return |
| `esc` / `Ctrl+c` | cancel, exit 130 |

At most twelve rows shown, scrolls past that. Fuzzy match, id ranked above about. No matches: `no kata matches "xyz"   kadou new sesami/xyz`. Non-TTY: `kadou run` with no id exits 2 with the non-interactive error above.

`kadou run <id>` prompts once per missing required arg on a TTY, defaulting to the last-used value when one exists (§6.6, D5); `↵` accepts, typing replaces, `esc` cancels. `kadou run <id> --ask` prompts for every arg (showing the header default or last-used value) so a human can walk the whole form. A `bool` prompts `y/n`; an `int` rejects non-digits before `↵`; a `select` is a four-row picker. High risk prompts `run? [y/N]`; critical prompts for the typed id, as in §6.3.

These are fixed, not configurable — there is no `[keys]` override map (`09` §6.2, §3.1 dropping the old TUI keybinding convention). `kadou completion <shell>` completes ids, folders, and arg names.

### 7.4 First-run experience

1. Missing config → write defaults (empty TOML is valid; missing keys mean defaults).
2. `kata/starter/` is materialized from the embed **only if `kata/` does not exist**. If deleted, it stays deleted; `kadou get starter` restores it.
3. `kadou` prints the starter frame and the next command (§7.2). Not an empty panel.
4. `kadou mcp serve` speaks stdio immediately.
5. No questionnaire. No "add a folder" dead end.

Adding Sesami is additive: `kadou import ~/Bitbucket/sdo-dops-catalog/src --as sesami` (existing dops catalog) or `kadou get <url> --as sesami` (an already-converted repo), then `kadou vault set jenkins_user --plain` and `kadou vault set jenkins_token`.

### 7.5 Install one-liner

```sh
curl -fsSL https://<stable-install-url>/install.sh | sh
  kadou 0.1.0 → /usr/local/bin/kadou   sha256 ok
  next:  kadou            open the library
         kadou mcp serve  for agents
```

POSIX `#!/bin/sh`, `set -eu`, OS/arch detect, **checksum verification** (SHA-256 of the tarball against a published `SHA256SUMS`). Install to `/usr/local/bin` or `KADOU_INSTALL_DIR` / `~/.local/bin`. Idempotent: updates the binary, does not clobber config, vault, or extra folders.

Homebrew / Nix / cargo-binstall / winget must produce the **same first-run state**. `cargo install` is not the advertised path. Stable URL and GitHub release publishing are **gated** (repo README Gates); this PRD specifies the shape only.

### 7.6 Config file (`kadou.toml`, TOML, XDG)

| Kind | Path | Override |
|---|---|---|
| User config | `~/.config/kadou/kadou.toml` | `KADOU_HOME` replaces the config **root** for tests/containers. `DOPS_HOME` is honored as a documented, deprecated alias for one release. |
| User kata, user themes | `~/.config/kadou/kata/`, `~/.config/kadou/themes/` | |
| Product data (vault, keys, cloned kata folders, mined/proposed drafts) | `~/.local/share/kadou/` | |
| State (history, pending, last-used args, mine work queue) | `~/.local/state/kadou/` | |
| Starter kata / bundled themes | embedded in the binary | |

`XDG_CONFIG_HOME`/`XDG_DATA_HOME`/`XDG_STATE_HOME` are not a second override next to `KADOU_HOME` — `etcetera`'s default strategy (used when `KADOU_HOME` is unset) already reads those.

```toml
# Missing keys mean defaults. This file may be empty.
theme    = "doop"
max_risk = "medium"           # human ceiling for every folder

[agent]
max_risk = "low"
allow    = []                 # ids the agent may run above low, e.g. "sesami/ses-deploy"

[folder.sesami]                # optional per-folder policy; nothing to "register"
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

[notify]
enabled = true
```

**[shape]** No `[[catalogs]]` — a folder exists because it is on disk. **[shape]** No `[keys]` override map — picker and prompt keys are fixed (§7.3). **[tui]** `[notify] enabled = true` by default; `false` turns off the desktop notification (§9 below). Config mode `0600`. Vault `0600`. No parameter values in TOML.

### 7.7 Starter kata contents

Embedded, auto-registered, **all `risk: low`**, no `needs:`, no network required.

| Id | About | Args |
|---|---|---|
| `starter/hello` | Print a greeting | `name` text = `world` |
| `starter/disk-usage` | Disk usage of a directory | `target_dir` text = `.` |
| `starter/git-status` | `git status -sb` in a repo | `target_dir` text = `.` |
| `starter/health` | Resolve a host (`ping -c 1`) | `host` text = `localhost` |
| `starter/list-path` | List a directory (`ls -la`) | `target_dir` text = `.` |

Arg names use `target_dir`, not `path` — `path.to_ascii_uppercase()` is `PATH`, which clobbers the shell's own `$PATH`; §4.3's reserved-name rule rejects `path` as an arg name at load time. `health` uses `ping -c 1` only (`getent` does not exist on macOS).

Five is enough for first paint and for agents to have something to call the same day. Team folders (`kadou get` / `kadou import`) grow the library; they are not how the product becomes real.

### 7.8 The notification

`kadou mcp serve` writes a pending record or a draft, then posts one desktop notification:

```
kadou · grant wanted
sesami/ses-deploy (critical) from claude-code
kadou grant approve 7c1e
```

Same for `propose_kata` ("draft wanted: `kadou accept ops/argocd-sync`") and for `kadou mine run --once` when it queues a draft. Shells out to `osascript` on macOS and `notify-send` on Linux; no crate, and it is silently skipped when neither binary exists. `[notify] enabled = true` by default. The `needs you` block in `kadou` (§7.2) is the durable copy for a human who missed the toast.

The smallest human surface that does the job: five existing commands, one `needs you` block, one notification, and the agent's own chat.

---

## 8. Non-goals and risks

### 8.1 Non-goals (inherited from `03`, as revised by `09` §6.1)

The PRD does not grow these. A later phase that wants one is a principles revision.

- A general agent harness / chat REPL.
- Generic `run_shell` / `exec` MCP tool.
- One MCP tool per kata as the default (or opt-in) surface.
- Required `kadou init` or a first-run questionnaire.
- Empty-by-default install.
- Web UI / SPA as a core interface (`kadou open`).
- SaaS, accounts, multi-tenant server, hosted control plane.
- Cloud-required features (except install/update and explicit `kadou get`).
- Unattended high/critical by agents without a prior human grant.
- Schema-printed confirmations (`CONFIRM`).
- Secrets in config, git, MCP schemas, or history.
- JSON as the human config format.
- Plugin marketplace / extension host / foreign runtimes as the default authoring model.
- Windows-first or PowerShell-default scripts.
- Replacing kubectl / Terraform / CI; kadou packages them as kata.
- Auto-commit / auto-push / auto-install of agent-invented folders.
- Becoming a distro or theme shop.
- Unopinionated defaults.
- `mine_list` / `mine_get` / `mine_run` / `mine_review` as extra MCP tools, `kadou://mine/*` resources, or a `review-mined` prompt.
- Walking `~/Documents/Sessions` artifact dumps, ledger files, or tool stdout as a mining corpus.
- Wiki-ingest from the miner.
- Executing mined scripts before human approve.
- MCP-spawned child processes inheriting the agent host's full environment.
- A grant/pending state whose reachable states disagree across sections.
- **[shape]** A registry, a `catalog.yaml`, a format version, scopes on args, `secret` on args.
- **[tui]** A full-screen TUI in v1. Alternate screen, panes, palette, and key maps are a later principles revision (`09` §7), taken only if humans start more runs interactively than agents start over MCP.

From `01` §3.1 also dropped: MCP file watcher, `DecryptingVarResolver`, stub progress notifications, `dops://history` resource, integer-vs-number as two author types, demo runner as runtime.

### 8.2 Risks

| Risk | Mitigation |
|---|---|
| Sesami's `REPO_ROOT`/`TRIGGER` idiom breaks one level shallower under single-file kata | `kadou import` rewrites the idiom once to `$KADOU_ROOT`, stable under later folder nesting (§4.6) |
| Folder rename (`src` → `sesami`) breaks vault keys and history | folder name is stable; docs warn; no auto-rename (decision 3) |
| Age 0.12.1 labeled BETA | Go vault import is **verified** (§6.5), reducing this to an upstream-maintenance watch. |
| `serde_yaml` ecosystem churn | Pin `serde-yaml-ng` 0.10.0 on the `kadou import` path only; fixture round-trip the 32 files; `serde-saphyr`/`serde_norway` as swap candidates |
| Agents ignore kadou and shell out to `kubectl` anyway | Product still must not *offer* a shell tool. Skill docs teach when to call kadou |
| HTTP MCP accidentally bound to `0.0.0.0`, or reachable by any local process without auth | Refuse non-loopback binds; require non-empty `allowed_origins` and a per-launch bearer token before HTTP ships at all |
| MCP-spawned children inherit the agent host's parent environment | Explicit env allowlist + `[exec] pass_env` for MCP; full parent env stays CLI-only (§6.1) |
| An agent overrides a need to redirect where the vault's secret is sent | Structural: needs can never appear in `args` (§4.4, §6.1) |
| `run_kata` blocks for an entire long-running build | `exec.timeout` + `mcp.max_wait` → `status: running` with a pollable log; concurrency limit (§6.1) |
| Token budget creep in tool descriptions | `insta` snapshot of `docs/design/tools-list.json` bytes; CI fails > 2 800 bytes. The byte gate is a *proxy* for token cost, not a tokenizer measurement — treat it as a ceiling, not a target. |
| Mining adds four MCP tools | Decision 15: reuse list/describe; CLI for run/review. Snapshot still 4 tools after the mining slice |
| Miner copies raw transcript lines into product dirs | Fail closed (`06` §4.1); synthetic `ghp_`-style fixtures only |
| Publishing install URL / crates / GitHub release | Out of scope here; repo Gates require Mason |
| **[shape]** A bespoke header parser is a new surface; its error messages are its user interface | Fixture corpus of bad headers under `tests/fixtures/headers/bad/`, snapshot-tested the way `tools/list` is (§4.7, `08` §6.2 risk 1) |
| **[shape]** Nothing distinguishes "a folder that failed check" from "a folder that is fine" without a registry's `active` flag | The `✗`/dimmed listing and the run refusal carry that weight; `kadou update` must never leave a folder half-pulled (`08` §6.2 risk 2) |
| **[tui]** Project-local `./kata` from an untrusted repo reaching the vault or an agent | `kadou trust` gate, path-keyed; agents never see an untrusted `./kata` at all (§4.5, `09` D3) |
| **[tui]** Shebang-as-runtime widens what a kata can declare as its interpreter | `kadou check` warns when the declared interpreter is missing from `PATH`; no shebang still defaults to `/bin/sh` (§6.1, `09` D2) |
| **[tui]** Inline picker misbehaves in a hostile terminal (tmux quirks, Terminal.app, Windows) | Numbered-list fallback; every command has a non-interactive form; `TERM=dumb` disables it (`09` §3.1 rule 3, §7) |
| **[tui]** Prompts become a second arg parser and drift from MCP | Prompts produce `key=value` strings and hand them to the same parser `kadou run` and `run_kata` use |
| **[tui]** The desktop notification is missed or unwanted | Convenience on top of the agent's own message and the `needs you` block; off with one config key; never the only channel |

---

## 9. MVP slice plan (re-cut for the kata shape and no-TUI decision)

The slice plan keeps a safe MCP path over Sesami before CLI polish, matching `01` rank 1–6 and `03` "8, 10, 11 are load-bearing." **[shape/tui]** Slices 2, 4, 5, 7, and 8 change scope from the first PRD revision; the rest are renamed (`runbook`→`kata`, `catalog`→`folder`) with no scope change.

### Slice 1 — Workspace, domain, XDG config

**Scope:** Cargo workspace, `kadou-core` types (`RiskLevel`, `Kata`, `Arg`, `Config`), `kadou.toml` load/save (missing keys = defaults), XDG paths via `etcetera` (B10), `kadou version`, `kadou --help`.
**Tests:** parse empty TOML; parse full example; `KADOU_HOME` isolation; `DOPS_HOME` fallback with a deprecation warning; risk order; config file `0600`; macOS paths resolve to `~/.config`/`~/.local/{share,state}`, not Application Support.
**Done:** `cargo test -p kadou-core` green; `kadou --help` lists the command tree stubs.

### Slice 2 — Header parser + folder scanner + `kadou check` + `kadou import` **[shape, re-cut]**

**Scope:** Header parser (§4.3), folder scanner and multi-file detection (§4.1), `kadou check` diagnostics with the bad-header fixture corpus (§4.7), `kadou import` with the full 32-file conversion (§4.6) including the `REPO_ROOT`/`TRIGGER` rewrite and boolean/numeric coercion. Read-only use of `~/Bitbucket/sdo-dops-catalog/src` locally; CI uses the sanitized fixture folder (decision 11, B15).
**Tests:** 32 real YAML files (local run) and the sanitized fixture set (CI) import; header parser fixtures in `tests/fixtures/headers/{good,bad}/` snapshot-tested; unknown header key fails; missing `about`/`risk` fails; `ses-argocd-sync`'s empty-default-but-required arg imports as required; 87 string-typed defaults coerce (86 booleans + 1 integer); `select` without options fails; shared `scripts/` ignored as kata; a bad file in one folder does not break another; `x.sh`/`x/kata.sh` collision fails.
**Done:** `kadou import ~/Bitbucket/sdo-dops-catalog/src --as sesami && kadou check sesami` reports 32 kata, 0 errors.

### Slice 3 — Executor + CLI `run --dry-run`

**Scope:** `kadou-exec` shebang-runtime exec (env injection: CLI full parent env; the MCP allowlist path lands with slice 5), cwd = kata dir, cancel, `kadou run --dry-run`, `kadou show`. Uses the **embedded starter kata** stub from the outset.
**Tests:** fixture kata echoes `$FOO`; secret-shaped names appear in `env_names` not `env_public`; process-group cancel test (Unix, SIGTERM then SIGKILL); stdin is `/dev/null` and a `cat`-running script does not hang; stdout/stderr piped, not inherited; a bash helper invoked via `sh` still runs as bash; reserved arg names (`path`, `home`, …) rejected at load; a kata with `#!/usr/bin/env python3` runs under python; a kata with a missing interpreter warns at check time.
**Done:** `kadou run starter/hello --dry-run` and `kadou run sesami/cc4-aaa --dry-run` print env names including `JENKINS_TOKEN` as secret, no Jenkins HTTP.

### Slice 4 — Vault + flat needs payload + Go import **[shape, re-cut]**

**Scope:** age envelope per the byte-for-byte spec (§6.5), `0700`/`0600` atomic writes, the **flat** needs payload, `kadou vault set [--plain] <name>` (TTY/stdin only), last-used args store (`~/.local/state/kadou/last/`, §6.6), optional `keyring-core` + `apple-native-keyring-store` feature compiled and tested but off by default. Real Go `vault.json`/`keys.txt` import test against a **synthetic** payload.
**Tests:** round-trip vault; mask in history; MCP-schema helper strips secrets; Go-envelope import decrypts correctly and maps `global.*` to flat entries, reporting `catalog.*` runbook-scope values as dropped-or-last-used; wrong key gives a clean error; import is idempotent and non-destructive to `~/.dops/`; directories `0700`; `keyring-core` feature compiles on the macOS target.
**Done:** `kadou vault set jenkins_user --plain` persists via TTY/stdin only; files are `0600`; a synthetic Go-written vault imports successfully into the flat payload.

### Slice 5 — MCP list / describe / run / propose (ship, agent-usable, and safe on day one)

**Scope:**
- `kadou-mcp` + `kadou mcp serve` stdio. All **four** tools (`list_kata`, `describe_kata`, `run_kata`, `propose_kata`), including propose — without it, the first agent-usable ship has no "no kata → propose" loop.
- Wire agent ceiling default `low`, full visibility formula including the trust gate (§6.2, §4.5).
- MCP env allowlist + `[exec] pass_env`, including the `KADOU_*` vars (§6.1).
- `exec.timeout`, per-kata `timeout:` header, `mcp.max_wait` → `status: running`, cancellation wiring, concurrency limit.
- History write path with the fresh plain-text tier and stream redaction (§6.6) — every MCP run gets an audit record from the start.
- Needs-can-never-be-args as a tested invariant (§4.4, §6.1) and "MCP never writes the vault" as a tested invariant.
- The embedded starter kata is complete here (full 5 kata, §7.7), not stubbed.
- Truncation to 50 lines, compact JSON, `isError` rules, `missing_needs` error.
- `insta` snapshot of the exact four-tool `tools/list` against `docs/design/tools-list.json`.
- `pending_grant` itself, TTL, and dedupe may land in the *next* slice (6) as long as high/critical stay invisible to a default agent in the meantime.
- **[tui]** Trust gate test: an untrusted `./kata` is absent from `list_kata` even at `low` risk.

**Tests:** compact `tools/list` ≤ 2 800 bytes and byte-identical to `docs/design/tools-list.json`; list of Sesami at `max_risk=low` returns **exactly** the five expected ids; `describe sesami/cc4-aaa` (test config `max_risk=medium`) has `needs: ["jenkins_url","jenkins_user","jenkins_token"]` and no value; `run starter/hello` succeeds and produces a history record with `interface: mcp`; run above ceiling → `isError: true, error: no_such_kata`; a script that echoes `$JENKINS_TOKEN` shows `****` in both the MCP result **and** the on-disk log; a parent-env value like `FOO_TOKEN` set on the MCP server's own process is **not** visible inside the child; `args: {"jenkins_url": "…"}` is rejected as `invalid_args` (need, not arg); no code path lets MCP persist to the vault; stdin/stdout purity under MCP; `max_wait` returns `status: running` with a pollable `log_path`; HTTP bind `0.0.0.0` refused, and HTTP itself stays behind its build feature; **an untrusted project-local `./kata/tidy.sh` with `risk: low` is absent from `list_kata`.**
**Done:** a local MCP client can list / describe / run / propose over the starter kata and Sesami, with an audit trail, env isolation, and a structurally scope-locked argument surface from day one. **This is the first agent-usable ship, and it ships safe.**

### Slice 6 — Grants, pending, unified confirm

**Scope:** CLI `--confirm <id>` and the TTY prompt (§6.3), pending_grant files with sha/HEAD pin, TTL, dedupe, `pending_path` outcome visibility, `approve`/`expires` fields on the result, `kadou grant *`. (History itself already exists from slice 5.)
**Tests:** MCP run of a fixture `risk: high` kata, with folder policy `critical` and agent `max_risk: critical` so it is **visible** but not allow-listed → `pending_grant`; `grant allow` then run succeeds; without the ceiling raised, the same kata is simply invisible (`no_such_kata`); TTL expiry; dedupe on repeated identical args; approve refuses on a changed sha256; approve runs in the CLI's environment, not MCP's; the CLI TTY confirm and `--confirm` accept the same protocol as MCP's grant.
**Done:** with folder policy `critical` and agent `max_risk: critical`, `ses-deploy` (not allow-listed) returns `pending_grant`; a human `grant approve` executes it with the pinned kata; at the *default* config it is simply invisible to the agent.

### Slice 7 — Folder git install + accept hardening

**Scope:** `kadou get`/`update`/`remove`, git CLI clone/pull, `--root` monorepo symlink, `kadou accept [--into]` targeting only user-owned folders with a diff and `y/N` (§6.7), refusing a git-checkout target. `propose_kata` itself already shipped in slice 5.
**Tests:** `--root` cannot escape; accept refuses a git-backed folder target; accept prints the diff; `kadou get <url> --as sesami` clones cleanly (D4: Sesami needs no `--root`, kata sit at the repo root).
**Done:** a human can accept an agent's proposal from slice 5 into a user folder; `kadou get` round-trips a git URL.

### Slice 8 — Styled CLI, picker, prompts, notification, starter installer **[tui, re-cut]**

**Scope:** `ui` module (`style`, `frame`, `picker`, `prompt` — §3, `09` §3.7), the `kadou` library frame and the `needs you` block (§7.2), `kadou run`'s TTY prompts and confirm (§7.3, §6.3), the inline picker for id-less commands (§7.3), `[notify]` and the desktop notification (§7.8), `install.sh` with SHA-256 verify, rust-embed starter kata packaged under `crates/kadou/starter/`.
**Tests:** styled and plain snapshots of the four frames (`kadou`, `kadou run`, `kadou check`, picker preview); picker filter ranks id over about; `run` without a TTY and without an id exits 2; critical prompt requires the exact id typed; `NO_COLOR` strips; `osascript`/`notify-send` missing is not an error; first-run with empty config shows 5 starter kata with the `target_dir` param names; installer dry-run on a temp prefix, checksum mismatch aborts.
**Done:** `curl | sh` shape is in-tree; `kadou` after install is a finished frame; `kadou run` with no id picks; `kadou mcp serve` is still the agent path. **No ratatui app, no ratatui/kadou-tui dependency anywhere in the tree.**

### Slice 9 — Session mining (`kadou-mine`)

**Scope:** `crates/kadou-mine` + `kadou mine` subcommands per `06` §2–4 and §6.8. Drafts in `mined` (never in the library path, reserved name). `list_kata include_drafts` / `folder=mined` and `describe_kata` on `mined/*` list/describe regardless of ceiling. `run_kata` on a draft → `error=draft`. LaunchAgent `dev.kadou.mine` installer. Table-driven redaction tests with **synthetic** inputs only. No Go `internal/mine`.
**Tests:** `tools/list` still 4 tools and ≤ 2 800 bytes; `mine_list` is not a registered tool; fixture cluster of 3 synthetic sessions proposes one single-file header draft; `ghp_`-style fixture never appears in mine logs; `kadou mine approve` copies into `mined`; `kadou accept mined/<x> [--into folder]` is the only path to executable; MCP run of `mined/*` fails as a draft; missing Claude transcript → `skip: transcript_missing`.
**Done:** scheduled `kadou mine run --once` can queue a redacted draft; agents can list/describe it regardless of ceiling; they cannot execute or approve it.

### Cross-cutting

CI runs on **macOS and Linux** (`/bin/sh` differs — dash vs. bash-in-POSIX-mode) and includes: `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace`, the MSRV check (`cargo +1.88 check --workspace --all-targets`, §3.2), `cargo-deny` (§3.3), and the `tools/list` byte-snapshot against `docs/design/tools-list.json`.

---

## 10. Crate verification log

Retrieved from `https://crates.io/api/v1/crates/<name>` on **2026-09-11**. User-Agent `kadou-prd-research/0.1`. **[shape]** rows for `ratatui` are removed; rows for `anstyle`, `anstream`, `nucleo-matcher`, `inquire` are added and newly verified this revision.

| Crate | max_stable_version | license field |
|---|---|---|
| rmcp | 3.3.0 | Apache-2.0 |
| rmcp-macros | 3.3.0 | Apache-2.0 |
| clap | 4.6.6 | MIT OR Apache-2.0 |
| clap_complete | 4.6.9 | MIT OR Apache-2.0 |
| crossterm | 0.29.0 | MIT |
| **anstyle** | **1.0.14** | **MIT OR Apache-2.0** |
| **anstream** | **1.0.0** | **MIT OR Apache-2.0** |
| **nucleo-matcher** | **0.3.1** | **MPL-2.0** |
| **inquire** | **0.9.4** | **MIT** |
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

`directories` 6.0.0, bare `keyring` 4.2.0, and **`ratatui` 0.30.2** are removed from this table (rejected — B10, B12, and `09` D7 respectively; see §3.1 "Explicitly not used"). `anstyle` and `anstream`'s declared `rust-version` is 1.66.0, `inquire`'s is 1.80.0, `nucleo-matcher` declares none — all well under the workspace's 1.88 MSRV (§3.2).

**Verified, not unverified:** age identity file interoperability with Go `filippo.io/age` `keys.txt`. **Still unverified:** live MCP JSON-RPC framing overhead beyond the tens-of-bytes estimate; a real-tokenizer (as opposed to byte-ratio) measurement of `docs/design/tools-list.json`; live elicitation support in Claude Code or Cursor (`09` §4.2 — elicitation is rejected as the grant channel regardless).

---

## 11. Key decisions (index)

1. Four meta-tools, lazy describe, no resources/prompts capability advertised at all — `03` §8, `02` §7.1, `01` §6; `07` B1, B11; **[shape]** renamed `*_kata`.
2. **[shape]** One header grammar; no `format_version`; old dops catalogs convert once via `kadou import` — `08` §1, §3.
3. **[shape]** Folder name is the stable id prefix; renaming a folder is an explicit directory rename — `08` §4.1.
4. One visibility-and-grant state machine: hidden → `no_such_kata` everywhere; visible + high/critical + not allow-listed → `pending_grant` with a pinned, TTL'd, deduped record carrying `approve`/`expires` — `03` §10–11; `07` B4; **[tui]** two faces (CLI, MCP), not three.
5. Human ceiling medium (per-folder, replaces not intersects the global), agent ceiling low, stated as an exact formula; **[tui]** trust is a visibility term — `03` §1, §10; `07` B4; `09` D3.
6. Age vault with a byte-for-byte-specified Go-compatible envelope; **[shape]** flat needs payload with a secret bit, replacing scoped catalog/runbook keys — `01` §2.4, `03` §6; `07` B12, B13; `08` §6.2.
7. Exec the kata file with env; **[tui]** the shebang is the runtime, `/bin/sh` default; MCP children get an env allowlist, a timeout, and a concurrency limit, not the full parent environment — `01` §7; `07` B5, B6; `09` D2.
8. History 90d / 50MB / 10MB archives, 7-day plain-text fresh tier, pre-write stream redaction; **[shape]** field renames (`folder`, `id`); **[tui]** `interface` drops `tui` — `01` §9 Q8; `07` B3.
9. Skills parse-only in MVP; `*.md` beside kata ignored by the loader — `01` §9 Q9.
10. **[tui]** CLI+MCP only; no TUI in v1; picker and prompts inside the CLI — `03` charter 13 (revised); `07` B8; `09` D7, §5.
11. `kadou check` is the loader; per-folder failure isolation — `01` §9 Q12; `07` B9; `08` §6.2.
12. Official `rmcp` 3.3.0; `serde-yaml-ng` on the import path only; `etcetera` not `directories`; `keyring-core` not bare `keyring`; **[shape]** `ratatui` removed, picker/prompt/styled-output crates added — `07` B10, B12; `09` §3.7.
13. Starter kata embedded with non-`PATH`-colliding arg names; install = ready — `03` §2–3; `07` B6.
14. Token budgets: ≤2 800 compact bytes / ≤800 tokens connect (measured: **2 028 B / ~507 tok**), ≤1 500 typical run, ≤60s to first kata — `07` B1; `08` §6.2 (payload shrank further from the first revision's 2 330 B).
15. Session mining is `kadou-mine` + `kadou mine`, not four MCP tools; drafts are single-file headers in the reserved, non-configurable staging areas `mined`/`proposed`, list/describe-only regardless of ceiling, until a human runs `kadou accept` — `06` §5 vs `03` §8; `07` B14; `08` §6.2.
16. **[new, shape]** Kata is one file with a closed-grammar header; folders are namespaces; no registry — `08` §1, §11.
17. **[new, tui]** Human review is told, not housed: `approve` in results, desktop notification, `needs you` block — `09` §4, §6.2 §11.

---

## 12. Deviations and limits

- This document does not implement code. Crate versions will drift after 2026-09-11.
- Token-per-connect target is a budget on **our** `tools/list` payload, measured as compact bytes and a 4 B/token (and 3.5 B/token) estimate — not a billed-token measurement from Claude/Cursor, and not a real tokenizer count.
- Go vault import is **verified**, not best-effort; the envelope is specified byte-for-byte (§6.5).
- `docs/design/04-naming.md` now exists in this tree (merged from `sd/dops/naming`); it documents the naming *research*, not a still-open decision — the product name `kadou` and the unit name `kata` are both settled (`08` §5).
- Publishing `install.sh` to a stable URL, crates.io, and GitHub Releases is gated on Mason (repo README Gates).
- `06` §5.1 Go layout is explicitly ignored.
- **[shape]** `kadou catalog rename --migrate-vault` from the first revision is moot — there is no catalog registry to rename; renaming a folder is a directory rename (decision 3), and history/last-used args do not migrate by design.
- Live MCP host behavior (per-call timeouts, tool-search deferral by real hosts) was not run; `07`'s findings are static analysis, not a live host trace. The Linux `/bin/sh` (dash) behavior of the Sesami scripts was not run in this review pass.
- **[shape]** Byte counts for `08` §6.2's estimate (2 330 B minus ~250 B ≈ 2 080 B) versus this revision's measured 2 028 B differ by ~50 bytes — both are estimates over the same hand-authored schema; the measured number in §1.3 and §5.4 is authoritative.
- Did not wiki-ingest (assignment). Did not push.

---

## 13. Revision log

This document has had two revisions. The **first revision** (2026-09-11, earlier) resolved `docs/design/07-review.md` §8.1 (blocking) against the original runbook/catalog shape and took the cheap items from §8.2; that table is kept below for history. The **second revision** (this one, same day) applies `docs/design/08-shape-review.md` (single-file kata, folders, no registry) and `docs/design/09-tui-decision.md` (no full-screen TUI; styled CLI + picker) in full, per Mason's decisions D1–D7 recorded in `09` §0. The new delta map is below the first revision's tables.

### First revision — Blocking (`07` §8.1)

| Rank | ID | Revision | Sections changed (first revision) |
|---|---|---|---|
| 1 | B4 | One visibility-and-grant state machine; hidden → `no_such_runbook`; pending record gets a sha/HEAD pin, 24h TTL, dedupe, `pending_path` | §2 row 4, row 5; §5.5; §6.2; §6.3; §6.4 |
| 2 | B2 | No secret exfiltration through MCP `args`; MCP never writes the vault | §1.2; §4.6; §6.1; §6.5 |
| 3 | B5 | MCP child environment allowlist, `exec.timeout`, `mcp.max_wait`→`running`, cancel, concurrency limit | §3; §3.1; §6.1; §6.2; §8.2 |
| 4 | B3 | Redact before persisting; 7-day plain-text fresh tier; retention for pending/proposed | §2 row 8; §3.1; §5.5; §6.6 |
| 5 | B8 | Re-cut the slice plan; move starter embed, history, redaction, env allowlist, arg lock, lifecycle, propose/accept forward | §9 (entire section) |
| 6 | B1 | Meet the token budget; hand-authored schemas; gate at 2 800 B; compact JSON; `isError` rules | §1.3; §5.1; §5.2; §5.4; §5.5; §8.2; `tools-list.json` |
| 7 | B6 | Env naming/reserved names; per-type serialization; rename starter `path` params; drop `getent` | §4.4; §5.4; §6.1; §7.6 |
| 8 | B7 | Harden `propose_runbook`/accept: pattern + containment, size caps, required risk, strict load, `proposal_id` rename, user-owned-catalog accept | §5.4; §5.5; §6.7 |
| 9 | B9 | Loader: string-default coercion, complete known-key list, per-catalog failure isolation, opt-in shared params | §2 row 2, row 12; §4.3; §4.5; §4.7 |
| 10 | B10 | XDG paths: `etcetera` replaces `directories` | §3; §3.1; §7.5 |
| 11 | B11 | HTTP transport: `allowed_origins`, bearer token, or gated behind a build feature | §5.1 |
| 12 | B12 | Vault crypto: byte-for-byte Go envelope spec; passphrase-in-keyring; MSRV resolved | §2 row 6; §3.1; §3.2; §6.5 |
| 13 | B13 | Secret entry: `kadou vault set` from TTY/stdin; `config set` refuses vault keys | §1.2; §6.5; §7.1; §7.3 |
| 14 | B14 | Staging model: visible regardless of ceiling, reserved names, flattened layout, accept, no `active = true` | §2 row 14, row 15; §4.1; §4.2; §5.3; §6.8; §7.5 |
| 15 | B15 | CI fixtures: sanitized shape-preserving folder for CI, real tree locally; scrub hostnames | §2 row 11; §4.5, §6.5; §9 |

Naming, suggestions, and deferred items from the first revision are unchanged in substance and are superseded in wording by the shape below; see git history for the exact first-revision text.

### Second revision — Shape (`08` §7) delta map

Every row of `08` §7's own "PRD deltas" table, and where it landed in this revision:

| `08` §7 row | Landed in |
|---|---|
| Title block, §1.1 | §1.1 |
| §1.2 users | §1.2 |
| §1.3 metrics | §1.3 |
| §2 row 1 (tool names) | §2 row 1, §5.4 |
| §2 row 2 (format versioning) | §2 row 2, §4.3 |
| §2 row 3 (catalog identity) | §2 row 3, §4.2 |
| §2 row 7 (script contract) | §2 row 7, §6.1 |
| §2 row 9 (skills) | §2 row 9 |
| §2 row 11 (compatibility tests) | §2 row 11, §9 slice 2 |
| §2 row 12 (loader strictness) | §2 row 12, §4.7 |
| §2 row 13 (secret flag) | §2 row 13, §4.4 |
| §2 row 14 (inactive catalogs, staging) | §2 row 14, §4.1, §4.5, §6.7 |
| §2 row 15 (mining) | §2 row 15, §6.8 |
| §3 workspace | §3, §3.1 |
| §4 (all — rewrite as "Kata format") | §4 (entire section) |
| §5.2 progressive disclosure | §5.2 |
| §5.4 schemas | §5.4, `tools-list.json` |
| §5.5 results | §5.5 |
| §5.6 snippet | §5.6 |
| §6.1 exec contract | §6.1 |
| §6.2 ceilings | §6.2 |
| §6.4 grants | §6.4 |
| §6.5 vault | §6.5 |
| §6.6 history | §6.6 |
| §6.7 propose/accept | §6.7 |
| §6.8 mining | §6.8 |
| §7.1 command tree | §7.1 |
| §7.2 keys | §7.3 (retitled "Picker and prompt keys" per `09`, not the `08` TUI key table — see the TUI-decision map below) |
| §7.3 first run | §7.4 |
| §7.5 config | §7.6 |
| §7.6 starter | §7.7 |
| §8.1 non-goals | §8.1 |
| §8.2 risks | §8.2 |
| §9 slices 2, 4, 5, 8 | §9 |
| §11 decisions index | §11 |
| §13 revision log | §13 (this table) |
| `docs/design/tools-list.json` | regenerated, measured 2 028 B |

### Second revision — TUI decision (`09` §6) delta map

D1–D7 from `09` §0, and every row of `09` §6.1 (→ `03-principles.md`) and `09` §6.2 (→ this document):

| `09` item | Landed in |
|---|---|
| D1 (name: kata) | §2 row 1, §11 item 1 (already the shape's own decision — `08` row 16) |
| D2 (shebang runtime) | §2 row 7, §6.1, §11 item 7 |
| D3 (project-local trust by path) | §2 row 5, §4.5, §6.2, §8.2, §11 item 5 |
| D4 (Sesami kata at repo root) | §4.6 (import notes), `03-principles.md` unaffected (repository layout, not product principle) |
| D5 (last-used args, interactive-prefill only) | §6.6, §7.3, §11 item 6 (folded into decision table row 6/8) |
| D6 (starter materialized on first run) | §7.4, §7.7 |
| D7 (no full-screen TUI) | §2 row 10, §8.1, §9 slice 8, §11 item 10 |
| `09` §6.1 (intro paragraph, mapping table rows 2/3/5, §4, §5, convention table, charter 3/5/11/13, non-goals, scoreboard) | `docs/design/03-principles.md` (see that file's own diff for the line-by-line application) |
| `09` §6.2 §1.1–§1.3 | §1.1, §1.2, §1.3 |
| `09` §6.2 §3, §3.1, §3.2 | §3, §3.1, §3.2 |
| `09` §6.2 §6.3, §6.4 | §6.3, §6.4 |
| `09` §6.2 §5.5 (`approve`/`expires`) | §5.5 |
| `09` §6.2 §6.6 (`interface` enum) | §6.6 |
| `09` §6.2 §6.7 | §6.7 (TUI palette line removed — no palette) |
| `09` §6.2 §7 title, §7.1, §7.2, §7.3, §7.5 | §7 title, §7.1, §7.3, §7.4, §7.6 |
| `09` §6.2 §8.1, §8.2 | §8.1, §8.2 |
| `09` §6.2 §9 slice 8 | §9 slice 8 |
| `09` §6.2 §11 decision 10, new decision 17 | §11 item 10, item 17 |
| `09` §6.2 §12 | §12 (elicitation note folded into §10) |
| `09` §6.2 §13 | §13 (this document's own revision log) |
| `09` §6.3 (`08` itself, not edited — superseded lines listed for the record) | Not applicable to this document; `08` stays a dated review per `09` §6.3 |
