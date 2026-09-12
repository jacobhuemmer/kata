# Session mining for dops-next

**Status:** design  
**Date:** 2026-09-11  
**Scope:** scheduled skill + CLI that turns repeated agent shell work into candidate runbooks, behind a human review gate  
**Non-goals:** wiki ingest, executing mined scripts, reading production clusters, walking session artifact dumps

Agents already write the same `kubectl`, `helm`, `git`, and POSIX one-liners across sessions. Session mining reviews local archives, extracts those commands and generated scripts, clusters near-duplicates, redacts secrets, and queues draft `runbook.yaml` + `script.sh` pairs for a human to approve, edit, or reject. Nothing enters a dops-next catalog, and nothing is exposed over MCP as an executable tool, until that gate passes.

This document is the contract. Implementation comes later. The survey below used metadata, headings, JSON keys, and file sizes only. No archive body text, no command strings, and no customer data are copied here.

---

## 1. Archive survey

Two trees exist. They must not be treated as the same corpus.

| Corpus | Path | What it is | Survey size (2026-09-11) |
|--------|------|------------|--------------------------|
| Session ledger | `~/Documents/Sessions/` | Hook archives + curated ledgers + **operator-copied artifacts** | ~3,060 dirs, ~85,700 files, **~1.46 GB** |
| Event markdown | `~/Documents/Sessions/<dir>/events/*.md` | One file per hook event | **~3,844 files, ~15.3 MB** |
| Index | `~/Documents/Sessions/index.jsonl` | One JSON object per event | **~3,846 lines, ~2.0 MB** |
| Native transcripts | `~/.claude/projects`, `~/.codex/sessions`, `~/.grok/sessions`, `~/.cursor` | Where shell/tool payloads actually live | Claude ~147 MB; Codex ~2.76 GB + ~169 MB archived; Grok ~1.02 GB; Cursor jsonl present |

The 1.46 GB figure is the whole Sessions tree. Almost all of that mass is **not** event markdown: JSON dumps, logs, binaries, and other artifacts operators dropped into session folders. Mining **must not** walk those files. It reads `index.jsonl`, the `events/*.md` files those rows point at, and native transcripts named in hook metadata.

### 1.1 Directory shapes

```
~/Documents/Sessions/
  index.jsonl
  .sessions.json          # agent|sessionId|project → session dir
  .dedupe                 # hook-level 1-minute dedupe keys
  <yyyyMMdd-HHmmss>-<agent>-<project>-<sessionId>/
    SOURCES.md
    PAGES.md
    CLAIMS.md
    ACTIVITY.md
    events/
      <agent>__<project>__<sessionId>__<event>__<timestamp>.md
```

| Kind | Count | Notes |
|------|------:|-------|
| Session directories | ~3,060 | Includes ledger-only dirs |
| Dirs with `events/` | ~2,847 | |
| Dirs with ledger files (`SOURCES`/`PAGES`/`CLAIMS`/`ACTIVITY`) | ~3,052 | Almost every dir |
| Ledger-only (no `events/`) | ~213 | Curated session ledgers; **out of mining ingest** |
| Unique `sessionId` values in event filenames | ~2,814 | |
| Distinct project slugs in filenames | 31 | Not listed here |

Ledger files are the wiki-ingest contract (local Sources / Pages / Claims / Activity). They are not a command corpus. Adjacent wiki workflow: “OKF Session Ingest Skill” (Sesami wiki, Draft) promotes session lessons into wiki pages. That is knowledge ingest, not runbook mining. Session mining does not write the wiki.

### 1.2 Event types and agents

Index window: **2026-07-11 → 2026-09-11**.

| Agent | Events | Share |
|-------|-------:|------:|
| `claude` | ~2,691 | ~70% |
| `grok` | ~600 | ~16% |
| `codex` | ~553 | ~14% |
| `cursor` | 0 | Hook is wired; no events in this corpus |
| `unknown` | 0 | |

| Event | Count | Who emits it |
|-------|------:|----------------|
| `session_end` | ~2,758 | Claude (~2,663), Grok (~95) |
| `stop` | ~472 | Grok only (historical; current hook README says Stop is not wired because it fires every turn) |
| `pre_compact` | ~308–309 | Codex (~278), Grok (~16), Claude (~14) |
| `post_compact` | ~305 | Codex (~275), Grok (~16), Claude (~14) |

Event markdown size: min ~1.0 KB, median ~1.4 KB, p90 ~11 KB, max ~14.6 KB, mean ~4.0 KB. About **1,620 / 3,844** summaries are the empty placeholder. Median summary body is ~64 characters. **Command mining cannot use the markdown summary as the primary source.**

Writer: `~/.local/bin/agent-session-archive` → `~/.local/share/agent-session-archive/agent-session-archive.mjs`. Hooks:

| Agent | Config | Events hooked today |
|-------|--------|---------------------|
| Claude | `~/.claude/settings.json` | PreCompact, PostCompact, SessionEnd |
| Codex | `~/.codex/hooks.json` | PreCompact, PostCompact, SessionEnd |
| Cursor | `~/.cursor/hooks.json` | preCompact, sessionEnd |
| Grok | `~/.grok/hooks/session-archive.json` | PreCompact, PostCompact, SessionEnd |

