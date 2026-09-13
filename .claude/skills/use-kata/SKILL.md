---
name: use-kata
description: "Use Kata's DevOps script library instead of inventing kubectl, helm, git, unittest, or OKE one-offs. Call list_kata first. Triggers on: kata, kadou, scripts, runbooks, dojo, starter, catalog, kubectl, helm, CrashLoop, OKE, git status, unittest, Jenkins, DevOps, list_kata, run_kata."
user-invocable: true
---

# Use Kata

**Kata** is a DevOps script library. Each **kata** is one reviewed script
with a closed header. The collection is a **catalog**. MCP tools:
`list_kata`, `describe_kata`, `run_kata`, `propose_kata`. Prefer those
over a one-off shell for cluster triage, Helm, git snapshot, unittest,
or other repeatable ops.

## What people say (same library)

If they say **kata**, **kadou**, **scripts**, **script library**, **runbooks**,
**dojo**, or **starter**, they mean this library. `list_kata` is the look.

| They say | You do |
|---|---|
| "look at my kata" / "what's in kadou" | `list_kata` `{}` |
| "look at my scripts" / "script library" | `list_kata` `{}` |
| "look at my runbooks" | `list_kata` `{}` (legacy dops word) |
| "starter" / "starter dojo" / "the starter set" | `list_kata` `{ "query": "starter" }` |
| "sesami scripts" / "the sesami dojo" | `list_kata` `{ "query": "sesami" }` |
| "jenkins jobs" / "what's in CI" | `list_kata` `{ "query": "jenkins" }` then `jenkins-jobs` |
| "what's on the cluster" / "k8s" / "OKE" | `list_kata` `{ "query": "k8s" }` then `k8s-list` |
| "run the pod triage" / "CrashLoop" | `k8s-trouble` after `k8s-list` has a context |
| "git status of this repo" | `git-snapshot` if `list_kata` `{ "query": "git" }` matches |

Then `describe_kata` → `run_kata`. CLI: `kata --plain list`, `kata run <id>`.

## When to look

Before `kubectl`, `helm`, `git status`/`diff`/`log`/`fetch`, `python3 -m
unittest`, or Jenkins-shaped shell: **`list_kata`** with a short `query`
(`pods`, `helm`, `git`, `unittest`, `trouble`, `jenkins`, `k8s`). Do not
dump the whole library. For Jenkins, run `jenkins-jobs` (optional
`query=`) before guessing a job path, then `jenkins-log`. For Kubernetes,
run `k8s-list` (contexts → namespaces → workloads) before `k8s-trouble`.

## How to call

1. `list_kata` `{ "query": "…" }` — ids, about, risk only.
2. If one matches: `describe_kata` `{ "id": "prefix/name" }` then
   `run_kata` `{ "id": "…", "args": { … } }`. Start with `dry_run: true`
   when args are unclear.
3. If none match: run the one-off. If you just did the same work twice,
   `propose_kata` (never registers it). Do not invent a thin wrapper.
4. Needs are vault-only. Never put a need in `args`. Default ceiling is
   `low`; missing kata is `no_such_kata`, not a grant prompt.

CLI fallback when MCP is not connected: `kata --plain list`,
`kata --plain show <id>`, `kata --plain run <id> k=v`.

## Token budget

A kata must cost fewer tokens than the one-off commands it replaces.
Measure with `session-mining` **Token budget** (`kata --plain run` vs
concatenated one-off stdout+stderr). **PASS only when kata bytes <
one-off bytes.** If the match would lose, use the shell.

Do not SessionStart-dump `kata list`. Discovery is `list_kata` with a
query, not a hook that injects the whole library.
