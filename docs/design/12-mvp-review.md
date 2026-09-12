# kadou MVP review (closing, slices 1–9)

**Date:** 2026-09-12
**Reviewer:** independent worker `dops-mvp-review` (Claude Opus 5), reporting to `dops-planner`
**Code reviewed:** `9c076c0` — "chore: record kadou slice-8 and kadou-mine mutation baselines, raise CI floors" (`main`, slice-9 tip)
**Scope:** all five crates (`kadou`, `kadou-core`, `kadou-exec`, `kadou-mcp`, `kadou-mine`), `install.sh`, `.github/workflows/ci.yml`, `.cargo/mutants.toml`, `CLAUDE.md`
**Contract:** `docs/design/05-prd.md` (product/architecture), `docs/design/06-session-mining.md` (mining), `docs/design/08-shape-review.md` and `docs/design/09-tui-decision.md` (winning over the PRD), `docs/design/03-principles.md` (charter/non-goals)
**Prior review:** `docs/design/11-code-review.md` (slices 1–6, at `89a30c4`). **Mutation baselines:** `docs/design/10-mutation-baseline.md`.
**Standard applied:** `CLAUDE.md` and `.claude/skills/clean-code-*`, `.claude/skills/tdd-workflow`, `.claude/skills/mutation-testing`.

**Method.** Every non-test source file in §4's scope (`kadou-mine`, `kadou/src/ui`, and `kadou-core`'s `folder`/`git`/`runner`/`redact`/`history`/`visibility`) was read in full, as was every module implementing a CLAUDE.md invariant (`schema.rs`, `visibility.rs` ×2, `resolve.rs`, `env.rs`, `redact.rs`, `pending.rs`, `drafts.rs`, `state.rs`, `tools.rs`, `fsutil.rs`, `history.rs`). The rest of the tree — `header.rs`, `import.rs`, `vault.rs`, `check.rs`, `scan.rs`, `commands.rs`'s non-mine half — was read selectively, targeted at the claims this review makes; `11-code-review.md` read those in full at `89a30c4` and I did not repeat that pass. Every safety invariant was checked twice: by reading the code, and by driving the real binary and the real MCP stdio server end to end under an **isolated `HOME` + `KADOU_HOME`** (`env -i`, a throwaway temp home — Mason's real `~/.dops`, `~/.config/kadou` and `~/Documents/Sessions` were never read). `~/Bitbucket/sdo-dops-catalog` was read only as the `kadou import` source. No Jenkins was reached. No code was modified: the only file this review adds is this one.

---

## 0. Evidence collected this pass

| Check | Command | Result |
|---|---|---|
| Build | `cargo build --workspace` | clean, 14.2 s |
| Format | `cargo +stable fmt --all -- --check` | **clean** (exit 0) |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | **clean** (exit 0) |
| Tests | `cargo test --workspace` | **553 passed, 0 failed** across 11 non-empty targets: `kadou` unit 69, `kadou/tests/cli.rs` 80, `kadou/tests/install_sh.rs` 4, `kadou-core` unit 170, `header_fixtures` 2, `sesami_shaped` 4, `kadou-exec` unit 22, `kadou-mcp` unit 62, `mcp_server` 30, `kadou-mine` unit 106, `kadou-mine/tests/pipeline.rs` 4 |
| MSRV | `cargo +1.88 check --workspace --all-targets` | **clean** (exit 0) |
| Licenses / advisories | `cargo deny check` | **`advisories ok, bans ok, licenses ok, sources ok`** |
| Wire schema | `kadou mcp schema --bytes` vs `docs/design/tools-list.json` | **byte-identical, 2028 B** (`cmp` clean) |
| Wire schema over a live stdio client | JSON-RPC `tools/list`, re-serialized compactly | **byte-identical, 2028 B** |
| Sesami import | `kadou import ~/Bitbucket/sdo-dops-catalog/src --as sesami && kadou check sesami` | **32 kata, 0 errors, 58 warnings** (all "no vault entry yet") |
| Mutation | `cargo mutants --workspace -j 4` | see §5 |
| Pedantic/nursery baseline | `cargo clippy … -W clippy::pedantic -W clippy::nursery` | **418 warnings** (lib + lib-test double-count included) |

**One intermittent test failure was observed and is reported honestly.** One of four full `cargo test --workspace` runs failed with **2 failures in `crates/kadou-mcp/tests/mcp_server.rs`**, that target taking 34.4 s instead of its usual 2.6 s, while `cargo mutants` was saturating the machine. Three subsequent full runs and five repeat runs of that target alone passed (30/30 each). The failing test names were lost to output filtering and I could not reproduce them again, so I will not name them — but `mcp_server.rs:476` and `mcp_server.rs:502` are the only two tests in that file that race a real wall clock (`mcp.max_wait` of 150 ms and 100 ms against a `sleep 1` / `sleep 5` child, plus a 2 s `tokio::time::sleep` at `:493`), and they are the obvious suspects. See finding **L4**.

---

## 1. Verdict: is the MVP complete?

**Yes, for every "Done" line in `docs/design/05-prd.md` §9, slices 1–9, and for every item in the Cross-cutting CI list.** Each Done line below was re-run, not read. **One** Done line (slice 3) holds only with a qualifier, marked ⚠ and explained; the Cross-cutting list carries one more caveat that applies to all of it — CI has never actually run.