### 1.3 Shared event markdown (all agents)

Every `events/*.md` file uses the same skeleton. Extra headings under `## Summary` are compaction-summary bleed (instruction files, turn markers), not a second schema.

```markdown
# Agent session archive — <event>

- **When:** <iso-8601>
- **Agent:** claude | codex | cursor | grok
- **Event:** pre_compact | post_compact | session_end | stop
- **Session:** `<id>`
- **Project:** <cwd basename>
- **CWD:** `<path>`
- **Trigger:** <optional>
- **Transcript:** `<native transcript path>`

## Summary
<redacted preview, or empty placeholder>

## Local sources
- `<path>`   # transcript and/or grok session dir

## Hook metadata
```json
{ /* see sketch below */ }
```
```

Hook-metadata JSON keys (present on all ~3,844 files): `agent`, `event`, `sessionId`, `cwd`, `project`, `trigger`, `transcriptPath`, `payloadKeys`. Later files also have `env`, and Grok/Cursor-shaped files add `cursorVersion`, `generationId`, `grokHookEvent`, `grokSessionId` (often null).

Index JSONL row (one object per line):

```json
{
  "when": "string, ISO-8601",
  "agent": "claude | codex | cursor | grok",
  "event": "pre_compact | post_compact | session_end | stop",
  "sessionId": "string",
  "project": "string, cwd basename",
  "cwd": "string, absolute path",
  "trigger": "string | null",
  "sessionDir": "string, path under Sessions/",
  "path": "string, path to events/*.md",
  "bytes": "number, markdown byte length"
}
```

`payloadKeys` recorded in hook metadata (union across the corpus, names only):

| Shape | Keys |
|-------|------|
| Claude-like (~3,259) | `hook_event_name`, `session_id`, `transcript_path`, `cwd`, `reason`; often `prompt_id` / `permissionMode` / `stopHookActive` |
| Codex (~553) | `model`, `trigger`, `turn_id`, plus shared cwd/session/transcript keys |
| Grok/Cursor-like (~653) | `hookEventName`, `sessionId`, `timestamp`, `transcriptPath`, `workspaceRoot` |

### 1.4 Where commands actually live

| Agent | Archive pointer | Native store | Command / script location | Pointer still on disk |
|-------|-----------------|--------------|---------------------------|-----------------------|
| Claude | `transcriptPath` → `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl` | ~75 jsonl files, ~147 MB under `projects/` | `message.content[]` where `type=tool_use` and `name=Bash`; `input.command` (+ `description`, optional `timeout`, `run_in_background`). Also `attachment.command` / `toolUseResult`. `Write` to `*.sh` is a second channel. | **~38 / 2,691 exist (~1%)** |
| Codex | `transcriptPath` → `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<id>.jsonl` | ~1,933 jsonl, **~2.76 GB**; plus ~17 archived (~169 MB) | JSONL envelope `{timestamp, ordinal, type, payload}`. Command bodies: `payload.item` with `type=CommandExecution`, fields `command`, `parsed_cmd[].cmd`, `stdout`, `stderr`. Also `payload.type=function_call` / `custom_tool_call`. | **~549 / 553 exist (~99%)** |
| Grok | `transcriptPath` often `.../updates.jsonl`; sources also include the session dir | `~/.grok/sessions/<encoded-cwd>/<sessionId>/` (~1.02 GB): `events.jsonl`, `chat_history.jsonl`, `updates.jsonl`, compaction segments | `chat_history.jsonl`: `tool_calls[].name=run_terminal_command`, `arguments.{command,description,timeout}`. `events.jsonl`: `tool_started` / `tool_completed` with `tool_name` (no command body). Compaction markdown is a weak secondary. | **~360 / 601 exist (~60%)** |
| Cursor | none in this corpus | `~/.cursor/**/*.jsonl` (agent transcripts) | `message.content[]` with `name=Shell` or `execute`; `input.{command,description,working_directory,block_until_ms}` | n/a (no archive rows) |

**Implication:** scheduled mining follows `index.jsonl` → event markdown → `transcriptPath` (and Grok session dir). It skips missing transcripts. Expected yield on this machine, today: Codex-heavy, Grok medium, Claude near-zero until jsonl retention improves, Cursor zero until hooks produce events.

### 1.5 Redacted schema sketches (native)

Values omitted on purpose. These are key paths the parser binds to.

**Claude jsonl line (tool call):**

```json
{
  "type": "assistant | user | attachment | …",
  "sessionId": "string",
  "timestamp": "string",
  "cwd": "string",
  "message": {
    "role": "assistant | user",
    "content": [
      {
        "type": "tool_use | text | tool_result | …",
        "name": "Bash | Write | …",
        "input": {
          "command": "string",
          "description": "string",
          "timeout": "number?",
          "run_in_background": "boolean?"
        }
      }
    ]
  },
  "toolUseResult": { "stdout": "string", "stderr": "string" }
}
```

Extract: `Bash.input.command` and `Write.input` when the path ends in `.sh`. Drop `stdout` / `stderr`.

**Codex jsonl line (command execution):**

