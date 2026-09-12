# Mutation testing baseline

Measured against `main` at `ab5033b` (2026-09-11) with `cargo-mutants` v27.1.0, using the
`.cargo/mutants.toml` config committed alongside this doc. This is a baseline and a CI
ratchet, not a to-do list of tests to write in this slice — per the assignment, the tests
themselves are the next slice's job under TDD.

## Known blocker: `kadou` (the bin crate) is excluded from this baseline

`crates/kadou/src/main.rs` has `#[cfg(test)] mod tests` that call
`assert_cmd::Command::cargo_bin("kadou")` — i.e. unit tests inside the `kadou` bin target's
own test harness, invoking the bin they're compiled as part of. `CARGO_BIN_EXE_kadou` is only
set by Cargo for *other* targets in the package (integration tests under `crates/kadou/tests/`,
other bins) — never for a bin's own unit tests. `assert_cmd` says so directly:

```
thread 'tests::bare_invocation_is_a_stub_not_a_crash' panicked at .../assert_cmd-2.2.2/src/cargo.rs:232:5:
`CARGO_BIN_EXE_kadou` is unset
help: if this is running within a unit test, move it to an integration test to gain access to `CARGO_BIN_EXE_kadou`
```

Confirmed with `cargo test -p kadou` on a clean, unmutated `ab5033b` checkout: 22 of 24 tests
in `crates/kadou/src/main.rs` fail this way (2 pass — the ones that don't shell out to the
built binary). This is **pre-existing and unrelated to mutation testing** — it also means the
`test` job already in `.github/workflows/ci.yml` (`cargo test --workspace`) cannot pass as
written, once this repo is actually hosted on GitHub and that job runs for real. Fixing it is
out of this slice's authorized scope (no changes under `crates/**`); the fix is to move those
tests from `src/main.rs` into `crates/kadou/tests/` as integration tests. **This is the
single highest-priority action for whoever picks up `crates/kadou`'s tests next** — until
then, `kadou` cannot be mutation-tested at all, because `cargo-mutants --workspace` baselines
with `cargo test --workspace`, and that baseline fails before any mutant runs.

`.cargo/mutants.toml` excludes `crates/kadou/**` (and `crates/kadou-mine/**`, which is an
empty stub per `docs/design/05-prd.md` §9 slice 9 — nothing to mutate) so the rest of the
workspace can still be measured. Once the CLI tests move to `tests/`, narrow that exclusion
back to just `crates/kadou/src/main.rs`'s `fn main` (the original intent) and re-baseline.

## Commands

```
cargo install cargo-mutants --version 27.1.0 --locked
cargo mutants --workspace -j 4
```

Config (`.cargo/mutants.toml`, checked in) sets `minimum_test_timeout = 60` and
`timeout_multiplier = 5.0` — kadou-exec spawns real child processes and asserts SIGTERM/SIGKILL
timing (`KILL_GRACE = 5s`), and a mutant that breaks the kill path can let a `sleep 30` child
run to completion. 60s floor plus 5x the ~8s baseline test time gives enough headroom to
distinguish "genuinely slow" from "actually hung."

Ran on an 18-core Apple Silicon Mac at `-j 4`. **624 mutants tested in 14 minutes**: 409
caught, 147 missed, 65 unviable, 3 timeouts. Well under the ~40-minute shard threshold, so no
sharding was needed for this baseline. CI's own wall-clock time is unverified (see the
`ci.yml` comment) — shard there if a real run runs long.

## Per-crate counts

| Crate | Total | Caught | Missed | Timeout | Unviable | Caught ratio (of viable) |
|---|---|---|---|---|---|---|
| kadou-core | 388 | 280 | 68 | 1 | 39 | 280/349 = **80.2%** |
| kadou-exec | 43 | 31 | 8 | 0 | 4 | 31/39 = **79.5%** |
| kadou-mcp | 193 | 98 | 71 | 2 | 22 | 98/171 = **57.3%** |
| **Total** | **624** | **409** | **147** | **3** | **65** | 409/559 = **73.2%** |

