---
name: session-mining
description: Turn repeated agent shell work recorded under ~/Documents/Sessions into redacted, human-reviewed kadou kata drafts. Use when the user asks what automations they keep repeating, wants to propose kata from past sessions, or mentions scheduled/session mining. Triggers on "what are we repeating", "mine my sessions", "propose kata from sessions", "kadou mine". Execs `kadou mine`; never parses session transcripts itself.
user-invocable: true
---

# Session mining

`docs/design/06-session-mining.md` (pipeline, redaction, bounds, review
gate) is the contract; `docs/design/05-prd.md` §6.8 is the packaging
override this skill implements: the engine is **`crates/kadou-mine`** and
the CLI surface is **`kadou mine …`** on kadou's existing command tree —
there is no separate mining binary or Go `internal/mine` package to wrap.

## Rules (non-negotiable)

1. **Call `kadou mine`, never parse `~/Documents/Sessions` yourself.** The
   redaction pipeline (R1–R13, fail-closed R14) only runs inside
   `crates/kadou-mine`; reading a transcript directly bypasses it.
2. **Never dump event markdown or transcripts into the conversation.**
   `kadou mine show <fingerprint>` prints only the already-redacted draft
   and its meta — that is the one safe thing to quote back to a human.
3. **Never wiki-ingest anything this skill touches.** Session content is
   local and potentially secret-shaped; it does not belong in Notion.
4. **Approve is a human gate, always.** This skill may run, list, and show
   queued drafts; it never runs `kadou mine approve` on the user's behalf.
   Tell the user what to review and let them approve it themselves.
5. **A kata must cost fewer tokens than the one-off commands it replaces.**
   Measure it. A thin wrapper around one `kubectl`/`git`/`helm` invocation
   fails this gate — kadou's run frame alone is ~80–90 tokens on top of
   the same stdout. Do not recommend approve, accept, or landing a file
   that loses the comparison.

## Token budget (required before recommending a kata)

Run this after `kadou mine show` (or after rewriting a draft) and
**before** telling the user it is ready. Isolated `KADOU_HOME` — copy
the redacted source into `…/kata/probe/` and `kadou --plain run` it
there. That is not `kadou mine approve` or `kadou accept`.

Report **counts only**. Never paste command bodies, cluster dumps, or
logs into the conversation.

**One-off side.** The command sequence the draft replaces, stdout+stderr
concatenated. Three `git` calls means three outputs joined.

**Kata side.** `kadou --plain run probe/<name> …` (or the `run_kata` JSON
if that is the agent path). Include the CLI frame / MCP envelope.

```sh
one_off=$(mktemp); kata=$(mktemp)
{ cmd1; cmd2; } >"$one_off" 2>&1
kadou --plain run probe/name k=v >"$kata" 2>&1
python3 -c '
from pathlib import Path
import sys
o = Path(sys.argv[1]).read_bytes()
k = Path(sys.argv[2]).read_bytes()
print("one-off %d B ~%d tok" % (len(o), len(o)//4))
print("kata    %d B ~%d tok" % (len(k), len(k)//4))
print("PASS" if len(k) < len(o) else "FAIL")
' "$one_off" "$kata"
rm -f "$one_off" "$kata"
```

Tokens are UTF-8 bytes / 4 (`docs/design/05-prd.md` scoreboard, ~4
B/token). **PASS only when kata bytes < one-off bytes.** Extra
one-off turns cost more in the session, but must not excuse a larger
payload.

| Fail (do not land) | Pass |
|---|---|
| One command, same stdout, plus the kadou frame | Several commands collapsed into one run |
| `kubectl get pods -o wide` / full `describe` | Filter (unhealthy only), cap rows, clip columns |
| Unittest / cargo full traceback | FAIL names + one summary line |

On FAIL: print the two byte counts, do not recommend `kadou mine
approve`. The human may reject (`kadou mine reject <fp> --reason
'loses to one-off commands'`). Compact and re-measure, or leave it
queued.

## Commands

```sh
kadou mine run --once      # ingest new sessions, queue new draft proposals
kadou mine status          # queued/approved/rejected counts
kadou mine list            # one line per queued draft
kadou mine show <fp>       # the full redacted draft + its meta (safe to show the user)
kadou mine review          # list plus the next commands to run
kadou mine approve <fp> [--into <name>]   # human-only: copy into mined/
kadou mine reject <fp> --reason …         # human-only: ban this fingerprint
kadou mine install-schedule [--load]      # LaunchAgent (macOS) / crontab line (Linux)
```

A queued draft is not runnable until a human runs `kadou mine approve`
*and* `kadou accept mined/<name> --into <folder>` — this skill can surface
that path, never walk it on the user's behalf (`docs/design/05-prd.md`
§6.8, CLAUDE.md invariant 8 "Drafts are never runnable").