```json
{
  "timestamp": "string",
  "ordinal": "number",
  "type": "response_item | event_msg | session_meta | …",
  "payload": {
    "type": "item_completed | function_call | custom_tool_call | …",
    "item": {
      "type": "CommandExecution | FileChange | McpToolCall | …",
      "command": "string",
      "parsed_cmd": [{ "cmd": "string" }],
      "stdout": "string",
      "stderr": "string"
    }
  }
}
```

Extract: `item.command` and `parsed_cmd[].cmd` when `item.type=CommandExecution`. Drop stdout/stderr.

**Grok `chat_history.jsonl` line:**

```json
{
  "type": "assistant | user | tool_result | reasoning | system",
  "tool_calls": [
    {
      "id": "string",
      "name": "run_terminal_command | read_file | …",
      "arguments": { "command": "string", "description": "string", "timeout": "number?" }
    }
  ],
  "tool_call_id": "string",
  "content": "string | array"
}
```

Extract: `tool_calls` where `name=run_terminal_command`. Do not persist `tool_result` content.

**Cursor jsonl line (from native store; archive events not yet present):**

```json
{
  "role": "assistant | user",
  "message": {
    "content": [
      {
        "type": "string",
        "name": "Shell | execute | …",
        "input": {
          "command": "string",
          "description": "string",
          "working_directory": "string?",
          "block_until_ms": "number?"
        }
      }
    ]
  }
}
```

Hook stdin (Cursor, from the archive test payload; no live events):

```json
{
  "conversation_id": "string",
  "generation_id": "string",
  "hook_event_name": "preCompact | sessionEnd",
  "cursor_version": "string",
  "workspace_roots": ["string"],
  "transcript_path": "string | null"
}
```

**Grok compaction markdown** (copied into some event summaries): headings such as `## Summary (curated by compaction step)`, `## Verbatim turns`, `### Turn N (Human|System)`. Treat as last-resort text; it often contains instruction files, not commands.

### 1.6 What not to mine

- Anything under a session dir other than `events/*.md` (the ~1.45 GB of artifacts).
- Ledger-only directories.
- Tool stdout/stderr, `tool_result` bodies, screenshots, sqlite, `.rdb`.
- Compaction instruction dumps (`AGENTS.md` / `Claude.md` bleed).
- One-off editor commands that are already a dops-next file tool (`Read`, `Write` of non-`.sh`, `ApplyPatch`).

---

## 2. Pipeline

```
index.jsonl
    │
    ▼
 ingest ──► parse ──► extract ──► normalize ──► cluster ──► rank
                                                         │
                                                         ▼
                                              redact ──► propose
                                                         │
                                                         ▼
                                              human review queue
                                              (approve / edit / reject)
                                                         │
                                                         ▼
                                              catalog ingest (approved only)
```

All stages after extract operate on **candidates**, never on raw transcripts. A candidate is a structured record; the original line stays in the native store.

### 2.1 Ingest

1. Open `~/Documents/Sessions/index.jsonl` (or `AGENT_SESSION_LEDGER_DIR`).
2. Skip rows already in the checkpoint (see §3).
3. For each new row, read the event markdown at `path` only to recover `transcriptPath`, `agent`, `event`, `sessionId`, `when`, and Grok session-dir sources.
4. If `transcriptPath` is missing on disk, record `skip: transcript_missing` and continue. Do not search the Sessions artifact tree for a replacement.
5. Bound work per run: max files, max bytes, max wall time. Codex jsonl alone is ~2.8 GB; the miner streams line-by-line.

Idempotent unit: `(path, bytes, mtime)` of the event file, plus a content hash of the native transcript if it exists.

### 2.2 Parse

One adapter per agent. Output is a homogeneous event stream:

```json
{
  "agent": "claude | codex | cursor | grok",
  "session_id": "string",
  "when": "string",
  "kind": "shell | script_file | script_fence",
  "cwd_class": "generic workspace class, not the raw path",
  "source_ref": "opaque pointer: agent + session + ordinal, not a copy of the line"
}
```

Parsers never write the source line to the mine working directory.

### 2.3 Extract

Keep:

| Kind | Source |
|------|--------|
| Shell invocation | Claude `Bash.input.command`; Codex `CommandExecution.command`; Grok `run_terminal_command.arguments.command`; Cursor `Shell`/`execute` `input.command` |
| Generated script file | `Write` / `FileChange` / equivalent whose path ends in `.sh` / `.bash` (body only, after redaction) |
| Fenced script | Assistant markdown fences tagged `sh` / `bash` / `shell` with ≥3 command lines, excluding instruction-file fences |

Drop:

- `git status`, `ls`, `pwd`, `whoami` unless they appear inside a longer sequence
- Commands whose only tokens are the agent’s own tools (`rg` wrappers, `cat` of a single file the agent just wrote)
- Anything matching a secret pattern **before** normalization (fail closed: drop, don’t queue)

Sequences: consecutive shell invocations in the same session with a gap under 2 minutes become one candidate with `steps[]`. That is the interesting automation, not the one-liner.

### 2.4 Normalize

Produce a **template** used for clustering, plus a **parameter map**.

