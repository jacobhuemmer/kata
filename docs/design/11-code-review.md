# kadou code review (slices 1–6)

**Date:** 2026-09-11
**Reviewer:** independent worker `dops-code-review-c` (Claude Opus 5), reporting to `dops-planner`
**Code reviewed:** `89a30c4` — "feat(kadou): unified confirm protocol, grant subcommands, and starter fix"
**`main` at time of writing:** `5186a3b`. The two commits `89a30c4..main` (`6a56db5`, `5186a3b`) are **docs-only** — they add `CLAUDE.md`, `docs/CONTRIBUTING.md` and twelve `.claude/skills/*/SKILL.md` files, and touch no file under `crates/`. The code tree reviewed here is byte-identical to `main`'s.
**Scope:** crates `kadou`, `kadou-core`, `kadou-exec`, `kadou-mcp` (`kadou-mine` is an empty slice-9 stub).
**Contract:** `docs/design/05-prd.md`; charter `docs/design/03-principles.md`; decisions D1–D7 in `docs/design/09-tui-decision.md`.
**Standard applied:** `CLAUDE.md` and the `.claude/skills/clean-code-*` files that landed on `main` in `6a56db5` — Mason's dops practices (TDD, clean code, spec-driven, mutation testing) as this repo now codifies them for Rust. Where those files state a number (functions **under 30 lines**, **≤2 arguments**, tests that never sleep or read the real `$HOME`), this review uses theirs, not a generic one.

**Method.** Every source file in the four crates was read in full. Claims are backed by one of: a cited file and line; a reproduced command; or a clippy/cargo run recorded below. No code was modified.

**Evidence collected this pass**

| Check | Result |
|---|---|
| `cargo build -p kadou --all-targets` | clean |
| `cargo fmt --all -- --check` | clean |
| `cargo test --workspace` | **225 passed, 0 failed** (52 bin + 85 core + 2 header-fixture + 4 sesami-shaped + 20 exec + 41 mcp + 21 mcp-server) |
| `cargo clippy --workspace --all-targets` (CI settings) | clean (`-D warnings` passes) |
| `cargo clippy … -W clippy::pedantic -W clippy::nursery` | **459 warnings**. A crate's lib and its lib-test target compile separately, so most sites are counted twice — ≈230 unique. Breakdown in §6 |
| 8× parallel `cargo test -p kadou-exec` | **5 of 8 runs FAILED** — see finding G1 |

`CLAUDE.md`'s forced-verification list also names `cargo +1.88 check`, `cargo deny check` and `cargo mutants`. The first two are CI jobs (`.github/workflows/ci.yml:54-71`) and were not re-run here; `cargo mutants` has no config in the tree yet (`mutation-testing/SKILL.md` says so explicitly) and is being baselined on `sd/dops/mutants`.

---

## 1. Verdict per crate

| Crate | LOC (src) | Verdict | One-line reason |
|---|---|---|---|
| `kadou-core` | 4 785 | **Good, with two correctness defects** | Clean module split, the strongest test suite in the tree (85 unit + 6 integration, real fixture corpora). But needs-resolution inverts the PRD's precedence (I-1), and the loader silently accepts three classes of invalid kata the PRD says are check errors (I-11, I-12, H below). |
| `kadou-exec` | 737 | **Good** | Small, focused, correct process-group semantics; the exec contract is well tested. One reproduced flaky test (G1), one unbounded buffer (E6), and no hook for the stream-redaction the PRD requires (I-7). |
| `kadou-mcp` | 2 855 | **Good safety posture, weak internal structure** | Every §9 slice-5/6 safety invariant is implemented and tested end-to-end over a real rmcp client — this is the best-verified crate in the tree. Undermined by a 188-line `run_kata`, a propose path that writes before it validates containment (I-13), and a `status: running` log that is empty until the run ends (I-7). |
| `kadou` (bin) | 3 034 | **Weakest crate — needs work before slice 7** | Three ~200-line command functions, the run pipeline triplicated, `grant allow` destroys the user's config comments (A1), the CLI run path neither redacts nor records history (I-8, I-9), and ~1 180 lines of integration tests live inside `main.rs`. |
| `kadou-mine` | 5 | **Not started, correctly** | Doc-comment stub scoped to slice 9 per §9. Nothing to review; the empty `[dependencies]` is right. |

**Overall.** This is disciplined, spec-literate work. Nearly every function carries a doc comment citing the PRD section it implements, deviations are flagged in prose rather than hidden, and the safety story that §9 calls "ships safe" genuinely does. The problems are concentrated in two places: the **bin crate's command layer**, which has grown past what one function per command can carry, and a set of **quiet spec drifts** (§4) that are individually small but collectively mean the PRD no longer describes the product.

---

## 2. Findings

### A. Error handling

**A1 — `kadou grant allow` rewrites the whole config, destroying comments and freezing defaults. (High)**
`crates/kadou/src/commands.rs:1033` calls `config.save(…)`, which is `toml::to_string_pretty` of the entire `Config` (`crates/kadou-core/src/config.rs:196`). Every default is then written explicitly and every human comment is lost. This contradicts PRD §3.1, which lists `toml_edit` specifically for "Comment-preserving `kadou trust` / `kadou grant allow` config edits" — and `toml_edit` is declared at `Cargo.toml:61` but used nowhere (E2). It also defeats the stated intent of `EMPTY_TEMPLATE` at `config.rs:9-12`: "keeps every key absent until a human sets it, so a later change to a default is not frozen out by an old file." One `grant allow` freezes all of them.

**A2 — `expect`/`unwrap` outside tests (8 sites).**
`crates/kadou-core/src/header.rs:389,390` (`about.expect("checked above")`), `crates/kadou-core/src/check.rs:67` (`.expect("just built")`), `crates/kadou-mcp/src/tools.rs:591` (`need.value.clone().expect("checked missing above")`), `crates/kadou/src/commands.rs:598` and `:1162` (same pattern), `crates/kadou-exec/src/lib.rs:269,270` (`.expect("piped stdout")`), `crates/kadou-mcp/src/concurrency.rs:25,41` (`lock().unwrap()` / `.expect("running_ids mutex poisoned")`). Each is locally justified, and the invariant is real in every case. The problem is the class, not the instance: a refactor that moves the `missing.is_empty()` guard at `tools.rs:535` below the env-build loop turns `:591` into a panic inside the MCP server, and `concurrency.rs:25` panics inside `Drop`, which aborts the process. All eight are mechanically removable (§5, R6).

**A3 — An unparseable `[exec] timeout` is silently ignored.**
`crates/kadou-exec/src/lib.rs:142-146`: `humantime::parse_duration(config_timeout_raw).unwrap_or(Duration::from_secs(30 * 60))`. `timeout = "30 minuts"` in `kadou.toml` produces no diagnostic and a 30-minute timeout. The same pattern is at `crates/kadou-mcp/src/tools.rs:631` for `mcp.max_wait`. Config typos in a safety-relevant bound should be loud. The right place is `Config::load` (validate the duration strings once, return `ConfigError`), which also lets `ExecConfig::timeout` become a `Duration` instead of a `String` (`config.rs:86`).

**A4 — `main.rs`'s catch-all arm is a panic trap.**
`crates/kadou/src/main.rs:265-288` re-matches on `Command` and writes `unreachable!("handled above")` for the six variants already dispatched at `:230-264`. Adding a real implementation for, say, `Accept` without also updating this table leaves a live `unreachable!` reachable from the CLI. The arm exists only to recover a `(name, slice)` label; that belongs on the variant (a `#[derive]`d method or a `const` table), not in a second match that must be kept in sync by hand.

**A5 — `Concurrency::try_start` returns `Result<RunGuard, ()>`.**
`crates/kadou-mcp/src/concurrency.rs:39`. The unit error discards which of the two refusals happened, so the caller at `crates/kadou-mcp/src/tools.rs:557` has to hedge in the user-facing text: "`{} is already running, or the server is at its concurrency limit`". An agent reading that cannot tell whether to retry the same id later or a different id now. `clean-code-errors/SKILL.md` requires "a one-sentence problem statement, then a line … that says exactly what to run to fix it"; an `or` in the problem statement means the fix line cannot exist. A two-variant enum makes both exact and costs nothing.

**A6 — The MCP server swallows a broken config or an undecryptable vault.**
`crates/kadou-mcp/src/state.rs:42` (`Config::load(...).unwrap_or_default()`) and `:47` (`VaultStore::new(...).load().unwrap_or_default()`). A malformed `kadou.toml` silently reverts the agent ceiling to the defaults; a vault the identity can no longer decrypt silently becomes "every need is missing". Neither writes a word to stderr, which the stdio transport leaves free precisely for this (PRD §3.1 tracing row). The CLI equivalents do warn (`crates/kadou/src/commands.rs:40-44`, `:83-86`) — the MCP path should match.

