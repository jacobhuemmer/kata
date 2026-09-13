---
name: use-kata
description: "Use kadou's DevOps script library (kata) instead of inventing kubectl, helm, git, unittest, or OKE one-offs. Call list_kata first. Triggers on: kubectl, helm, CrashLoop, OKE, git status, unittest, Jenkins, DevOps, list_kata, run_kata, kadou."
user-invocable: true
---

# Use kadou kata

kadou is a **DevOps script library**. Each kata is one reviewed script with
a closed header. The MCP tools are `list_kata`, `describe_kata`,
`run_kata`, and `propose_kata`. Prefer those over a one-off shell when
the work is cluster triage, Helm, git snapshot, unittest, or other
repeatable ops.

## When to look

Before `kubectl`, `helm`, `git status`/`diff`/`log`/`fetch`, `python3 -m
unittest`, or Jenkins-shaped shell: **`list_kata`** with a short `query`
(`pods`, `helm`, `git`, `unittest`, `trouble`, `jenkins`). Do not dump
the whole library. For Jenkins, run `jenkins-jobs` (optional `query=`)
before guessing a job path, then `jenkins-log`.

## How to call

1. `list_kata` `{ "query": "…" }` — ids, about, risk only.
2. If one matches: `describe_kata` `{ "id": "folder/name" }` then
   `run_kata` `{ "id": "…", "args": { … } }`. Start with `dry_run: true`
   when args are unclear.
3. If none match: run the one-off. If you just did the same work twice,
   `propose_kata` (never registers it). Do not invent a thin wrapper.
4. Needs are vault-only. Never put a need in `args`. Default ceiling is
   `low`; missing kata is `no_such_kata`, not a grant prompt.

CLI fallback when MCP is not connected: `kadou --plain list`,
`kadou --plain show <id>`, `kadou --plain run <id> k=v`.

## Token budget

A kata must cost fewer tokens than the one-off commands it replaces.
Measure with `session-mining` **Token budget** (`kadou --plain run` vs
concatenated one-off stdout+stderr). **PASS only when kata bytes <
one-off bytes.** If the match would lose, use the shell.

Do not SessionStart-dump `kadou list`. Discovery is `list_kata` with a
query, not a hook that injects the whole library.