| Raw class | Template token |
|-----------|----------------|
| Absolute home paths | `$HOME` |
| Repo / worktree roots | `$ROOT` |
| Other absolute paths | `$PATH_N` |
| UUIDs, git SHAs, session ids | `$ID` |
| RFC3339 / unix timestamps | `$TS` |
| Integers that are not flags | `$N` |
| Single-quoted / double-quoted values | `$VAL` or a named param if the flag is known (`--namespace` → `$NAMESPACE`) |
| IPv4 / IPv6 | `$IP` |
| Hostnames | `$HOST` |
| Emails | `$EMAIL` |
| kube context / cluster-looking tokens | `$CONTEXT` |
| Ticket ids (`SDO-123`) | `$TICKET` |

Keep flag names and argv0. `kubectl --context prod-us -n payments get pods -l app=foo` becomes `kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP`.

The normalized template is the only form that later stages persist besides the redacted proposal.

### 2.5 Cluster

Fingerprint = SHA-256 of:

1. argv0
2. ordered flag names (not values)
3. normalized subcommands (`get pods`, `helm upgrade`)
4. for sequences: the ordered list of step fingerprints

Near-duplicates: Jaccard similarity of token sets ≥ 0.85, or one template is a prefix of the other (same argv0 + flags, extra `| tail`). Union-find into clusters. Cluster id is the hash of the **medoid** template (most frequent exact fingerprint), stable across runs.

A cluster stores: fingerprint, template, step count, member refs (agent/session/when only), first_seen, last_seen. No raw commands.

### 2.6 Rank

For each cluster:

```
score = log(1 + freq)
      * recency          # exp(-age_days / 30), last_seen
      * unique_sessions  # distinct sessionId
      * unique_agents    # 1 + 0.25 * (agents - 1)
      * sequence_bonus   # 1.3 if steps >= 3
      * catalog_penalty  # 0.1 if an existing runbook already covers argv0+subcommand
```

Default proposal cutoff (tunable): `unique_sessions >= 3` **or** (`freq >= 8` and `unique_sessions >= 2`), and `score` above a floor. Cross-session reuse is required; a noisy single session cannot mint a runbook.

Already-approved or already-rejected fingerprints are excluded (see audit log).

### 2.7 Redact

Run on the template **and** on the draft `script.sh` before either hits disk outside the native archive. Rules and tests: §4. If a candidate still matches a secret pattern after redaction, drop it and write only `{fingerprint, rule_id, when}` to `redaction-failures.jsonl`.

### 2.8 Propose

For each ranked cluster that passes cutoff, write a draft under the mine working dir (not into a live catalog):

```
$DOPS_HOME/mine/queue/<fingerprint>/
  meta.json         # score, freq, sessions, agents, first/last seen, source_refs
  runbook.yaml      # draft, risk_level defaulted
  script.sh         # POSIX, parameters as env
```

`runbook.yaml` follows the existing dops-next schema (`docs/architecture.md` “Runbook Format”, `docs/guides/runbooks.md`):

```yaml
name: <slug-from-argv0-subcommand>
version: 0.0.0-draft
description: "<one line from normalized template; no hostnames>"
risk_level: medium          # see §4.3; miner may raise, never lower below medium for mutating verbs
script: script.sh
parameters:
  - name: namespace
    type: string
    required: true
    scope: catalog
    secret: false
```

`script.sh` uses the create-runbook template: `#!/bin/sh`, `set -eu`, parameters as uppercase env, `main` at the bottom, no copied stdout from sessions.

Heuristic parameter types: flags with closed value sets observed ≥ 3 times become `select`; booleans stay `boolean`; paths become `file_path`; everything else `string`. Anything that looked like a credential is `secret: true`, `scope: local`, no default.

### 2.9 Human review queue

States: `queued` → `approved` | `rejected`, plus `skipped` (never terminal -- a skipped
fingerprint can still be approved or rejected later). There is no separate `edited` state
(amended from the original design; D11, `docs/design/12-mvp-review.md` §3): `docs/design/
09-tui-decision.md` rules out a full-screen TUI in v1, and a walking approve/edit-in-`$EDITOR`
review loop needs one to do well. The cheaper, `09`-compatible shape kadou ships instead: edit
the queued `kata.sh`/`meta.json` files yourself with any editor, then run `kadou mine approve`
-- "edited" is not a state the audit log needs, because the file on disk *is* the edit, and
`approve` records exactly the same `approved` action either way.

CLI (see §5): `kadou mine review` lists the queue and prints the next commands
(`show`/`approve`/`reject`/`skip`) rather than walking it interactively -- also a consequence of
`09`'s no-TUI rule. `kadou mine skip <fingerprint>` records `skipped` and leaves the draft
queued for a later pass; `kadou mine reject <fingerprint> --reason …` bans it.

Approve copies the (possibly edited) pair into a **staging catalog** the operator already added or that `dops mine install-catalog` adds as inactive:

```
$DOPS_HOME/catalogs/mined/<runbook-name>/
  runbook.yaml
  script.sh
```

The staging catalog starts with `active: false` and `policy.max_risk_level: medium`. MCP does not see inactive catalogs. The operator flips the catalog on, or moves individual runbooks into a real catalog, after reading the script.