**A7 — Stringly-typed errors (4 sites).**
`clean-code-errors/SKILL.md`: "Variants name the failure in domain terms … not the mechanism", and `#[source]`/`#[from]` should preserve the chain.
- `ProposeError::BadHeader(String)` (`crates/kadou-mcp/src/drafts.rs:57`) flattens a `Vec<Diagnostic>` into one joined string at `:70-77`. See I-23 — this is a contract violation as well as a typing one.
- `VaultError::Keyring(String)` (`crates/kadou-core/src/vault.rs:103`) stringifies the backend error instead of carrying it as `#[source]`, so the chain stops there.
- `ResolveError::InvalidArg { name, message: String }` (`crates/kadou-core/src/resolve.rs:49`) is built from three distinct causes in `coerce` (`:109`, `:113`, `:119`) — not-an-integer, not-a-bool, not-in-options. A caller that wanted to offer the option list (slice 8's `select` prompt, §7.3) has to re-derive it from prose.
- `active_block: Option<&str>` with `Some("args")` (`crates/kadou-core/src/header.rs:185`, matched at `:198`) is stringly *state* rather than an error, but it is the same failure mode: a typo in either literal is a silent behavior change the compiler cannot catch. There is one block key today; a two-variant enum costs one line and makes adding a second safe.

**A8 — User-facing errors missing the fix line.**
`clean-code-errors/SKILL.md` requires "a one-sentence problem statement, then a line starting with `=` … that says exactly what to run to fix it" (PRD §4.7, §5.5). Most of the CLI honours this — `commands.rs:231-232`, `:338-339`, `:457-458`, `:1092` and `cli_confirm`'s `:178` are model examples. The gaps:
- `crates/kadou/src/commands.rs:497` — "invalid arg `{bad}`, expected key=value" states the shape but offers nothing to paste.
- `:510` — a bare `error: {err}` for a `ResolveError`. Two of its four variants carry a fix inline in their `Display` (`resolve.rs:44`, `:46`); two do not (`:42`, `:48`).
- `:566`, `:672`, `:734`, `:1019`, `:1083` — five bare `error: …: {err}` I/O failures with no `=` line.
- MCP side: `no_such_kata` (`tools.rs:37`), `invalid_args` (`tools.rs:433`) and `busy` (`tools.rs:557`) carry no human command. §5.5 only *requires* one for `missing_needs` and `draft`, and both deliver it (`tools.rs:544`, `:455`) — but `invalid_args` is the error an agent is most likely to hit and most able to act on. `expected` (the args schema, `tools.rs:434`) is arguably that fix in structured form; it deserves a decision rather than an accident.

### B. Function length and mixed abstraction levels

`clean-code-functions/SKILL.md` sets the bar: "Functions should be small — **under 30 lines** of logic", and it names the two files in question — "`header.rs` and `tools.rs` are already large files; keep each function inside them small even when the file is not." The table below is measured by `clippy::too_many_lines` at clippy's *default* 100-line threshold, so every entry is more than triple the repo's own limit.

| # | Site | Measured | What it mixes |
|---|---|---|---|
| B1 | `crates/kadou-mcp/src/tools.rs:463` `run_kata` | **188 lines** | draft check, visibility, JSON coercion, arg resolution, vault load, dry-run, the grant gate, missing-needs, concurrency, env assembly, history `begin`, spawn, and a three-way `select!` with detach semantics |
| B2 | `crates/kadou/src/commands.rs:1052` `run_grant_approve` | **200 lines** | record load, expiry, sha/HEAD pin, confirm, arg re-resolution, vault, env, history, exec, write-back, exit-code mapping |
| B3 | `crates/kadou/src/commands.rs:478` `run_run` | **152 lines** | kv parsing, lookup, resolution, dry-run rendering, ceiling, confirm, interactive vault capture, env, exec, last-args write |
| B4 | `crates/kadou-core/src/header.rs:101` `parse_header` | **275 lines**, cognitive complexity **26/25** | CRLF/empty guards, shebang, framing scan, tab/line-count limits, per-key dispatch, six value grammars, post-checks, notes extraction |
| B5 | `crates/kadou-core/src/header.rs:427` `parse_arg_line` | **114 lines** | help split, name validation, type word, four type grammars with three near-identical error arms, default coercion |
| B6 | `crates/kadou-mcp/src/tools.rs:123` `list_kata` | **106 lines** | three independent source scans (library, project-local, drafts), then filter/sort/dedupe/paginate/serialize |

**B1 is worth singling out.** `clean-code-functions/SKILL.md` illustrates "Single Responsibility, Single Level of Abstraction" with a sketch of what `run_kata` *should* look like:

```rust
pub fn run_kata(state: &ServerState, args: RunArgs) -> (Value, bool) {
    let kata = match resolve_visible(state, &args.id) { Ok(k) => k, Err(e) => return e };
    let env = build_mcp_env(&kata, &args)?;
    let outcome = finish_run(state, &kata, env)?;
    truncate_output(outcome)
}
```

Every one of those four helpers already exists: `resolve_visible` (`tools.rs:48`), `build_mcp_env` (`env.rs:26`), `finish_run` (`tools.rs:786`), `truncate_output` (`tools.rs:872`). The decomposition is done. What is missing is the orchestrator — the real `run_kata` inlines 188 lines of detail *around* its own helpers instead of reading like the sketch. R7 is mostly a matter of moving existing blocks into three more named functions, not designing anything new.

**B7 — Argument counts, and the two `#[allow]`s that hide them.**
The same skill: "Zero, one, or two arguments are ideal; three or more — reach for a config struct." `render_header` takes **8** (`crates/kadou-core/src/header.rs:619`) and `PendingStore::create` takes **9** (`crates/kadou-mcp/src/pending.rs:159`); both silence the lint with `#[allow(clippy::too_many_arguments)]` (`:618`, `:158`) rather than take the struct. These are the only two `#[allow]`s in the workspace, which makes them easy to retire: `render_header(&ParsedHeader, body)` and `PendingStore::create(&PendingRequest)`. The repo already uses exactly this pattern elsewhere — `FinishOutcome` (`history.rs:167-174`) was introduced to "keep the call site (and clippy's `too_many_arguments`) sane", and `ListArgs`/`DescribeArgs`/`RunArgs`/`ProposeArgs` (`tools.rs:89,243,405,913`) are the same idea.

**B8 — `kadou-exec` has no `Runner`.**
PRD §3's crate table specifies `kadou-exec`'s responsibility as "`Runner::run(ctx, kata_path, env)`", and `clean-code-boundaries/SKILL.md` uses it as its worked example of defining a trait at the consumer: "`kadou-exec::Runner` defines what 'run a kata' means; the process-group/shebang implementation lives behind it. A future test double implements the same shape without `kadou-exec` importing test code." No `Runner` trait or type exists — `crates/kadou-exec/src/lib.rs` exposes free functions `run` (`:235`) and `run_blocking` (`:225`). Two documents describe a seam that isn't there, and its absence is why the three copies in C6 have nothing to share. Introducing it is the core of R2.

B4 deserves a note of its own: PRD §4.3 describes the header parser as "kadou's own **~200-line** parser". It is 910 lines in `header.rs`, of which `parse_header` alone is 275. The parser is correct and well tested — the count is a signal that the stated design budget was quietly exceeded, not that the code is wrong.

### C. Duplication

| # | Sites | Note |
|---|---|---|
| C1 | `crates/kadou-core/src/config.rs:204` vs `crates/kadou-core/src/fsutil.rs:11` | Byte-identical `write_atomic_0600`. `fsutil.rs:3-4` documents the duplication rather than removing it: "`config.rs` predates this module and keeps its own copy." |
| C2 | `crates/kadou/src/commands.rs:208`, `crates/kadou-mcp/src/server.rs:182`, `crates/kadou-core/src/header.rs:264-268` | Three hand-written risk-word parsers. `RiskLevel` has `as_str` (`risk.rs:23`) but no `FromStr`. |
| C3 | `crates/kadou/src/commands.rs:111` vs `crates/kadou-mcp/src/visibility.rs:22` | `human_ceiling` implemented twice. PRD §6.2 calls this one formula; two copies is exactly how the Go product's confirm logic drifted (`01` §2.1, cited in decision 4). |
| C4 | `crates/kadou-core/src/scan.rs:220` vs `crates/kadou-core/src/import.rs:438` | `read_dir_sorted`, differing only in error type. |
| C5 | `crates/kadou-core/src/digest.rs:18-22` vs `crates/kadou-mcp/src/pending.rs:78-82` | The `sha256:`+hex formatting loop, verbatim. |
| C6 | `crates/kadou-mcp/src/tools.rs:578-651`, `crates/kadou/src/commands.rs:590-649`, `crates/kadou/src/commands.rs:1149-1231` | **The important one.** The run pipeline — build `KADOU_*` env, push args, push needs, build `RunSpec`, exec, redact, finish history — exists three times with three different subsets of the steps. This is the mechanism by which CLI and MCP drift apart, which decision 4 and PRD §3 ("one engine … so confirm/risk/vault cannot drift") exist to prevent. Findings I-8 and I-9 are already-realised instances of that drift. |
| C7 | `crates/kadou/src/main.rs:316-1481` | `kadou().env("KADOU_HOME", home.path())` (sometimes with `.env_remove("HOME")`, sometimes not — see G4) appears ~40 times with no helper. |
| C8 | `crates/kadou-core/src/import.rs:387` vs `:407` | `collect_extra` and `copy_dir_recursive` are the same recursive walk, one buffering into memory and one writing through. |
| C9 | `crates/kadou-core/src/header.rs:471-500` | "unexpected text after type" written out three times for `text`, `int`, `bool`. |

### D. Crate boundaries

**D.1 — The CLI depends on the MCP crate for non-MCP work. (Design defect)**
`crates/kadou/src/commands.rs:14-16` imports `kadou_mcp::history`, `kadou_mcp::pending`, and `kadou_mcp::redact`. `kadou grant approve` — a human CLI command that never speaks MCP — writes a history record with `interface: "cli"` (`commands.rs:1185`) through the MCP crate. PRD §3's crate table assigns "history, last-used args" to **`kadou-core`**; `kadou-mcp`'s stated responsibility is "stdio + loopback HTTP; the four tools; result truncation". `pending.rs:6-11` argues the placement ("the MCP server is the only writer"), which is true today and stops being true the moment a CLI-initiated grant exists. The practical cost is D.2.

**D.2 — Redaction is unreachable from the CLI run path.**
Because `redact` lives in `kadou-mcp` (`crates/kadou-mcp/src/redact.rs`), and `kadou-exec` cannot depend on `kadou-mcp` without a cycle, the merged line stream in `kadou_exec::run` (`crates/kadou-exec/src/lib.rs:347-352`) has nowhere to apply it. That is the structural reason for I-7 (redaction happens after the fact, not in the stream) and I-9 (the CLI does not redact at all). Moving `redact` to `kadou-core` and taking an optional redactor in `RunSpec` fixes both at once.

**D.3 — Boundaries that are correct, and worth keeping.** `rmcp` appears only in `crates/kadou-mcp/src/server.rs` and `schema.rs`; `inquire` only in `crates/kadou/src/commands.rs:150-196`; `age` only in `crates/kadou-core/src/vault.rs`; `serde_yaml_ng` only in `crates/kadou-core/src/import.rs`, exactly as PRD §3 requires ("the product's own loader never parses YAML again"). `tokio` surfaces in `kadou-exec`'s public API only as `Option<oneshot::Receiver<()>>` (`lib.rs:237`), which is a reasonable price for cancellation. `run_blocking` (`lib.rs:225-231`) builds a fresh current-thread runtime per call; acceptable for a CLI, worth noting if `kadou mine` ever batches runs.

### E. Dead code and unused dependencies

**E1 — `zeroize` is declared and never used.** `crates/kadou-core/Cargo.toml` (and `Cargo.toml:63`) pull in `zeroize`; a workspace-wide grep finds zero uses. PRD §3.1 lists it as "Wipe decrypted vault buffers". The decrypted plaintext at `crates/kadou-core/src/vault.rs:173-180` and the deserialized `Vault` entries at `:366-371` live and die as ordinary `Vec<u8>`/`String`. This is a stated security control that was never built.
**E2 — `toml_edit` declared (`Cargo.toml:61`), never used.** Cause of A1.
**E3 — `tracing` / `tracing-subscriber` declared as `kadou-mcp` dependencies, never used.** There is no logging anywhere in the server. PRD §3.1: "Structured logs to stderr (MCP must not write on stdout)". Related to A6 — there is currently no channel for the swallowed errors.
**E4 — `similar` declared in `kadou-core`, never used there.** `crates/kadou-core/src/import.rs:427` hand-rolls `unified_diff` against `/dev/null` instead, while `kadou-mcp` (`drafts.rs:142`) and `kadou` (`commands.rs:968`) both use the real crate. Three diff renderings, one of them bespoke.
**E5 — Header-less helper files become phantom `ScannedFile` entries.** `crates/kadou-core/src/scan.rs:124-130` pushes every non-kata file into the scan result with an id (`sesami/scripts/trigger`) and no header. Only `render_report`'s `-v` branch (`check.rs:278`) and `sibling_files` (`tools.rs:286-301`) consume them. A helper named `sesami/deploy.sh.bak` produces the id `sesami/deploy.sh`; nothing today collides on it, but `find_kata` matches purely on `f.id == id` (`lookup.rs:34`), so the namespace is shared between real kata and ignored files by construction.
**E6 — `RunOutcome.output` is unbounded.** `crates/kadou-exec/src/lib.rs:275-281` collects every line into a `Vec<String>` with no cap. A kata that emits a gigabyte does so into the server's heap before any truncation (`tools.rs:872`) applies. PRD §6.1 bounds wall-clock and §5.5 bounds the *result*; nothing bounds memory.

### F. Naming

`clean-code-naming/SKILL.md`: "A module names what it *provides*: `header`, `risk`, `vault`, `visibility` — **not** `util`, `common`, `helpers`, `misc`."

**F1 — `fsutil` is a `util` module.** `crates/kadou-core/src/fsutil.rs` is the one module in the workspace named after a category rather than a capability, and it is `pub` (`lib.rs:10`) so the name is part of other crates' call sites (`pending.rs:143`, `history.rs:108`, `drafts.rs:115`). What it provides is *atomic writes at a fixed mode* — `atomic` or `secure_write` says that. The rename is cheap now and gets more expensive with each new caller; it also pairs naturally with R4, which changes what the module guarantees.
**F2 — `fs_err_create_dir_all` (`crates/kadou-core/src/import.rs:465`) does not use the `fs-err` crate** — it wraps `std::fs::create_dir_all`. The name states a dependency that isn't there. (`fs-err` is in the PRD §3.1 table but, like `zeroize` and `toml_edit`, is not a dependency of any member crate.)
**F3 — `where_` (`crates/kadou-core/src/check.rs:206`, `:232`)** — a trailing underscore to dodge a keyword, for a value that is a rendered location list. `locations` costs nothing.
**F4 — `inquire` is not behind a `ui` module.** `clean-code-boundaries/SKILL.md` and PRD §3 both place `inquire`/`crossterm` "inside the `kadou` bin's `ui` module"; today `inquire` is called directly from `crates/kadou/src/commands.rs:150-196` and `:188-196`. Slice 8 creates the `ui` module, so this resolves itself — worth noting only so the prompts get moved rather than re-implemented there.

### G. Test quality

**G1 — `timeout_escalates_to_sigkill_when_sigterm_is_ignored` is genuinely flaky. Reproduced. (High)**
`clean-code-testing/SKILL.md`, FIRST: "**Fast** — no real network, **no sleeping**"; "**Repeatable** — control time … explicitly".
`crates/kadou-exec/src/lib.rs:623-641`. Eight concurrent `cargo test -p kadou-exec` runs on this machine: **five failed**, three passed.

```
thread 'tests::timeout_escalates_to_sigkill_when_sigterm_is_ignored' panicked at
crates/kadou-exec/src/lib.rs:633:9:
a SIGTERM-ignoring script must wait out the SIGKILL grace: took 203.720792ms
```

The mechanism is worth stating precisely, because it is not generic slowness. The script is `#!/bin/sh\ntrap '' TERM\nsleep 30` and the test sets `timeout = 200ms` (`:627`). On the happy path the shell installs the ignore-disposition, `sleep` inherits `SIG_IGN` across `exec`, `killpg(SIGTERM)` (`:367`) is absorbed, and the run ends at the 5 s `KILL_GRACE`. Under load, process startup exceeds 200 ms, so the timeout fires **before `trap '' TERM` has executed** — the shell still carries the default disposition and dies at once, in ~203 ms. The assertion at `:634` (`elapsed >= KILL_GRACE`) then fails.

The sibling assertions are the same class, in the opposite direction and so far lucky: `timeout_terminates_a_sleeping_kata_via_sigterm:598` (`elapsed < KILL_GRACE`), `timeout_escalates…:638` (`elapsed < KILL_GRACE + 3s`), `stdin_is_dev_null_a_cat_script_does_not_hang:510` (`elapsed < 2s`). Each pins a wall-clock bound against a 200 ms–2 s budget on a shared CI box. The fix is not a bigger timeout — it is to remove the race: have the script write a readiness file before sleeping and have the test block on that file appearing before the timeout clock can matter. (The PRD's own §9 slice 3 asks only for "process-group cancel test (Unix, SIGTERM then SIGKILL)", which a readiness-gated test satisfies exactly.)

**G2 — Copy-paste where table-driven is already the house style.**
`clean-code-testing/SKILL.md` calls for "`rstest`'s `#[case]` … or a plain `for` loop over a local slice of cases when it doesn't [depend on rstest]" — the plain-loop form, since no crate here depends on `rstest`.
`crates/kadou/src/main.rs:894-960` is four near-identical confirm tests; `:1092-1218` is five near-identical grant tests; `crates/kadou-core/src/resolve.rs:287-367` is five near-identical need-resolution tests differing only in `(default, vault entry, secret bit)` → `(value, secret)`. The good pattern is in-tree three files away: `crates/kadou/src/confirm.rs:81-108` loops over `[RiskLevel::Low, RiskLevel::Medium]` and `[High, Critical]`. `resolve.rs`'s five cases are a four-row table; the confirm tests are a five-row table of `(risk, flag, tty, expected)`.

**G3 — 1 180 lines of subprocess integration tests live in `main.rs`.**
`crates/kadou/src/main.rs:303-1481` is a `#[cfg(test)] mod tests` that drives `assert_cmd::Command::cargo_bin("kadou")`. These are integration tests by every definition — they spawn the built binary — and belong in `crates/kadou/tests/`. As written they force a rebuild of the binary's unit-test harness for every test edit, and they make `main.rs` (1 481 lines) the largest file in the workspace while its actual logic is 300 lines.

**G4 — Test isolation depends on the developer not having `~/.dops`.**
`clean-code-testing/SKILL.md`, FIRST: "**Repeatable** — … **never depend on the real `$HOME`**".
`crates/kadou/src/commands.rs:56-60` reads `$HOME` and auto-imports a legacy Go vault on every vault use. Tests that set `KADOU_HOME` but *not* `.env_remove("HOME")` therefore reach the developer's real `~/.dops/vault.json`: `main.rs:403` (`run_starter_hello_actually_executes`), `:420`, `:1425` (`grant_allow_pins_to_the_current_sha256_by_default`), among others. Roughly half the tests in the file do remove it (`:704`, `:718`, `:1246`, …) and half do not — there is no rule, just history. On this machine `~/.dops` does not exist, so the suite is green; on a machine with a real dops install, those tests would decrypt and copy real secrets into a temp dir. Either `env_remove("HOME")` belongs in the shared `kadou()` helper (`:312`), or `auto_import_go_vault` needs an explicit opt-out env var.

**G5 — Missing negative cases.** No test asserts: that a CLI `kadou run` redacts a secret (it doesn't — I-9); that `pending/` and `history/` directories are `0700` (they aren't — I-6); that `propose_kata` rejects an oversized `source` or a traversing `id` (neither is checked — I-13); that duplicate arg names, malformed aliases, or non-conforming id segments fail `check` (none do — F1–F3); or that the **wire** `tools/list` payload is byte-identical to `docs/design/tools-list.json`. On that last point, `crates/kadou-mcp/tests/mcp_server.rs:104-109` checks names and order over the wire, and `crates/kadou-mcp/src/schema.rs:150-161` checks bytes against the static `tools_list_value()` — nothing checks that what rmcp actually serializes equals the fixture. The two halves are each correct; the join is untested.

**G6 — `check`'s cross-kata diagnostics are order-nondeterministic.**
`crates/kadou-core/src/check.rs:200` and `:228` iterate `HashMap`s. With one conflict the output is stable by luck; with two or more, diagnostic order varies run to run. PRD §4.7 requires `insta` snapshots of this output; a future multi-conflict snapshot would flake. `BTreeMap` costs nothing here.

**G7 — What the tests get right, and should not be traded away.** `crates/kadou-mcp/tests/mcp_server.rs` drives the real server through rmcp's own client over an in-process duplex and asserts on JSON, not on internals; the sesami-shaped fixture (32 stub kata, `tests/fixtures/sesami-shaped/`) proves the exact five-visible-ids claim from PRD §6.2 rather than restating it; `tests/fixtures/headers/{good,bad}/` is insta-snapshotted as §4.7 requires; and `mcp_never_writes_the_vault` (`:311-348`) asserts on bytes *and* mtime. This is the standard the rest of the suite should be raised to.

---

### H. Loader strictness gaps (`kadou check` accepts what the PRD calls errors)

Reproduced against the built binary on a scratch `KADOU_HOME`:

```
$ printf '#!/bin/sh\n# ---\n# about: Dup\n# risk:  low\n# alias: Bad/Alias UPPER\n# args:\n#   x: text = a\n#   x: text = b\n# ---\necho hi\n' > kata/sesami/dup.sh
$ printf '#!/bin/sh\n# ---\n# about: Bad id\n# risk:  low\n# ---\necho hi\n' > kata/sesami/My_Kata.sh
$ kadou check sesami
checked 4 kata in sesami   0 errors  0 warnings
$ kadou list | grep My_Kata
sesami/My_Kata               ● low     Bad id
```

**H1 — Duplicate arg names are accepted.** `crates/kadou-core/src/header.rs:201-204` pushes each parsed arg unconditionally. Top-level keys *are* de-duplicated (`:242-250`); args are not. Two `x:` lines produce two `Arg`s, two identical `TARGET_DIR`-style env entries (`tools.rs:585-587`), and a `describe_kata` schema whose `properties` map silently keeps only the last (`tools.rs:379`).
**H2 — Aliases are never validated.** `crates/kadou-core/src/header.rs:316-320` pushes whitespace-separated tokens verbatim. PRD §4.2: "Aliases have no slash and are unique across all folders, checked." Uniqueness *is* checked (`check.rs:228-248`); the shape is not, so `alias: Bad/Alias` and `alias: UPPER` both load.
**H3 — Id segments are never validated.** No code anywhere enforces PRD §4.2's `^[a-z0-9][a-z0-9-]*$`, which the PRD says is "a check error with a rename suggestion". `My_Kata.sh` loads, lists, and runs. This also leaves `LastArgsStore::path` (`crates/kadou-core/src/last_args.rs:44-47`) joining an unvalidated id straight into a filename — safe today only because ids come from real directory entries.
**H4 — Reserved top-level names are unenforced.** PRD §4.2 reserves `starter`, `proposed`, `mined`; `crates/kadou-core/src/scan.rs:69-86` will happily scan a user-created `kata/mined/` folder, which then shadows the draft namespace in `list_kata`'s dedupe (`tools.rs:210`). `kadou get --as mined` is a slice-7 stub, so this is currently only reachable by hand.
**H5 — Diagnostic columns mix bytes and characters.** `Diagnostic.col` is documented as "1-based column (**character**, not byte)" (`header.rs:42-43`), but it is computed from `str::find` byte offsets at `header.rs:218,228,232-233`, `:456-458`, and from `content.len()` at `:208`. A header with a non-ASCII `about:` renders its caret in the wrong place (`check.rs:332-334`). Cosmetic, but the PRD calls these messages "the bespoke parser's user interface" (§8.2).
**H6 — `MAX_HEADER_LINES` is off by the framing.** `header.rs:143` allows 64 lines *after* the opener, so a 66-line header block passes. PRD §4.3 says "Max 64 header lines". Trivial.

## 3. Safety invariants, re-checked by reading

`CLAUDE.md` (on `main` as of `6a56db5`) lists six invariants under "never regress these"; the brief adds a seventh (pending records never hold need values). Rows 1–6 below are `CLAUDE.md`'s, in its order; row 7 is the brief's. Each was verified by reading the implementing code, not by trusting a test name.

**Five of seven hold as written. One holds for MCP and fails for the CLI (row 5). One holds in effect but not in the manner the invariant specifies (row 5's "before either is written").**

| # | Invariant (PRD) | Verdict | Evidence |
|---|---|---|---|
| 1 | Four tools, byte-identical to `docs/design/tools-list.json` | **Holds, with an untested join** | `schema.rs:20-85` is the single source; `build_tools` (`:96-104`) derives the served `Tool`s from the same `Value`, so code-vs-fixture cannot drift (`:150-161`, compact 2 028 B vs the 2 800 B gate at `:164-170`; `wc -c` on the fixture is 2 029 with its trailing newline). Order and names confirmed over the wire at `mcp_server.rs:104-109`. **Gap:** nothing asserts the rmcp-serialized wire payload byte-equals the fixture (G5). |
| 2 | Agents never see high/critical by default | **Holds** | `visibility.rs:32-45` implements §6.2's `min()` exactly, including `--max-risk` as a narrowing term; `is_visible_risk` (`:49-51`) is `<=`, matching "a kata **at** the ceiling is allowed". Defaults are `agent.max_risk = Low` (`config.rs:62`) and `max_risk = Medium` (`config.rs:39`). Proven end-to-end: `mcp_server.rs:113-146` returns exactly the five low-risk ids from the 32-kata fixture and asserts `ses-deploy`/`ses-release-build` are absent; `:568-581` returns `no_such_kata` for the critical one at defaults. |
| 3 | Needs can never be args | **Holds, structurally** | `resolve.rs:60-67` rejects any provided key naming a need *before* any value is resolved or any env built; the error is a distinct variant (`ResolveError::ArgNamesNeed`, `:46`). Wire-verified at `mcp_server.rs:292-308`. This is the grammar-level guarantee decision 13 asks for, not a runtime scope check. |
| 4 | MCP never writes the vault | **Holds** | `state.rs:45-49` exposes only `VaultStore::load`; no `VaultStore::save` / `Vault::set` call exists anywhere in `crates/kadou-mcp` (grep-verified). `mcp_server.rs:311-348` runs `run_kata`, `describe_kata` and `propose_kata` and asserts vault bytes **and** mtime are unchanged. |
| 5 | Secrets redacted in results and logs, **before either is written**; "never redact-on-read" | **Holds for MCP by outcome, not by mechanism; FAILS for the CLI** | MCP: `tools.rs:820` redacts before both the result body and `history.finish`, so the log (`history.rs:148`) and the result see identical text; proven at `mcp_server.rs:230-263` against result *and* on-disk log. But `CLAUDE.md` specifies the mechanism — redaction "runs on **the line stream** before it hits the history log or the MCP result" — and the code redacts the *joined, completed* output instead (`tools.rs:819-820`), which is why a `status: running` log is empty until the end (I-7) and why the raw secret sits in `RunOutcome.output` (`kadou-exec/src/lib.rs:275-281`) for the life of the run. **CLI `kadou run` never redacts and writes no log at all** (`commands.rs:615-617`) — see I-8/I-9. `kadou grant approve` redacts for the log but prints the raw stream first (`commands.rs:1198-1202`). Partial: §6.6's fourth encoding, base64 of `user:value`, is unimplemented and documented as such at `redact.rs:8-10`. |
| 6 | MCP env allowlist | **Holds, exactly** | `env.rs:8-19` lists PATH, HOME, USER, LOGNAME, SHELL, LANG, TZ, TMPDIR, SSH_AUTH_SOCK, KUBECONFIG; `:34` adds `LC_*`; `:35` adds `[exec] pass_env`; `:38` forces `TERM=dumb`. That is §6.1's list with nothing added and nothing missing. `env_clear: true` is set on the MCP path only (`tools.rs:604`) and left `false` for the CLI (`commands.rs:610`, `:1173`), matching "CLI keeps the full parent environment". Proven at `mcp_server.rs:266-289` and `main.rs:1333-1367` (the inverse, for `grant approve`). |
| 7 | Pending records never hold need values | **Holds** | `PendingRecord.args` (`pending.rs:33`) is built solely from `resolved_args` at `tools.rs:723-726`; `resolved_needs` is never passed to `PendingStore::create` (`tools.rs:738-746`). Asserted at `mcp_server.rs:553-557`. **One caveat, not a violation:** the record does store the kata's full `source` (`pending.rs:45`, for the "diff since request" of §6.4 item 4). The file itself is `0600`, but it sits in a `0755` directory (I-6), so a kata that hard-codes a credential is one `chmod`-gap away from being world-readable. |

---

## 4. Spec drift

Every place the code and `docs/design/05-prd.md` disagree, with a recommendation for which side moves.

| # | Topic | PRD says | Code does | Which side changes |
|---|---|---|---|---|
| **I-1** | **Needs precedence** | §4.4: "Resolution: **vault entry, else default**, else missing"; "`name=default` gives a … fallback used **when the vault has no entry**" | Header default wins first; the vault is consulted only when there is no default — `resolve.rs:136-153`, documented as deliberate at `:3-5` and locked in by the test at `:336-351` | **CODE.** The PRD's order is the one the product needs: §1.2 and §7.4 both tell an operator to `kadou vault set jenkins_url`, which under the current order is a silent no-op for every kata whose header carries a `jenkins_url=` default — which, after `kadou import`, is all 32 Sesami kata (`import.rs:202-206`). The code's stated rationale ("a default in a git-tracked file is by definition not a secret") justifies forcing `secret: false` on a default-backed need, not giving it precedence. Fix: vault first, `secret` from the vault entry's own bit; keep `secret: false` when the default supplies the value. |
| **I-2** | **Starter materialization** | §7.4 item 2: "materialized from the embed **only if `kata/` does not exist**. **If deleted, it stays deleted**; `kadou get starter` restores it." `09-tui-decision.md:21` **D6** says the same: "materialized on first run if `kata/` does not exist; **deletable**" | Every missing starter file is rewritten on **every** command that scans kata — `starter.rs:25-38`, called from `commands.rs:223,307,491,662,806,891,919,1006,1054` and `main.rs:227`. `starter.rs:89-99` tests that a deleted file **is recreated**, the exact opposite of the contract | **BOTH.** The code is fixing a real slice-5 defect (documented at `starter.rs:6-13`: an `import`-first home never got the starter, because `kata/` already existed) — but it overshot from "only if `kata/` is absent" to "always", losing deletability. Correct resolution: gate on **`kata/starter/`**, not `kata/` and not per-file. That fixes the import bug and restores "if deleted, it stays deleted" (delete the folder, it stays gone). Then amend §7.4 item 2 and D6 to read `kata/starter/` instead of `kata/`, and delete the `a_deleted_starter_file_is_recreated_on_the_next_scan` test. **This is the single most important doc-vs-code disagreement in the tree, because the code currently cites D6 as its authority for behavior D6 forbids.** |
| **I-3** | **`pending_id` encoding** | §5.5 and §7.2 show `"pending_id": "7c1e…"` and `approve: "kadou grant approve 7c1e"` — a short prefix | Full UUID v4 (`pending.rs:174`), emitted in full in `approve` (`tools.rs:776`), and resolved by exact filename match with no prefix support (`pending.rs:101-103`, `commands.rs:1057`). `kadou grant approve 7c1e` fails | **BOTH.** Keep the full UUID on disk and in the record; add unambiguous-prefix resolution in `PendingStore::get` (≥4 chars, error on ambiguity, the `git` convention), and have `approve`/`needs you` print the short form. Then §5.5's examples are literally true. If prefixes are rejected, §5.5 and §7.2 must be rewritten to show full UUIDs — but a 36-character token in the one line a human is meant to copy is a UX regression the `09` §4 "told, not housed" decision was written to avoid. |
| **I-4** | **Allow-list pin encoding** | §6.4 item 5 says `grant allow` pins "to the current sha256 unless `--any-version`" but never says how; §7.6's config example shows only bare ids (`allow = [] # e.g. "sesami/ses-deploy"`) | `"<id>@sha256:<64 hex>"` — written at `commands.rs:1017`, parsed at `tools.rs:691-698`. The parser's doc comment (`tools.rs:686-690`) cites "the worker brief", not the PRD | **DOC.** The encoding is sound (a bare entry means any version; an `@`-suffixed one pins) and `@` cannot appear in a valid id, so it is unambiguous. Add it to §6.4 item 5 and show both forms in §7.6's `[agent] allow` example. A code comment whose only authority is an ephemeral worker brief is a spec hole by definition. |
| **I-5** | `grant allow` / `trust` config edits | §3.1: `toml_edit` for "**comment-preserving** `kadou trust` / `kadou grant allow` config edits" | Whole-file `toml::to_string_pretty` rewrite (`commands.rs:1033` → `config.rs:196`); `toml_edit` unused | **CODE.** See A1. |
| **I-6** | State directory modes | §6.6: "History and pending directories `0700`, files `0600`" (§6.5 likewise for vault/keys) | Only the vault uses `ensure_dir_0700` (`vault.rs:310,377,419`). `pending/` (`pending.rs:143`), `history/logs/<date>/` and `history/records/` (`history.rs:108,137,148`), `proposed/` (`drafts.rs:96`) and `last/` (`last_args.rs:68`) are all created by `fsutil::write_atomic_0600`'s bare `create_dir_all` (`fsutil.rs:14`), i.e. at the process umask — **0755** at the default 022 | **CODE.** One-line fix: call `ensure_dir_0700` on the parent inside `write_atomic_0600`, or at each store's entry point. Files are correctly `0600` throughout, so only the directory bit is wrong — but that bit is what makes a pending record's pinned `source` (invariant 7) and a run log readable by other local users. |
| **I-7** | Live log for a `status: running` run | §6.1: `max_wait` returns `running` "with `history_id`/`log_path` **so the agent can poll the log itself**"; §6.6: "**Redaction happens in the line stream, before the log is written**"; §1.3: "the fresh-tier log is a plain-text file an agent's own tools can read" | `begin` creates an **empty** log (`history.rs:108`); the whole output is joined, redacted and written once in `finish` (`tools.rs:819-820`, `history.rs:148`). A polling agent reads an empty file for the entire run. `mcp_server.rs:351-374` asserts the file exists, then sleeps 2 s for the run to end before checking content — the test is consistent with the gap | **CODE.** This is the one functional promise of the `running` status and it is not delivered. Fix with D.2: move `redact` to `kadou-core`, give `RunSpec` an optional redactor plus a line sink, and have `pump_lines` (`kadou-exec/src/lib.rs:347-352`) append each already-redacted line to the log as it arrives. That also delivers §6.6's "before it is written" literally, and bounds E6's buffer. |
| **I-8** | CLI runs are audited | §6.6 defines `interface` as `cli \| mcp` and lists the record fields for both | `run_run` writes **no history record at all** (`commands.rs:613-649`) and no log. `grant approve` does (`commands.rs:1181-1231`, `interface: "cli"`). `kadou history` is still a stub (`main.rs:280`) | **CODE**, or an explicit doc deferral. Falls out of the C6 refactor for free: one shared runner means `kadou run` gets the audit trail the same way `grant approve` already has it. If it is genuinely meant to wait for slice 8, §9 should say so — today §6.6 reads as though it already works. |
| **I-9** | CLI output redaction | §6.6: "the MCP result, the on-disk log, **and the CLI** all see the same already-redacted text"; "Also redact secret values supplied through CLI prompts (defense in depth)" | `commands.rs:615-617` prints the raw stream. Values captured by the inline missing-need prompt (`commands.rs:561-571`) are never added to any redaction set | **CODE.** Same fix as I-7. Lower severity than it looks — this is a human's own terminal and their own secret — but it is a stated invariant that silently does not hold, and the scrollback is what gets pasted into a ticket. |
| **I-10** | CLI visibility ceiling | §6.2: `visible(k,f) = rank(k.risk) ≤ human_ceiling(f)` **[CLI]** | `run_list` applies it (`commands.rs:255-266`) and `run_run` applies it (`commands.rs:541`, via `check_human_ceiling:123-135`), but **`run_show` does not** (`commands.rs:654-721`). Reproduced: with the default `max_risk = medium`, `kadou list` hides `sesami/ses-deploy` (critical) while `kadou show sesami/ses-deploy` prints it in full, including resolved non-secret need values (`:693`) | **CODE.** `show` is the CLI projection of `describe_kata` (§7.1), and `describe_kata` *does* gate on the ceiling (`tools.rs:264`). One call to `check_human_ceiling` closes it. Note `--dry-run` also returns before the ceiling check (`commands.rs:522-539`) — defensible, since it never spawns, but see I-16. |
| **I-11** | Id segment rule | §4.2: segments match `^[a-z0-9][a-z0-9-]*$`; "Uppercase or underscore in a filename **is a check error with a rename suggestion**" | Unenforced anywhere. `My_Kata.sh` checks clean and lists (reproduced, §2 H) | **CODE.** Validate in `scan_dir` (`scan.rs:88-168`) and emit a `Diagnostic` with the kebab-case suggestion as the fix line. Also makes `import` surface a bad source directory name instead of writing an invalid kata (`import.rs:238-242`). |
| **I-12** | Alias rules | §4.2: "Aliases have no slash and are unique across all folders, **checked**" | Uniqueness checked (`check.rs:228-248`); shape not checked at all (`header.rs:316-320`). `alias: Bad/Alias` loads | **CODE.** Reuse `validate_name` (`header.rs:409-425`), minus the env-reserved check — an alias is not an env var. |
| **I-13** | `propose_kata` hardening | §5.4/§6.7: the server "canonicalizes the resolved path and refuses anything outside `~/.local/state/kadou/proposed/`", "**size-caps `source`**", and strict-loads the header | Header strict-load ✓ (`drafts.rs:90-93`). **No size cap** — `source` is written unchecked (`drafts.rs:115`) despite the schema's `maxLength: 65536` (`schema.rs:78`). **Containment is checked after the write**, not before: the lexical guard at `:108-113` passes for `proposed/../../x.sh` (the raw path *does* `starts_with` the root), and the real canonical check at `:124-133` runs only after `write_atomic_0600` has already created the file, then `remove_file`s it. The comment at `:81-83` asserts the id is "already schema-validated … so a `..` path-traversal segment is structurally impossible" — **rmcp does not validate `inputSchema` server-side**, so nothing enforces the `pattern` at `schema.rs:77` | **CODE.** Validate `id` against the PRD's own pattern and `source.len()` against the cap at the top of `propose`, and canonicalize the *parent* before writing rather than the target after. The current arrangement is safe only because every real client happens to send well-formed ids. |
| **I-23** | `propose_kata` diagnostic text | §5.4: "a bad header is `invalid_args` with **the same diagnostic text `kadou check` prints**" | `render_diagnostics` (`drafts.rs:70-77`) joins only `Diagnostic.message` with `"; "`, discarding every `fix` line and the whole cargo-shaped `--> file:line:col` + caret rendering that `check::render_diagnostic` (`check.rs:311-342`) produces. An agent proposing a kata with `# risk: mediun` gets `unknown risk level ...` where a human running `kadou check` gets that line **plus** `= risk is one of low, medium, high, critical` and a caret under the offending token | **CODE.** `render_report` already exists and already takes a `FolderReport`; wrapping the parsed draft in one — exactly what `check_path` does at `check.rs:131-162` — yields the identical text for free. This matters more than it looks: §5.4's argument for a two-property `propose_kata` schema is that the *diagnostics* teach the agent the grammar, and the fix lines are where the grammar actually lives |
| **I-14** | Go vault import secret bits | §6.5: "secret bit **from the source YAML's `secret: true` flags**" | Every imported value is marked `secret: true` (`vault.rs:545`), with the reasoning documented at `vault.rs:500-507`: the Go plaintext carries no per-value marker, and re-parsing `runbook.yaml` is out of scope for a vault-only import | **DOC.** The fail-safe default is right. Amend §6.5 to say every imported value is secret and a human downgrades with `kadou vault set --plain <name>`, and note the visible consequence: `jenkins_url` arrives secret, so it will not appear in `env_public` (§5.5's dry-run example) until downgraded. |
| **I-15** | Importer drops the `secret:` flag | Decision 13 / §4.4: "Args cannot be secret … a secret must **never** be [agent-settable]" | `DopsParam` (`import.rs:31-46`) has no `secret` field, so the flag is discarded. Need-vs-arg is decided solely by `scope == "global"` (`import.rs:201`). A dops param with `scope: runbook, secret: true` therefore converts into a **plain, agent-settable arg** | **CODE.** This is the one place in the tree where a secret can cross into the arg namespace, and it crosses silently. Add `secret: bool` to `DopsParam` and make a non-global secret param a hard `ImportError::Param` telling the operator to re-declare it as a need. No Sesami runbook hits it today (all three secrets are `scope: global`), which is why it is invisible. |
| **I-16** | `dry_run` and the gates | §6.3/§6.4 describe the gates for `run_kata` without carving out `dry_run` | MCP `dry_run` returns before the grant gate (`tools.rs:498-510`, gate at `:515`); CLI `--dry-run` returns before both the human ceiling and confirm (`commands.rs:522-539`). Both are deliberate and commented (`tools.rs:512-514`, `commands.rs:121-122`) | **DOC.** The behavior is right — `dry_run` never spawns and never resolves a secret value (`kadou-exec/src/lib.rs:202-209`). Add one sentence to §6.3: "`dry_run` resolves names only and is not subject to the confirm protocol, the grant gate, or the human ceiling." Without it, a reader of §6.4 would expect a `pending_grant`. |
| **I-17** | `list_kata` aliases | §5.5: "`aliases: []` and `draft: false` are **omitted**, not printed, on entries that don't need them" — implying they appear when non-empty | `aliases` is never emitted (`tools.rs:216-228`); `draft` is correctly emitted only when true (`:223-225`) | **DOC.** Drop `aliases` from the §5.5 sentence. A list result is the per-connect-cost-sensitive payload (§1.3); the alias is already searchable via `query` (`tools.rs:108-113`) and visible in `describe_kata`. |
| **I-18** | Redaction encodings | §6.6: redact "the literal value, its standard base64 encoding, **base64 of `user:value`**, and its URL-encoded form" | Three of four (`redact.rs:36-42`); the HTTP-basic pairing is unimplemented and documented as a gap at `redact.rs:8-10` | **DOC or CODE.** The gap is real: a Sesami kata doing `curl -u "$JENKINS_USER:$JENKINS_TOKEN"` with `curl -v` would echo the basic-auth header unredacted. Since `jenkins_user`/`jenkins_token` is exactly the shipped pairing, I lean **CODE**: redact `base64(v1 + ":" + v2)` for every ordered pair of resolved need values, which needs no "username" concept — just the cross product, which is tiny. |
| **I-19** | `kadou run --ask` | §6.3/§7.3: "`kadou run <id> --ask` prompts for **every** arg" | Parsed (`main.rs:102`) and **silently dropped** — `main.rs:240` forwards `id, kv, dry_run, confirm` and never `ask` | **CODE.** Until slice 8, `--ask` should be an explicit "not yet implemented (slice 8)" refusal. A flag that parses and does nothing is worse than one that errors. |
| **I-20** | Declared-but-absent controls | §3.1 assigns a purpose to `zeroize` ("wipe decrypted vault buffers"), `toml_edit` ("comment-preserving edits"), `tracing` ("structured logs to stderr") | All three declared, none used (E1–E3) | **CODE.** Each is a stated control, not a nice-to-have: E1 is the only thing standing between a core dump and the decrypted vault; E3 is the only channel A6's swallowed errors could use. |
| **I-21** | Header line budget | §4.3: "Max 64 header lines" | 64 lines counted *after* the opener (`header.rs:143`), so 66 total pass | **CODE** (one-character fix) **or DOC**. Trivial either way. |
| **I-22** | Parser size budget | §4.3: "kadou's own **~200-line** parser" | `header.rs` is 910 lines; `parse_header` alone is 275 | **DOC.** The parser earns its size — it is the entire kata grammar plus its diagnostics. Restate the budget as a design intent ("small enough to audit in one sitting") rather than a line count the code will never meet. |

---

## 5. Ranked refactor list

Ranked by (safety or contract impact) × (cost to fix later). Each item names the files, the exact change, the test that proves it, and an effort estimate (S ≤ 1 h, M ≤ half a day, L ≤ 2 days).

### Before slice 7

Slice 7 is "folder git install + accept hardening" — it adds `kadou get/update/remove/accept`, which means more commands in `commands.rs`, more paths under `kata/`, and the first code that writes into a git checkout. These eight items are the ones that get materially more expensive once that lands.

**R1 — Fix needs precedence (I-1). Effort: S.**
Files: `crates/kadou-core/src/resolve.rs:136-153` (invert the two blocks: vault first, then default), `:3-5` (rewrite the module doc), `:336-351` (rewrite the test that locks in the current order).
Change: `resolve_one_need` consults `vault.get(&need.name)` first and uses the entry's own `secret` bit; falls back to `need.default` with `secret: false`; else `value: None`.
Proof: rename the existing test to `a_vault_entry_overrides_a_header_default_and_carries_its_own_secret_bit` and assert `value == "https://vault.example.com"`, `secret == true`; keep `need_with_default_resolves_and_is_not_secret` (`:288`) for the no-vault-entry case; add a CLI case to `main.rs:774` asserting `kadou vault set jenkins_url --plain` then `--dry-run` shows the vault's URL, not the header's.
Why first: it is one function, and it is the difference between `kadou vault set jenkins_url` working and silently doing nothing on all 32 Sesami kata.

**R2 — Extract one shared `Runner` and collapse the three run pipelines (C6, I-7, I-8, I-9, D.1, D.2, E6). Effort: L.**
Files: move `crates/kadou-mcp/src/redact.rs` → `crates/kadou-core/src/redact.rs` (re-export from `kadou-mcp` for one release); move `crates/kadou-mcp/src/history.rs` → `crates/kadou-core/src/history.rs` (PRD §3 already assigns it there); add `RunSpec.redact: Vec<String>` and `RunSpec.log_sink: Option<PathBuf>` at `crates/kadou-exec/src/lib.rs:150-165`; redact and append per line inside `pump_lines` (`:347-352`) with a line-count cap for E6; add `kadou_core::runner::run_one(kata, args, needs, interface, …)` that does env → spec → exec → history and returns a `RunReport`; rewrite `crates/kadou-mcp/src/tools.rs:578-651`, `crates/kadou/src/commands.rs:590-649` and `:1149-1231` as three thin callers.
Proof: move `secret_need_values_are_redacted_in_both_the_result_and_the_on_disk_log` (`mcp_server.rs:230-263`) to a table over `interface ∈ {cli, mcp}`; add `max_wait_returns_running_with_a_log_that_already_has_partial_output` — assert the log contains the first line **while** the run is still in flight (the current test at `mcp_server.rs:351-374` sleeps past the end); add `cli_run_writes_a_history_record_with_interface_cli`.
Why before slice 7: `kadou accept` and `kadou get` add two more commands to `commands.rs`, and the third copy of this pipeline is already the reason the CLI silently lost redaction and history. A fourth copy is how it becomes permanent.

**R3 — Close the propose path (I-13). Effort: S.**
Files: `crates/kadou-mcp/src/drafts.rs:84-133`.
Change: at the top of `propose`, reject `source.len() > 65_536` and any `input_id` not matching `^[a-z0-9][a-z0-9-]*(/[a-z0-9][a-z0-9-]*)+$` (lift the pattern from `schema.rs:77` into one shared `const` so the schema and the check cannot diverge); canonicalize `proposed_root` and verify the *resolved parent* of `target` before writing; delete the lexical `starts_with` at `:108-113` and the write-then-remove at `:124-133`; fix the comment at `:81-83`, which asserts a validation that does not happen.
Proof: `propose_rejects_an_id_that_escapes_the_proposed_root` (`"a/../../x"`) asserting `PathEscape` **and** that no file exists anywhere outside `proposed/`; `propose_rejects_an_oversized_source` (65 537 bytes); `propose_rejects_an_uppercase_id`.
Why before slice 7: slice 7 adds `kadou accept`, which reads from `proposed/` and writes into `kata/`. Containment has to be true at the write, not repaired after it, before anything downstream trusts that tree.

**R4 — `0700` on every state directory (I-6). Effort: S.**
Files: `crates/kadou-core/src/fsutil.rs:11-33` — call `ensure_dir_0700(parent)` instead of `create_dir_all(parent)`.
Proof: extend `write_atomic_0600_creates_parents_and_sets_mode` (`fsutil.rs:58-64`) to assert the parent's mode is `0o700`; add `pending_and_history_directories_are_0700` to `crates/kadou-mcp/tests/mcp_server.rs` after a `pending_grant` and a run.
Why before slice 7: one line, and it is the containment for the pinned kata `source` that invariant 7 relies on.

**R5 — Enforce the loader rules the PRD calls errors (H1, H2, H3, I-11, I-12). Effort: M.**
Files: `crates/kadou-core/src/header.rs:201-204` (reject a duplicate arg name the way `:242-250` rejects a duplicate key), `:316-320` (validate alias shape via `validate_name` minus the env-reserved rule), `crates/kadou-core/src/scan.rs:88-168` (validate each id segment against `^[a-z0-9][a-z0-9-]*$`, with the kebab-case rename as the fix line).
Proof: three fixtures in `tests/fixtures/headers/bad/` (`duplicate-arg.sh`, `bad-alias.sh`, and a `scan`-level case for `My_Kata.sh`) — the existing insta harness at `crates/kadou-core/tests/header_fixtures.rs:41-52` picks them up with no new test code, which is exactly what that corpus is for.
Why before slice 7: `kadou get` clones arbitrary team repositories. The loader is the only thing standing between a cloned folder and the library, and today it accepts three classes of malformed kata.

**R6 — Remove every non-test `expect`/`unwrap` (A2), and surface config errors (A3, A6). Effort: M.**
Files: the eight sites in A2; `crates/kadou-core/src/config.rs:84-97` (parse `[exec] timeout` and `[mcp] max_wait` into `Duration` at load, returning `ConfigError`); `crates/kadou-mcp/src/state.rs:41-49` (log to stderr on a load failure — the `tracing` dependency of E3 already exists for this).
Proof: `config_with_an_unparseable_timeout_is_a_load_error`; `run_kata_reports_a_broken_config_on_stderr`; then turn on `clippy::expect_used`/`unwrap_used` (§6) so the class cannot come back.

**R7 — Split the six oversized functions (B1–B6). Effort: L.**
Files and targets: `tools.rs:463` `run_kata` → `resolve_request` / `gate` / `spawn_and_await` (the `select!` at `:647-683` and its detach semantics deserve their own named function with the `cancel_tx` lifetime comment attached to it); `commands.rs:1052` and `:478` → thin callers over R2's `run_one`; `header.rs:101` → `scan_frame` / `parse_keys` / `finish`; `header.rs:427` → a `parse_type` helper that collapses C9's three arms; `tools.rs:123` → `library_rows` / `project_local_rows` / `draft_rows` / `paginate`.
Proof: no new behavior, so the existing suite is the proof — plus `clippy::too_many_lines` at a 60-line threshold (§6) as the ratchet.
Why before slice 7: `run_kata` and `run_grant_approve` are where slice 7's `accept` logic will want to live. At 188 and 200 lines they have no room left.

**R8 — De-flake the exec timing tests (G1). Effort: S.**
Files: `crates/kadou-exec/src/lib.rs:588-641` and `:504-517`.
Change: replace the bare `sleep 30` scripts with ones that write a readiness file **before** sleeping (`trap '' TERM; : > "$READY"; sleep 30`), pass the path in `spec.env`, and have the test poll for that file before starting the timeout clock. Keep `KILL_GRACE` as the assertion, drop every other wall-clock bound.
Proof: the same two tests, run 20× under `--test-threads=16` with a parallel build in flight — currently 5 of 8 such runs fail (§2 G1).
Why before slice 7: CI is not yet running (`.github/workflows/ci.yml:1-3` — no GitHub remote). The first time it does, on a shared runner, this test fails intermittently and teaches everyone to re-run the build.

### Later

**R9 — Reconcile starter materialization with D6 (I-2). Effort: S.** `crates/kadou/src/starter.rs:25-38` gates on `kata_dir.join("starter").exists()`; delete `:89-99`; amend §7.4 item 2 and `09-tui-decision.md:21` to say `kata/starter/`. *Needs Mason's call on whether "deletable" survives — it is his decision D6, and the code currently overrides it while citing it.*

**R10 — Comment-preserving config edits (A1, I-5). Effort: M.** Use `toml_edit` in `run_grant_allow` (`commands.rs:1004-1046`) and in the slice-5 `trust` stub. Proof: `grant_allow_preserves_comments_and_does_not_write_unset_defaults` — write a `kadou.toml` with a comment and only `max_risk`, run `grant allow`, assert the comment survives and `[mcp]` was not materialized.

**R11 — Prefix-resolvable `pending_id` (I-3). Effort: S.** `PendingStore::get` (`pending.rs:127-130`) resolves an unambiguous ≥4-char prefix; `tools.rs:776` and `commands.rs:902` print the short form. Proof: `grant_approve_accepts_an_unambiguous_prefix` and `…_rejects_an_ambiguous_one`.

**R12 — Single source for risk parsing and ceilings (C2, C3). Effort: S.** `impl FromStr for RiskLevel` in `crates/kadou-core/src/risk.rs`; move `human_ceiling`/`agent_ceiling`/`is_visible_risk` from `crates/kadou-mcp/src/visibility.rs:22-51` into `kadou-core` and have `commands.rs:111` call it. Proof: move `visibility.rs:105-148`'s ceiling tests to `kadou-core` and add a CLI case asserting `kadou list` and `list_kata` agree on the same config.

**R13 — Zeroize decrypted vault material (E1, I-20). Effort: M.** `vault.rs:173-180` and `:366-371` — wrap the plaintext in `Zeroizing<Vec<u8>>`, derive `ZeroizeOnDrop` for `VaultEntry`. Proof: hard to test directly; assert the type-level contract (`VaultEntry: ZeroizeOnDrop`) and record the limitation.

**R14 — Fix the importer's secret-flag hole (I-15). Effort: S.** Add `secret: bool` to `DopsParam` (`import.rs:31-46`); a `secret: true` param with a non-global scope becomes an `ImportError::Param`. Proof: `a_runbook_scoped_secret_param_is_an_import_error_not_an_arg`.

**R15 — Move the CLI integration tests out of `main.rs` (G3, C7, G4). Effort: M.** `crates/kadou/src/main.rs:303-1481` → `crates/kadou/tests/cli.rs`, with one `fn kadou_in(home: &Path) -> Command` helper that always applies `.env_remove("HOME")` and `KADOU_NOTIFY_TEST_NOOP`. Proof: the same tests, green, with `main.rs` back to ~300 lines.

**R16 — Table-driven the copy-paste suites (G2). Effort: M.** `main.rs:894-960` → one `(risk, confirm_flag, expected_code, expected_stderr)` table; `resolve.rs:287-367` → one `(default, vault_entry, vault_secret) → (value, secret)` table. Proof: identical coverage, ~60% fewer lines, and the empty cell in the table is the missing case.

**R17a — Same diagnostics for `propose_kata` as for `kadou check` (I-23, A7). Effort: S.** `crates/kadou-mcp/src/drafts.rs:70-77` — build a one-file `FolderReport` and call `kadou_core::render_report` instead of joining messages; change `ProposeError::BadHeader` to carry the rendered report. Proof: `propose_with_a_bad_risk_word_returns_the_same_text_kadou_check_prints`, asserting the MCP `message` contains the `= risk is one of …` fix line.

**R17 — Wire-level `tools/list` byte gate (G5, invariant 1). Effort: S.** In `crates/kadou-mcp/tests/mcp_server.rs`, serialize the `ListToolsResult` the client receives and compare to `docs/design/tools-list.json` byte for byte. Closes the join between the two half-proofs.

**R18 — Deterministic check diagnostics (G6). Effort: S.** `check.rs:176-177`: `HashMap` → `BTreeMap`. Proof: a two-conflict folder snapshot that is stable across 10 runs.

**R19 — Base64 `user:value` redaction (I-18). Effort: S.** `redact.rs:36-42`: add `base64(a + ":" + b)` for every ordered pair of resolved secret-or-plain need values. Proof: `redacts_a_basic_auth_header_built_from_two_needs`.

**R20 — Remaining small items. Effort: S each.** `--ask` refusal (I-19, `main.rs:240`); header line budget (I-21, `header.rs:143`); character-accurate diagnostic columns (H5); `locations` for `where_` (F3); `Concurrency` two-variant error (A5); `main.rs:265-288`'s `unreachable!` table (A4); retire the two `#[allow(clippy::too_many_arguments)]` via config structs (B7); de-duplicate `write_atomic_0600` (C1), `read_dir_sorted` (C4), the sha256 hex loop (C5), and `collect_extra`/`copy_dir_recursive` (C8); rename `fs_err_create_dir_all` (F2) and `fsutil` (F1); use `similar` in `import.rs:427` instead of the hand-rolled diff (E4).

---

## 6. Proposed clippy configuration

### Measured baseline

`cargo clippy --workspace --all-targets -- -W clippy::pedantic -W clippy::nursery` → **459 warnings**. Counts below are raw; lib and lib-test compile separately, so most real counts are half.

| Count | Lint | Recommendation |
|---:|---|---|
| 114 | `must_use_candidate` | **allow** — noise on a binary-shaped workspace |
| 58 | `missing_errors_doc` | **allow** for now; revisit when the crates are published |
| 46 | `too_long_first_doc_paragraph` | **allow** — the long first paragraphs are PRD citations, which are the point |
| 45 | `needless_pass_by_value` | **warn, then fix** — 20-odd real cases, all in command/tool entry points |
| 36 | `match_same_arms` | **warn, then fix** — several are genuine (`tools.rs:831-836` maps `Failed` and `TimedOut` to the same pair) |
| 20 | `missing_panics_doc` | **allow** (R6 removes most of the panics that trigger it) |
| 16 | `format_push_string` | **warn, then fix** — all in `header.rs:635-666`'s `render_header`; `write!` is both faster and clearer |
| 12 | `too_many_lines` | **deny at a 60-line threshold** — the direct ratchet for B1–B6 |
| 12 | `map_unwrap_or` | **warn, then fix** — mechanical |
| 12 | `option_if_let_else` | **allow** — nursery, and its suggestions are usually less readable here |
| 12 | `doc_markdown` | **warn, then fix** — only 12, and they are all missing backticks |
| 10 | `single_match_else` | **allow** |
| 8 | `missing_const_for_fn` | **warn** |
| 7 | `redundant_clone` | **deny** — `commands.rs:323,606,1170`, `main.rs:1456`; all real |
| 6 | `derive_partial_eq_without_eq` | **warn** |
| 6 | `unused_self` | **warn** |
| 4 | `cast_possible_truncation` / `cast_possible_wrap` | **deny** — `server.rs:130,135` (`u64`→`usize` on the `limit`/`offset` path, which is attacker-influenced) and `kadou-exec/src/lib.rs:366` (`u32`→`i32` for `Pid::from_raw`, the signal target) |
| 2 | `cognitive_complexity` | **deny at 15** — `header.rs:101` is the only current offender |

### Proposed `Cargo.toml` (workspace root)

```toml
[workspace.lints.rust]
unsafe_code = "deny"                 # one exception: crates/kadou-mcp/src/notify.rs:74 (test-only set_var)
missing_debug_implementations = "warn"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
nursery  = { level = "warn", priority = -1 }

# Deny: the lints that encode this review's structural findings.
too_many_lines            = "deny"   # B1-B6
cognitive_complexity      = "deny"   # B4
redundant_clone           = "deny"
cast_possible_truncation  = "deny"
cast_possible_wrap        = "deny"
cast_sign_loss            = "deny"
# Panic hygiene (A2, R6). Test modules opt out; see below.
unwrap_used               = "deny"
expect_used               = "deny"
panic                     = "deny"
indexing_slicing          = "warn"   # header.rs:404,445,455,560 index &str by byte offset

# Allow: high-count, low-value on a workspace whose public API is one binary.
must_use_candidate         = "allow"
missing_errors_doc         = "allow"
missing_panics_doc         = "allow"
too_long_first_doc_paragraph = "allow"
option_if_let_else         = "allow"
single_match_else          = "allow"
module_name_repetitions    = "allow"
```

Each member crate then adds `[lints] workspace = true`.

### Proposed `clippy.toml` (repo root)

```toml
too-many-lines-threshold      = 60    # dops's "a function fits on a screen"
cognitive-complexity-threshold = 15
too-many-arguments-threshold  = 6     # retires both #[allow]s: header.rs:618, pending.rs:158
max-fn-params-bools           = 2     # commands.rs:1052 and starter paths pass bool pairs
doc-valid-idents = ["MCP", "XDG", "TTY", "SIGTERM", "SIGKILL", "UTF-8", "JSON", "TOML", "YAML", "kadou", "rmcp", "dops", "..", "sha256", "base64"]

# Directory modes are a safety control (I-6): route every state write through fsutil.
disallowed-methods = [
  { path = "std::env::set_var",   reason = "edition 2024 made this unsafe; pass values explicitly (see paths.rs:45-51)" },
  { path = "std::fs::create_dir_all", reason = "use kadou_core::fsutil::ensure_dir_0700 — state dirs are 0700 (PRD 6.5, 6.6)" },
]
```

### Rollout, and one warning about CI

`.github/workflows/ci.yml:37` already runs `cargo clippy --workspace --all-targets -- -D warnings`. Turning on `pedantic` + `nursery` at `warn` therefore **breaks CI immediately** — every one of the ~230 unique warnings becomes an error. Land it in three commits:

1. Add `clippy.toml` and the `allow` list only, plus the four `deny`s that are already clean (`cast_*`, `redundant_clone` after fixing the four sites). CI stays green.
2. Add `pedantic`/`nursery` at `warn` **and** change CI to `-D warnings -A clippy::pedantic -A clippy::nursery`, so the new groups are advisory while R7 and R20 work through them.
3. Once the count is zero, drop the `-A` flags from CI. `too_many_lines` and `cognitive_complexity` become the permanent ratchet against B1–B6 recurring.

`unwrap_used`/`expect_used` need a test carve-out — either `#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]` at each crate root, or (cleaner, and it also fixes G3) move the subprocess tests to `tests/`, where a single `#![allow]` at the top of each file covers them.

### Beyond clippy

- **`cargo-mutants`** is being baselined on `sd/dops/mutants`; `mutation-testing/SKILL.md` already sets the priority order — `visibility.rs`, `redact.rs`, `env.rs`, `header.rs`, `vault.rs`. Two additions from this review: **`crates/kadou-mcp/src/tools.rs:691-698` (`allow_permits`)** belongs near the top — a mutant that always returns `true` silently disables the entire grant gate, and nothing in the skill's list covers it; and **`crates/kadou-core/src/resolve.rs:56-73`** (the needs-are-never-args check, `CLAUDE.md` invariant 3) is the structural guarantee behind invariant 3 and is currently proven by exactly two tests (`resolve.rs:226-238`, `mcp_server.rs:292-308`). Note also that running mutants against `header.rs` today will report survivors for H1–H3 that are not weak tests but **missing rules** — the parser genuinely does not check those things, so fix R5 first or the signal will be misread.
- **MSRV**: `.github/workflows/ci.yml:54-64` runs `cargo check` on 1.88 for both OSes as §3.2 requires. Note that the job does *not* pin a lockfile, so a dependency bumping its own `rust-version` will fail this job before it fails anything else — which is the intended behavior, but worth knowing when it happens.
- **`cargo-deny`**: `deny.toml` is present and correct, with all four target triples §3.1 implies. The `licenses.allow` list matches §3.3 exactly.

---

## 7. Limits of this review

- Static reading plus the commands recorded in the evidence table. No live MCP host (Claude Code, Cursor) was connected; the wire behavior claims rest on `crates/kadou-mcp/tests/mcp_server.rs`'s in-process rmcp client, not on a real host.
- Linux `/bin/sh` (dash) behavior was not exercised — this machine is macOS, where `/bin/sh` is bash in POSIX mode. G1's mechanism (inherited `SIG_IGN` across `exec`) is POSIX-specified and should hold on dash, but the 200 ms race will have a different shape there.
- The real `~/Bitbucket/sdo-dops-catalog` was not read; all import claims are against `tests/fixtures/sesami-shaped/` (32 kata, which the suite confirms converts with 86 boolean and 1 integer coercion, 0 errors).
- `~/.dops` does not exist on this machine, so G4's Go-vault auto-import path was never executed — the finding is from reading `crates/kadou/src/commands.rs:56-74`, not from a reproduction.
- Both parallel branches exist and were read for context, per the brief. `sd/dops/practices` is already merged to `main` (`6a56db5`, `5186a3b`) — its `CLAUDE.md` and twelve skill files are the standard this review applies, and they are docs-only, so the code tree is unchanged. `sd/dops/mutants` is still behind `main` and carries no `.cargo/mutants.toml` yet, so no mutation baseline was available to cite; §6's mutation notes are recommendations, not measurements.
- No mutation testing was run (no config in the tree; `cargo-mutants` is not installed here). Every claim about test *strength* in §2 G is from reading assertions, not from surviving mutants.
- No code was modified. This document is the only change on this branch.