| Slice | §9 "Done" line | Verdict | Evidence |
|---|---|---|---|
| **1** — workspace, domain, XDG | `cargo test -p kadou-core` green; `kadou --help` lists the command tree stubs | **Done** | `cargo test -p kadou-core` → 170 unit + 2 `header_fixtures` + 4 `sesami_shaped`, all green. `kadou --help` lists all 19 subcommands (`version run list show new edit check get update remove import accept trust vault history grant mine mcp completion`), no stubs left. `DOPS_HOME` fallback warns: `warning: DOPS_HOME is deprecated and honored for one release only; set KADOU_HOME instead` (`crates/kadou-core/src/paths.rs:77`), reproduced under a temp home. `KADOU_HOME` isolation verified throughout this review. |
| **2** — header parser, scanner, `check`, `import` | `kadou import ~/Bitbucket/sdo-dops-catalog/src --as sesami && kadou check sesami` reports 32 kata, 0 errors | **Done** | Reproduced verbatim under the isolated home: `imported 32 kata into sesami (86 booleans coerced, 1 integers coerced)` then `checked 32 kata in sesami   0 errors  58 warnings`. The coercion counts match §4.6's "86 booleans + 1 integer" exactly. Risk mix on disk is §1.3's predicted `25 medium / 5 low / 1 high / 1 critical` (`grep '^# risk:'` over the imported folder). Multi-file `device-log-metrics` converted to `sesami/device-log-metrics/kata.sh`. |
| **3** — executor, `run --dry-run` | `kadou run starter/hello --dry-run` and `kadou run sesami/cc4-aaa --dry-run` print env names including `JENKINS_TOKEN` **as secret**, no Jenkins HTTP | **Done ⚠** | With the vault populated: `env_names: JENKINS_URL, JENKINS_USER, JENKINS_TOKEN, BRANCH, VERSION, SEND_EMAIL, PUBLISH_IMAGE, PUBLISH_API, ALLOW_IMAGE_OVERRIDE` / `secret_env_names: JENKINS_TOKEN`. **On a fresh home, before `kadou vault set`, the same command prints `secret_env_names: (none)`** — the secret bit is read only off a vault entry (`crates/kadou-core/src/resolve.rs:136-160`), and an unresolved need falls through to `secret: false` at `:158`. See drift **D1**. No network was touched (`--dry-run` never spawns, `crates/kadou-mcp/src/tools.rs:541-553`). |
| **4** — vault, flat needs, Go import | `kadou vault set jenkins_user --plain` persists via TTY/stdin only; files are `0600`; a synthetic Go-written vault imports into the flat payload | **Done** | Three `kadou vault set` calls over stdin persisted (`saved jenkins_url` / `jenkins_user` / `jenkins_token`); `kadou vault list` shows `jenkins_token secret`, the other two `plain`. On disk: `vault.json` and `keys/identity.txt` both `-rw-------`, `~/.local/share/kadou` and `.../keys` both `drwx------`. Go-envelope import is covered by `crates/kadou-core/src/vault.rs:407` `import_go_vault_once` and its synthetic-payload tests (in the 80-test `kadou-core` target). |
| **5** — MCP list/describe/run/propose | a local MCP client can list / describe / run / propose over the starter kata and Sesami, with an audit trail, env isolation, and a scope-locked argument surface | **Done** | Driven over real stdio JSON-RPC against `kadou mcp serve`. `tools/list` → 4 tools in order, **2028 wire bytes, byte-identical to the fixture** (well under the 2800 B gate). `list_kata folder=sesami` at the default ceiling returns **exactly** the five low-risk ids (`clone-ses-repos`, `device-log-metrics`, `helm-package`, `sdo-k8s-ses`, `ses-automation`). `describe_kata sesami/cc4-aaa` at the default ceiling → `no_such_kata`. `run_kata` on a kata echoing `$JENKINS_TOKEN` returned `token=****` and the on-disk log holds `token=****`; `grep -r 'hunter2-secret-token'` over the entire isolated home found **nothing**. `FOO_TOKEN=leaked-parent-secret` on the server process surfaced in the child as `parent=none`, and `TERM=dumb`. `args: {"jenkins_token": …}` → `invalid_args: `jenkins_token` is a need, supplied by the vault; needs can never be passed as args`. History record written with `"interface":"mcp"`. |
| **6** — grants, pending, unified confirm | with folder policy `critical` and agent `max_risk: critical`, `ses-deploy` returns `pending_grant`; a human `grant approve` executes it with the pinned kata; at the default config it is simply invisible | **Done** | At defaults: `run_kata sesami/ses-deploy` → `no_such_kata`. With `max_risk = "critical"` and `[agent] max_risk = "critical"`: → `{"status":"pending_grant","risk":"critical","pending_id":"17d361e2-…","approve":"kadou grant approve 17d361e2","expires":"2026-09-13T16:17:34Z","reason":"critical; not in [agent] allow"}` — note the **short** id in `approve` (R11/I-3 landed). A second identical call returned the **same** `pending_id` (dedupe). The record holds `args`, `args_hash`, `sha256`, `source`, `folder_head` and **no need values**; file `-rw-------` in a `drwx------` directory. `kadou grant approve 17d361e2` with stdin closed → `error: sesami/ses-deploy is critical and needs confirmation` + the exact `--confirm` line. |
| **7** — folder git install, accept hardening | a human can accept an agent's proposal into a user folder; `kadou get` round-trips a git URL | **Done** | Against a local bare fixture repo: `kadou get file://…/teamrepo --as team` cloned, `kadou check team` → `1 kata, 0 errors`, `kadou update team` → `updated team`. `kadou get --as mined` → `error: `mined` is a reserved name (starter, proposed, mined)`. `propose_kata` over MCP wrote `proposed/team/agent-draft`; `kadou accept team/agent-draft` into the **git-backed** `team` → `error: team is a git-backed folder; kadou accept only copies into a user-owned folder, never a git checkout`; `--into ops` succeeded, printed the diff, and `checked 2 kata in ops   0 errors`. |
| **8** — styled CLI, picker, prompts, notification, installer | `curl \| sh` shape is in-tree; `kadou` after install is a finished frame; `kadou run` with no id picks; `kadou mcp serve` is still the agent path. **No ratatui anywhere** | **Done** | `install.sh` exercised end to end from a `file://` fixture release: dry run → `kadou latest → …/kadou   sha256 ok (dry run)`; real install → binary in place, `kadou version` → `kadou 0.1.0`; a corrupted `SHA256SUMS` → `install.sh: checksum mismatch …` and **exit 1** before extraction. `kadou` over a real pty renders the frame with `kadou 稼働   3 folders · 39 kata                  needs you: 1 grant`; piped it prints one tab-separated kata per line (§7.2's own rule). `kadou history --limit 3` lists newest-first with `(cli)`/`(mcp)`. `grep -ril ratatui` over the tree (sources, manifests, `Cargo.lock`) returns **one** file — `crates/kadou/src/ui/mod.rs:4`, the comment asserting its absence. No dependency, no lock entry. |
| **9** — session mining | scheduled `kadou mine run --once` can queue a redacted draft; agents can list/describe it regardless of ceiling; they cannot execute or approve it | **Done** | `kadou mine run --once --index <fixture>` → `events: 4 transcripts: 3 (missing 1) candidates: 3 clusters: 1 queued: 1`; a second run → all zeroes (idempotent). `kadou mine show <fp>` prints the meta plus a valid `risk: medium` header with `context`/`namespace` required. `kadou mine approve` → `.../state/kadou/mined/kubectl-get-pods.sh`. Over MCP: `list_kata folder=mined` returns it with `"draft":true`; `run_kata mined/kubectl-get-pods` → `{"error":"draft","message":"draft; a human must run: kadou accept mined/kubectl-get-pods"}`; `kadou accept mined/… --into ops` is the only path to executable and it worked. `tools/list` is still exactly 4 tools with mining installed; no `mine_*` tool is registered. |

### Cross-cutting (§9)

| Item | Status |
|---|---|
| `cargo fmt --check` | ✅ green, and `.github/workflows/ci.yml:19-27` runs it |
| `clippy -D warnings` | ✅ green, `ci.yml:29-37` |
| `cargo test --workspace` on **macOS and Linux** | ✅ green locally on macOS; `ci.yml:39-52` has the two-OS matrix. ⚠ **The workflow has never run** — `ci.yml:1-3` says so explicitly ("the repo currently lives on origin.cursor.com, so this workflow will not run until the repo … is hosted on GitHub"). Everything below inherits that caveat. |
| MSRV `cargo +1.88 check --workspace --all-targets` | ✅ green locally; `ci.yml:54-64` |
| `cargo-deny` | ✅ green locally; `ci.yml:66-71` |
| `tools/list` byte-snapshot against `docs/design/tools-list.json` | ✅ **two independent gates**, both green: `crates/kadou-mcp/src/schema.rs:174-186` (value-level) and `crates/kadou-mcp/tests/mcp_server.rs:115-135` (the payload a real rmcp client receives). I reproduced both from outside the test suite. |
| Mutation ratchet (**not** in §9's list — added later, `d75326e`/`9c076c0`) | `ci.yml:85-124`. Two problems, both in §5: the `kadou` floor is **red** (87.7 % measured against a 90 % floor), and 1 592 mutants in a measured 56 minutes is a bad fit for a 60-minute wall at `-j 3`. Neither affects the §9 verdict — the ratchet is not one of §9's Cross-cutting items — but both are first-run failures waiting to happen. See **B6** and **Later-4**. |

**Verdict: the MVP is complete.** Nothing in §9's Done lines or its Cross-cutting list is missing, and every one of them was re-run rather than read. What remains is: one behavioural gap that slice 3's Done line papers over (**D1**); a set of mining-contract gaps inside slice 9's *scope* but outside its *Done line* (**D2–D11**); the mutation ratchet's own red floor, which post-dates §9 (**B6**); and hardening (§6).

The product does what `05-prd.md` says it does. An agent connecting today sees four constant tools costing ~507 tokens, five low-risk Sesami kata out of 32, no secrets in any result or log, no vault write path, no draft it can execute, and no way to raise its own ceiling — all verified against a running server in this review, not inferred.

---

## 2. Safety invariants, re-checked by reading **and** by running

`CLAUDE.md`'s six numbered invariants plus the five extra checks the brief names. Every row was checked twice.

| # | Invariant | Verdict | Read | Ran |
|---|---|---|---|---|
| **1** | `tools/list` byte-identical to `docs/design/tools-list.json`, four tools in that order, nothing else | **Holds** | `crates/kadou-mcp/src/schema.rs:35-100` is the single `json!` literal; `build_tools` (`:121-129`) derives the served `Tool`s from that same `Value`, so code and fixture cannot diverge. `preserve_order` is on workspace-wide (`schema.rs:8-11`) so field order is the literal's order. | `kadou mcp schema --bytes` → 2028 B, `cmp` against `docs/design/tools-list.json` (2029 B with its trailing newline) **identical**. Independently, a live JSON-RPC `tools/list` re-serialized compactly → **2028 B, identical**. `tools_list_stays_four_tools_with_mine_installed` (`crates/kadou-mcp/tests/mcp_server.rs:1077`) covers decision 15. |
| **2** | Agents never see high/critical by default | **Holds** | The formula now lives in exactly one place, `crates/kadou-core/src/visibility.rs:18-47` (`human_ceiling`, `agent_ceiling`, `is_visible_risk`); `crates/kadou-mcp/src/visibility.rs:25` only re-exports it (R12/C3 closed). `--max-risk` participates in the same `min()` (`visibility.rs:37-39`) so it can only narrow. Defaults: `max_risk = Medium` (`crates/kadou-core/src/config.rs:61`) and `agent.max_risk = Low` (`:84`). `is_visible_risk` is `<=` (`:45`), matching "a kata **at** the ceiling is allowed". | Fresh home, default config, live server: `list_kata folder=sesami` → **exactly 5 ids, all `low`**; `run_kata sesami/ses-deploy` (critical) → `no_such_kata`; `describe_kata sesami/cc4-aaa` (medium) → `no_such_kata`. |
| **3** | Needs are never args — structurally | **Holds** | `crates/kadou-core/src/resolve.rs:60-67`: every provided key is checked against `kata.needs` **before** any value is resolved or any env is built, and the error is its own variant `ResolveError::ArgNamesNeed` (`:46-47`). There is no runtime scope flag to get wrong. | Live: `run_kata {"id":"probe/needarg","args":{"jenkins_token":"x"}}` → `invalid_args` / `` `jenkins_token` is a need, supplied by the vault; needs can never be passed as args ``. |
| **4** | MCP never writes the vault, in any scope | **Holds** | `crates/kadou-mcp/src/state.rs:57-58` calls only `VaultStore::load`. Grepping `VaultStore` and `.save(` across the tree: the **only** non-test callers of `VaultStore::save` are in `crates/kadou/src/commands.rs` (`:696`, `:1604`, `:1731`) — the human CLI. `kadou-mcp` has zero. The only `vault` string in `kadou-mcp` outside comments is the `kadou vault set …` **fix line** in the `missing_needs` error (`tools.rs:596`). | `mcp_never_writes_the_vault` (`crates/kadou-mcp/tests/mcp_server.rs:436-470`) asserts vault bytes *and* mtime unchanged after `run_kata`/`describe_kata`/`propose_kata`. Independently: after a full MCP run session in the isolated home, `vault.json` was byte-unchanged. |
| **5** | Secrets redacted in results **and** logs, in the line stream, before either is written — on **every** path | **Holds now, mechanically** (was "by outcome only") | `crates/kadou-core/src/redact.rs:21` `redact_all` is the one implementation. `crates/kadou-core/src/runner.rs:181-193` builds the redaction closure and the log sink into the `RunSpec`; `crates/kadou-exec/src/lib.rs:245-268` redacts **each line as it arrives** and appends the already-redacted line to the log before it is ever collected. All three interfaces go through `runner::begin`/`finish`: MCP (`tools.rs:634-672`), `kadou run` and `kadou grant approve` (thin callers over `runner::run_one_blocking`, `runner.rs:314-321`). §6.6's fourth encoding (base64 of `user:value`) is implemented (`redact.rs:42-57`). | **CLI run:** printed `token=****`. **MCP run:** result `token=****`, on-disk log `token=****`. `grep -rl 'hunter2-secret-token'` over the **entire** isolated home (config, data, state, history, pending) → **no matches**. `runner.rs:449-507` is a table over `interface ∈ {cli, mcp}` asserting result, log and history record together. **Mining:** the pipeline integration test (`crates/kadou-mine/tests/pipeline.rs:23-50`) walks every file the run produced and asserts eight forbidden strings are absent; drafts pass `redact_or_drop` (`propose.rs:100-105`) which fails closed on R14. **Grant approve** uses the same shared runner, so it no longer prints the raw stream (I-9/R2 closed). |
| **6** | MCP children get an explicit env allowlist, never the host's full environment | **Holds, exactly** | `crates/kadou-mcp/src/env.rs:8-19` is §6.1's list verbatim — PATH HOME USER LOGNAME SHELL LANG TZ TMPDIR SSH_AUTH_SOCK KUBECONFIG — plus `LC_*` (`:34`), plus `[exec] pass_env` (`:35`), plus a forced `TERM=dumb` (`:38`). Nothing added, nothing missing. `env_clear: true` is set **only** on the MCP path (`tools.rs:662`, with the reason in the comment at `:660-661`); the CLI leaves it `false` because §6.1 says a human keeps their own shell. | Live, with `FOO_TOKEN=leaked-parent-secret` exported onto the `kadou mcp serve` process: the child printed `parent=none` and `term=dumb`. The **same** kata run from the CLI in the same shell printed `parent=leaked-parent-secret` — the intended asymmetry, proven both ways. |
| **7** | Pending records never hold need values | **Holds** | `PendingRecord.args` (`crates/kadou-mcp/src/pending.rs:31`) is built solely from `resolved_args` (`tools.rs:796-799`); `resolved_needs` is never passed to `PendingStore::create` (`pending.rs:207-212` has no needs parameter at all — structural, not a filter). | The on-disk record from a real `pending_grant` contains `args` (4 public args), `args_hash`, `sha256`, `source`, `folder_head`, and **no `JENKINS_*` value of any kind**. The pinned `source` sits in a `drwx------` directory at `-rw-------` (I-6 fixed for `pending/`). |
| **8** | Drafts are never runnable | **Holds** | `crates/kadou-mcp/src/drafts.rs:23-26` `is_draft_id` is a pure top-segment test; `tools.rs:522-524` short-circuits `run_kata` **before** lookup, visibility, or arg resolution — an id under `proposed/` or `mined/` can never reach a spawn even if a file exists there. | Live: `run_kata mined/kubectl-get-pods` with `agent.max_risk = critical` → `{"error":"draft","message":"draft; a human must run: kadou accept mined/kubectl-get-pods"}`. `run_kata_on_a_mined_draft_fails_as_a_draft_even_with_a_high_ceiling` (`crates/kadou-mcp/tests/mcp_server.rs:1054`) covers the same. |
| **9** | `mined` and `proposed` are never in the library path | **Holds** | Drafts are scanned out of `<state_dir>/{proposed,mined}` (`drafts.rs:31-41`), never `kata_dir`. `library_rows` (`tools.rs:123-130`) returns early when `folder` names a draft namespace, so a draft can never be served as a library row. `RESERVED_FOLDER_NAMES` (`crates/kadou-core/src/folder.rs:11`) blocks `kadou get --as mined/proposed/starter` (`:62`) and `kadou remove` on them (`:246`). | After a full session (import + get + propose + mine + accept), `kata/` contained exactly `sesami`, `starter`, `team`, `ops`, `probe` — no `mined`, no `proposed`. `kadou get --as mined` → reserved-name error, and no `mined` directory was created. |
| **10** | `--allow-risk`/`--max-risk` may only narrow | **Holds** | `visibility.rs:37-39` folds the flag into the same `min()` chain; the final `.min(human_ceiling(...))` at `:40` means no flag can lift an agent above the human's own ceiling. Unit-proven at `visibility.rs:95-102` and `:72-80`. | Covered by the default-config probes above. |
| **11** | Project-local trust gate | **Holds** | `crates/kadou-mcp/src/visibility.rs:30-38` compares canonicalized paths; `resolve_visible` (`tools.rs:51-55`) and `project_local_rows` (`:173-175`) both return nothing for an untrusted directory *before* any risk check. | `untrusted_project_local_kata_is_absent_from_list_kata_and_cannot_run` (`crates/kadou-mcp/tests/mcp_server.rs:577`) and `trusted_project_local_kata_is_visible_and_runs` (`:605`), both green. |

### Two partial holds worth naming

**P1 — `history/` and `history/logs/` are `0755`, not `0700`.** §6.6 says "History and pending directories `0700`". `fsutil::write_atomic_0600` calls `ensure_dir_0700(parent)` (`crates/kadou-core/src/fsutil.rs:14`), and `ensure_dir_0700` (`:36-44`) does `create_dir_all` then chmods **only the leaf**. So `history/records`, `history/logs/<date>` and `pending/` are `0700` — and the *intermediate* `history/` and `history/logs/` are created at the process umask. Reproduced on disk:

```
drwxr-xr-x  .../state/kadou/history
drwxr-xr-x  .../state/kadou/history/logs
drwx------  .../state/kadou/history/logs/2026-09-12
drwx------  .../state/kadou/history/records
drwx------  .../state/kadou/pending
```

Log *content* is still protected (the leaf is `0700` and every file is `0600`), so this is defence-in-depth, not a leak. But the invariant as stated does not hold, and the test that claims it does — `pending_and_history_directories_are_0700`, `crates/kadou-mcp/tests/mcp_server.rs:282-330` — only asserts the three leaf directories. See **L1**.

**P2 — R14's fail-closed check has a base64-shaped blind spot.** `crates/kadou-mine/src/redact.rs:373-377` treats "the match contains a `/`" as "this is a path, not a secret". Standard base64's alphabet *includes* `/`, so a 44-character base64 blob — the single most common shape of an unredacted secret — very often contains one and is therefore **not** flagged. The doc comment at `:368-372` states the assumption honestly ("a bare secret blob … essentially never contains a literal `/` in the fixtures this crate actually produces"), but the fixtures are not the threat model. See **L2**.

### Stale pointers in `CLAUDE.md` itself

`CLAUDE.md` is the standard this repo is judged against, and three of its own citations no longer resolve. Fixing them is cheap and they are load-bearing for the next reviewer:

- Invariant 2 says the visibility formula "lives in `crates/kadou-mcp/src/visibility.rs` (`human_ceiling`, `agent_ceiling`, `is_visible_risk`)". It moved to `crates/kadou-core/src/visibility.rs:18,28,45` in `40668bf` (R12); `human_ceiling` is not even re-exported from the mcp module.
- Invariant 5 says redaction lives in `crates/kadou-mcp/src/redact.rs` (`redact_all`). It moved to `crates/kadou-core/src/redact.rs:21` in `58ba86b` (R2).
- The Forced-verification section says "`cargo mutants` is not yet a CI job, run it locally". `.github/workflows/ci.yml:85-124` now **is** a `mutants` job with per-crate floors.

---

## 3. Spec drift since `11-code-review.md`

The previous review's 23 drift rows (I-1 … I-23) are all resolved — 18 in code, 5 in docs (`423d1fc`), and I-2's doc side in `ad42579`. I re-verified the safety-relevant ones by running: vault-first needs precedence (I-1) is live (the dry run showed the vault's `https://ci.example.com`, not the header's `https://ci.sesami.io`); short `pending_id` in `approve` (I-3); `propose_kata` id validation (I-13) rejects `a/../../../pwn` with the schema's own pattern in the message; `0700` state dirs (I-6, partially — see P1); `--ask` is now implemented on a TTY (slice 8, §7.3) and off one errors cleanly — `error: --ask needs a terminal` — rather than parsing and silently doing nothing, which is what I-19 flagged. **The rows below are new or still open.**

Numbered `D*` for drift. "Which side" is my recommendation, not a decision.

| # | Topic | Spec says | Code does | Which side |
|---|---|---|---|---|
| **D1** | **Secret bit on an unresolved need** | §9 slice 3's Done line: `kadou run sesami/cc4-aaa --dry-run` prints env names "including `JENKINS_TOKEN` **as secret**". §5.5's dry-run shape has `secret_env_names`. | The secret bit comes **only** from a vault entry (`crates/kadou-core/src/resolve.rs:138-143`). A need with no vault entry and no default falls through to `secret: false` (`:155-160`), so on a fresh install `secret_env_names` is empty for exactly the kata the Done line names. Reproduced. | **CODE.** A need with no default and no vault entry should resolve `secret: true` — fail-safe, and it makes the Done line literally true. There is no redaction consequence (an unresolved need has no value to redact), so the change is safe: one line at `resolve.rs:158`. §4.4's own logic supports it ("a need **with a default** is always plain" implies a need *without* one is not). |
| **D2** | **Miner streams; does not slurp** | `06` §2.1 item 5: "Codex jsonl alone is ~2.8 GB; **the miner streams line-by-line**". §3.5: "**Stream jsonl; never slurp** Codex/Grok stores", "Max 512 MB RSS". | `crates/kadou-mine/src/orchestrate.rs:173` is `std::fs::read(&transcript_path)` — the whole transcript into memory, then `String::from_utf8_lossy` at `:185` (a second copy). The bounds check at `:162` runs *before* each row, so a single 2.8 GB transcript is read in full regardless. `crates/kadou-mine/src/ingest.rs:30` slurps `index.jsonl` the same way. The module doc at `orchestrate.rs:22-24` asserts the opposite ("streams … line-by-line rather than slurping them") — that comment is false today. | **CODE.** This is the one place the mining implementation will actually fail against Mason's real corpus rather than a fixture. Either stream with `BufReader::lines()` (the parsers are already line-oriented — `crates/kadou-mine/src/parse.rs:103`), or at minimum `stat` the transcript and skip anything over a per-file byte cap, charging it to `bytes_scanned` first. Also fix the comment. |
| **D3** | **Idempotency rule 2** | `06` §3.4 rule 2: "Same transcript sha256 → skip extract". `ProcessedRecord.transcript_sha256` exists for it (`ingest.rs:65`). | Never populated: all four `record_processed` call sites pass `None` (`orchestrate.rs:168, 174, 182, 213`). Rule 2 is dead. | **CODE or DOC.** Rule 1 (`(path, bytes)`) already covers the repeat case in practice — proven: the second `mine run` did zero work. If rule 2 stays unimplemented, delete the `transcript_sha256` field rather than leaving a field the format promises and never fills. |
| **D4** | **`mine/state.json` checkpoint** | `06` §3.3 specifies `state.json` with `index_offset`, `index_sha256`, `last_when`, `last_run`, `transcripts_seen/missing`, `candidates_emitted`, `clusters`, and "**a killed run resumes from `index_offset`**". | `MineHome::state_path()` (`crates/kadou-mine/src/store.rs:44-46`) is **defined and never called** — grep across the workspace finds exactly one hit, its own definition. No `state.json` is written; `RunSummary` (`orchestrate.rs:51-61`) carries the counters in memory and the CLI prints them. `clusters.json` (also §3.3) does not exist at all. | **CODE or DOC.** Crash-resume as specified does not exist; the run is idempotent through `processed.jsonl` instead, which is a defensible simplification but not what §3.3 says. Pick one: implement `state.json`, or amend §3.3 to say `processed.jsonl` is the checkpoint and delete the dead `state_path()`. |
| **D5** | **Rank score floor** | `06` §2.6: the cutoff is `unique_sessions >= 3` or (`freq >= 8` and `unique_sessions >= 2`), "**and `score` above a floor**". | `rank::passes_cutoff` (`crates/kadou-mine/src/rank.rs:54-57`) implements the session/freq half and **ignores `score` entirely** — it does not even take a score. | **CODE** (add a `MIN_SCORE` and take the score) **or DOC** (drop the clause). Low stakes; a cluster that clears the session bar almost always clears any sane floor. |
| **D6** | **Catalog-collision penalty** | `06` §2.6: `catalog_penalty  # 0.1 if an existing runbook already covers argv0+subcommand`. | The formula implements it (`rank.rs:43-47`) but the one production call site hard-codes `false` (`orchestrate.rs:285`), so the penalty never fires. Documented as a known gap in the slice-9 handoff. | **CODE.** The engine already exists; what is missing is a lookup of the library's kata by argv0+subcommand. Small, and it is the mechanism that stops the miner from proposing something Sesami already has. |
| **D7** | **RSS bound** | `06` §3.5: "Max 512 MB RSS". | Not measured (`orchestrate.rs:22-24` says so). Combined with **D2**, the bound is not merely unmeasured — it is actively violated by a single large transcript. | **CODE** (fix D2 and the bound becomes structurally true) **or DOC** (replace "512 MB RSS" with "streams, never slurps", which is the real control). |
| **D8** | **`--since`** | `06` §3.2: `dops mine run --since <iso>` is the backfill trigger; §5.1 lists it. | Parsed and **warned about, not honoured**: `crates/kadou/src/commands.rs:2395-2403` prints `warning: --since is accepted but not yet a real filter; running the full unprocessed backlog`. | **CODE.** The warning is the honest interim (much better than I-19's silent drop), but a flag that warns on every backfill is not shippable long-term. One `if row.when < since { continue }` in `ingest_new_candidates`' loop. |
| **D9** | **`--watch`** | `06` §3.2 marks `--watch` "Optional". | Refused with exit 2 and a message citing §3.2 (`commands.rs:2389-2394`). | **Neither** — the spec calls it optional and the refusal names its own authority. Correct as-is. |
| **D10** | **Extraction kinds** | `06` §2.3 keeps three kinds: shell invocation, **generated script file** (`Write`/`FileChange` ending in `.sh`), and **fenced script** (assistant markdown fences with ≥3 command lines). §2.2's event stream has a `kind` field for exactly this. | Only the shell-invocation kind exists. `crates/kadou-mine/src/parse.rs` emits `RawShellEvent` (`model.rs:8-11`) with no `kind`; there is no `.sh`-write or fence extractor anywhere in the crate. | **CODE or DOC.** This is the largest single gap against `06`, and it is the one that matters most for Claude sessions specifically (`06` §4.7: "Claude transcripts are ~99% gone" — the fenced/written-script path is how a Claude session still yields a candidate). If it stays out of v1, say so in §2.3 rather than leaving two of three rows unimplemented. |
| **D11** | **Interactive mine review** | `06` §2.9: "`dops mine review` **walks the queue**. Actions: approve, open editor, reject (reason required), skip." §4.5 lists per-proposal keys. | `run_mine_review` (`commands.rs:2496-2515`) prints the queue and then the three commands a human should type next. There is no walk, no `$EDITOR` action, no `skip` state (`store.rs`'s audit actions are `queued`/`approved`/`rejected` only — §4.6 also lists `edited` and `skipped`). | **CODE or DOC.** The non-interactive shape is consistent with `09-tui-decision.md`'s "no full-screen TUI" and honestly labelled. But `06` §2.9's four states are part of the contract and two of them do not exist. Recommend: amend `06` §2.9/§4.6 to the three states the code has, or add `skip` (cheap) and drop `edited` in favour of "edit the file, then approve". |
| **D12** | **Mined drafts' home** | §6.8's path table maps `$DOPS_HOME/catalogs/mined/` → `~/.local/share/kadou/kata-drafts/mined/`. | Drafts live at `~/.local/state/kadou/mined/` (`crates/kadou-mine/src/store.rs:28`, `crates/kadou-mcp/src/drafts.rs:4-5`). | **DOC.** The code matches §4.1 ("`~/.local/state/kadou/` — history, pending, proposed, **mined**, last-used args") and §7.6's table; §6.8's row is the outlier. The PRD contradicts itself; §6.8 should be corrected to `~/.local/state/kadou/mined/`. |
| **D13** | **The mining skill** | §6.8: "The skill (`06` §5.2) **stays a `SKILL.md` that execs `kadou mine`**." | No such skill exists — `.claude/skills/` has twelve entries, none of them mining, and nothing in the tree mentions `kadou mine` outside the crates and docs. | **CODE** (add the skill) **or DOC**. Not in slice 9's Done line, so this is not an MVP blocker; it is the missing agent-facing half of §6.8. |
| **D14** | **Themes** | §4.1 shows `~/.config/kadou/themes/  # drop-in *.toml themes`; §7.6's config example has `theme = "doop"` and its table says bundled themes are "embedded in the binary". | `Config.theme` is parsed and round-tripped (`crates/kadou-core/src/config.rs:42`, default `"doop"` at `:60`) and `KadouPaths::themes_dir()` exists (`crates/kadou-core/src/paths.rs:24`) — but **nothing reads either**. `crates/kadou/src/ui/style.rs` has a single hard-coded palette. `themes_dir()` has no non-test caller. | **DOC.** This is the I-20 class again: a config key and a path that promise a control the product does not have. Either mark themes post-v1 in §4.1/§7.6, or drop `theme` from the config until a loader exists. A config key that silently does nothing is worse than no key. |
| **D15** | **`git clone` argument injection** | §6.7: "The product never `git commit`s, `git push`es, or `kadou get`s an agent-invented URL" — the URL is assumed human-supplied. | `crates/kadou-core/src/git.rs:84` does `args.push(url)` with **no `--` separator and no scheme check**, so a URL beginning with `-` is parsed by `git clone` as an option. `--upload-pack=…` / `-c core.sshCommand=…` are the classic escalations. `--ref` is safe (it is `--branch`'s value). | **CODE.** One line: `args.push("--")` before the URL, and/or reject a URL starting with `-`. Severity is low — this is human-typed input at the same trust level as running `git clone` yourself — but it is free to close and `kadou get <url>` is exactly the string a team README will tell people to paste. |
| **D16** | **`mine approve --into <name>` path traversal** | `06` §2.9 / §6.8: approve copies into the `mined` staging area. | `store::approve` does `home.mined_dir().join(format!("{name}.sh"))` (`crates/kadou-mine/src/store.rs:227`) with `name` taken verbatim from `--into` (`commands.rs:2522-2523`). `kadou mine approve <fp> --into ../../../.config/kadou/kata/sesami/pwn` writes an executable kata straight into the library, bypassing `kadou accept`. The **default** name is safe — the derived slug is sanitized to `[a-z0-9-]` (`crates/kadou-mine/src/propose.rs:79-95`). | **CODE.** Validate `name` against the §4.2 segment rule before joining — `drafts::valid_id_segment` (`crates/kadou-mcp/src/drafts.rs:77-84`) already exists; `kadou-mine` cannot depend on `kadou-mcp` (§9 slice 9: "no MCP types"), so lift the segment check into `kadou-core` and use it from both. Human-only input, so not agent-reachable — but it is the one place a typo silently makes an unreviewed script executable. |
| **D17** | **`kadou history` output shape** | §6.6 defines the record fields; §7.1 lists `kadou history [--limit N]`. | Implemented (`commands.rs:1539`, `crates/kadou-core/src/history.rs:170-181`) and newest-first. `list_recent` sorts by `start_time`, an RFC3339 **seconds**-resolution string, so two runs in the same second order arbitrarily. | **Neither, but note it.** Harmless for a human's history list; it is the same second-resolution timestamp that forces a 1.1 s `thread::sleep` in a test — see **L3**. |
| **D18** | **`log_path` is written twice** | §6.6: "Redaction happens in the line stream, **before** the log is written". | True — and then `HistoryStore::finish` (`crates/kadou-core/src/history.rs:150-155`) *rewrites the whole log file* with the joined output via `write_atomic_0600`. The content is the same already-redacted text, so there is no leak; but the fresh-tier log is written twice (once incrementally, once whole), the atomic rename swaps the inode a polling agent may be holding, and the final file loses the per-line trailing newline the stream wrote. | **CODE.** `finish` should stop writing the log now that `kadou-exec` owns it — it only needs to update the record. One deletion; the `interface` table test at `runner.rs:494-496` already pins the content. |

**Also still open from `11-code-review.md` (acknowledged, not regressions):** E6 — `kadou-exec`'s collector `Vec<String>` (`crates/kadou-exec/src/lib.rs:254-267`) is still unbounded, so a kata that prints gigabytes buffers all of it; R2's plan said "with a line-count cap for E6" and the cap did not land. The MCP *result* is capped (`MAX_OUTPUT_BYTES = 8192`, `tools.rs:21`), but that is after the fact. R16 (table-driven the copy-paste suites) and the phase-2/3 half of §6's clippy rollout were not attempted — `import.rs:641` carries an `#[allow(clippy::too_many_lines)]` that says so.

---

## 4. Clean-code pass on the new and heavily changed code

Standard: `.claude/skills/clean-code-{naming,functions,errors,design,solid,boundaries,testing}/SKILL.md`. Crates in scope: **`kadou-mine`** (new), **`kadou/src/ui`** (new), **`kadou-core/src/{folder,git,runner,redact,history,visibility}.rs`** (new), and `kadou/src/commands.rs` (heavily changed).

### What is good, and worth saying

- **`kadou-core::runner` is the single best structural change since the last review.** `docs/design/05-prd.md` §3 asks for "one engine … so confirm/risk/vault cannot drift"; `runner.rs` is that engine, and C6's three hand-copied run pipelines are now three thin callers. The `begin`/`finish` split (`runner.rs:141`, `:256`) is exactly right: MCP needs the record before the run ends, the CLI does not, and `run_one_blocking` (`:314`) composes both for the callers that just want to block. The table test at `:449` over `interface ∈ {cli, mcp}` is the `clean-code-testing` "table-driven, and the empty cell is the missing case" pattern applied where it matters.
- **Panic hygiene is real.** `unwrap_used`/`expect_used` are `deny` workspace-wide (`Cargo.toml`), and the only two carve-outs in production code (`crates/kadou-mine/src/{normalize,redact}.rs:41/79`) are compile-time-fixed regex literals with the reasoning written down. That satisfies `clean-code-errors`' "prefer proving the impossible case can't happen" while being honest that a `LazyLock<Regex>` cannot be.
- **`kadou-core::folder` and `::git` are model modules.** Small named functions, each doing one thing at one level (`validate_root_syntax` vs `resolve_root` — the pre-clone lexical half and the post-clone symlink half, `folder.rs:82` and `:96`, with the split explained). `UpdateOutcome` (`folder.rs:183-188`) reports every folder rather than silently skipping — `clean-code-errors`' "never swallow". `git::is_git_backed` is defined once and used by `accept`, `remove`, and the pending HEAD pin (`git.rs:43-55`), which is the DRY-on-knowledge rule, not DRY-on-shape.
- **`ui` respects the boundary rule.** `crossterm`/`inquire`/`anstyle` are confined to `crates/kadou/src/ui`, exactly as `clean-code-boundaries` requires ("terminal output stays inside the `kadou` bin's `ui` module"). `style::use_color(is_tty, no_color_set, plain_flag)` (`ui/style.rs:16`) is a pure function of injected state — the decision is unit-testable without a terminal, which is why `ui` has snapshot tests at all.
- **`kadou-mine` has no MCP types and no cycle.** `crates/kadou-mine/src/lib.rs:6-9` states the rule and the dependency graph honours it: `kadou-mine → kadou-core` only. Drafts reach MCP through `kadou_core::scan_folder` generically. This is the `clean-code-solid` DIP guidance applied correctly.

### Findings

Numbered `L*`. Effort: S ≤ 1 h, M ≤ half a day, L ≤ 2 days.

**L1 — `ensure_dir_0700` only chmods the leaf (P1). Effort: S.**
`crates/kadou-core/src/fsutil.rs:36-44`. `create_dir_all` creates intermediates at the umask; the chmod applies to `dir` alone. Fix: walk the components created under the caller's root and set `0700` on each, or have `HistoryStore` call `ensure_dir_0700` on `history/` and `history/logs/` at `begin` (`history.rs:89-113`). Proving test: extend `pending_and_history_directories_are_0700` (`crates/kadou-mcp/tests/mcp_server.rs:282`) to assert `history/` and `history/logs/` themselves, which is where it should have asserted in the first place.

**L2 — R14's "contains a slash means it's a path" heuristic (P2). Effort: S.**
`crates/kadou-mine/src/redact.rs:373-377`. Standard base64 contains `/`. Fix: decide "is a path" on the *surrounding* context (the match is preceded by `/` or `$`, or the whole token parses as a path) rather than on the match's own content, and separately exempt `--flag`-shaped matches as `06` §4.3 asks. Proving test: a table case `TOKEN=<44 base64 chars including a slash>` asserting `has_high_entropy_leak` is `true`, alongside the existing path-negative case.

**L3 — A test sleeps 1.1 s to get a distinct timestamp. Effort: S.**
`crates/kadou/src/commands.rs:2891` — `std::thread::sleep(Duration::from_millis(1100))` in `build_grant_rows_includes_only_outstanding_records_oldest_first`. `.claude/skills/clean-code-testing/SKILL.md:31` ("no … sleeping") and `:129` ("Do not depend on … wall-clock sleeps") both forbid this. Root cause is D17: `PendingStore::create` stamps RFC3339 **seconds**, so two records made in the same second are indistinguishable. Fix: give `create` an injectable timestamp (the `now`-as-a-parameter pattern `rank::score` already uses at `rank.rs:33`), and pass two fixed instants. Proving test: the same test, with the sleep deleted and two explicit timestamps.

**L4 — Two `mcp_server.rs` tests race a real wall clock. Effort: S.**
`crates/kadou-mcp/tests/mcp_server.rs:476-500` and `:502-540` set `mcp.max_wait` to 150 ms / 100 ms against `sleep 1` / `sleep 5` children, and `:493` sleeps 2 s outright. Under load these are the only plausible source of the intermittent 2-failure workspace run in §0 (that target took 34.4 s instead of 2.6 s). This is the same class R8/G1 already fixed once for `kadou-exec` — the fix there was a readiness file, and the same technique applies: have the child write a readiness marker, poll for it, and assert on *ordering* (log has content before the run ends) rather than on a millisecond budget. Proving test: the same two tests, run 8× in parallel with a loaded machine — which is how G1 was demonstrated and fixed.

**L5 — `commands.rs` is 2 962 lines and ten command families. Effort: M.**
Every function in it is now under the 60-line ratchet (that part of R7 landed and holds), but the *file* is `run`/`show`/`list`/`check`/`new`/`edit`/`vault`/`grant`/`get`/`update`/`remove`/`accept`/`import`/`trust`/`history`/`completion`/`mcp`/`mine` in one module. `clean-code-design`'s separation-of-concerns rule and `clean-code-solid`'s "a module is also a unit of responsibility" both point at a `commands/` directory: `commands/{run,vault,grant,folder,draft,mine,frame}.rs` with the shared helpers (`resolve_paths`, `vault_store`, `auto_import_go_vault`) in `commands/mod.rs`. This is not urgent — it is the single highest-leverage structural change left, and it gets more expensive with every command added. Proof: no behaviour change, so `kadou`'s existing 69 unit tests plus the 80 in `tests/cli.rs` are the proof.

**L6 — `runner::finish` swallows both history writes. Effort: S.**
`crates/kadou-core/src/runner.rs:264` and `:285` are both `let _ = history_store.finish(...)`. `clean-code-errors` says "never swallow" and `state.rs:44-58` sets the precedent for how this crate handles the same class (log to `tracing` and continue). A history write that silently fails is exactly the audit gap §6.6 exists to prevent. Fix: `tracing::warn!` on `Err` at both sites. Proof: a test with a read-only records directory asserting the run still returns `Ok` and a warning is emitted.

**L7 — `state.json` / `themes_dir()` are dead code (D4, D14). Effort: S.**
`crates/kadou-mine/src/store.rs:44-46` and `crates/kadou-core/src/paths.rs:24-26` are `pub fn`s with no caller anywhere in the workspace. `clean-code-design`'s YAGNI rule ("Do not add config keys, feature flags, or extension points the PRD doesn't require") cuts both ways: either wire them or delete them. They currently read as implemented features to anyone skimming.

**L8 — Append-only logs are rewritten whole on every append. Effort: S.**
`crates/kadou-mine/src/store.rs:167-177` (`append_audit`), `crates/kadou-mine/src/ingest.rs:84-93` (`append_processed`), and `crates/kadou-mine/src/orchestrate.rs:261-276` (`record_redaction_failure`) all `read_to_string` the entire file, push one line, and `write_atomic_0600` the whole thing back. That is O(n²) in rows and contradicts §3.3/§4.6's "append-only". For `processed.jsonl` this is the hot path: one full read+write per index row, and Mason's real ledger has thousands. Fix: `OpenOptions::new().create(true).append(true)` with an explicit `0600` mode on create — the `0600` guarantee is why `write_atomic_0600` was reached for, and `OpenOptions::mode(0o600)` gives it directly. Proof: the existing round-trip tests, plus one asserting the file mode after a create-by-append.

**L9 — `ingest_row` takes eight parameters behind an `#[allow]`. Effort: S.**
`crates/kadou-mine/src/orchestrate.rs:151-161`. `clean-code-functions` asks for "zero, one, or two arguments … three or more — reach for a struct". Six of the eight are already one logical unit (the in-flight run: `home`, `index_base`, `processed`, `started`, `bytes_scanned`, `out`). A `struct IngestRun<'a>` with `ingest_row(&mut self, config, row) -> bool` retires the `#[allow]` and reads better. `crates/kadou-mcp/src/pending.rs:206` and `crates/kadou-core/src/header.rs:792` carry the same `#[allow]` for the same reason (this is B7 from the last review, still open).

**L10 — `normalize_command`'s dispatch chain. Effort: S.**
`crates/kadou-mine/src/normalize.rs:267-338` is 72 lines whose body is one `for` loop with seven `continue`-terminated branches — the `06` §2.4 rule table inline. `clean-code-functions`' "extract a block that needs a `// this handles X` comment into its own function" applies: `fn classify_word(ctx, word, pending) -> WordOutcome` turns the loop into three lines and makes each §2.4 row separately testable. Same shape, smaller: `crates/kadou-core/src/header.rs:402` `parse_keys` (77 lines) and `crates/kadou-mine/src/propose.rs:216` `build_proposal` (74).

**L11 — function length against the skill's own number.** `clean-code-functions/SKILL.md:14` says "**under 30 lines** of logic"; the enforced ratchet (`clippy.toml:1`) is 60. Counting raw lines including doc comments, **90 non-test functions exceed 30 lines**; the longest is 77 (`header.rs:402`). By clippy's own count (comments excluded) every one is ≤ 60 except the documented `schema.rs:35` wire literal, so the repo is *compliant with its ratchet* and *not* with its skill. That is a deliberate, documented compromise from `11-code-review.md` §6 — I flag it only so the gap is on the record rather than rediscovered.

**L12 — `record_redaction_failure` always writes `rule_id: "R14_HIGH_ENTROPY"`. Effort: S.**
`crates/kadou-mine/src/orchestrate.rs:264`. `propose::build_proposal` can fail for reasons other than R14, and `06` §2.7 says the failure row records "`{fingerprint, rule_id, when}`" — the actual rule. `ProposeError` already distinguishes them; thread the variant through.

**L13 — Minor naming/idiom.** `crates/kadou-core/src/fsutil.rs` — `clean-code-naming`'s "a module names what it *provides*" argues for `atomic` or `secure_write`; F1/F2 from the last review, still open. `crates/kadou-mine/src/rank.rs:34` `(1.0 + x).ln()` should be `x.ln_1p()` (clippy nursery flags it, and it is the accuracy-correct form for the exact `log(1 + freq)` the spec writes). Five `usize as f64` casts in `rank.rs` are `cast_precision_loss` — harmless at these magnitudes, worth an `#[allow]` with a one-line reason rather than leaving them silently outside the crate's own `cast_*` deny policy.

**L14 — One note on the notification's inputs.** `crates/kadou-mcp/src/tools.rs:822-829` builds the toast body from `mcp_client`, which is **self-reported by the agent** (§6.4 calls it "a label, not an identity"). It reaches an AppleScript string literal via `notify.rs:54-61`, which escapes `"` and `\`. That is sufficient — a crafted name breaks `osascript`'s parse and the failure is discarded, it does not escape the literal. No change needed; recording it so the next reviewer does not have to re-derive it.

---

## 5. The mutation and test picture

**Test suite:** `cargo test --workspace` → **553 tests, 0 failures**, 11 non-empty targets. That is up from 225 at `89a30c4`. The `kadou` bin's integration tests moved to `crates/kadou/tests/cli.rs` (R15/G3), which is why `kadou` can be mutation-tested at all now.

**Mutation run:** `cargo mutants --workspace -j 4`, cargo-mutants 27.1.0, the committed `.cargo/mutants.toml`, on an 18-core Apple Silicon Mac.

```
1592 mutants tested in 56m: 207 missed, 1185 caught, 196 unviable, 4 timeouts
```

(Some of that 56 minutes overlapped with the `cargo test`/`clippy` runs in §0; an uncontended run would be faster. It is still not an 18-minute job — see the CI budget note below.)

### Per-crate ratios against the committed CI floors

Ratio = `caught / (caught + missed + timeout)`, the same formula `.github/workflows/ci.yml:98-117` computes.

| Crate | Caught | Missed | Timeout | Viable | Unviable | Ratio | CI floor | |
|---|---|---|---|---|---|---|---|---|
| `kadou-core` | 422 | 58 | 2 | 482 | 82 | **87.6 %** | 85 % | ✅ |
| `kadou-exec` | 27 | 6 | 0 | 33 | 23 | **81.8 %** | 80 % | ✅ |
| `kadou-mcp` | 194 | 34 | 1 | 229 | 31 | **84.7 %** | 80 % | ✅ |
| `kadou` | 264 | 37 | 0 | 301 | 27 | **87.7 %** | **90 %** | ❌ **below floor** |
| `kadou-mine` | 278 | 72 | 1 | 351 | 33 | **79.2 %** | 75 % | ✅ |
| **Total** | **1185** | **207** | **4** | **1396** | **196** | **84.9 %** | — | |

### ❗ The committed ratchet is red, and the reason is exact

`kadou` is at **87.7 %** against its own **90 %** floor. The job would fail on its first run.

The floor was set in `9c076c0` from `docs/design/10-mutation-baseline.md:355-374`, a **`cargo mutants -p kadou`** measurement: "306 mutants tested: 94.3% caught". That measurement was taken on `sd/dops/impl-08` — *before* `49e406e` ("wire `kadou mine` on the CLI") added ~230 lines of `kadou mine` command wiring to `commands.rs`. The floor commit landed immediately after the code it does not cover.

The arithmetic closes exactly. Of `kadou`'s 37 missed mutants, **21 are in slice 9's mine CLI** — 20 in `crates/kadou/src/commands.rs:2336-2620` and one on `crates/kadou/src/main.rs:326` `dispatch_mine`. Remove them and `kadou` is `264 / 280 = **94.3 %**` — the baseline's number, to the decimal. The mine CLI layer landed with essentially no CLI-level coverage: whole-function mutants on `run_mine_run`, `run_mine_status`, `run_mine_list`, `run_mine_show`, `run_mine_review`, `run_mine_approve`, `run_mine_reject`, `run_mine_install_schedule`, `print_queue_row`, `load_redact_extra` and `default_sessions_index_path` are all uncaught, meaning **replacing each of those functions with a no-op does not fail a single test**. The `kadou-mine` *library* is well covered (106 unit tests + the pipeline integration test); its CLI face is not.

Two ways out, and the choice is Mason's: add `crates/kadou/tests/cli.rs` cases for the eight `kadou mine` subcommands (the same `assert_cmd` shape the other 80 cases use — this is the right fix and closes 21 of 37), or lower the `kadou` floor to 85 % with a dated note saying why. **Do not leave it as-is**: a ratchet whose first real run is red teaches everyone to ignore it.

### Highest-value remaining misses

Ranked by what a real bug at that site would cost, not by count.

**1. `crates/kadou-core/src/redact.rs` — the `MIN_SECRET_LEN` boundary and a tautological test.** Four misses on invariant 5's own implementation:
- `:24` `replace < with <=` — nothing pins that an exactly-8-character secret **is** redacted. The boundary between "redacted" and "silently passed through" is untested.
- `:50` `replace < with <=` / `with ==` — the same boundary on the basic-auth pairing.
- `:70` and `:73` — `percent_encode` can be replaced wholesale with `"xyzzy"` and no test fails. The reason is visible at `crates/kadou-core/src/redact.rs:99-106`: `redacts_base64_and_url_encoded_forms` builds its expected URL-encoded value **with the function under test** (`let url = percent_encode(secret);`), so the assertion is a tautology for that encoding. `clean-code-testing`'s "the test must be able to fail" applies directly.
*Fix:* hard-code the expected `hunter2ok%21` string instead of computing it, and add an 8-char and a 7-char secret to the table. Effort S, and it is the highest-value test in this list because §6.6's redaction is the invariant most of the product's safety story rests on.

**2. `crates/kadou-mine/src/risk.rs` — 10 misses on `06` §4.4's risk table.** The valuable ones are the `true`-direction guard mutants at `:34` (`terraform` + `apply`), `:35` (`docker` + `push`), `:36` (`oci` + `delete`) and especially `:38` (`git` + `log|status|diff`). Flipping `:38` to `true` makes **every** `git` template Medium — including `git push --force` — where today the catch-all correctly gives High. The existing table (`crates/kadou-mine/src/risk/tests.rs`) has only positive rows; every mutant that widens a guard lands on a template no test covers.
*Fix:* add the negative rows — `git push origin main` → High, `terraform destroy` → High, `docker build .` → High, `oci os object list` → High. Effort S. (The `||`→`&&` mutants at `:17` and `:26` are genuinely low-value: both the correct code and the mutant fall through to the same safe `High`. Say so in the baseline rather than writing a contrived test.)

**3. `crates/kadou-mcp/src/tools.rs:455-456` — JSON number and bool args are never sent over the wire.** Deleting the `Value::Number` and `Value::Bool` arms of `json_args_to_strings` fails no test, which means §6.1's arg-serialization table (`int` → decimal text, `bool` → `"true"`/`"false"`, `select` → the chosen option) is exercised only through string inputs. The schema at `schema.rs:78` explicitly permits `additionalProperties: true` on `args`, and 86 of the Sesami folder's args are booleans — so this is the shape a real agent sends constantly.

**I drove this path myself rather than infer it, and the behaviour is correct.** Over the live server, `{"count": 3, "flag": true, "mode": "uat", "name": "hi"}` produced `count=3 flag=true mode=uat name=hi` in the child, and the three rejection paths are right too: `{"mode":"nope"}` → `` invalid value for arg `mode`: `nope` is not one of the declared options: dev, uat, prod ``; `{"name":{"a":1}}` → `` arg `name` must be a string, number, or boolean ``; `{"count":3.5}` → `` invalid value for arg `count`: `3.5` is not an integer ``. So this is a **missing regression test, not a defect** — but it is the single most agent-exercised path in the product with no test behind it.
*Fix:* one `mcp_server.rs` case running a kata with all four arg types over the wire and asserting what the child saw, plus the three rejection rows. Effort S.

**4. Cap and TTL boundaries, all off-by-one shaped.** `crates/kadou-mcp/src/drafts.rs:70` (the 128-char `propose_kata` id cap) and `:196` (the 65 536-byte source cap); `crates/kadou-mcp/src/pending.rs:22` (the 24 h TTL is `24*60*60`, and replacing `*` with `+` is uncaught, so nothing pins the actual duration) and `:77` (`is_expired`'s `>`); `crates/kadou-mcp/src/tools.rs:925-933` (the 8 192-byte output cap). Each is a one-line "exactly at the limit" / "one over" pair. Effort S each; the `propose_kata` caps are the ones an agent can actually push against.

**5. `crates/kadou-mine/src/rank.rs` — 18 misses, the whole score formula.** `age_days` can return a constant, every `*` in `score` can become `/`, and `06` §2.6's formula still passes. The tests assert `passes_cutoff` (well covered), that a catalog conflict lowers the score (`rank/tests.rs:128`), and that the score is finite and non-negative (`:140-141`) — but never a number. *Fix:* one golden case with fixed `now`, fixed cluster stats, and the expected score to 6 decimals. Effort S, and it is cheap because `score` is already pure and takes `now` as a parameter.

**6. `crates/kadou-mine/src/orchestrate.rs` — 21 misses.** 8 are arithmetic on the bound constants (`MAX_WALL_TIME`, `MAX_BYTES_SCANNED`, `LOCK_STALE_AFTER` at `:25-29`) — no test pins the bounds, which is the same gap **B2/D2** already calls out; making the bounds injectable for tests closes both at once. The rest are the lock lifecycle (`lock_is_held`, `acquire_lock`, `release_lock` can all be replaced with no-ops, so `06` §3.4 rule 5's "second run exits 0 with `already running`" is unpinned) and `record_redaction_failure` (replaceable with `()` — the fail-closed audit row is never asserted). *Fix:* a concurrent-run test asserting `already_running`, and a fixture whose draft trips R14 asserting a `redaction-failures.jsonl` row appears. Effort S–M.

**7. `crates/kadou-mcp/src/notify.rs` — 8 misses.** The notification path is deliberately test-gated (`KADOU_NOTIFY_TEST_NOOP`, `notify.rs:17`), so the platform `send_with` bodies can never run under `cargo test`. This is the same "a mutant here can never be caught" category `.cargo/mutants.toml` already carves out for `fn main` and the TTY-only picker code. *Recommendation:* add `^crates/kadou-mcp/src/notify\.rs:.*send_with` to the exclude list rather than writing a test that cannot exist. That alone lifts `kadou-mcp` from 84.7 % to ~87 %.

**8. `crates/kadou-core/src/vault.rs` — 17 misses, 14 of them keyring.** Already diagnosed in `docs/design/10-mutation-baseline.md:92-99`: `#[cfg(feature = "keyring")]` bodies that the default-feature test run never compiles into anything executable. Not a real coverage gap; measuring it needs `cargo mutants -p kadou-core --features keyring`, which no one has run. Leave it, or add the feature to the CI job's own invocation.

### Timeouts (4)

`crates/kadou-core/src/header.rs:129` (×2, `+=` → `*=`/`-=` in `find_closer` — an infinite scan), `crates/kadou-mcp/src/tools.rs:595` (`delete !` in `gate`), `crates/kadou-mine/src/propose.rs:165` (`+` → `-` in `replace_placeholder`). All four are genuine "this mutant hangs the suite" cases, which the ratio formula counts as *not caught* — correctly, since a hang is not a detection. `minimum_test_timeout = 60` with a 5× multiplier (`.cargo/mutants.toml`) is doing its job.

### Test-quality notes

- **TDD discipline held through slices 7–9.** Every behaviour in `89a30c4..9c076c0` lands as a `test(...)` commit followed by a `feat(...)`/`fix(...)` commit — 12 such pairs in `kadou-mine` alone (`a7f1b68`→`1bff780`, `aebcd0a`→`5385d6a`, …). That is `CLAUDE.md`'s "two commits minimum per behavior, not one that includes both", honoured without exception in the range I reviewed.
- **The bad-header fixture corpus is doing its job.** `crates/kadou-core/tests/header_fixtures.rs:27-55` picks up new files in the repo-root `tests/fixtures/headers/{good,bad}/` with no new test code, which is why H1/H2/I-11 closed as three fixtures rather than three test functions (`1b5aecf`).
- **Two wall-clock dependencies remain** (L3, L4) — the only violations of `clean-code-testing`'s "no sleeping" rule in the tree, and the likely cause of the one intermittent failure in §0.
- **`kadou-exec`'s de-flake held.** The R8/G1 readiness-file fix (`79d4183`) survived: 8 parallel `cargo test -p kadou-exec` runs during this review, zero failures, where the previous review measured 5 of 8 failing.

### Risk: the `mutants` CI job's budget

`.github/workflows/ci.yml:93` runs `cargo mutants --workspace -j 3` under `timeout-minutes: 60` (`:88`). `docs/design/10-mutation-baseline.md:253` records the last **workspace** run as "925 mutants in ~18 minutes" at `-j 4` — but that predates `kadou`'s un-exclusion and `kadou-mine` entirely, and the two most recent per-crate numbers (`kadou` 306, `kadou-mine` 384) were taken with `-p`, not `--workspace`.

**Measured here: 1 592 mutants, 56 minutes at `-j 4`** on an 18-core Apple Silicon Mac (some of it contended with this review's own `cargo test`/`clippy` runs). CI runs at `-j 3`, on a smaller shared runner, against a 60-minute wall. That is not headroom; that is a coin flip. The workflow's own comment anticipates it ("if the first real run … blows past ~40 minutes, shard with `cargo mutants --shard i/N`"). Take that advice **before** the first real run — see §6 Later-4.

Also worth noting: `docs/design/10-mutation-baseline.md` now has four dated sections whose numbers were taken under three different invocations (`--workspace`, `-p kadou`, `-p kadou-mine`). That is how the `kadou` floor ended up set from a measurement CI does not reproduce. Whatever the next measurement is, take it with **the command CI actually runs**, and say so in the section header.

---

## 6. What must be fixed before Mason points a real agent at kadou

Ranked by (safety or first-use impact) × (cost of fixing it later). "Blocking" means: do this before the install, because an agent or a first-week workflow hits it.

### Blocking

**B1 — Secret bit on an unresolved need (D1). Effort: S.**
*Files:* `crates/kadou-core/src/resolve.rs:155-160`.
*Change:* the final fall-through (`value: None`) returns `secret: true`, not `secret: false`. A need with no default and no vault entry is, by §4.4's own logic, the secret-capable kind.
*Why blocking:* it is the single behaviour a first-run agent sees and gets wrong. Before the vault is populated, `describe_kata`/`dry_run` on all 32 Sesami kata report `secret_env_names: []`, which tells the agent `JENKINS_TOKEN` is an ordinary value. Reproduced in §1 slice 3. It also makes §9 slice 3's own Done line literally true, which it currently is not.
*Proving test:* in `crates/kadou-core/src/resolve.rs`'s existing need-resolution table, add the `(no default, no vault entry)` row asserting `value == None, secret == true`; plus a CLI case in `crates/kadou/tests/cli.rs` asserting `kadou run sesami/cc4-aaa --dry-run` on an **empty** vault lists `JENKINS_TOKEN` under `secret_env_names`.

**B2 — The miner slurps whole transcripts (D2, D7). Effort: M.**
*Files:* `crates/kadou-mine/src/orchestrate.rs:162-185` (the `std::fs::read` at `:173` and the bounds check at `:162`), `crates/kadou-mine/src/ingest.rs:29-37`, and the false claim in the module doc at `orchestrate.rs:22-24`.
*Change:* stream the transcript with `BufReader::lines()` — the per-agent parsers in `crates/kadou-mine/src/parse.rs` are already line-oriented (`:103` iterates `content.lines()`) — and charge bytes to `bytes_scanned` **as they are read**, so the 2 GB bound can stop mid-file. At minimum, `stat` the file and skip anything over a per-file cap.
*Why blocking:* `06` §2.1 measured Mason's own Codex store at ~2.8 GB. The first `kadou mine run --once` against the real ledger reads a multi-gigabyte file into a `Vec<u8>` and then copies it again through `from_utf8_lossy`. Everything in the fixture-sized tests passes; the real corpus is where it fails, and it fails on the machine Mason is installing on.
*Proving test:* a fixture whose transcript exceeds a test-lowered byte bound, asserting `bounded_stop == true`, that the run still exits cleanly, and that the next run resumes. Lower `MAX_BYTES_SCANNED` behind a `MineConfig` field so the test does not need a real 2 GB file.

**B3 — `history/` and `history/logs/` are `0755` (P1, L1). Effort: S.**
*Files:* `crates/kadou-core/src/fsutil.rs:36-44`.
*Change:* set `0700` on every directory component `ensure_dir_0700` creates under the caller's root, not only the leaf.
*Why blocking:* §6.6 states it as an invariant, a test claims to prove it, and it is false on disk (reproduced in §2 P1). The content is still protected by the leaf mode, so this is defence-in-depth — but the whole point of installing this on a laptop that also runs agents is that the state directory is not readable by anything else, and a stated-and-tested invariant that does not hold is worse than an unstated one.
*Proving test:* extend `pending_and_history_directories_are_0700` (`crates/kadou-mcp/tests/mcp_server.rs:282`) to assert `history/` and `history/logs/`.

**B4 — `mine approve --into` path traversal (D16). Effort: S.**
*Files:* `crates/kadou-mine/src/store.rs:214-229`, `crates/kadou/src/commands.rs:2519-2538`; lift `valid_id_segment` (`crates/kadou-mcp/src/drafts.rs:77-84`) into `kadou-core` so both crates share one copy.
*Change:* reject a `name` that is not a single `^[a-z0-9][a-z0-9-]*$` segment, before the `join`.
*Why blocking:* it is the one path in the product where a human's typo or a pasted command writes an executable script directly into `~/.config/kadou/kata/`, skipping `kadou accept`'s diff-and-confirm — the review gate that `06` §2.9 and §6.7 exist to enforce. It is cheap, and Mason will be running `mine approve` in his first week.
*Proving test:* `approve_refuses_a_name_that_escapes_the_mined_directory`, asserting the error **and** that no file exists outside `mined/`.

**B5 — De-flake the two `max_wait` tests (L4). Effort: S.**
*Files:* `crates/kadou-mcp/tests/mcp_server.rs:476-500`, `:502-540`.
*Change:* readiness-file synchronisation and ordering assertions instead of millisecond budgets — the same fix `79d4183` applied to `kadou-exec`.
*Why blocking:* `cargo test --workspace` is the first thing anyone runs, and it failed once in four attempts under load during this review (§0). CI has never run; the first time it does, on a shared GitHub runner, this is what goes red, and an intermittently red first CI run is how a team learns to re-run the build instead of reading it.
*Proving test:* the same two tests, 8× in parallel under load — the method that demonstrated and then closed G1.

**B6 — The committed mutation ratchet is red (§5). Effort: S (lower the floor) or M (write the tests).**
*Files:* `.github/workflows/ci.yml:123` (`check kadou 90`), `docs/design/10-mutation-baseline.md:355-374`, and — for the real fix — `crates/kadou/tests/cli.rs`.
*Change:* either add `assert_cmd` cases for the eight `kadou mine` subcommands (closes 21 of `kadou`'s 37 misses and restores the crate to ~94 %), or lower the `kadou` floor to 85 % with a dated note recording why.
*Why blocking:* this is not about the install — it is about the first time CI ever runs for this repo. `kadou` measures **87.7 %** against its own **90 %** floor, and the reason is precise: the floor was set from a pre-slice-9 `-p kadou` measurement one commit before the mine CLI landed untested. Combined with **Later-4**'s timeout risk, the very first `mutants` job this repo runs goes red for a reason that has nothing to do with the change that triggered it. Fix it now, while the cause is known, rather than in a hurry during someone else's PR.
*Proving test:* `cargo mutants --workspace -j 3` (the CI invocation, not `-p`) reporting `kadou` at or above whatever floor you land on.

### Later

**Later-1 — `git clone` argument injection (D15). Effort: S.** `crates/kadou-core/src/git.rs:77-98`: push `"--"` before the URL and reject a URL starting with `-`. Proof: `clone_refuses_a_url_that_looks_like_a_flag`. Not blocking because the input is human-typed at the same trust level as running `git clone` directly, but it is a one-line close on the exact string a team README tells people to paste.

**Later-2 — Cap `kadou-exec`'s output buffer (E6). Effort: S.** `crates/kadou-exec/src/lib.rs:254-267`: keep the last N lines in a ring rather than an unbounded `Vec`, with N above the MCP view cap so nothing observable changes. Proof: a kata printing 10⁶ lines, asserting bounded memory and an unchanged 50-line result.

**Later-3 — Stop rewriting the log in `finish` (D18). Effort: S.** `crates/kadou-core/src/history.rs:150-155`: delete the write; `kadou-exec` already owns the log. Proof: the existing `runner.rs:494` content assertion, plus one that the trailing newline the stream wrote survives.

**Later-4 — Shard the `mutants` CI job before its first real run (§5). Effort: S.** `.github/workflows/ci.yml:85-124`: a `shard: [0,1,2,3]` matrix with `cargo mutants --workspace --shard ${{matrix.shard}}/4` and the floor check over the merged outputs — or raise `timeout-minutes` with eyes open. **Measured: 1 592 mutants, 56 minutes at `-j 4`** against a 60-minute wall at `-j 3`. Proof: the job going green on its first run. Pair this with **B6** — they are the same first-run failure.

**Later-5 — The remaining mining-contract gaps (D3, D4, D5, D6, D8, D10, D11). Effort: M–L.** Ranked within themselves: **D8 `--since`** first (a one-line filter that removes a warning printed on every backfill), then **D6 catalog penalty** (the mechanism that stops the miner proposing what Sesami already has), then **D10 extraction kinds** (the largest gap against `06`, and the one that decides whether Claude sessions yield anything at all), then D5/D11 (adjust spec or code), then D3/D4 (implement `state.json` or delete `transcript_sha256` and `state_path()`).

**Later-6 — Split `commands.rs` (L5). Effort: M.** 2 962 lines, ten command families, every function individually small. Gets more expensive with each command added.

**Later-7 — R14's base64 blind spot (L2). Effort: S.** `crates/kadou-mine/src/redact.rs:373-377`.

**Later-8 — The small clean-code items: L3, L6, L7, L8, L9, L10, L12, L13.** All S, all mechanical. L8 (`append_processed`'s O(n²) rewrite) is the one with a real performance consequence on Mason's actual ledger and should lead this group.

**Later-9 — Doc-side corrections.** `CLAUDE.md`'s three stale pointers (§2 above); the PRD's §6.8 `mined/` path (D12); the theme key and `themes/` directory (D14); the mining `SKILL.md` (D13); §9 slice 3's Done line if B1 is *not* taken.

**Later-10 — The regression tests §5 names. Effort: S each, M in total.** In value order: the `MIN_SECRET_LEN` boundary and the de-tautologised `percent_encode` assertion (`crates/kadou-core/src/redact.rs:99-106`); the four typed-arg wire cases (`crates/kadou-mcp/tests/mcp_server.rs`); the negative rows in the mined-risk table (`crates/kadou-mine/src/risk/tests.rs`); the `propose_kata` id/source cap boundaries and the pending TTL/`is_expired` boundary; one golden `rank::score` value; the `already_running` lock case and a `redaction-failures.jsonl` case. Separately, add `^crates/kadou-mcp/src/notify\.rs:.*send_with` to `.cargo/mutants.toml`'s exclude list — those eight mutants can never be caught by any test, the same category `fn main` and the TTY-only picker code already carve out.

---

## 7. Install-and-first-agent dry run — a checklist for Mason

Every step below was executed in this review under an isolated `HOME`/`KADOU_HOME`, except the one marked ⏸ (it touches your real agent-client config, which this review did not). Expected output is quoted from the actual run. No Jenkins pipeline was triggered at any point and none is triggered here.

### 0. Build and stage a release (no GitHub remote yet)

```sh
cd ~/origin/kadou
cargo build --release --workspace
mkdir -p /tmp/kadou-rel/latest /tmp/kadou-stage
cp target/release/kadou /tmp/kadou-stage/kadou
tar -czf "/tmp/kadou-rel/latest/kadou-latest-$(uname -s | tr 'A-Z' 'a-z')-$(uname -m).tar.gz" -C /tmp/kadou-stage kadou
( cd /tmp/kadou-rel/latest && shasum -a 256 *.tar.gz > SHA256SUMS )
```

> **Note on the asset name.** `install.sh:63` builds it from `uname -m`, which on Apple Silicon is `arm64`. Whoever publishes the first real release must name the asset `…-darwin-arm64.tar.gz`, not the `aarch64` most release tooling defaults to. (Publishing to a stable URL is gated — repo README Gates.)

### 1. Install via `install.sh` from that local fixture

```sh
KADOU_INSTALL_BASE_URL="file:///tmp/kadou-rel" \
KADOU_INSTALL_DIR="$HOME/.local/bin" \
  sh ~/origin/kadou/install.sh --dry-run
```

**You should see** (verified):

```
kadou latest → /Users/mason/.local/bin/kadou   sha256 ok (dry run)
```

Then drop `--dry-run`:

```
kadou latest → /Users/mason/.local/bin/kadou   sha256 ok
  next:  kadou            open the library
         kadou mcp serve  for agents
```

**Worth doing once:** corrupt one line of `/tmp/kadou-rel/latest/SHA256SUMS` and re-run. You should get `install.sh: checksum mismatch for …`, the expected and actual digests, and **exit 1 before anything is extracted** (verified). That is the control that matters in the one-liner.

### 2. First run

```sh
kadou
```

**You should see** a frame headed `kadou 稼働   1 folder · 5 kata`, the `starter` folder with `hello`, `disk-usage`, `git-status`, `health`, `list-path` — all `● low` — and the four next-step lines (`run` / `new` / `team` / `agents`). Cold-home first paint measured at **1.27 s** on a debug build; §1.3's target is ≤ 10 s.

Two things to know:
- `kata/starter/` is materialised only if it does not exist. Delete the folder and it **stays** deleted (`8d73d73`, I-2/R9); `kadou get starter` is the restore path.
- Piped (`kadou | cat`) you get one tab-separated kata per line, not the frame. That is §7.2's own rule, not a bug.

### 3. Convert the Sesami folder — no Jenkins

```sh
kadou import ~/Bitbucket/sdo-dops-catalog/src --as sesami
kadou check sesami
```

**You should see** (verified, verbatim):

```
imported 32 kata into sesami (86 booleans coerced, 1 integers coerced)
…
checked 32 kata in sesami   0 errors  58 warnings
```

The 58 warnings are all `need <name> has no vault entry and no default` with the fix line `kadou vault set <name>`. They disappear after step 4. Spot-check the `REPO_ROOT` rewrite — `grep TRIGGER ~/.config/kadou/kata/sesami/cc4-aaa.sh` should show `TRIGGER="${KADOU_ROOT}/scripts/trigger-pipeline.sh"` (§4.6).

The brief says `kadou get` of the converted folder; there is no converted-folder remote yet (pushing is gated), so `kadou import` is the path today. When there is one, `kadou get <url> --as sesami` round-trips — verified against a local bare fixture in §1 slice 7, including `kadou update`.

### 4. The three Jenkins needs

```sh
kadou vault set jenkins_url  --plain
kadou vault set jenkins_user --plain
kadou vault set jenkins_token
kadou vault list
```

Each prompts on the TTY and echoes `saved <name>` — the value never goes on argv (§6.5). **You should see**:

```
jenkins_token                secret
jenkins_url                  plain
jenkins_user                 plain
```

Check the modes once: `~/.local/share/kadou` and `~/.local/share/kadou/keys` are `drwx------`, `vault.json` and `keys/identity.txt` are `-rw-------` (verified).

Now confirm resolution end to end, still with no network:

```sh
kadou run sesami/cc4-aaa --dry-run
```

**You should see** `env_names:` listing all nine variables, `env_public: JENKINS_URL=<what you just saved>` — **the vault's value, not the header's `https://ci.sesami.io`** default (that is the I-1 precedence fix, verified) — and `secret_env_names: JENKINS_TOKEN`.

> ⚠ If you run that `--dry-run` **before** step 4, `secret_env_names` reads `(none)`. That is finding **B1**, not a mistake on your part.

### 5. Wire `kadou mcp serve` into Claude Code

⏸ *Not executed against your real machine.* Add to your MCP client config (`claude_json`/`.mcp.json` — §5.6 has the snippet):

```json
{ "mcpServers": { "kadou": { "command": "kadou", "args": ["mcp", "serve"] } } }
```

Nothing else — no `--max-risk`, no `--allow-risk`. The default agent ceiling is `low` and those flags can only narrow it (§6.2, verified in §2 invariant 10).

### 6. What the agent should see, and what it must not

Ask the agent to list your kata. **It should report exactly five Sesami kata** — `clone-ses-repos`, `device-log-metrics`, `helm-package`, `sdo-k8s-ses`, `ses-automation` — plus the five starter kata. Verified over a live stdio client at the default config.

It should **not** see, and you should sanity-check that it does not:

| Ask the agent to… | Expected |
|---|---|
| describe or run any `cc4-*` kata (medium) | `no such kata` — hidden, not refused (§5.5 deliberately does not distinguish) |
| run `sesami/ses-deploy` (critical) | `no such kata` |
| pass `jenkins_token` in `args` | `invalid_args: jenkins_token is a need, supplied by the vault; needs can never be passed as args` |
| propose a kata | a draft under `~/.local/state/kadou/proposed/`, a unified diff, and a `kadou accept …` line — **never** a file under `~/.config/kadou/kata/` |
| pass a JSON number, bool and select value as args | they serialize per §6.1 (`3` → `COUNT=3`, `true` → `FLAG=true`); a float for an `int` arg, an object for any arg, and an off-list `select` value are each `invalid_args` with the reason named. All four verified over the wire in this review — and all four are **untested in the suite** (§5 miss 3) |

Then run one that *is* visible. **Start with `dry_run: true`** — `run_kata {"id": "sesami/helm-package", "dry_run": true}` resolves names only, never spawns, and never resolves a secret value, so no Jenkins call happens (§6.3, verified). When you want a real one, `sesami/helm-package` is the mildest of the five: `low`, and it triggers a chart-packaging pipeline rather than a deploy.

A real run gives you back `output` (last 50 lines, compact), `history_id` and `log_path`. `cat` that log and confirm your token appears as `****`. Verified here with a synthetic kata that echoes `$JENKINS_TOKEN`: the MCP result, the on-disk log, and a `grep -rl` for the raw secret across the **entire** home were all clean. Also worth one look: `kadou history --limit 5` should show that run with `(mcp)` next to it — that is §6.6's audit trail, and it is written whether or not you were watching.

### 7. Exercise the grant path once, so you have seen it

Raise the ceiling deliberately, in `~/.config/kadou/kadou.toml`:

```toml
max_risk = "critical"
[agent]
max_risk = "critical"
```

Ask the agent to run `sesami/ses-deploy`. **You should get back** `status: pending_grant`, a `pending_id`, a `pending_path`, `expires` 24 h out, and `approve: kadou grant approve <8-char prefix>` — plus one desktop notification. Then:

```sh
kadou grant list
kadou grant show <prefix>      # record + a diff of the kata since the request
kadou grant deny <prefix>      # deny it; you have seen the loop
```

`kadou grant approve` without `--confirm` on a critical kata refuses and prints the exact line to copy (verified). **Put the config back to the defaults afterwards** — `max_risk = "medium"`, `[agent] max_risk = "low"` — or just delete those lines; missing keys mean defaults.

### 8. Mining — optional, and read §6 B2 first

`kadou mine run --once` against the synthetic fixture works end to end (verified: queue, redact, approve, `kadou accept`). Against your **real** `~/Documents/Sessions`, finding **B2** applies: the first multi-gigabyte Codex transcript is read whole into memory. Either take B2 first, or run it once with `--index` pointed at a small copy of the ledger and watch the process's RSS. Do not `kadou mine install-schedule --load` until B2 is closed — a LaunchAgent that OOMs at 03:15 every night is a bad first impression of the feature.

---

## 8. Limits of this review

- CI has never run. Everything in §1's Cross-cutting table is "green on this macOS machine", not "green on the matrix". The Linux half of the `/bin/sh` dash-vs-bash concern (§9 Cross-cutting) is untested by anyone so far.
- No Jenkins pipeline was triggered, so the Sesami kata are verified through `--dry-run`, `kadou check`, and env-name comparison only — §4.6's "same env names as today" half of decision 11, not the "it actually triggers the build" half.
- The one intermittent `mcp_server.rs` failure (§0) could not be reproduced after the fact and its two test names are not recorded. L4/B5 names the two tests that race a clock; that is an inference from reading, not a captured failure.
- The keyring feature (`[vault] keyring = true`) is compiled but not exercised here; `docs/design/10-mutation-baseline.md:92-99` already records that its `#[cfg(feature)]` bodies distort `kadou-core`'s mutation numbers.
- Mason's real `~/.dops`, `~/.config/kadou`, and `~/Documents/Sessions` were never read. Every path in this document under `~/` is either the isolated temp home or a step for Mason to run himself.
- This review read selectively outside §4's scope (see **Method**). `header.rs` (1 157 lines), `import.rs` (818) and `vault.rs` (1 001) were last read in full by `11-code-review.md` at `89a30c4`; the commits touching them since are small and cited individually above, but no fresh line-by-line pass was made over them.
- The picker's key handling and the interactive arg prompts run only over a real terminal and are excluded from mutation testing by design (`.cargo/mutants.toml`). I rendered the bare frame over a pty but did not drive the picker interactively; slice 8's own handoff records manual `expect` transcripts for that.