"Caught ratio" = `caught / (caught + missed + timeout)` — i.e. of mutants that compiled
(excluding `unviable`, which cargo-mutants' own docs call "inconclusive about test coverage,
no action needed"), the share a test actually failed on. `timeout` counts against the ratio
(not toward it) since a hang isn't a confirmed test failure, even though in practice a
timeout would also fail CI.

`unviable` mutants (65) are almost entirely cargo-mutants generating a syntactically valid
but non-compiling mutation (e.g. a type mismatch from swapping a return value) — normal noise
for a codebase this size, no action needed per cargo-mutants' own guidance.

### CI ratchet (`.github/workflows/ci.yml`, job `mutants`)

Floors are this baseline's caught ratio, rounded down to the nearest 5%:

| Crate | Floor |
|---|---|
| kadou-core | 80% |
| kadou-exec | 75% |
| kadou-mcp | 55% |

Raise a floor only after re-measuring a real improvement; never lower one just to make a red
build pass. Note `kadou-mcp`'s floor is noticeably lower than the other two — see the
`tools.rs` breakdown below (52 of its 71 misses) before assuming this crate is just weaker
across the board.

### A caveat on `kadou-core`'s number: `#[cfg(feature = "keyring")]` dead code

17 of `kadou-core`'s 68 missed mutants are inside `#[cfg(not(feature = "keyring"))]` bodies
(`vault.rs` lines 213, 218, 292, 334, 344, 592, 599, 604, 610, 618, 625, 636) — stub functions
whose entire body is `unreachable!(...)` or a hardcoded `false`, compiled in because this
baseline built with default features only (`keyring` is off by default, §9 slice 4). Mutating
already-unreachable or already-constant code can't be "caught" by any test, because the
mutated code never runs — this is a known cargo-mutants limitation with `#[cfg(feature)]`
gating, not a real test gap. The real signal for that code needs a separate run with
`cargo mutants -p kadou-core --features keyring`, which this baseline does not include.
Excluding these from the numerator/denominator would raise kadou-core's true ratio to
280/332 = 84.3%, but the CI floor below uses the as-measured 80.2%, per the assignment
("start at the baseline you measured"). One `kadou-exec` mutant (`terminate`'s
`#[cfg(not(unix))]` fallback, line 379) is the same kind of artifact — dead code on the macOS
runner that only exists on Windows.

## Full list of missed mutants, grouped by module

Each line is `N missed — file:line function — reading`. "Reading" is what the miss reveals
about current test coverage, not an instruction to write that exact test (see the ranked list
below for that).

### `kadou-core`

**`check.rs`** (5 missed) — the cargo-shaped diagnostic renderer has no test on its actual
rendered output at all; `crates/kadou-core/tests/header_fixtures.rs` snapshots the raw
`Diagnostic` struct list via `{:?}`, never `render_report`/`render_diagnostic`'s formatted
text (arrows, gutter, carets, message).
- 1 — `check.rs:229` `cross_kata_diagnostics` (`<`→`>`) — no test with folder-name ordering that would distinguish which of two folders "wins" a duplicate-need/alias diagnostic.
- 1 — `check.rs:278` `render_report` (`&&`→`||`) — no verbose-mode test with a *well-formed* kata (has a header) to confirm the "ignored: ..." line is suppressed for it.
- 1 — `check.rs:318` `render_diagnostic` (whole fn → `()`) — confirms the point above: nothing asserts the rendered diagnostic text/location/snippet/carets at all.
- 2 — `check.rs:345` `display_path` (whole fn → `String::new()` / `"xyzzy"`) — same: the printed path in a diagnostic's `--> path:line:col` line is never checked.

**`config.rs` / `fsutil.rs`** (3 missed, one pair duplicated across both files)
- 1 — `config.rs:164` `Config::load` (guard → `true`) — no test distinguishes "config file missing" (defaults) from "some other read error" (e.g. permission denied, or a directory at that path); both currently look the same to the tests.
- 1 — `config.rs:206` `write_atomic_0600` / 1 — `fsutil.rs:13` `write_atomic_0600` (guard → `true`) — the module doc for `fsutil.rs` says outright "`config.rs` predates this module and keeps its own copy of the same write-to-temp/fsync/rename pattern"; neither copy has a test for writing to a bare filename with no parent component (empty `parent()`).

**`header.rs`** (25 missed) — the ~200-line hand-written header parser's boundary conditions and compound conditionals are the weak spot, not its happy path.
- 2 — `header.rs:25` `MAX_TIMEOUT` constant (`24 * 60 * 60`, `*`→`+`/`+`... arithmetic) — feeds the timeout-boundary check at line 322; see below, no test is close enough to the 24h boundary to notice the constant's actual value.
- 16 — `header.rs:143,168,233,322,348` `parse_header` — several distinct gaps bundled in one function:
  - `143` (`&&`→`||`, `<`→`<=`): no test for a header at exactly the 64-line (`MAX_HEADER_LINES`) boundary — only "way too long" and "normal" are tested.
  - `168,233` (`+`/`-` arithmetic in column math, several columns): diagnostic *column* positions for value/type errors are never asserted precisely, only the message text.
  - `322` (guard→`false`, `>`→`==`/`>=`): no test sets a `timeout:` value at or past the 24h maximum — the "timeout exceeds the 24h maximum" diagnostic is never triggered.
  - `348` (`>`→`>=`): no test for an `about:` line at exactly 120 chars (`MAX_ABOUT_CHARS`) — only well-under and well-over are tested.
- 2 — `header.rs:412,419` `validate_name` (`&&`→`||`, `||`→`&&`) — the first-char/rest-chars check and the 4-way reserved-name OR are each tested only with names where the individual clauses happen to agree; e.g. a name like `1abc` (bad first char, otherwise-valid tail) would prove the `&&`, and no fixture tries that shape.
- 2 — `header.rs:458,513` `parse_arg_line` (arithmetic in `-`, `||`→`&&`): column math for the type portion of an arg line is unchecked (same pattern as `parse_header`'s column math above); and no `select` option list is tested with an *empty* option (`select a||b`) to hit "invalid select option" via the empty-string branch specifically.
- 2 — `header.rs:559,581` `coerce_default` (`&&`→`||`, `==`→`!=`) — no test for a text default with an unbalanced quote (`"hello` with no closing quote); no test for a `select` default that is *not* one of the declared options (only valid defaults are exercised).
- 1 — `header.rs:690` `render_arg_line` (guard→`true`) — no test for an arg with `help: ""` (present but empty) vs. no help at all.

**`import.rs`** (8 missed)
- 2 — `import.rs:28` `default_script` (whole fn) — no import test omits the `script:` key from `runbook.yaml` to exercise the serde default (`"script.sh"`).
- 1 — `import.rs:331` `convert_arg` (`==`→`!=`) — same shape as `coerce_default` above: no import fixture has a `select`-type dops parameter whose default isn't one of its own options.
- 2 — `import.rs:359` `split_shebang` (`+` arithmetic) — a shebang-stripping off-by-one isn't caught because no test asserts the exact remainder text after the shebang line, byte for byte.
- 1 — `import.rs:408` `copy_dir_recursive` (whole fn → `Ok(())`, i.e. copies nothing) — no import test has non-kata sibling files/directories to copy alongside the converted script.
- 2 — `import.rs:428` `unified_diff` (whole fn → empty string) — `kadou import`'s "prints per-file diffs" (§4.6) output text is never asserted against any expected diff content.

**`resolve.rs`** (1 missed)
- 1 — `resolve.rs:112` `coerce` (delete `"false"` match arm) — a bool arg override is only ever tested with the literal `"true"`; `"false"` falls through to the same `Err` path either way in existing tests.

**`scan.rs`** (3 missed)
- 1 — `scan.rs:49` `ScannedFile::warning_count` (`-`→`+`) — no `ScannedFile` in any test has *both* an error and a warning diagnostic at once, so `len - error_count` and `len + error_count` give the same answer (both zero errors, or all diagnostics are warnings already).
- 2 — `scan.rs:79,97` `scan_kata_dir`/`scan_dir` (`||`→`&&`) — no scan test includes a dotfile- or underscore-prefixed entry (`.git`, `_archive`) to confirm it's actually skipped; every test folder is apparently "clean."

**`vault.rs`** (23 missed, 17 of them the `#[cfg(feature = "keyring")]`-off dead-code artifact noted above)
- 5 real — `vault.rs:55,59,63` `Vault::remove`/`is_empty`/`len` (whole fn → constants) — this in-memory `Vault` struct's own basic accessors have no direct `kadou-core` unit test; they're presumably only exercised indirectly through the `kadou vault` CLI subcommands, which live in the (untestable, see above) `kadou` bin crate.
- 1 real — `vault.rs:470` `GoVaultSource::exists` (`&&`→`||`) — no test has exactly one of `vault.json`/`keys.txt` present without the other; only "both present" and (implicitly) "neither" are exercised.
- 17 artifact — `vault.rs:213,218,292,334,344,592,599,604,610,618,625,636` `VaultStore::with_keyring`/`keyring_enabled`/`load_identity_via_keyring`/`write_identity_via_keyring`/`keyring_backend::*` — see the caveat above; needs a `--features keyring` run to mean anything, not a new test under default features.

### `kadou-exec`

- 2 — `lib.rs:114` `declared_interpreter` (`&&`→`||`, `>`→`>=`) — no test covers a bare `#!/usr/bin/env` shebang with *nothing* after `env` (`words.len() == 1`); every `env`-wrapped fixture supplies an interpreter name.
- 5 — `lib.rs:339,344` `exit_code_of` (delete `-`, whole fn → `None`/`Some(0)`/`Some(1)`/`Some(-1)`) — the SIGTERM/SIGKILL tests (`timeout_terminates_a_sleeping_kata_via_sigterm` etc.) check *timing*, never the resulting `RunOutcome.exit_code` — the signal-derived negative exit code is computed but never asserted.
- 1 — `lib.rs:379` `terminate` (whole fn → `()`) — the `#[cfg(not(unix))]` fallback; dead code on this (macOS) CI runner, per the caveat above.

### `kadou-mcp`

**`tools.rs`** (52 missed) — by far the largest cluster, concentrated in a few functions.
- 22 — `tools.rs:758,761,766,768,769` `truncate_output`, and `tools.rs:778,781,784` `last_non_empty_line` — **no `run_kata` test produces output big enough to hit either cap.** `truncate_output`'s line-count cap, its `MAX_OUTPUT_BYTES` (8192) byte cap, and the char-boundary walk-back are all untested; `last_non_empty_line`'s 200-character truncation of the summary line is untested. This is the single biggest concentration of misses in the whole run.
- 9 — `tools.rs:25(top_folder),177,186,201,212` `list_kata` — the draft-folder filter (`include_drafts` / `top_folder` matching), the `risk` filter (`rows.retain`), and the pagination `truncated` flag (`offset + page.len() < total`) are each under-exercised: no test pages through more results than fit in one page, filters by a folder that partially matches, or combines `risk` with drafts.
- 18 — `tools.rs:22(SOURCE_CAP_BYTES const),264,285,291(sibling_files),331(build_describe_value),338,339,343,344(read_source_capped),352(build_args_schema),391(default_to_json)` — `describe_kata`'s three optional-detail paths are each essentially untested: sibling helper files in a folder (e.g. Sesami's shared `trigger-pipeline.sh`), a source file over the 16KB (`SOURCE_CAP_BYTES`) cap, and an arg with a non-`Text` default (`Int`/`Bool`/`Select`) — `build_args_schema`/`default_to_json`'s per-type branches are never distinguished in an assertion.
- 3 — `tools.rs:415,416,417` `json_args_to_strings` — `run_kata` is apparently only ever tested with string-typed JSON args; `Number` and `Bool` args are accepted by the code but no test confirms they convert correctly.

**`server.rs`** (8 missed)
- 6 — `server.rs:183-187` `parse_risk` (whole fn, delete each match arm) — **`list_kata`'s `risk` filter argument is never exercised end-to-end** — every arm can be deleted independently without a test noticing, meaning no test ever passes a `risk` value through the MCP JSON args at all.
- 1 — `server.rs:37` `serve_stdio` (whole fn → `Ok(())`, i.e. does nothing and returns success) — untested by design: the e2e test drives the server over an in-process duplex, not real stdio. Lower priority; this is thin transport wiring around `rmcp`, not product logic.
- 1 — `server.rs:47` `get_info` (whole fn → `Default::default()`) — the e2e duplex test does an MCP `initialize` handshake but never asserts the returned server name/version/capabilities match what `get_info` actually returns.

**`drafts.rs`** (2 missed)
- 2 — `drafts.rs:71` `render_diagnostics` (whole fn → `String::new()`/`"xyzzy"`) — `propose_kata`'s `BadHeader` error path is exercised (a bad header is rejected) but the *message text* it reports (joined error diagnostics) is never asserted.

**`history.rs`** (5 missed)
- 2 — `history.rs:51` `now_rfc3339`, 2 — `history.rs:57` `current_initiator` (whole fn) — a written history record is checked for existence/structure but its `timestamp`/`initiator` field *values* are never asserted (timestamp is inherently time-dependent and lower priority to pin exactly; `initiator` is deterministic under a controlled `$USER`/`$USERNAME` and worth asserting).
- 1 — `history.rs:74` `HistoryStore::logs_dir` (whole fn → `Default::default()`) — the exact `<state_dir>/history/logs` path is never asserted, only that *some* log gets written.

**`redact.rs`** (3 missed)
- 1 — `redact.rs:23` `redact_all` (`<`→`<=`) — no test with a secret at exactly the 8-character (`MIN_SECRET_LEN`) boundary.
- 2 — `redact.rs:47,50` `percent_encode` (whole fn, delete the safe-char match arm) — **the URL-encoded redaction variant (one of the three §6.6 requires) is never exercised** — no test's secret contains a character that needs percent-encoding.

**`visibility.rs`** (1 missed)
- 1 — `visibility.rs:79` `discover_project_local` (`==`→`!=`) — no test walks up to *exactly* `$HOME` without finding `kata/` or `.git` first; the "stop at `$HOME`" boundary itself is untested (only "stops at `.git`" and "finds `kata/`" are).

## Ranked test additions (closes the most misses / highest-value first)

Per the assignment, these are not to be implemented in this slice — TDD, next slice.

1. **`run_kata` output-truncation coverage** (`kadou-mcp/tools.rs::truncate_output`,
   `last_non_empty_line`) — closes ~22 misses with 1-2 tests: run a kata whose stdout exceeds
   both `max_output_lines` and the 8192-byte cap, and whose last non-empty line exceeds 200
   characters.
2. **`describe_kata`'s optional-detail paths** (`sibling_files`, `read_source_capped`,
   `build_args_schema`/`default_to_json`) — closes ~18 misses with 3 tests: a folder with a
   headerless sibling file, a kata source file over 16KB, and an arg with an `Int`/`Bool`/
   `Select` default.
