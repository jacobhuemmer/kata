# dops-next design principles

This is the contract the PRD must obey. It applies Omarchy's omakase philosophy — opinionated defaults, batteries included, fast install, beauty, keyboard-first, dotfiles, convention over configuration — to a **Rust CLI + TUI + MCP server that is a script library for AI agents**. Codename: **dops-next**. No product-name proposals live here.

**Sources (2026-09-11):**

- Omarchy: [omarchy.org](https://omarchy.org), [the Omarchy Doctrine](https://omarchy.org/doctrine/), [Omarchy 3 manual](https://learn.omacom.io/2/the-omarchy-manual) (legacy; current getting-started is [omarchy.org/manual](https://omarchy.org/manual/getting-started/))
- dops today: `README.md`, `docs/architecture.md`, `cmd/init.go`, `install.sh`, `internal/config`, `internal/theme`, plus MCP (`cmd/mcp.go`, `internal/mcp`), risk policy, keyboard shortcuts, and runbook format as they exist on this tree (`795d2d2`)

Each Omarchy-derived principle has: meaning for this product, 3–6 concrete rules, and a dops-today assessment (satisfies / partial / violates). Product-specific principles follow. The charter and non-goals at the end are the PRD checklist.

---

## How Omarchy maps

Omarchy is DHH's opinionated Arch + Hyprland setup: *oma* is omakase — chef's choice. The ISO asks a handful of questions, hands back a finished desktop in about a minute, and looks coordinated out of the box. Everything important is a keyboard chord. Config is files in `~/.config`; product files live under `~/.local/share/omarchy` and you override them rather than patch them. The [Doctrine](https://omarchy.org/doctrine/) adds: beauty is truth, welcome the agents, own the machine, command is service (someone picks).

dops-next is not a distro. The mapping is:

| Omarchy | dops-next |
| --- | --- |
| ISO + five questions → finished desktop | One install command → `dops` and `dops mcp serve` already work |
| Neovim, Foot, Hyprland, theme, agents pre-wired | Starter catalog, TUI, CLI, MCP, vault, risk, history, default theme — one binary |
| `Super + Space` / `Super + K` | TUI is fully keyboard-operable; `?` and a command palette cover every action |
| `~/.config` is yours; `~/.local/share/omarchy` is the product | User config vs product data; catalogs are directories of yaml + `script.sh` |
| Theme restyles the whole system | One theme restyles TUI, CLI help, and errors together |
| Agents are first-class on the OS | Agents are first-class on the script library — through a **small, lazy MCP**, not a tool per script |

Omarchy welcomes agents everywhere. dops-next welcomes them as **callers of reviewed scripts**, not as free-form operators. That gap is why this document adds token frugality, scripts over reasoning, safety gates, and human review.

---

## 1. Opinionated omakase defaults

**Omarchy:** "We pick the tools and tune the details, so you can get straight to work. But this is your computer. You're free to change everything." Command is service: someone makes the call.

**For dops-next:** The product arrives decided. First run is not a quiz. Theme, catalog layout, risk ceiling, MCP surface, script contract, and config location are chosen. Users and agents override after they have a working system, not before.

### Rules

1. **Canonical home.** Default config lives at `~/.config/dops/config.toml`. `DOPS_HOME` is the only override (tests, containers). No second hidden directory, no per-command flags that relocate config in normal use.
2. **Zero-config first run.** After install, `dops` launches the TUI with the starter catalog. `dops mcp serve` speaks stdio. There is no required `init` step, no first-run wizard, no "please add a catalog" dead end.
3. **One default theme.** The product theme (`doop`, or its successor) is the default. Not a generic editor theme. `theme = "rainbow"` is not a default.
4. **Human vs agent risk.** Humans default to `max_risk_level = "medium"` (high/critical visible only after the ceiling is raised, and still confirm). Agents default to `allow_risk = "low"`. Raising the agent ceiling is a human config edit.
5. **POSIX `sh` is the script runtime.** `runbook.yaml` + `script.sh`. Parameters become `UPPER_SNAKE` environment variables. That is the contract. PowerShell, bashisms, and interpreters are not the default path.
6. **MCP transport is stdio.** HTTP is opt-in and localhost-bound. The default agent integration is a local stdio server.

### dops today

| | |
| --- | --- |
| Partial | `EnsureDefaults()` writes `~/.dops/config.json` on first load; default theme exists (`github`); human `max_risk_level` defaults to `medium`; vault keys are created on first use; `dops` with no args launches the TUI. |
| Violates | First useful catalog requires `dops init` (hello-world only). Bare `dops` after install is an empty sidebar. Default theme is `github`, not the product theme `doop`. MCP `--allow-risk` defaults to **`critical`**, inverted from omakase-safe. Home is `~/.dops/` (not XDG). Config is JSON. `install.sh` tells the user to run `dops init` as a second step. |

---

## 2. Batteries included

**Omarchy:** Not a grab bag of packages. A complete system: editor, terminal, browser, shell tools, AI harnesses, themes, gaming, even Zoom. "Zero bloat: just everything I use."

**For dops-next:** One binary is a complete script library. A new operator or a new agent can list, describe, and run real runbooks the same day, without cloning a git catalog or writing YAML.

### Rules

1. **Bundled starter catalog.** The binary embeds a small, safe starter catalog (health, git status, disk, hello-world-class checks). It is registered automatically. It contains no high/critical runbooks and no secrets.
2. **Four capabilities, one artifact.** CLI, TUI, MCP server, encrypted vault, risk policy, execution history, and bundled themes ship in the single binary. No Node runtime, no sidecar, no cloud account.
3. **Runbook authoring kit.** YAML schema, POSIX script conventions, and a `propose_runbook` / create-runbook path ship with the product so agents and humans produce the same shape.
4. **Catalog install is additive.** `dops catalog install <git-url>` adds team catalogs on top of starter. It is not the way to get a non-empty product.
5. **Themes ship in the binary.** A curated set is embedded. Custom themes are extra files in the config dir, not a download step to look acceptable.
6. **Help is in the product.** `dops --help`, TUI `?`, and MCP `describe_runbook` are enough to operate. The website is documentation, not a dependency of first run.

### dops today

| | |
| --- | --- |
| Satisfies | Single Go binary: TUI, CLI, MCP, web UI, 20 embedded themes, age vault, history, risk ceilings, `create-runbook` MCP prompt, schema resources. |
| Violates | Example catalogs live in the **git repo** (`catalogs/`), not in the installed binary. `dops init` scaffolds one hello-world runbook; otherwise the product is empty. Operators must `dops catalog install` or hand-write YAML to do real work. The Vue web UI is a fourth interface the assignment for dops-next does not include — extra battery, extra surface. |

---

## 3. One-line install

**Omarchy:** Lightning-fast ISO. Five questions, full-disk or free-space, often under a minute, a finished desktop. No "which desktop environment?" quiz. (Omarchy itself is not `curl | sh`; the *spirit* is: install is the last decision, not the first research project.)

**For dops-next:** One command puts a working `dops` on PATH. The next command is `dops`, not `dops init`.

### Rules

1. **Canonical shape.** `curl -fsSL https://<stable-install-url>/install.sh | sh` downloads the verified binary for this OS/arch, installs to `/usr/local/bin` (or `DOPS_INSTALL_DIR` / `~/.local/bin`), and leaves first run ready (config defaults + starter catalog available from the binary). Checksum verification is part of the script, not a footnote.
2. **Install = ready.** After that one line, `dops` and `dops mcp serve` work. No compiler, no `cargo install` as the advertised path, no language toolchain, no extra `init`.
3. **Package managers are peers, same state.** Homebrew, Nix, cargo-binstall, and OS packages must produce the same first-run behavior as the curl installer. They are not allowed to install a hollow binary that still needs a ritual.
4. **POSIX installer.** `install.sh` stays `#!/bin/sh`, `set -eu`, OS/arch detect, no bashisms. Windows gets winget/scoop as the one-liner equivalent, not a manual PATH lecture as the primary path.
5. **One user-visible artifact.** `dops` on PATH. MCP is `dops mcp serve`, not a second binary. Agent config is a snippet the docs can paste (`command: dops`, `args: ["mcp", "serve"]`).
6. **Idempotent.** Re-running the installer updates the binary and does not clobber user config, vault, or extra catalogs.

### dops today

| | |
| --- | --- |
| Satisfies | `curl -fsSL https://raw.githubusercontent.com/rundops/dops/main/install.sh \| sh` exists; brew, winget, scoop, `go install`, Docker. POSIX `install.sh` with OS/arch detect. |
| Violates | Installer does **not** create a ready home or register a starter catalog. It prints `dops init` then `dops`. README Quick Start is two (really three) steps. No checksum verification in `install.sh`. `go install` is still advertised (toolchain). Docker MCP image still expects a populated `~/.dops`. |

---

## 4. Beautiful by default

**Omarchy:** "Beauty is truth. Great tools are beautiful because they're right." One theme restyles terminal, bar, notifications, wallpaper. The first boot looks finished. "There's enough desaturated brutalism in this world already."

**For dops-next:** The first TUI frame should look like a product, not a scaffold. CLI help and errors share the same palette. Empty states are designed, not blank.

### Rules

1. **Product theme is the default.** Ship `doop` (or successor) as default. Bundled alternatives (Catppuccin, Gruvbox, Nord, …) are one config key away. Random-on-launch is opt-in, never default.
2. **One theme, all human surfaces.** TUI, CLI help, CLI errors, and confirm copy share the theme. A theme file is the single source of color. No unstyled debug chrome in default `View()`.
3. **First-run layout is finished.** Sidebar shows the starter catalog, metadata pane shows a selected runbook, footer shows key hints. An empty catalog is a designed empty state with one line of next action, not a vacant panel.
4. **Help is beautiful and complete.** `?` is a first-class overlay, not an afterthought. Footer key hints stay accurate to focus/mode.
5. **Theme files are drop-in.** Bundled themes are embedded. A user theme is a file in `~/.config/dops/themes/<name>.toml` (or `.json`). Activating it is `theme = "<name>"`.
6. **Visual tests guard the default.** Any change to `View()`, styles, footer, or wizard is accompanied by a visual check (VHS/Freeze or Rust equivalent). Beauty is enforced, not hoped for.

### dops today

| | |
| --- | --- |
| Satisfies | 20 embedded JSON themes; dark/light auto-detect; web UI mirrors theme; styled Cobra help; `?` overlay; VHS tapes; product theme `doop` exists. |
| Violates | Default is `github` (safe, generic). First run without `init` is an empty catalog. `theme=rainbow` randomizes. Web UI is a second visual language (Vue) that must stay in sync with Lip Gloss. |

---

## 5. Keyboard-first

**Omarchy:** "Everything happens via the keyboard — EVERYTHING." First boot cannot be operated with the mouse alone. `Super + Space` launcher, `Super + Alt + Space` menu, `Super + K` hotkeys. Mouse is allowed; it is never required.

**For dops-next:** A human operates the TUI without touching the mouse. Agents operate MCP/CLI and do not need a keyboard. Keyboard-first does not mean "no pointing device"; it means every action has a key.

### Rules

1. **Keyboard completes every TUI flow.** Navigate, search, parameterize, confirm, execute, cancel, theme, help, quit. Mouse may scroll, focus, and select text. Mouse must not be the only path.
2. **Default bindings (convention).** `j`/`k` or arrows move; `Enter` runs; `/` searches; `Tab` switches panes; `?` help; `q` quits; `Esc` backs out; `Ctrl+c` / a documented chord cancels execution. These defaults are the product. Document them in `?` and the footer.
3. **Command palette is the menu.** One chord (`Ctrl+Shift+P` or successor) reaches every action (theme, catalog, help, quit, confirm policy). Analog of Omarchy's control menu.
4. **Bindings are data.** Defaults live in code. Overrides live in `config.toml` as a map. Users do not fork the binary to swap `q` for `Ctrl+q`. Unbound keys do nothing surprising.
5. **Wizards are keyboard forms.** Select, multi-select, boolean, secret, confirm — all keys. No click-only control.
6. **MCP and CLI are the non-keyboard interfaces.** They must be complete for agents and scripts. The TUI is not a gate in front of `dops run` or MCP.

### dops today

| | |
| --- | --- |
| Satisfies | Documented shortcuts; vim-ish `h`/`l`, `g`/`G`, `n`/`N`; `?` help; command palette; keyboard wizard; `q` / `Ctrl+c` quit. CLI `dops run` and MCP do not require the TUI. |
| Partial | Mouse is implemented (hover-to-focus, click, drag-select). That is fine if keyboard remains sufficient; hover-to-focus can steal pane focus and fight the keyboard. |
| Violates | Keybindings are **hardcoded** — not in `config.json`. Web UI is promoted as a first-class, mouse-first interface. No `j`/`k` in the published shortcut table (arrows only in docs). |

---

## 6. Dotfile-native config

**Omarchy:** "Omarchy is primarily configured through the so-called dotfiles that live in `~/.config`. Those are considered your files. The files that live in `~/.local/share/omarchy` belong to Omarchy itself… If you need to change anything [there], you should be overwriting the value in `~/.config` instead." Agents can read and edit those files.

**For dops-next:** Config is a file a human or an agent can open in an editor. Product files are not that file. Secrets are not that file. Catalogs are files too.

### Rules

1. **Plain text, commentable.** `~/.config/dops/config.toml` is the source of truth. TOML over JSON so comments, trailing commas, and diffs work. `dops config get/set` is sugar; the file wins.
2. **XDG split.** User-owned: `~/.config/dops/` (config, user themes, user catalogs). Product-owned: embedded in the binary (starter catalog, bundled themes) or `~/.local/share/dops/` for caches. State: `~/.local/state/dops/` (history). Users never patch product files; they override.
3. **Catalogs are directories.** Each runbook is `runbook.yaml` + `script.sh` in a folder. Git-friendly. No database as the system of record.
4. **Secrets stay out.** `vault` is a separate 0600 age-encrypted blob (path under config or data dir). Config has theme, catalogs, risk, keybinding overrides, agent policy — never parameter values.
5. **Missing keys mean defaults.** A missing `theme` is `doop`. A missing `max_risk_level` is `medium`. An empty config file is valid. No required boilerplate.
6. **Agents edit files, not APIs.** The supported way for an agent to change behavior is: read the toml/yaml, propose a diff, wait for human accept (see principle 11). There is no "set config" MCP tool on the default surface.

### dops today

| | |
| --- | --- |
| Satisfies | File-backed `~/.dops/config.json`; catalogs are yaml+sh trees; vault split from config; `dops config`; custom themes as files; `DOPS_HOME`; history as files. |
| Violates | JSON (no comments). Everything mixed under `~/.dops/` (config, vault, keys, themes, catalogs, history) — no user-vs-product split. `config.json` written `0644`. No share dir for product files because starter catalog is not in the binary. `dops config set` is a live mutation with no review step. |

---

## 7. Convention over configuration

**Omarchy:** Neovim, Foot, Hyprland, JetBrainsMono, square corners, 2× display assumption. You can change any of it; you should not have to. Convention is the product.

**For dops-next:** A valid runbook is two files with boring names. IDs, env vars, and risk words are determined, not designed per team. Configuration exists for taste and policy, not for wiring the engine.

### Rules

1. **Two-file runbook.** Directory name = `name` in YAML = default script `script.sh`. Do not make authors fill three identifiers. `script:` is omitted when it is `script.sh`.
2. **ID shape.** Runbook ID is `catalog.runbook` (directory names). That is the CLI id, the TUI id, and the MCP describe/run argument. Aliases are optional sugar, not a second identity system.
3. **Env-var contract.** Parameter `namespace` → `$NAMESPACE`. Only that. No flags-to-script adapter, no JSON blob as the default input.
4. **Risk is four words.** `low | medium | high | critical`. No custom levels, no numeric scores, no per-team vocab.
5. **Convention vs configurable (closed list).**

   | Convention (not configurable) | Configurable |
   | --- | --- |
   | Config path (`~/.config/dops/config.toml`) | `DOPS_HOME` |
   | Runbook layout (`runbook.yaml` + `script.sh`) | Extra catalogs, aliases |
   | ID shape `catalog.runbook` | Display names |
   | Env-var mapping | Parameter values (vault) |
   | Risk enum | Risk *ceilings* (human and agent) |
   | Default MCP tool set (list / describe / run / propose) | `allow_risk`, allowed-runbook grants |
   | POSIX `sh` | Theme, keybinding overrides |
   | Starter catalog included | Installing more catalogs |

6. **A runbook has no compile step.** No SDK, no framework, no plugin host. If it needs a language runtime beyond `/bin/sh`, it is not a default-path runbook.

### dops today

| | |
| --- | --- |
| Satisfies | yaml + script directory layout; uppercase env vars; four risk levels; catalog/runbook ids; optional aliases; parameter scopes. |
| Partial | `name` "must match directory" is documented but still a duplicate field; `script:` is required even when `script.sh`. |
| Violates | Nine parameter types (`string`, `integer`, `number`, `float`, `boolean`, `select`, `multi_select`, `file_path`, `resource_id`) — more knobs than omakase wants. Catalog entries carry `display_name`, `sub_path`, `url`, `policy`. Windows `script.ps1` fork in `init`. `theme=rainbow` is a hidden extra convention. MCP registers **one tool per runbook**, which is configuration-by-catalog-size rather than a fixed convention. |

---

## Product-specific principles

Omarchy's list does not cover a **token-billed MCP surface**, **ops safety**, or **review of agent output**. These four are first-class for dops-next.

---

## 8. Token frugality

The MCP surface must be small and lazy. Agents pay per token. A catalog of 80 runbooks must not become 80 tool schemas on `tools/list`.

### Rules

1. **Fixed, tiny default surface.** Default tools are meta-tools, on the order of:

   - `list_runbooks` — names, one-line descriptions, risk, catalog
   - `describe_runbook` — full schema and script path for **one** id
   - `run_runbook` — execute one id with args
   - `propose_runbook` — draft a new yaml+script for human review (does not execute)

   Target: **≤ 4 tools** in `tools/list`. Not one tool per runbook.

2. **Lazy describe.** Parameter JSON Schema is returned only from `describe_runbook` (and from `run_runbook` error messages when args are wrong). `list_runbooks` is names + one liners. No bulk schema dump.
3. **Skills and prompts are not all registered.** Domain skills load on demand (name/trigger), matching a lazy dispatcher. Do not inject every skill into the prompt list at session start.
4. **Execution output is short.** Last N lines + exit code + duration + log path. Full logs stay on disk. Secrets never appear in tool results.
5. **List is filterable.** `list_runbooks` accepts query / catalog / risk filters so the agent does not pull the whole library to find one script.
6. **Hard budget.** The default `tools/list` payload must stay usable on a cheap model. If a change makes the default surface grow, it is a principles violation, not a feature.

### dops today

| | |
| --- | --- |
| Partial | Tool result truncated to last 50 lines; sensitive params excluded from schemas; `--allow-risk` can hide tools; catalog JSON summaries exist as resources. |
| Violates | `registerTools` **AddTool per runbook**, full JSON Schema each. All catalog skills registered as MCP prompts. Schema + catalog resources dump on the wire. `dops mcp serve --allow-risk critical` by default exposes the entire library as tools. This is the opposite of lazy. |

---

## 9. Scripts over reasoning

dops-next is a **script library**. The model should call a reviewed runbook, not reconstruct `kubectl` from memory.

### Rules

1. **The unit of work is a runbook.** YAML + `script.sh`. Not a free-form shell string. Not a generated one-shot in `/tmp` as the supported path.
2. **No generic `run_shell` tool.** MCP does not give the agent a bash pipe. If the task has no runbook, `run_runbook` fails with "no such runbook" and `propose_runbook` is the next step.
3. **Read before run.** `describe_runbook` returns the script path and body (or a way to read it). Agents are expected to read what they will execute. Humans can too, in the TUI metadata pane.
4. **Knowledge lives in the catalog.** Descriptions, parameter help, and optional skill docs are the domain prompt. Do not compensate for a thin catalog by stuffing a system prompt.
5. **New automation is a new runbook.** The loop is: propose → human accept → it becomes a script in a catalog → then it may be executed. Not: model reasons, runs, maybe writes a file later.
6. **Empty catalog is a product bug.** If first run has nothing to call, agents will reason. Batteries (principle 2) exist so this principle can hold.

### dops today

| | |
| --- | --- |
| Satisfies | Execution is always a runbook script; MCP tools wrap runbooks (not raw shell); `create-runbook` prompt; `type: skill` runbooks; TUI metadata shows the definition. |
| Violates | Empty default catalog pushes agents (and humans) to improvise outside dops. No first-class "this is a draft, not a run" path — `create-runbook` is a prompt; the agent then writes files with its own tools and can execute immediately. No prohibition in the protocol against treating dops as optional while the agent `kubectl`s directly (dops cannot police that, but it must not *offer* a shell tool). |

---

## 10. Safety gates for agents

Omarchy leans all the way into agents. dops-next runs **operational scripts**. Agents are welcome as callers; they are not trusted with the default ceiling.

### Rules

1. **Agent ceiling defaults to `low`.** `dops mcp serve` without flags exposes only low-risk runbooks. `medium` is a human-raised config. `high` / `critical` require an explicit grant (ceiling **and** an allow-list of runbook ids).
2. **Confirmation is not a string in the schema.** `_confirm_id` / `_confirm_word` with the expected value in the description is theater: a capable model copies it. High/critical from MCP is either (a) rejected, (b) turned into a pending request the TUI must accept, or (c) allowed only if that runbook id is in `agent.allowed_runbooks`. The model cannot mint the grant.
3. **Load-time hiding still applies.** Runbooks above the ceiling are invisible to list/describe/run, not merely blocked at exec.
4. **Secrets never cross MCP.** Secret parameters are not in schemas, not in list/describe payloads, not in tool results, not in history parameter maps. Vault injection happens at exec on the host.
5. **HTTP is local.** Default bind is loopback. No unauthenticated remote MCP. Stdio is the advertised agent path.
6. **Starter catalog is harmless.** Nothing in the bundled catalog is high/critical or secret-bearing. Destructive team catalogs are opt-in via `catalog install`.

### dops today

| | |
| --- | --- |
| Satisfies | Four risk levels; global + catalog ceilings; load-time filter; TUI confirm for high/critical; secret params excluded from MCP schema; vault encryption; `--allow-risk` exists. |
| Violates | MCP `--allow-risk` **defaults to `critical`**. High = `_confirm_id` must equal the id (printed in the schema). Critical = `_confirm_word` must be `CONFIRM` (printed in the schema). HTTP listens `:{port}` (8080), not specified as loopback-only. Agent path is weaker than the human TUI path. |

---

## 11. Human review of anything agents propose

Anything an agent wants to add, change, or run above its grant is a **proposal** until a human accepts it.

### Rules

1. **Propose ≠ execute.** `propose_runbook` writes a draft (e.g. `~/.local/state/dops/proposed/<id>/`) and returns a diff. It does not register the catalog entry and does not run the script.
2. **Accept is a human verb.** TUI (and `dops catalog accept <id>`) moves a draft into a user catalog. MCP has no `accept` tool on the default surface.
3. **Config and policy are human-owned.** Theme, keybindings, risk ceilings, catalog URLs, `agent.allowed_runbooks` — agents may *suggest* a diff; they may not `dops config set` via MCP by default.
4. **Executions above the agent grant queue.** A high/critical `run_runbook` without a grant becomes a pending execution the human sees in the TUI (or is rejected). It does not run first and notify later.
5. **History records the initiator.** Every execution stores `interface` (`tui` / `cli` / `mcp`) and, for MCP, enough to see it was an agent. Humans can filter "what did agents run?"
6. **No silent git.** The product never commits, pushes, or `catalog install`s an agent-invented repo on the default path.

### dops today

| | |
| --- | --- |
| Satisfies | TUI confirmation for high/critical; `ExecutionRecord.Interface` already distinguishes `tui` / `cli` / `web` / `mcp`; history is persistent; vault/config split keeps secrets off the config file. |
| Violates | MCP `HandleToolCall` **executes immediately**. No draft/proposed catalog. No accept command. `create-runbook` prompt encourages the agent to write files itself. `dops catalog install` is a live git clone with no agent-vs-human split. Web UI is another unattended-capable surface. |

---

## Convention vs configurable (summary)

| Item | Default | Override |
| --- | --- | --- |
| Config file | `~/.config/dops/config.toml` | `DOPS_HOME` |
| Product theme | `doop` | `theme = "…"` or drop-in theme file |
| Human risk ceiling | `medium` | `max_risk_level` |
| Agent risk ceiling | `low` | config grant + optional `--allow-risk` (never above config) |
| MCP tools | list / describe / run / propose | none (do not add a tool per runbook) |
| MCP transport | stdio | HTTP loopback, opt-in |
| Install | `curl -fsSL <url>/install.sh \| sh` | brew / nix / winget, same first-run state |
| Runbook files | `runbook.yaml` + `script.sh` | `script:` only if not `script.sh` |
| Script runtime | POSIX `sh` | not on the default path |
| Keybindings | product map (`Enter`, `/`, `?`, `q`, palette) | override map in config |
| Starter catalog | embedded, auto-registered | additional catalogs via install/path |
| Vault | age blob, 0600 | not optional on; values via TUI/CLI save |
| Agent config writes | off | human-only (no MCP setter) |

---

## Principles charter

The PRD (and every subsequent spec) must obey this page. If a feature fights a line below, the feature yields.

1. **Omakase first.** dops-next ships decided: paths, theme, catalog layout, risk ceilings, MCP surface, script contract. First run asks nothing.
2. **Install is one line, then it works.** `curl | sh` (or the package-manager peer) leaves `dops` and `dops mcp serve` usable. No `init` ritual, no empty catalog, no compiler.
3. **Batteries are in the binary.** Starter catalog, themes, TUI, CLI, MCP, vault, history, risk. Git catalogs are how teams grow, not how the product becomes real.
4. **Beautiful by default.** Product theme on first paint. Finished layout. Styled help and errors. Visual tests on View/style changes.
5. **Keyboard-first TUI.** Every human flow has a key. Palette + `?`. Mouse is extra. MCP/CLI are complete without a terminal UI.
6. **Dotfiles are the API.** Commentable config in `~/.config/dops`. Catalogs are yaml+sh directories. Product files are not user files. Secrets are not config.
7. **Convention over configuration.** Two-file runbooks, `catalog.runbook` ids, `UPPER_SNAKE` env, four risk words. Configure taste and policy, not the engine.
8. **MCP is small and lazy.** At most the four meta-tools by default. Schema on describe, not on list. Skills on demand. Short results. Token cost is a product constraint.
9. **Scripts over reasoning.** Agents call runbooks. No shell tool. No runbook → propose, don't improvise. Read the script before running it.
10. **Agents are gated.** Default agent risk is `low`. High/critical need a human grant, not a confirm string in a schema. Secrets never cross MCP. HTTP is loopback.
11. **Humans accept.** Agent-authored runbooks, config diffs, and over-grant executions are proposals. Accept lives in the TUI/`dops catalog accept`. History shows who initiated.
12. **Own the machine.** Local-first, files on disk, no required cloud, no tollbooth, no telemetry as a feature. The operator can read every script the product will run.
13. **One binary, three interfaces.** CLI, TUI, MCP. Same engine, same catalogs, same vault, same risk. A fourth interface is not implied.

---

## Explicit non-goals

The PRD must not grow these. If a later phase wants one, it is a new principles revision, not a silent expansion.

- **Product naming.** This document uses the codename dops-next only.
- **A general agent harness.** Not OpenCode, Claude Code, Cursor, or a chat REPL. Those *call* dops-next.
- **A generic `run_shell` / `exec` MCP tool.**
- **One MCP tool per runbook** as the default surface.
- **Required `dops init`** or any first-run questionnaire.
- **Empty-by-default install.** If the binary has no starter catalog, the build is wrong.
- **Web UI / SPA as a core interface.** dops today has `dops open` (Vue). dops-next's charter is CLI + TUI + MCP. A browser app is not required for v1 and must not drive the architecture.
- **SaaS, accounts, multi-tenant server, hosted control plane.**
- **Cloud-required features.** No mandatory network except install/update and explicit `catalog install`.
- **Unattended high/critical execution** by agents without a prior human grant.
- **Schema-printed confirmations** (`type CONFIRM`) as the agent safety model.
- **Secrets in config, git, MCP schemas, or history parameter maps.**
- **JSON as the human config format** (implementation may keep a machine view; the user-facing file is commentable text, TOML unless a later revision says otherwise).
- **Plugin marketplace / extension host / foreign runtimes** as the default authoring model. A runbook is two files.
- **Windows-first or PowerShell-default scripts.** POSIX `sh` is the convention; Windows is a port.
- **Replacing the user's shell, kubectl, Terraform, or CI.** dops-next *packages* those as runbooks.
- **Auto-commit, auto-push, or auto-install of agent-invented catalogs.**
- **Becoming a Linux distro, a theme shop, or Omarchy itself.** Steal the philosophy, not the scope.
- **Democracy over defaults.** Users can change everything; the product still picks. "Make it unopinionated so everyone is happy" is a non-goal.

---

## dops-today scoreboard

| Principle | Verdict |
| --- | --- |
| 1. Omakase defaults | Partial — defaults exist, but init + empty catalog + MCP `critical` + `github` theme fight the spirit |
| 2. Batteries included | Partial — rich binary, hollow catalog |
| 3. One-line install | Partial — curl installer exists, does not finish the job |
| 4. Beautiful by default | Partial — strong theme system, wrong default, empty first paint |
| 5. Keyboard-first | Partial — TUI is keyboard-capable; bindings not configurable; web UI is mouse-first |
| 6. Dotfile-native | Partial — files yes; JSON + mixed `~/.dops` + no product/user split |
| 7. Convention over configuration | Partial — yaml+sh is right; too many knobs; per-runbook MCP tools |
| 8. Token frugality | **Violates** — tool-per-runbook |
| 9. Scripts over reasoning | Partial — execution model is right; empty catalog and no propose/accept loop |
| 10. Safety gates for agents | **Violates on the agent path** — default `--allow-risk critical` + copyable confirms |
| 11. Human review of proposals | **Violates** — MCP executes; no draft/accept |

The rewrite (Rust CLI + TUI + MCP) should treat **8, 10, and 11** as load-bearing product changes, not as a port of current MCP behavior. Port the runbook format, vault split, risk enum, keyboard TUI, and single-binary idea. Do not port "every runbook is a tool" or "agents default to critical."