Reject records the fingerprint so it will not be re-proposed unless `--force` is set.

### 2.10 Catalog ingest

Reuse `dops catalog add` / the disk loader. No new persistence format. Approved runbooks are ordinary runbooks. Skills (`type: skill`) are out of scope for mining.

---

## 3. Schedule, triggers, idempotency

Mining is a **local batch job**. It does not need a cluster, and it does not use Executor, OKE, Jenkins, or Mongo.

### 3.1 Default trigger (macOS)

A user LaunchAgent, installed by `dops mine install-schedule`:

- Label: `dev.dops.mine` (launchd label, not a product name)
- StartCalendarInterval: 03:15 local, daily
- Program: `dops mine run --once`
- Nice / LowPriorityIO: on
- Logs: `$DOPS_HOME/mine/logs/launchd.out.log` (counts and fingerprints only)

Linux: a user crontab line with the same command. dops-next does not ship a systemd unit in v1.

### 3.2 Other triggers

| Trigger | When to use |
|---------|-------------|
| `dops mine run --once` | Manual / LaunchAgent |
| `dops mine run --since <iso>` | Backfill |
| `dops mine run --watch` | Optional: tail `index.jsonl` (fsnotify), debounce 5 minutes, still checkpointed |
| Grok / harness scheduler | Optional overlay (`scheduler_create` interval `1d`) calling the same CLI. Not required. The engine must run without any agent harness. |

Do not hook `Stop`. The archive already learned that lesson.

### 3.3 Checkpoints

```
$DOPS_HOME/mine/state.json
```

```json
{
  "version": 1,
  "index_path": "$HOME/Documents/Sessions/index.jsonl",
  "index_offset": 3846,
  "index_sha256": "hex of file at last successful run",
  "last_when": "ISO-8601",
  "last_run": "ISO-8601",
  "transcripts_seen": 1204,
  "transcripts_missing": 2898,
  "candidates_emitted": 86,
  "clusters": 41
}
```

Plus:

| File | Role |
|------|------|
| `checkpoints/processed.jsonl` | `{event_path, bytes, transcript_sha256?, status}` |
| `clusters.json` | fingerprint → cluster stats (no raw commands) |
| `queue/<id>/` | drafts |
| `audit.jsonl` | review actions |
| `redaction-failures.jsonl` | dropped candidates |

Crash safety: write a new `state.json.tmp` and rename. A killed run resumes from `index_offset`. Re-reading a hashed transcript is a no-op.

### 3.4 Idempotency rules

1. Same event path + bytes → skip.
2. Same transcript sha256 → skip extract.
3. Same cluster fingerprint already in `queue/` in any state, or in `audit.jsonl` as approved/rejected → do not re-queue.
4. Rank/score may be recomputed in place on `meta.json` without creating a new proposal.
5. LaunchAgent + manual overlap: a lockfile `$DOPS_HOME/mine/mine.lock` (flock). Second run exits 0 with `already running`.

### 3.5 Resource bounds (per run)

- Max 20 minutes wall time
- Max 512 MB RSS
- Max 2 GB transcript bytes scanned
- Stream jsonl; never slurp Codex/Grok stores
- After bounds: write checkpoint and exit 0 so the next scheduled run continues

---

## 4. Safety model

Operational gates match chief-of-staff access-gates: this job is local read + local write under `$DOPS_HOME/mine`. It never calls OKE, Jenkins, Mongo, Datadog write APIs, Notion write APIs, or wiki-ingest. Gated remote work that a mined script *might* later perform is irrelevant until a human approves the runbook and then runs it.

### 4.1 Never written to disk outside the native archive / transcript

The miner may **read** these. It may not **copy** them into `$DOPS_HOME`, `/tmp`, the repo, git, MCP responses, logs, or the design artifacts:

- Raw transcript lines
- Unredacted commands or script bodies
- Tool stdout / stderr / `tool_result`
- Event markdown `## Summary` bodies (they can contain instruction files and residual secrets; the archive hook’s redaction is a 3-pattern filter)
- Anything in `~/Documents/Sessions/<dir>/` other than the event markdown path from the index
- Emails, hostnames, customer identifiers, ticket descriptions, kubeconfigs, connection strings
- Screenshots, sqlite, redis dumps, log bundles
- Notion tokens, 1Password references that include secrets, PEM material

Permitted writes (all under `$DOPS_HOME/mine/` unless noted):

- Normalized templates (already path/value stripped)
- Redacted `runbook.yaml` + `script.sh` drafts
- Fingerprints, counts, agent names, session-id hashes (not raw ids in MCP output)
- Audit rows
- Checkpoints

If redaction cannot be proven, the candidate is dropped. Fail closed.

### 4.2 Redaction rules

Layered, in order. Each rule has an id used in tests and failure logs.

