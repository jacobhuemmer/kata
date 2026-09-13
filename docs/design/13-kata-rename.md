# 13 — Rename kadou → Kata

**Date:** 2026-09-13
**Status:** adopted (Mason: yes to Origin repos, `kata` binary / `kata-cli` crates, and starting the code rename)
**Wins over** `05-prd.md` on product name, paths, env prefix, and the collection noun. `08-shape-review.md` still wins on single-file kata and **no registry**.

## Nouns

| Word | Means |
|---|---|
| **Kata** | The product: CLI + MCP server |
| **kata** | One script with a closed six-key header |
| **catalog** | The collection on disk (`ls` is the listing). Not `catalog.yaml`, not `catalog add` |

Retired in product voice: **kadou**, **folder** (as a user-facing noun). Ids stay `prefix/name` (`agent/k8s-list`).

`08` retired **catalog as a registry**. This doc reclaims **catalog** only as the directory tree of kata.

## Repos (Origin)

`origin repo` cannot rename. Sequence:

1. Create `masonhuemmer/catalog`; push the current scripts repo (`masonhuemmer/kata`) there.
2. Delete `masonhuemmer/kata` (scripts already on `catalog`).
3. Create `masonhuemmer/kata`; push the engine (`masonhuemmer/kadou`) there.
4. Keep `masonhuemmer/kadou` until `kata version` and a vault `needs` run work; then stop pushing it.

Local: `~/origin/catalog` = scripts, `~/origin/kata` = engine (the kadou checkout retargets).

## Identity

| Layer | Value |
|---|---|
| Binary | `kata` (one-release `kadou` shim → same binary) |
| MCP server | `kata mcp serve` |
| MCP tools | **unchanged:** `list_kata`, `describe_kata`, `run_kata`, `propose_kata` |
| Crates | `kata-cli`, `kata-core`, `kata-exec`, `kata-mcp`, `kata-mine` |
| crates.io | **do not** publish `kata` (taken: yukimemi template applier). Publish `kata-cli` if we publish at all |
| Config | `~/.config/kata/kata.toml` |
| Catalog | `~/.config/kata/catalog/<prefix>/` |
| Env | `KATA_HOME`; honor `KADOU_HOME` one release with a warning (same pattern as `DOPS_HOME`) |
| Keyring | `kata`; copy `kadou-jenkins` → `kata-jenkins` on migrate |

## Scripts

Stay POSIX kata files. Live only in catalog git repos. Install with `kata get <url> --as agent`. `$KADOU_ROOT` → `$KATA_ROOT`. No crates in the catalog repo.

## Compatibility

`kata migrate` (or first run): copy `~/.config/kadou/kata` → `~/.config/kata/catalog`, vault and mine state, leave the old tree until confirmed. `kadou` argv0 prints a one-line deprecation and execs the same program.