3. **`list_kata` pagination, draft-folder, and risk filtering** — closes ~9 misses with 2-3
   tests: more results than one page, a folder filter that should exclude some drafts, and a
   risk-level filter.
4. **`parse_risk` / `list_kata` risk argument end-to-end** (`kadou-mcp/server.rs`) — closes 6
   misses with 1 test: call the `list_kata` tool with an explicit `risk` argument over the
   in-process duplex and confirm only matching kata come back.
5. **`kadou check`'s rendered diagnostic output** (`kadou-core/check.rs::render_report`/
   `render_diagnostic`/`display_path`) — closes 5 misses but is disproportionately important:
   nothing currently checks that `kadou check`'s actual printed text (location, source
   snippet, carets, fix line) is correct — only the underlying `Diagnostic` data is snapshotted.
6. **`header.rs` boundary values**: a `timeout:` at/over 24h (also pins down the
   `MAX_TIMEOUT` constant itself), an `about:` at exactly 120 chars, and a header at exactly
   64 lines — closes ~8 misses with 3 fixtures alongside the existing
   `tests/fixtures/headers/{good,bad}/` corpus.
7. **`header.rs` name-validation compound conditions** (`validate_name`'s AND and the
   reserved-name OR chain) — closes 2 misses with 2 fixtures: a name with a bad first
   character but an otherwise-valid tail (e.g. `1abc`), isolating each disjunct of the
   reserved-name check.