| ID | Match | Replacement |
|----|-------|-------------|
| `R1_PEM` | `BEGIN … PRIVATE KEY` blocks | `[REDACTED PRIVATE KEY]` |
| `R2_BEARER` | `Authorization: Bearer …`, `Basic …` | `[REDACTED]` |
| `R3_ASSIGN` | `(api[_-]?key\|token\|secret\|password\|passwd\|authorization)\s*[:=]\s*\S{8,}` | keep key, value `[REDACTED]` |
| `R4_KNOWN_TOKENS` | `sk-`, `ghp_`, `gho_`, `github_pat_`, `xox[baprs]-`, `glpat-`, `AKIA[0-9A-Z]{16}`, `AIza`, `eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.` (JWT) | `[REDACTED]` |
| `R5_CONNECTION` | `postgres(ql)?://`, `mongodb(\+srv)?://`, `redis://`, `amqp://`, `https?://[^/@]+:[^/@]+@` | scheme + `$USER@$HOST/$DB` |
| `R6_EMAIL` | RFC5322-ish `local@domain` | `$EMAIL` |
| `R7_HOSTNAME` | FQDNs not in an allowlist of public docs domains | `$HOST` |
| `R8_IP` | IPv4 / IPv6 | `$IP` |
| `R9_CUSTOMER` | configured denylist of customer slugs / environment names (file `$DOPS_HOME/mine/redact-extra.txt`) | `$CUSTOMER` / `$ENV` |
| `R10_KUBECONFIG` | `certificate-authority-data`, `client-key-data`, `token:` YAML values | `[REDACTED]` |
| `R11_OP_REF` | `op://` URIs | keep vault/item **structure** only if no secret query-string; else `[REDACTED OP]` |
| `R12_HOME` | `/Users/<name>`, `/home/<name>` | `$HOME` |
| `R13_SESSION_QUOTE` | lines that look like chat turns (`Human:`, `Assistant:`, `### Turn`) | drop the line |

The existing archive hook (`redactSecrets` in `agent-session-archive.mjs`) covers a subset of R1/R3/R4 only. Mining does **not** reuse that as sufficient.

Allowlist for R7 (docs domains only): `github.com`, `gitlab.com`, `kubernetes.io`, `go.dev`, `pkg.go.dev`, `developer.hashicorp.com`. Everything else is `$HOST`.

### 4.3 Redaction tests

Table-driven tests in `internal/mine/redact_test.go` (implementation). Inputs are synthetic.

| Case | Input (synthetic) | Expect |
|------|-------------------|--------|
| PEM | a 3-line fake RSA PEM in a heredoc | `R1_PEM`, no `BEGIN` left |
| GitHub PAT | `ghp_` + 36 `A`s | `R4_KNOWN_TOKENS` |
| Slack bot | `xoxb-` + digits | `R4_KNOWN_TOKENS` |
| JWT | `eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig` | `R4_KNOWN_TOKENS` |
| AWS key | `AKIA` + 16 `A`s | `R4_KNOWN_TOKENS` |
| Assignment | `API_TOKEN=s3cretvalue99` | `API_TOKEN=[REDACTED]` |
| Bearer | `Authorization: Bearer abcdefghijklmnop` | `[REDACTED]` |
| Postgres DSN | `postgres://u:p@db.internal:5432/app` | no password, host is `$HOST` |
| Email | `person@example.com` | `$EMAIL` |
| IPv4 | `10.1.2.3` | `$IP` |
| FQDN | `api.customer.example` | `$HOST` |
| Home path | `/Users/someone/src/repo` | `$HOME/src/repo` or `$ROOT` |
| kubeconfig token | `token: k8s-secret-token-value` | `[REDACTED]` |
| `op://` secret | `op://Employee/item/password` | `[REDACTED OP]` or structure-only |
| Negative: kubectl get | `kubectl -n kube-system get pods` | unchanged except namespace may become `$NAMESPACE` |
| Negative: public docs | `https://kubernetes.io/docs` | unchanged |
| Fail closed | `TOKEN=` + 64 random chars that miss R3/R4 but match a high-entropy detector | drop candidate, `redaction-failures` row |
| No leak in logs | run extractor on a fixture with `ghp_AAA…` | log file must not contain `ghp_` |

High-entropy detector (optional R14): strings ≥ 24 chars matching `[A-Za-z0-9+/=_-]{24,}` that are not paths or `--flags` → treat as secret. Tunable; start in report-only then fail closed.

### 4.4 Risk defaulting (before the human)

| Signal in template | Default `risk_level` |
|--------------------|----------------------|
| `kubectl apply/delete`, `helm upgrade/uninstall`, `terraform apply`, `mongo` write-ish, `oci` delete, `docker push` to a registry | `high` |
| `kubectl get/describe/logs`, `helm template/status`, `git log/status/diff` | `medium` (still not `low`: mined scripts are unreviewed) |
| Contains `$` secret params | at least `high`; `secret: true` |
| Unparseable / mixed | `high` |

Miner never assigns `low` or `critical` automatically. `critical` is a human choice.

### 4.5 Review-gate UX

```
kadou mine review
```

Lists every queued proposal: fingerprint, score, freq, unique sessions, agents, first/last seen, redacted template, draft yaml, draft script -- and the next commands to run against a fingerprint (see below). Does **not** show source session bodies or cwd, and does **not** walk the queue interactively (`docs/design/09-tui-decision.md` rules out a full-screen TUI in v1; D11, `docs/design/12-mvp-review.md` §3).

