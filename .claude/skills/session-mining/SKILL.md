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