8. **`Vault` and `GoVaultSource` direct unit tests** (`kadou-core/vault.rs`) — closes 6 misses
   with cheap `kadou-core`-only tests for `remove`/`is_empty`/`len` and a partial (one-of-two-
   files) `GoVaultSource::exists` — bypasses the broken `kadou` CLI test harness entirely.
9. **`exit_code_of`'s signal-derived exit code** (`kadou-exec/lib.rs`) — closes 7 misses:
   extend the existing SIGTERM/SIGKILL tests to assert `RunOutcome.exit_code` is the expected
   negative signal number, not just the elapsed time; add a bare `#!/usr/bin/env` (no
   interpreter name) fixture for `declared_interpreter`.
10. **`import.rs` gaps**: a `runbook.yaml` missing the `script:` key, a `select`-type
    parameter whose default isn't in its options, an exact-content assertion on the
    shebang-stripped remainder and the printed diff text, and extra sibling files/directories
    alongside the converted script — closes ~8 misses with 5 tests.

Not ranked (lower value or needs different tooling, not "closes the most misses" material):
`redact.rs::percent_encode`'s URL-encoding path (3 misses, real gap, but narrow); `history.rs`
field assertions (5 misses, partly time-dependent); `server.rs::get_info` assertion in the
existing duplex test (2 misses, cheap but small); the 18
`#[cfg(feature = "keyring")]`/`#[cfg(not(unix))]` dead-code artifacts (need a separate
`--features keyring` run, not a new test, per the caveat above); `server.rs::serve_stdio`
(thin `rmcp` transport wiring, hard to test without a real stdio subprocess, low value).