Subcommands (each is its own `kadou mine <verb>`, not a key pressed inside `review`):

| Action | Effect |
|--------|--------|
| `approve <fp> [--into name]` | copy (the possibly hand-edited) `kata.sh` to `mined/`; audit `approved` |
| `reject <fp> --reason …` | audit `rejected`; fingerprint banned from re-queueing |
| `skip <fp>` | audit `skipped`; leave queued, never terminal |
| `review --dump <fp> [--redacted]` | print yaml+sh to stdout (already redacted) |

There is no separate `edit` action: edit the queued draft's files yourself with any editor, then run `approve` -- the file on disk already carries the edit, so the audit log only ever needs to say `approved`.

MCP review tools require the same confirmation fields as high-risk runbooks (`_confirm_id`). Agents can list and read redacted drafts; they cannot approve without the confirm param, and even then the design default is **approve is human-only** (`dops mine review --allow-agent-approve` off).

### 4.6 Audit log

`$DOPS_HOME/mine/audit.jsonl`, append-only, 0600:

```json
{
  "when": "ISO-8601",
  "action": "queued | approved | rejected | skipped | dropped_redaction",
  "fingerprint": "hex",
  "actor": "user | schedule | mcp",
  "reason": "string?",
  "catalog_id": "mined.<name>?",
  "script_sha256": "hex of redacted script"
}
```

No command text. Retention: keep forever locally; do not ship off-box.

### 4.7 Residual risk

- Claude transcripts are ~99% gone; mining will under-count Claude automations. That is a data-availability issue, not a reason to scrape Sessions artifacts.
- Codex stores ~2.8 GB of jsonl that still contain secrets in `item.command`. Streaming extract must not spill those lines to stderr.
- Grok `stop` events inflate session counts if someone re-enables Stop; the miner should prefer `session_end` / compact events for session identity, and treat `stop` as a weak signal.
- Compaction summaries can include customer prose. They are not an extract source in v1.

---

## 5. Packaging and MCP surface

Two delivery vehicles, one engine. The engine lives in dops-next (Go). The skill is a thin, agent-readable wrapper so Claude/Codex/Cursor/Grok can *review* proposals without reimplementing parsers.

### 5.1 Engine (dops-next)

```
cmd/mine.go                         # dops mine …
internal/mine/
  ingest.go                         # index.jsonl + event md pointers
  parse_claude.go
  parse_codex.go
  parse_cursor.go
  parse_grok.go
  extract.go
  normalize.go
  cluster.go
  rank.go
  redact.go
  redact_test.go                    # §4.3 table
  propose.go
  review.go
```

CLI:

```
dops mine run [--once | --watch | --since]
dops mine status
dops mine list
dops mine show <fingerprint>
dops mine review
dops mine approve <fingerprint>
dops mine reject <fingerprint> --reason …
dops mine install-schedule          # LaunchAgent / crontab snippet
dops mine install-catalog           # add inactive $DOPS_HOME/catalogs/mined
```

Fits the existing Cobra layout (`cmd/` wiring, `internal/` logic, no env reads inside `internal/`).

### 5.2 Skill (agent-facing)

```
.claude/skills/session-mining/
  SKILL.md
  references/redaction-rules.md     # copy of §4.2 ids, not session data
  scripts/
    mine-run.sh                     # exec dops mine run --once
    mine-review.sh                  # exec dops mine list / show
```

`SKILL.md` trigger: scheduled mining, “what automations are we repeating”, “propose runbooks from sessions”. The skill **must**:

1. Call `dops mine`, not parse `~/Documents/Sessions` itself.
2. Never dump event markdown or transcripts into the conversation.
3. Never wiki-ingest.
4. Treat approve as a human gate.

Mirror the skill into Codex/Cursor/Grok skill dirs the same way `create-runbook` is mirrored, or keep a single copy under the dops-next repo and let operators install it. Do not invent a second product name; this is a dops-next skill.

### 5.3 MCP surface

Today dops-next MCP exposes runbooks as tools, plus:

- `dops://catalog`, `dops://catalog/{id}`
- `dops://schema/runbook`, `dops://schema/shell-style`
- prompt `create-runbook`

Session mining adds **read + review**, not a new way to execute shell from transcripts.

| Kind | Name | Notes |
|------|------|-------|
| Resource | `dops://mine/queue` | JSON list of fingerprints, scores, states. No scripts in the list payload. |
| Resource | `dops://mine/proposal/{fingerprint}` | Redacted yaml + sh + meta |
| Tool | `mine_list` | same as list resource; `risk` n/a |
| Tool | `mine_get` | path param fingerprint; returns redacted draft |
| Tool | `mine_run` | **hidden by default**. High risk. Starts a local `--once` run. Requires `_confirm_id`. Off unless `--allow-mine-run`. |
| Tool | `mine_review` | args: `fingerprint`, `action=reject\|skip`, optional `reason`. **No approve** unless `--allow-agent-approve`. |
| Prompt | `review-mined-runbook` | walks the operator through yaml/script edits using `dops://schema/runbook` |

Secret parameters stay excluded from tool schemas, same as existing MCP policy. Staging catalog remains inactive, so mined runbooks do not become executable tools by surprise.