## 2026-09-12 re-measurement (slice B refactor)

Measured on `sd/dops/refactor-b` after the slice B refactor (R2/R6/R7/R11/R12/R13, the lint
rollout, and the mutation-baseline ranked test additions #1-2 and #4-10 above), with the same
`cargo-mutants` v27.1.0 and `.cargo/mutants.toml`. This section is additive — it does not
replace the 2026-09-11 baseline above, which stays as the historical record of where the
project started.

```
cargo mutants --workspace -j 4
```

Ran on the same 18-core Apple Silicon Mac at `-j 4`. **925 mutants tested in ~18 minutes**:
662 caught, 117 missed, 144 unviable, 2 timeouts (`header.rs::find_closer`,
`tools.rs::gate` — both ordinary "mutant makes the code loop/hang" timeouts, not test
infrastructure problems). No sharding needed.

### Known blocker resolved: `kadou` is now measured

R15 (refactor A, already landed on `main` before this slice started) moved `crates/kadou`'s
`assert_cmd`-based subprocess tests out of `src/main.rs`'s own unit-test module and into
`crates/kadou/tests/cli.rs`, where `CARGO_BIN_EXE_kadou` is actually set. `cargo test
--workspace` passes cleanly now, so `.cargo/mutants.toml` no longer needs to exclude
`crates/kadou/**` — only `crates/kadou-mine/**` (still an empty stub) and `kadou`'s own
`fn main` (never exercised by `cargo test`, only by the built binary) are excluded. `kadou` is
mutation-tested here for the first time: 92 viable mutants, 75 caught, 17 missed.

### Per-crate counts

| Crate | Total | Caught | Missed | Timeout | Unviable | Caught ratio (of viable) |
|---|---|---|---|---|---|---|
| kadou-core | 431 | 371 | 59 | 1 | — | 371/431 = **86.1%** |
| kadou-exec | 33 | 27 | 6 | 0 | — | 27/33 = **81.8%** |
| kadou-mcp | 225 | 189 | 35 | 1 | — | 189/225 = **84.0%** |
| kadou | 92 | 75 | 17 | 0 | — | 75/92 = **81.5%** |
| **Total** | **781** | **662** | **117** | **2** | **144** | 662/781 = **84.8%** |

(`unviable` isn't split per crate above the same way the 2026-09-11 table did — the remainder
of each crate's total mutant count that isn't caught/missed/timeout is unviable; the workspace
row's 144 is the sum. Same "caught / (caught + missed + timeout)" definition as before.)

Every measured crate improved over the 2026-09-11 baseline, and `kadou` is now measured at
all: kadou-core 80.2%→86.1%, kadou-exec 79.5%→81.8%, kadou-mcp 57.3%→84.0% (the ranked test
additions #1-4 above specifically targeted `kadou-mcp/tools.rs`'s `truncate_output`/
`last_non_empty_line`/`list_kata`/`describe_kata` clusters, which were most of its old gap).

### CI ratchet (`.github/workflows/ci.yml`, job `mutants`)

Floors are this re-measurement's caught ratio, rounded down to the nearest 5%:

| Crate | Old floor (2026-09-11) | New floor (2026-09-12) |
|---|---|---|
| kadou-core | 80% | 85% |
| kadou-exec | 75% | 80% |
| kadou-mcp | 55% | 80% |
| kadou | (unmeasured) | 80% |

### Remaining misses, by crate

**`kadou` (17 missed, all `commands.rs`)** — thin CLI plumbing that's exercised through
integration tests for its happy paths but not every branch: `list_rows`/
`list_rows_for_folder`'s boundary comparisons and risk-equality checks, `run_show`'s
secret-need masking guard, `grant_status_label`'s expiry guard, `validate_pending_record`'s
mismatch check, and three whole-function mutants on `materialize_starter_for_bare_invocation`/
`auto_import_go_vault`/`check_all` (side-effecting or filesystem-touching paths that are
awkward to assert on without a heavier integration fixture). None of these touch a safety
invariant; they're `--verbose`/edge-case output-shape gaps, same character as the 2026-09-11
`kadou-mcp` list/describe gaps.

**`kadou-core` (59 missed)** — 17 of the 59 are the same `#[cfg(feature = "keyring")]`
dead-code artifact as the 2026-09-11 baseline (`vault.rs`, keyring off by default; excluding
them gives a true ratio of 371/414 = 89.6%, but the floor above uses the as-measured 86.1%,
same policy as before). Of the 42 real misses: `header.rs` still has the largest cluster (15,
mostly `parse_keys`'s column-arithmetic and a few compound-condition guards in
`is_valid_id_segment`/`parse_type`/`coerce_default`/`render_arg_line` — narrower than the
2026-09-11 baseline's 25 after the boundary-value tests added there, but the exact-arithmetic
column math was never targeted); `history.rs` (5, unchanged from before — timestamp/initiator
field values and the exact `logs_dir` path still aren't asserted); `redact.rs` (5, the
`percent_encode` URL-encoding path noted as low-priority before, plus a new `<=` boundary at
`redact_all`'s `MIN_SECRET_LEN` check introduced by R2's move into this crate); `config.rs`
(5) and `scan.rs` (4, one new: `classify_entries`'s dotfile-skip check, from the R7 split of
`scan_dir`); `import.rs`, `runner.rs`, `vault.rs`'s non-keyring accessors (5, unchanged —
`Vault::remove`/`is_empty`/`len` still have no direct unit test), `resolve.rs`, and
`fsutil.rs` (1-3 each).

**`kadou-mcp` (35 missed)** — down from 71 after ranked additions #1-4 landed
(`truncate_output`/`last_non_empty_line`/`list_kata`/`parse_risk` are now well covered).
Remaining: `tools.rs` (17, mostly the R7 split's new seams — `draft_rows`'s dedup guard,
`read_source_capped`'s byte-counter arithmetic, `json_args_to_strings`'s `Number`/`Bool`
match arms — plus `truncate_output`'s exact-boundary `>` vs `>=` at the cap itself, one step
narrower than the gap the ranked test closed); `pending.rs` (7, new surface from R11 —
`short_pending_id`'s whole-function mutants and `args_hash`'s length-prefix arithmetic,
`is_expired`'s boundary, and `delete`'s not-found guard); `notify.rs` (5, unchanged — the
grant-notification text and its `bool` return aren't asserted); `drafts.rs` (3) and
`server.rs` (2, `serve_stdio`/`get_info`, unchanged, still low-value per the note above).

**`kadou-exec` (6 missed)** — 5 of 6 are the `#[cfg(not(unix))]` dead-code artifact
(`exit_code_of`'s non-Unix fallback, `terminate`'s non-Unix fallback) confirmed by direct
inspection to be a different code path than the `#[cfg(unix)]` branch the new self-signaling
test (`a_kata_that_signals_itself_reports_the_negative_signal_as_its_exit_code`) exercises —
same kind of artifact as `terminate`'s 2026-09-11 miss, just doubled since `exit_code_of`
itself is now also `#[cfg]`-split. The one real miss is the manual `Debug for RunSpec` impl
(added because `RunSpec.redact`'s closure field isn't `Debug`) — its formatted output is never
asserted; low value, `Debug` output isn't part of any contract.

### What this pass did not attempt

Lint rollout phases 2/3 (workspace-wide `pedantic`/`nursery` groups at `warn`, per
`docs/design/11-code-review.md` §6) were out of this pass's time budget after the mutation
top-10 and CI floor work — phase 1 (the `deny` set: `unwrap_used`, `expect_used`,
`too_many_lines`, `cognitive_complexity`, `redundant_clone`, `cast_possible_truncation`,
`cast_possible_wrap`, `cast_sign_loss`) is in place and enforced in CI, but `pedantic`/
`nursery` at `warn` were not added. Left for a follow-up slice.

## 2026-09-12 re-measurement (`kadou`, §9 slice 8 UI/picker/frame/prompt)

Measured on `sd/dops/impl-08` after slice 8 landed the `ui` module (`frame`, `picker`,
`prompt`, `style`) and its own mutation-gap-closing commits (bare-frame helpers, the picker's
run-header glue, `spawn_editor`'s failure path, `render_list`'s exact gutter width). This
supersedes the "slice B refactor" section above's `kadou` row (92 mutants, 75 caught, 17
missed, 81.5%) for CI-floor purposes — that pass predates slice 8's UI code entirely, so its
92-mutant count no longer reflects the crate.

**306 mutants tested: 94.3% caught** (the exact `cargo mutants -p kadou` breakdown is carried
in slice 8's own commit history and handoff, not repeated here — this line exists so the CI
floor below has a citable, dated source). `.cargo/mutants.toml`'s `kadou` exclusions (picker
event loop, raw-terminal/TTY-gated branches, `fn main`) are unchanged from the reasoning
recorded in that file; see the slice 8 handoff for the manual pty (`expect`) transcripts that
cover what `cargo test` structurally cannot reach.