`--allow-risk` continues to cap which **runbooks** MCP can run. It does not bypass the mine approve gate.

### 5.4 What the skill does *not* need

- Notion / Datadog / Atlassian MCP
- Executor
- Write access to `~/Documents/Sessions`
- A new vault. Drafts contain no live secrets; runtime secrets stay in the existing age vault once a human parameterizes them.

---

## 6. Worked example (synthetic only)

The following commands never appeared in the survey dump. They are fixtures for the pipeline.

### 6.1 Synthetic sessions

Three sessions, two agents, same operator habit:

| Session | Agent | Day | Shell steps (synthetic) |
|---------|-------|-----|-------------------------|
| S1 | grok | 2026-09-01 | `kubectl --context eks-dev -n payments get pods -l app=api` → `kubectl --context eks-dev -n payments logs deploy/api --tail=200` |
| S2 | codex | 2026-09-04 | `kubectl --context eks-prod -n billing get pods -l app=api` → `kubectl --context eks-prod -n billing logs deploy/api --tail=200` |
| S3 | grok | 2026-09-09 | `kubectl --context eks-dev -n payments get pods -l app=api` → `kubectl --context eks-dev -n payments logs deploy/api --tail=100` |

S3 also writes a helper `scripts/pod-logs.sh` via a file tool (synthetic body: a `kubectl logs` wrapper).

### 6.2 Extract → normalize

Three two-step sequences plus one script_file.

Normalized template (all three sequences collapse):

```
kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP
kubectl --context $CONTEXT -n $NAMESPACE logs deploy/$APP --tail=$N
```

Parameters observed: `CONTEXT` ∈ {`$CONTEXT`}, `NAMESPACE` ∈ {`$NAMESPACE`}, `APP=api` (stable; becomes default), `N` ∈ {100,200} (default 200).

### 6.3 Cluster / rank

| Metric | Value |
|--------|------:|
| freq | 3 sequences (+ 1 script_file member) |
| unique_sessions | 3 |
| unique_agents | 2 |
| last_seen | 2026-09-09 |
| sequence_bonus | yes |
| catalog_penalty | none (no existing runbook with this fingerprint) |

Passes cutoff (`unique_sessions >= 3`). Fingerprint `f7a1…` (illustrative).

### 6.4 Redact / propose

Draft `runbook.yaml`:

```yaml
name: k8s-pod-logs
version: 0.0.0-draft
description: Get pods by app label, then tail deployment logs
risk_level: medium
script: script.sh
parameters:
  - name: context
    type: string
    required: true
    scope: catalog
    description: kube context
  - name: namespace
    type: string
    required: true
    scope: catalog
  - name: app
    type: string
    required: true
    scope: runbook
    default: api
  - name: tail
    type: number
    required: false
    scope: local
    default: 200
```

Draft `script.sh`:

```sh
#!/bin/sh
set -eu

CONTEXT="${CONTEXT:?context is required}"
NAMESPACE="${NAMESPACE:?namespace is required}"
APP="${APP:?app is required}"
TAIL="${TAIL:-200}"

main() {
  echo "==> Stage 1/2: List pods"
  kubectl --context "${CONTEXT}" -n "${NAMESPACE}" get pods -l "app=${APP}"

  echo "==> Stage 2/2: Tail logs"
  kubectl --context "${CONTEXT}" -n "${NAMESPACE}" logs "deploy/${APP}" --tail="${TAIL}"
}

main "$@"
```

No hostnames, no real context names, no session quotes.

### 6.5 Review

Operator runs `dops mine review`, edits `risk_level` to `high` because prod contexts are in play, approves. `dops catalog list` then shows `mined / k8s-pod-logs` once the staging catalog is activated. MCP still hides it until the catalog is active and `--allow-risk` includes `high`.

Reject path: if S1 had contained `kubectl --token eyJ…`, R4 would have dropped the candidate before queue; audit would show `dropped_redaction` / `R4_KNOWN_TOKENS` and no draft directory.

---

## Open questions (for implementation, not this doc)

1. Should Claude jsonl retention be fixed in the archive hook (copy is forbidden; maybe a command-only sidecar)? Out of scope here; mining must tolerate missing transcripts.
2. Whether `dops mine review` is a BubbleTea overlay or CLI-only in v1. Recommendation: CLI-only.
3. Whether the staging catalog name `mined` should be configurable (`dops config set mine.catalog mined`). Recommendation: yes, default `mined`.
4. Cursor: wait for hook events vs. optional direct scan of `~/.cursor` jsonl. Recommendation: wait for index rows so ingest stays one path.

---

## Survey method

Counts were taken on 2026-09-11 from `~/Documents/Sessions` (index + event filenames + byte sizes + markdown headings + hook-metadata JSON keys) and from native store filenames/JSON keys under `~/.claude/projects`, `~/.codex/sessions`, `~/.grok/sessions`, and `~/.cursor`. No archive body text was retained in this file. Adjacent wiki page consulted: “OKF Session Ingest Skill” (knowledge ingest, not this pipeline). Runbook schema from this repo’s `docs/architecture.md` and `docs/guides/runbooks.md`.