### CI floor

Rounded down to the nearest 5%: `kadou` 94.3% → **90%** (up from the slice-B-refactor
measurement's 80%).

## 2026-09-12 measurement (`kadou-mine`, §9 slice 9 session mining)

First measurement of `crates/kadou-mine` (`docs/design/06-session-mining.md`; previously an
empty stub excluded from `.cargo/mutants.toml` entirely). Measured on `sd/dops/impl-09` with
the same `cargo-mutants` v27.1.0 config, after implementing redact/normalize/cluster/rank/
risk/parse/extract/propose/store/ingest/orchestrate/schedule and the end-to-end pipeline test
against the synthetic fixture cluster.

```
cargo mutants -p kadou-mine -j 4
```

**384 mutants tested in ~5 minutes: 278 caught, 72 missed, 1 timeout, 33 unviable.**
Caught ratio (of viable) = 278 / (278 + 72 + 1) = 278/351 = **79.2%**.

One real correctness bug surfaced during this pass and was fixed before the final measurement:
`redact.rs`'s R14 fail-closed high-entropy check compared the characters *outside* a matched
span against `/`, but `/` is itself one of the characters `HIGH_ENTROPY`'s pattern matches —
any real path (or a `$PATH_1/segment`) long enough to match at all always absorbs an adjacent
slash into the match, so that outside-the-match comparison could structurally never fire. A
benign long path segment was therefore always flagged as a fail-closed leak. Fixed to check
whether the matched text itself contains a slash instead (`git log`: "R14's path guard checks
the match itself, not its neighbors"), with two new tests pinning the corrected behavior.

### Remaining misses, by module

**`risk.rs` (10)** — `tool_risk`'s per-tool match guards (`has("apply")`, `has("push")`,
`has("delete")`, the combined `has("log") || has("status") || has("diff")`, the `"mongo" |
"mongosh"` arm, and two `||`-vs-`&&` swaps in the outer helper) are exercised by the existing
table-driven tests only through their *outcome* (the resulting `RiskLevel`), not through a
test that pins each individual guard failing independently — e.g. no test asserts that
`has("apply")` alone, with every other guard held constant, is what selects `High`. Would need
per-branch isolation tests; left for a follow-up given every existing risk-level assignment is
already covered by at least one passing case.

**`store.rs` (5)** — `state_path`/`redaction_failures_path` (2, accessors on `MineHome` this
slice's code never calls — `state.json`/`redaction-failures.jsonl` are written by
`orchestrate.rs` through ad hoc paths rather than these two methods, an inconsistency worth
fixing in a follow-up so they're both used and asserted), `now_rfc3339`'s exact format (2,
`append_audit`'s timestamp value is stored but no test parses it back and checks it round-
trips as RFC3339), and `reject`'s already-decided guard's exact boolean shape (1, covered
in outcome by `reject_records_a_reason_and_bans_future_approval`/`approving_twice_is_refused`
but not by a test isolating `==`/`||` on that one line).

**`orchestrate.rs` (14)** and **`normalize.rs`/`cluster.rs`/`propose.rs`/`schedule.rs` (1-5
each)** — mostly arithmetic-constant mutants on bound/timeout constants
(`MAX_WALL_TIME`/`MAX_BYTES_SCANNED`/`LOCK_STALE_AFTER`) and the lock file's own
create/remove/staleness helpers, which the pipeline integration test's single-process,
sub-second runs never exercise close to their real thresholds (a deliberately unexercised
safety margin, not a logic gap the fixture can reach without mocking `SystemTime`), plus a
handful of `+`/`*` swaps in per-byte/per-second unit-constant expressions and `RunSummary`
field-deletion mutants that no test asserts the *absence* of a field on non-error paths.

### CI floor

Rounded down to the nearest 5%: `kadou-mine` **75%** (measured 79.2%).

## 2026-09-12 re-measurement (`docs/design/12-mvp-review.md` §6 fix slice, all five crates)

First full `--workspace` run since B6/Later-4 sharded the CI `mutants` job (`a08c652`) and
Later-10 added the six named regression tests (git `push --force` -> High, `propose_kata`'s id/
source caps, `is_expired`'s deadline boundary, one golden `rank::score`, the `already_running`
lock case, `redaction-failures.jsonl`'s format) plus the four typed-arg wire cases the dead
`sd/dops/fix-mvp` worker left uncommitted. Also the first run under `.cargo/mutants.toml`'s new
`notify.rs:.*send_with` exclusion.

```
cargo mutants --workspace -j 4
```

**1604 mutants tested in 62 minutes: 1241 caught, 162 missed, 3 timeouts, 198 unviable.**

| Crate | Caught/Viable | % | Floor | Status |
|---|---|---|---|---|
| `kadou-core` | 431/484 | 89.0% | 85% | pass |
| `kadou-exec` | 29/33 | 87.9% | 80% | pass |
| `kadou-mcp` | 191/217 | 88.0% | 80% | pass |
| `kadou` | 277/301 | 92.0% | 90% | pass |
| `kadou-mine` | 313/371 | 84.4% | 75% | pass |

Every crate clears its CI floor with room to spare; no floor changes needed.

The four typed-arg wire tests confirmed their own target directly: `json_args_to_strings`'s
`Value::String`/`Value::Number`/`Value::Bool` match-arm deletions (`kadou-mcp/src/tools.rs:454-
456`) are all now in `caught.txt`, verified with a targeted `cargo mutants -p kadou-mcp -f
src/tools.rs` before this workspace run and confirmed again here.

### Remaining misses, by crate (unchanged categories, not blocking)

**`kadou-core` (52)** — concentrated in `vault.rs` (17) and `header.rs` (15), the two files
`docs/design/12-mvp-review.md` §8 notes were "last read in full... at `89a30c4`" with only
targeted diffs since; `config.rs` (5), `history.rs` (4), `scan.rs`/`runner.rs` (3 each),
`import.rs` (2), `resolve.rs`/`fsutil.rs`/`check.rs` (1 each) — mostly error-path branches
(`io::ErrorKind::NotFound` guards in `Config::load`/`Config::edit`) that the test suite's
happy-path fixtures don't independently isolate.

**`kadou-exec` (4)** — `lib.rs` only; unchanged from prior measurements.

**`kadou-mcp` (25)** — `tools.rs` (11, mostly unrelated to the four newly-caught arms above),
`pending.rs` (7, down from before Later-10's TTL-boundary/fail-safe tests closed two of the
prior gaps), `notify.rs` (4 — `notify`, `spawn_scoped`, and `send` itself, *not* `send_with`,
which the new exclusion narrows out on purpose), `server.rs` (2), `visibility.rs` (1).

**`kadou` (24)** — all in `commands.rs`; a mix of the same TTY-gated/interactive branches
`.cargo/mutants.toml` already excludes by line and a handful of ordinary error-formatting
branches (`load_redact_extra`, `run_mine_install_schedule`'s exit-status guard) the CLI test
suite's happy-path assertions don't isolate individually.

**`kadou-mine` (57)** — `orchestrate.rs` (17, same lock/bound-constant category as the prior
`kadou-mine` measurement above), `rank.rs` (10) and `risk.rs` (9) newly measured in detail by
this pass's own additions (the golden-score and git-push-force tests each caught their own
named mutant but left sibling arithmetic-operator swaps and match-guard flips in the same
functions unpinned — e.g. `score`'s `*`/`/` swaps at line 49, `tool_risk`'s `has("apply")`/
`has("push")`/`has("delete")` guard-flip pairs), `cluster.rs` (6), `store.rs`/`normalize.rs` (5
each), `schedule.rs`/`redact.rs` (2 each), `propose.rs` (1, a `TIMEOUT` on `replace_placeholder`
rather than a real miss).

### CI floor

Unchanged — all five floors already pass at their existing values (`kadou-core` 85%,
`kadou-exec` 80%, `kadou-mcp` 80%, `kadou` 90%, `kadou-mine` 75%); no floor was raised or
lowered by this measurement.
