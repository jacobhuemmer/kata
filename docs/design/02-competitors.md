# Competitor analysis for dops-next

**Date:** 2026-09-11
**Status:** design input (phase 2)
**Codename:** dops-next. This document does not propose a product name.

dops-next is a script library that is also an MCP server for AI agents. The product goal is to cut model tokens by preferring scripts and automation over model reasoning. This document compares adjacent products so later design phases can copy what works and avoid what wastes context.

## How to read this

Each competitor is scored on the same axes:

| Axis | What we look for |
|---|---|
| What it is | Category and the job it actually does |
| Install UX | How a human gets it running |
| Discoverability | How humans find a script; how agents find a script |
| Safety | Confirmation, risk, sandboxing, secrets |
| Token cost | Schema size on connect, lazy vs eager listing, output truncation |
| License / maturity | License, age, maintenance, ecosystem size |

Token numbers are labeled **measured** (a cited measurement) or **architectural** (inferred from how tools are registered). Architectural estimates are not billed-token measurements.

Sources are listed inline and collected at the end. Retrieval date for live pages is 2026-09-11 unless a page has its own publication date.

## dops today (baseline, not a competitor)

Prior art lives in `~/origin/dops` (read-only). It already occupies the same niche dops-next wants: a catalog of YAML+script runbooks with TUI, CLI, web UI, and `dops mcp serve`.

Relevant MCP facts from current dops:

- **One MCP tool per runbook.** `registerTools` walks every catalog and exposes each runbook as a tool whose input schema is generated from `runbook.yaml` parameters. See `internal/mcp/server.go` and `internal/mcp/schema.go`.
- **Eager listing.** All allowed runbooks are registered at server start. There is no search/discover tool and no deferred schema.
- **Risk gates in the schema.** High-risk tools require `_confirm_id` matching the runbook ID; critical-risk tools require `_confirm_word=CONFIRM`. Sensitive parameters are omitted from the schema. `--allow-risk` hides higher-risk runbooks from agents. Documented in [MCP / AI Agents](https://rundops.dev/guides/mcp).
- **Output truncation.** Tool results keep the last 50 lines (`maxToolOutputLines = 50` in `internal/mcp/tools.go`) and point at a log path.
- **Skills as MCP prompts.** `type: skill` runbooks load `skill.md` and register as MCP prompts, not as tools. Plan: `plans/completed/2026-04-02-mcp-skills.md`.
- **Secrets.** Parameters persist in an age-encrypted vault (X25519 + ChaCha20-Poly1305). MCP does not send secret fields in the tool schema.
- **Install.** curl installer, Homebrew, winget, Scoop, `go install`, Docker. Catalogs install from git.

dops-next's rewrite target is not "more tools." It is: keep the catalog/risk/vault strengths, stop paying an eager per-runbook schema tax, and make scripts the default way an agent acts.

---

## 1. MCP script and tool servers

These products give an agent a way to run commands. They are the closest protocol neighbors. The important split is **raw shell** vs **named scripts**.

### 1.1 mcp-shell (sonirico)

**What it is.** A Go MCP server that used to be "give the model `shell_exec`." Current default is the opposite: secure mode registers typed file/git tools and, optionally, operator-defined scripts. Raw `shell_exec` exists only with `MCP_SHELL_ALLOW_UNSAFE=1`. Source: [github.com/sonirico/mcp-shell](https://github.com/sonirico/mcp-shell) (retrieved 2026-09-11).

**Install UX.** Docker is the documented easy path (`docker run … sonirico/mcp-shell:latest`). Source build is `git clone && make install`. Wired into Claude Desktop as a stdio MCP server.

**Discoverability.** Humans configure scripts in YAML:

```yaml
scripts:
  test: ["go", "test", "./..."]
  lint: ["golangci-lint", "run"]
```

Agents see a single `run_script` tool whose argument is a **name**. The argv is operator-owned and cannot be altered by the client. There is no catalog search, no descriptions-as-resources, and no human TUI.

**Safety.** Secure mode is default. Paths resolve against `working_directory` (symlink-followed; outside is rejected). Git refs starting with `-` are rejected. Child processes get only `PATH`, `HOME`, `LANG`. Writes and scripts are opt-in. Audit log is configurable. Unrestricted mode is explicit. Docker image is Alpine, non-root. Documented in the README Security section and `SECURITY.md`.

**Token cost.** Architectural: secure mode registers many typed tools (read_file, glob, grep, a git inspection set, optional writes, plus `run_script`). That is an eager `tools/list` dump. `run_script` itself is cheap (one required `name` string). Output is capped by `max_output_size` (default 1 MiB) and `max_execution_time` (default 30s) — a size cap, not a "last N lines + log path" summary.

The `run_script` design is the piece dops-next should steal: **the model picks a name, the server owns argv.**

**License / maturity.** GPL-3.0 ([LICENSE](https://github.com/sonirico/mcp-shell/blob/master/LICENSE)). Small project (~71 commits). Actively redesigned (0.x → typed tools; legacy allowlists removed). Copyleft is a product constraint if dops-next stays permissively licensed.

### 1.2 mcp-server-commands (g0t4)

**What it is.** A Node MCP server with one tool: `runProcess` (`run_process`). The model either passes a `command_line` string (shell) or an `argv` array (no shell). Optional `stdin` lets the model pipe a script into `bash`/`python`. Source: [github.com/g0t4/mcp-server-commands](https://github.com/g0t4/mcp-server-commands) (retrieved 2026-09-11).

**Install UX.** Best in class for MCP toys: `npx mcp-server-commands` in `mcpServers`. Also local build.

**Discoverability.** None as a script catalog. The agent invents commands. Prompts exist as a user-facing slash-command template, not as a catalog.

**Safety.** README warning: review every call; use "Approve Once"; do not run as sudo. No allowlist, no sandbox, no risk levels, no secret handling. Safety is entirely the MCP client's confirmation UI.

**Token cost.** Architectural: one tool schema (small, constant). Per-call cost is the command string plus full STDOUT/STDERR as text. No truncation policy is documented. This is the "cheap schema, expensive reasoning" pattern: the model must invent argv, recover from hallucinations, and parse unbounded output.

**License / maturity.** MIT ([LICENSE](https://github.com/g0t4/mcp-server-commands/blob/master/LICENSE)). npm-published. ~470 commits. Mature enough to be a reference "one generic exec tool" implementation, not a product.

### 1.3 Desktop Commander

**What it is.** The community "Claude has a terminal" MCP server: filesystem, ripgrep search, diff editing, long-running processes, in-memory Python/Node/R, Excel/PDF/DOCX, remote MCP. Source: [github.com/wonderwhy-er/DesktopCommanderMCP](https://github.com/wonderwhy-er/DesktopCommanderMCP) (retrieved 2026-09-11). Companion app: [desktopcommander.app](https://desktopcommander.app).

**Install UX.** Excellent. `npx @wonderwhy-er/desktop-commander@latest setup` writes client config. Also bash installer, Smithery, Docker (`install-docker.sh`), Claude Code (`claude mcp add`), Codex (`codex mcp add`). Auto-updates via `@latest`.

**Discoverability.** Humans get onboarding prompts and a large tool table. Agents get a **flat, eager tool list** (config, terminal, filesystem, editing, analytics). There is no named-script catalog. Skills exist in the repo (`skills/`) as a Claude Code plugin, not as the execution model.

**Safety.** Explicitly **guardrails, not a sandbox**. [SECURITY.md](https://github.com/wonderwhy-er/DesktopCommanderMCP/blob/main/SECURITY.md) states directory allowlists and command blocklists can be bypassed via the terminal; the connected client is assumed trusted. Docker isolation is the real boundary. Audit log with rotation; `get_recent_tool_calls` for review. Output pagination (`offset`/`length`) exists specifically to stop context overflow.

**Token cost.** Architectural: large eager catalog (dozens of tools with long descriptions). That is the GitHub-MCP-class tax. Mitigations already in the product: paginated process output, recursive listing depth limits, search streaming. This is what happens when "one server does everything" meets MCP's `tools/list`.

**License / maturity.** MIT. High adoption (npm + Smithery + Docker Hub). Freemium app. The honest SECURITY.md is a maturity signal. This is the default "give the agent a computer" product dops-next should **not** become.

### 1.4 Smithery-style registries (and the official MCP Registry)

Two layers:

**Official MCP Registry** at [registry.modelcontextprotocol.io](https://registry.modelcontextprotocol.io). Preview launched 2025-09-08 ([announcement](https://modelcontextprotocol.info/blog/mcp-registry-preview/)). Reverse-DNS names (`io.github.user/server`), `server.json` metadata, DNS/GitHub namespace auth. Docs: [The MCP Registry](https://modelcontextprotocol.io/registry/about) (retrieved 2026-09-11). It is a metadata index, not a runner.

**Smithery** ([smithery.ai](https://smithery.ai), CLI docs [smithery.ai/docs/concepts/cli](https://smithery.ai/docs/concepts/cli), retrieved 2026-09-11) is a discovery directory **plus** a hosted gateway, OAuth, secrets, and a CLI that can `mcp add` into Claude/Cursor and `skill add` into Claude Code. `smithery tool list` / `tool find` / `tool call` is a human/agent discovery surface over connected servers. CLI license is commonly reported as AGPL-3.0 ([tokrepo profile](https://tokrepo.com/en/workflows/b92722d6-71c5-429f-9a32-2e81cfda8021), 2026-04-06). A 2026-05-22 scan of 497 Smithery servers found 15.3% with tool-description injection or related findings ([DEV Community](https://dev.to/bawbel/we-scanned-500-mcp-servers-on-smithery-here-is-what-we-found-4g8i)).

**Install UX.** Smithery: `npx @smithery/cli install <server> --client claude` or `smithery mcp add`. Official registry: `mcp-publisher` after proving namespace ownership.

**Discoverability.** Best in the MCP ecosystem for **servers**, not for **scripts inside a server**. Agents still pay whatever `tools/list` the installed server emits. Smithery's `tool find` is a CLI search, not a protocol-level lazy catalog.

**Safety.** Registry trust is namespace authentication, not runtime sandboxing. Hosting a third-party MCP server is a supply-chain problem (see the Smithery scan). Secrets on Smithery live in the gateway; that is convenient and is also a trust boundary dops-next should not copy for operator vaults.

**Token cost.** Registries do not themselves dump tool schemas. They make it easy to **accumulate** servers, which is how sessions reach 40k–80k definition tokens (see § token measurements below).

**License / maturity.** Official registry: preview, community-owned, MIT-family MCP project. Smithery: commercial + OSS CLI, founded 2024, skills registry added. MCP Toplist tracked 125,904 servers across five directories as of 2026-09-11 ([mcptoplist.com](https://mcptoplist.com/)).

**Lesson:** publish dops-next to the official registry for humans. Do not rely on a marketplace to solve agent discoverability of *runbooks*.

### 1.5 Other "run scripts via MCP" servers

These are the actual "named script as tool" experiments, closest in spirit to dops:

| Server | Pattern | Notes | URL |
|---|---|---|---|
| PromptExecution/just-mcp | 4 meta-tools: `list_recipes`, `run_recipe`, `get_recipe_info`, `validate_justfile` | Explicitly sold as a context-saving abstraction vs bash. MIT-looking OSS. | [github.com/PromptExecution/just-mcp](https://github.com/PromptExecution/just-mcp) |
| ewired/just-mcp | Eager: JSON dump of justfile → **one MCP tool per recipe** with parameter schema | Deno one-liner; `ALLOWED_RECIPES` env. | [github.com/ewired/just-mcp](https://github.com/ewired/just-mcp) |
| toolprint/just-mcp | Dynamic tools from justfiles; slash command `/just:do-it` | Rust binary; "share the same justfile with the agent." | [github.com/toolprint/just-mcp](https://github.com/toolprint/just-mcp) |
| millsaj/just-runner-mcp | npx wrapper around a justfile directory | Tiny. | [github.com/millsaj/just-runner-mcp](https://github.com/millsaj/just-runner-mcp) |
| task-mcp | just recipes exposed only if tagged `[group('allow-agent')]` | Security boundary: untagged recipes never appear. `TASK_MCP_MODE=agent-only`. | [lib.rs/crates/task-mcp](https://lib.rs/crates/task-mcp) (2026-04-28) |
| rsclarke/mcp-taskfile-server | **Each Taskfile task becomes its own MCP tool** with generated variable schema | Native go-task library. Roots via MCP `roots/list`. | [github.com/rsclarke/mcp-taskfile-server](https://github.com/rsclarke/mcp-taskfile-server) |
| tsoernes/skills-mcp | Skills directory → `list_skills` / `get_skill_detail` / `search_skill_index` | Lazy-friendly: list metadata, fetch body on demand. | [github.com/tsoernes/skills-mcp](https://github.com/tsoernes/skills-mcp) |

The two stable patterns:

1. **Meta-tools (list + run + info)** — constant schema, extra round-trip, scales to hundreds of scripts.
2. **One tool per script** — matches current dops; dies of `tools/list` as the catalog grows.

dops-next should treat (1) as the default agent API and (2) as an opt-in for tiny catalogs or clients that cannot search.

---

## 2. Human task runners (scripts without MCP, plus new MCP/skill bolts)

### 2.1 just

**What it is.** A command runner (not a build system). Recipes live in a `justfile`. Source: [github.com/casey/just](https://github.com/casey/just) (retrieved 2026-09-11; ~35.6k stars). Docs book: [just.systems](https://just.systems).

**Install UX.** `brew install just`, cargo, nix, prebuilt binaries. Zero-config once a justfile exists. `just` lists and runs.

**Discoverability.** Humans: `just --list`, `just --summary`, comments above recipes become help, shell completions. Agents: the repo now ships `skills/just` (Agent Skills layout). Third-party MCP servers (above) are how most agents actually run recipes. `just --dump --dump-format json` is the structured surface MCP wrappers parse.

**Safety.** `--dry-run`, `--check`. No risk levels, no confirmation framework, no vault. Recipes are ordinary shell. `.env` loading is built in (`just` dotenv support) — convenient, easy to leak into agent context if an MCP wrapper dumps env.

**Token cost.** just itself is CLI: agents pay `just -l` output (small) plus whatever they read from the justfile (can be large if they open the file). PromptExecution's pitch is exactly dops-next's pitch: *do not let the model read the justfile; give it names, params, and hints.* Per-recipe MCP wrappers reintroduce the eager-schema tax.

**License / maturity.** CC0 ([LICENSE](https://github.com/casey/just/blob/master/LICENSE)). Extremely mature (v1.57.x in 2026). De facto standard for repo-local recipes.

### 2.2 Task (Taskfile)

**What it is.** Make-inspired YAML task runner. `Taskfile.yml`, deps, vars, includes, checksum/timestamp skip. Site: [taskfile.dev](https://taskfile.dev) (retrieved 2026-09-11). Repo: [github.com/go-task/task](https://github.com/go-task/task).

**Install UX.** Single binary; Homebrew, Snap, Scoop, `install-task.sh`. `task --list` / `task --list-all`.

**Discoverability.** Humans: YAML `desc:`, `task --list`, generated docs. Agents: `mcp-taskfile-server` maps **each task to a tool** (eager). Taskfile is widely used in MCP *projects* as their own build file (Firefox DevTools MCP, Firebolt MCP) — that is humans, not agents, discovering tasks.

**Safety.** No first-class risk model. Vars can come from `.env`. Precondition checks exist. Not a sandbox. CI-oriented (idempotent tasks, status/checksum).

**Token cost.** Same fork as just: CLI listing is cheap; one-tool-per-task MCP is expensive as Taskfiles grow. `desc` fields are short, so per-task schemas can be smaller than a chatty MCP server — still linear in task count.

**License / maturity.** MIT ([LICENSE](https://github.com/go-task/task/blob/main/LICENSE)). Production-grade; public adopters listed on taskfile.dev include Docker, HashiCorp, Anthropic.

### 2.3 mise tasks

**What it is.** mise-en-place: tools + env + tasks in `mise.toml` or `mise-tasks/` file scripts. Docs: [mise.jdx.dev/tasks](https://mise.jdx.dev/tasks/) (updated 2026-09-07). Repo: [github.com/jdx/mise](https://github.com/jdx/mise).

**Install UX.** `curl https://mise.run | sh`. Tasks run with the project's tool versions and env automatically (`mise run <task>`). File tasks are real scripts with `#MISE description=` headers — the closest human UX to dops catalogs.

**Discoverability.** Humans: `mise tasks ls`, `mise tasks info`, `mise generate task-docs` (Markdown, `--multi` per task, `--inject` into README). Agents:

- Built-in experimental MCP: `MISE_EXPERIMENTAL=1 mise --cd <project> mcp` ([mise.jdx.dev/mcp.html](https://mise.jdx.dev/mcp.html), 2026-09-07).
- Resources: `mise://tasks`, `mise://tools`, `mise://env`, `mise://config`.
- Tools: `list_commands` (with declared effect `read`/`write`/`destructive`), `run_task(task, args)`, `install_tool` (stub, errors).
- `mise skills` — "agent skills the active tools ship."
- Discussion to symlink tool-bundled `skills/<name>/SKILL.md` into `.claude/skills/` the way shims work ([mise#9479](https://github.com/jdx/mise/discussions/9479)).

This is the most complete "task runner grew an agent API" of the three.

**Safety.** `run_task` runs as the user, sets `MISE_YES=1`, no interactive stdin. Docs tell you to use client tool approval and to review tasks first. **`mise://env` returns real values, including secrets.** Resource reads evaluate templates — not a sandbox. mise has a separate configuration trust model ([security](https://mise.jdx.dev/security.html)). `list_commands` effect labels are documentation, not enforcement.

**Token cost.** Architectural: **constant tool list (3 tools)** + resources the agent reads on demand. This is the right MCP shape. Risk: `mise://tasks` can dump every task definition in one resource read; `mise://env` can dump secrets into context. Output of `run_task` is captured stdout/stderr after completion (not streamed); timeout via `task.timeout`.

**License / maturity.** MIT. Very widely installed as an asdf successor. MCP is explicitly experimental (`MISE_EXPERIMENTAL=1`, tools may change).

### 2.4 Comparison of the three runners vs dops catalogs

| | just | Taskfile | mise tasks | dops catalogs |
|---|---|---|---|---|
| Unit | recipe | task | toml or file script | runbook.yaml + script |
| Human list | `just -l` | `task --list` | `mise tasks ls` | TUI / `dops` search |
| Agent API | third-party MCP | third-party MCP | first-party MCP (experimental) | first-party MCP (eager per-runbook) |
| Params | recipe args | vars | args / env | typed YAML params |
| Risk | none | none | effect labels on CLI, not tasks | low/medium/high/critical + confirm |
| Secrets | dotenv | dotenv | env resources leak | age vault, omitted from schema |
| Docs gen | comments | desc | `mise generate task-docs` | VitePress site |

dops-next should keep risk + vault + typed params, and steal mise's **constant-size MCP surface** plus just's **doc comments as agent hints**.

---

## 3. Notebooks, terminals, and history

### 3.1 Runme

**What it is.** Markdown-as-notebook for DevOps. Cells are bash (and other) with a kernel, CLI, VS Code extension, CI. Site: [runme.dev](https://runme.dev). Docs: [docs.runme.dev](https://docs.runme.dev). Extension: [github.com/runmedev/vscode-runme](https://github.com/runmedev/vscode-runme) (latest 3.16.1 as of 2026-01-26).

**Install UX.** `brew install runme`, `npx runme`, VS Code Marketplace. Open any `.md` as a notebook.

**Discoverability.** Humans: notebook UI, cell Run buttons, Notebooks Explorer. Agents (2026): Runme AppKernel exposes **one** MCP tool, `agent_tools_v1_NotebookService_ExecuteCode`, that runs JavaScript against notebook helpers (`notebooks.list()`, `notebooks.execute()`, …). Documented in generated Go ([pkg.go.dev toolsv1mcp](https://pkg.go.dev/github.com/runmedev/runme/v3/api/gen/proto/go/agent/tools/v1/toolsv1mcp), 2026-08-22). That is code-mode, not a script catalog.

**Safety.** Cells run in the user's environment. Interactive mode uses a real terminal. Prompts for parameters exist for "generic docs." Not a risk-level system. CI can treat notebooks as tests (bitrot prevention) — a property dops catalogs also want.

**Token cost.** Architectural: one fat tool description (the ExecuteCode docstring is long) plus whatever JS the model writes plus cell output. Cheap catalog, expensive reasoning — same failure mode as generic shell, with better structure if the agent learns the helper API.

**License / maturity.** Apache-2.0. Real product (stateful/Runme), VS Code + CLI + Docker + CDE. Agent API is newer than the notebook UI.

### 3.2 Warp workflows

**What it is.** Parameterized, searchable commands inside Warp. Two formats: legacy YAML workflows and Warp Drive workflows (recommended). YAML spec: [docs.warp.dev/terminal/entry/yaml-workflows](https://docs.warp.dev/terminal/entry/yaml-workflows/) (retrieved 2026-09-10). Drive: [docs.warp.dev/knowledge-and-collaboration/warp-drive/workflows](https://docs.warp.dev/knowledge-and-collaboration/warp-drive/workflows/) (retrieved 2026-09-10). Agent: [docs.warp.dev/agents](https://docs.warp.dev/agents/) (retrieved 2026-09-11).

**Install UX.** Warp app (proprietary). YAML files in `~/.warp/workflows/` or `repo/.warp/workflows/`. Drive workflows sync in-app. Agent CLI: `curl -fsSL https://app.warp.dev/download/agent-cli | bash` ([blog, 2026-08-04](https://www.warp.dev/blog/introducing-the-warp-agent-cli-coding-agent)).

**Discoverability.** Humans: Command Palette / `Ctrl+Shift+R`, argument cycling (`Shift+Tab`), community workflow repo [github.com/warpdotdev/workflows](https://github.com/warpdotdev/workflows). Agents: Warp Agent has Skills, Rules, Memory, MCP, and full terminal use. Workflows can be saved from agent results. This is "the terminal product owns the agent," not "a library agents attach to."

**Safety.** Agent permission profiles (read / plan / execute). Inline approval cards. Cloud agents on Warp's Automation Platform. YAML workflows have no risk field — they paste into the terminal.

**Token cost.** Not an MCP server. Cost is Warp's agent loop (commands + diffs + terminal buffer). Workflows themselves are small YAML (name, command, arguments, description). Saving a workflow from an agent result is a way to **promote a successful command into a named script** — the same loop dops-next wants from session mining (later phase).

**License / maturity.** Proprietary client; workflow YAML spec and community repo are public. High polish, high lock-in. Not a library dops-next can embed.

### 3.3 Atuin (history + scripts + hooks)

**What it is.** Encrypted, synced, SQLite-backed shell history, plus **scripts**, KV, dotfiles, agent hooks, and Atuin AI. Repo: [github.com/atuinsh/atuin](https://github.com/atuinsh/atuin) (~31.6k stars). CLI crate 18.21.0 (2026-08-31, [crates.io](https://crates.io/crates/atuin)).

**Scripts.** Since 18.5.0: `atuin scripts new|list|run|get`. Create from last N history entries. Minijinja `{{vars}}`. Custom shebang. E2E-encrypted sync. Blog: [Atuin Scripts, 2025-04-09](https://blog.atuin.sh/atuin-scripts-shareable-syncable-shell-snippets/). Crate: [atuin-scripts](https://lib.rs/crates/atuin-scripts).

**History as agent memory.** `atuin hook install claude-code|codex|pi` records agent bash with `ATUIN_HISTORY_AUTHOR`. Docs: [AI Agent Hooks](https://docs.atuin.sh/latest/guide/agent-hooks/) (moved from `/cli/guide/agent-hooks/`, retrieved 2026-09-11). `secrets_filter` (default true) drops AWS keys, GitHub PATs, Slack tokens from history ([config](https://docs.atuin.sh/cli/configuration/config/)).

**MCP / AI.** Community cheatsheets mention `atuin mcp` for history search ([karandeepsingh.ca, 2026-08-11](https://karandeepsingh.ca/cheatsheets/terminal/atuin/)). Atuin AI gained `atuin_history` as a local tool with a permission system (PR [#3370](https://github.com/atuinsh/atuin/issues/3370), merged 2026-03). Desktop runbooks (separate product) were open-sourced 2025-09-30 ([blog](https://blog.atuin.sh/atuin-desktop-open-source/)).

**Install UX.** `curl … https://setup.atuin.sh | sh`, brew, cargo. Immediate `Ctrl-R` value.

**Discoverability.** Humans: TUI search, `atuin scripts list`, tags. Agents: hooks make history queryable; scripts are a personal snippet store, not a team catalog with risk metadata.

**Safety.** History encryption (PASETO V4). `secrets_filter`. Script sync is E2E encrypted. Execution is still your shell. Desktop is a local runbook UI, closer to Runme than to dops MCP.

**Token cost.** History search returns commands (small) not full output (output capture is roadmap, [packfiles post, 2026-08-25](https://blog.atuin.sh/making-atuin-sync-32x-faster-with-packfiles/)). Scripts as tools would be cheap if exposed as `run_script(name, vars)` — today they are a CLI.

**License / maturity.** MIT. Very mature history product; scripts and agent hooks are newer. Closest competitor on **mining real usage into named automation**.

---

## 4. Agent script registries and skills-as-scripts

### 4.1 Nx (and the "delete your MCP tools" move)

**What it is.** Monorepo build system that shipped a large MCP server, then deleted most of it in favor of Agent Skills plus a thin MCP for Nx Cloud/CI. Blog: [Why we deleted (most of) our MCP tools](https://nx.dev/blog/why-we-deleted-most-of-our-mcp-tools) (Max Kless, 2026-02-17). Follow-up: [Teach Your AI Agent How to Work in a Monorepo](https://nx.dev/blog/nx-ai-agent-skills). Reference: [Nx MCP Server](https://nx.dev/docs/reference/nx-mcp).

**Install UX.** `npx nx configure-ai-agents` writes MCP config, skills, and `CLAUDE.md`/`AGENTS.md` for Claude Code, Cursor, Copilot, Gemini, Codex, OpenCode. Alternative: `npx skills add nrwl/nx-ai-agents-config` (skills only).

**Discoverability.** Skills teach `nx show projects`, `nx generate`, graph navigation. MCP remains for authenticated Cloud and running-process connectivity. Minimal MCP is default; `--no-minimal` restores the old tool dump for clients without skills.

**Safety.** Skills tell the agent to verify generators; they are instructions, not enforcement. Cloud MCP carries Nx Cloud auth.

**Token cost.** Measured in Nx's own benches: question-answering token use dropped with skills vs MCP-only; generation tasks used **more** tokens because agents actually ran generators and verified — "better results matter more than cheaper bad results." They note Anthropic dynamic tool loading later narrowed the MCP gap.

**License / maturity.** MIT (Nx). This is the highest-signal 2026 design change in the space: **MCP for connectivity, skills for knowledge, CLI/scripts for work.**

### 4.2 Other agent script / skill registries

| Registry | Job | URL |
|---|---|---|
| Agent Skills spec | `SKILL.md` + `scripts/` + `references/` + `assets/`; progressive disclosure | [agentskills.io/specification](https://agentskills.io/specification) |
| Official MCP Registry | server metadata, not skills | [registry.modelcontextprotocol.io](https://registry.modelcontextprotocol.io) |
| Smithery skills | `smithery skill search/add` | [CLI docs](https://smithery.ai/docs/concepts/cli) |
| skillet | Git-backed skill package manager; optional MCP | [glama skillet](https://glama.ai/mcp/servers/jnMetaCode/skillet) (2026-09-10) |
| skills-mcp (gengirish) | Search/install ~9k GitHub skills | [github.com/gengirish/skills-mcp](https://github.com/gengirish/skills-mcp) |
| Agent Plugins 1.0 | Skills + `mcp.json` in one folder (Amazon, Cursor, Microsoft, OpenAI, Vercel; 2026-08-06) | [getclaudeskills.com, 2026-08-12](https://www.getclaudeskills.com/blog/agent-plugins-explained) |
| GitHub MCP toolsets | `--toolsets` / `--tools` to cut a 93-tool dump | [server-configuration.md](https://github.com/github/github-mcp-server/blob/main/docs/server-configuration.md) |

GitHub MCP is the cautionary measurement: ~55,000 tokens across 93 tools vs ~4,200 for the 26-tool default toolset ([getunblocked.com, 2026-05-15, updated 2026-09-10](https://getunblocked.com/blog/github-mcp-token-cost/)).

### 4.3 Claude Code skills (`SKILL.md` + `scripts/`)

**What it is.** Folders Claude loads on demand. Official docs: [code.claude.com/docs/en/skills](https://code.claude.com/docs/en/skills) (retrieved 2026-09-11). Standard: [agentskills.io](https://agentskills.io/specification). Steering guidance: [Steering Claude Code, 2026-06-18](https://claude.com/blog/steering-claude-code-skills-hooks-rules-subagents-and-more).

**Layout.**

```
skill-name/
├── SKILL.md          # frontmatter + instructions
├── scripts/          # executed, not loaded
├── references/       # read on demand
└── assets/
```

**Progressive disclosure (spec + Claude Code):**

1. **Metadata** (`name` + `description`) always in context. Spec: ~100 tokens/skill. Claude Code truncates listing text at 1,536 characters.
2. **SKILL.md body** loaded when triggered. Spec recommends < 5,000 tokens / keep under 500 lines.
3. **Resources** on demand. **Scripts are executed via bash; script source need not enter the context window — only output does.**

**Install UX.** Drop a folder in `~/.claude/skills/` (personal) or `.claude/skills/` (project). Plugins, managed enterprise dir, claude.ai sync. Live reload of `SKILL.md`. `/skill-name` invocation. `disable-model-invocation` for user-only. `allowed-tools` pre-approves for one turn. Dynamic `!`command injection can inline live command output into the skill body.

**Safety.** `allowed-tools` / `disallowed-tools`. Synced skills from claude.ai are sanitized; Cowork disables skill shell execution. Skills are not a sandbox; they grant whatever the session can do. `license` and `compatibility` fields exist in the spec; Claude Code accepts them without enforcing.

**Token cost.** This is the reference design for "cheap until used." The important dops-next mapping:

| Skills | dops-next analog |
|---|---|
| name + description in the always-on index | runbook id + one-line description |
| SKILL.md body | when-to-use, params, risk, examples |
| `scripts/` | the runbook script (run, don't paste) |
| `references/` | long runbook docs / schema |

Current dops inverts this: the **script's parameter schema** is always on, and skill markdown is a separate MCP prompt.

### 4.4 Codex skills

**What it is.** Same Agent Skills format, Codex-specific load paths and budget. Docs: [learn.chatgpt.com/docs/build-skills](https://learn.chatgpt.com/docs/build-skills) (OpenAI redirect from developers.openai.com/codex/skills; retrieved 2026-09-11). Sample authoring skill: [openai/skills skill-creator](https://github.com/openai/skills/blob/main/skills/.system/skill-creator/SKILL.md).

**Install UX.** Folders under `$CWD/.agents/skills`, parent dirs, `$REPO_ROOT/.agents/skills`, `$HOME/.agents/skills`, `/etc/codex/skills`, plus bundled system skills. `$skill-installer`, `$skill-creator`. Plugins for distribution. `[[skills.config]]` in `~/.codex/config.toml` to disable without deleting.

**Discoverability.** Explicit `$skill` / `/skills`; implicit via description. Codex initial skill list is capped at **2% of the context window** (or 8,000 characters if unknown). Descriptions get shortened first; overflow skills are omitted with a warning. `agents/openai.yaml` can set `allow_implicit_invocation: false` and declare MCP tool dependencies.

**Safety.** Codex permission profiles and sandboxing are host-level ([Codex sandboxing docs](https://learn.chatgpt.com/codex/sandboxing) exist as a product surface). Skill `allowed-tools` is spec-experimental. Prefer instructions over scripts unless determinism is required (official best practice).

**Token cost.** Same three-level disclosure. Extra: hard budget on the **index**, which dops-next should copy if a catalog has hundreds of runbooks.

---

## 5. Cross-cutting token measurements

These numbers are about MCP in general. They bound what dops-next must not repeat.

| Claim | Figure | Date | URL |
|---|---|---|---|
| Anthropic code-mode example | 150,000 → 2,000 tokens (98.7%) by treating MCP tools as files the agent reads | 2025-11-04 | [Code execution with MCP](https://www.anthropic.com/engineering/code-execution-with-mcp) |
| GitHub MCP full surface | ~55,000 tokens, 93 tools | 2026-05-15 (updated 2026-09-10) | [GitHub MCP Token Cost](https://getunblocked.com/blog/github-mcp-token-cost/) |
| GitHub MCP default toolset | ~4,200 tokens, 26 tools | same | same |
| Notion MCP | ~19,050 est. tokens, 24 tools | 2026-09-01 | [The MCP tax](https://okaneland.com/study/the-mcp-context-tax/) |
| Filesystem MCP | ~3,240 est. tokens, 14 tools | 2026-09-01 | same |
| Claude Code tool search default | schemas deferred; session-start delta inside noise | 2026-09-01 | same |
| Anthropic Tool Search | ~85% of definition tokens saved; Opus 4 MCP eval 49% → 74% | cited 2026-06-17 | [ismaelramos.dev](https://www.ismaelramos.dev/blog/what-an-mcp-server-costs-you-in-tokens/) |
| mcp-tool-search proxy | 4 proxy tools ~600 tokens vs 10k for 50 backend tools | retrieved 2026-09-11 | [github.com/KGT24k/mcp-tool-search](https://github.com/KGT24k/mcp-tool-search) |
| CLI vs MCP (84 tools) | full schema ~15,540 vs skill list ~300 | 2026-02-25 | [ubos.tech](https://ubos.tech/news/cli-vs-mcp-token-cost-savings-and-lazy-loading-explained/) |
| Scalekit GitHub task | MCP agent 44,026 vs CLI 1,365 tokens | cited 2026-09-01 | [okaneland.com](https://okaneland.com/study/the-mcp-context-tax/) |

**Implication for dops-next:** even a "good" per-runbook schema will lose to (a) a 4-tool search/run/info surface, (b) skills metadata + scripts, or (c) "just run the CLI." Current dops is (eager per-runbook) with 50-line truncation — half the lesson (truncate output) and not the other half (do not dump schemas).

Claude Code's deferred MCP loading helps **clients**. dops-next still has to work for clients that eager-load (Cursor without tool search, older hosts, CI agents). Server-side lazy listing is the portable fix.

---

## 6. Comparison matrix

Legend for token: **Low** = O(1) tools or metadata-only index; **Med** = small eager set or truncated output; **High** = linear in catalog size or unbounded shell output.

| Product | Kind | Agent API | Listing | Output control | Safety model | Secrets | Human UX | License | Maturity |
|---|---|---|---|---|---|---|---|---|---|
| **dops (today)** | Script catalog + MCP | 1 tool / runbook | Eager | Last 50 lines + log | Risk levels + confirm | age vault; omitted from schema | TUI/CLI/web | MIT | Real product |
| mcp-shell | Typed FS/git + named scripts | `run_script(name)` + many typed tools | Eager typed set | 1 MiB / 30s | Path jail; writes/scripts opt-in; unsafe opt-in | Stripped child env | Docker/YAML | GPL-3.0 | Small, sharp |
| mcp-server-commands | Generic exec | 1 tool `runProcess` | Eager, tiny | Full stdio | Client approve only | None | npx | MIT | Utility |
| Desktop Commander | Computer-use MCP | Many tools | Eager, large | Pagination | Guardrails; Docker is sandbox | Host env | npx setup, app | MIT | High adoption |
| Smithery / official registry | Server marketplace | N/A (installs servers) | N/A | N/A | Namespace auth; hosted secrets | Gateway vault | CLI / web | Mixed (registry OSS; Smithery commercial) | Preview / commercial |
| just | Recipe runner | CLI; 3rd-party MCP | `just -l` or per-recipe tools | Recipe stdio | Dry-run; no risk | dotenv | justfile | CC0 | Very high |
| Taskfile | YAML tasks | CLI; 1 tool / task MCP | `task --list` or eager tools | Task stdio | Preconditions | dotenv | Taskfile.yml | MIT | High |
| mise tasks | Tools+env+tasks | 3 MCP tools + resources | Lazy resources | Captured stdio | Effect labels; `MISE_YES=1` | **env resource can leak** | mise.toml / scripts | MIT | High; MCP experimental |
| Runme | Markdown notebooks | 1 ExecuteCode tool | Code-mode | Cell output | User env | In notebook/env | VS Code + CLI | Apache-2.0 | High UI; new agent API |
| Warp workflows | Terminal snippets | Warp Agent (not MCP lib) | Palette search | Terminal | Permission profiles | Warp account | Warp app | Proprietary | High, locked-in |
| Atuin | History + snippets | CLI; AI tools; hooks | TUI / `scripts list` | Commands, not output | E2E crypto; secrets_filter | Encrypted sync | Ctrl-R | MIT | High |
| Nx | Monorepo + skills | Skills + thin MCP | Skill metadata | CLI output | Skill instructions | Cloud auth in MCP | `configure-ai-agents` | MIT | High, post-MCP-trim |
| Claude Code skills | Skill folders | Progressive load | name+desc always | Script **output only** | allowed-tools; host perms | Skill-dependent | Drop folder | Host product + OSS spec | Canonical pattern |
| Codex skills | Skill folders | Progressive load + 2% index cap | name+desc+path | Script output only | Host sandbox + profiles | Skill-dependent | `.agents/skills` | Host product + OSS spec | Canonical pattern |

---

## 7. Lessons for dops-next

Ranked by combined impact on **token cost** and **agent adoption**. Higher = do this first.

### 1. Do not register one MCP tool per runbook as the default

**Impact: tokens (critical), adoption (high).** Current dops and ewired/just-mcp / mcp-taskfile-server scale linearly. GitHub MCP's 93-tool ~55k dump is the existence proof that "rich schemas" lose. Default agent API should be a **constant-size** surface: `search` / `list` / `info` / `run` (the PromptExecution/just-mcp and mise MCP shape). Offer eager per-runbook tools only as an opt-in for tiny catalogs.

### 2. Progressive disclosure is the product, not an optimization

**Impact: tokens (critical), adoption (high).** Copy Agent Skills three layers ([spec](https://agentskills.io/specification)):

1. Always-on index: id, one-line description, risk, trigger words (budget it; Codex caps the index at 2% of context).
2. On select: params, examples, confirmation requirements.
3. Execution: run the script; **do not put script source in context**.

dops already has `runbook.yaml` + `script.sh` + optional `skill.md`. Wire them as layers, not as N tools + M prompts.

### 3. Prefer running scripts over exposing shell

**Impact: tokens (high), adoption (high), safety (high).** mcp-server-commands and Desktop Commander make the model invent argv. mcp-shell's `run_script(name)` and dops runbooks make the operator invent argv once. Named scripts cut reasoning tokens and hallucination. Keep a generic shell out of the default tool list (mcp-shell's lesson: unsafe is opt-in).

### 4. Truncate, structure, and point at a log — on the server

**Impact: tokens (high).** Client-side tool search does not shrink a 10,000-line kubectl dump. Keep dops's last-N-lines + `log_path` (50 is a starting default; make it configurable). Add: exit code, duration, a one-line summary, optional JSON. Desktop Commander's pagination and Anthropic's "filter in the execution environment" are the same idea. Wrapper scripts that pre-filter (the [CLI scripts as agent tools](https://agentpatterns.ai/tool-engineering/cli-scripts-as-agent-tools/) pattern, 2026-09-11) belong in catalogs, not in the model.

### 5. Skills teach *when*; MCP/CLI *does*

**Impact: adoption (critical), tokens (high).** Nx's 2026-02-17 write-up is the strategy: delete MCP tools that wrap a CLI the agent can already run; keep MCP for auth, streaming, and things the agent cannot reach. dops-next should ship:

- A short skill (`SKILL.md`) that says when to search/run a runbook vs when to use the host shell.
- MCP (or CLI) that actually executes.
- Do not dump the catalog into `CLAUDE.md`.

### 6. Operator-owned argv, model-owned arguments

**Impact: safety (critical), tokens (medium).** mcp-shell: client picks a name, cannot change argv. dops params already map to env. Keep that. Never accept a free-form `command` string in the default profile. `task-mcp`'s `[group('allow-agent')]` is a second gate: some recipes are human-only.

### 7. Risk confirmation must survive lazy loading

**Impact: safety (critical), adoption (medium).** Current `_confirm_id` / `_confirm_word` fields live on the eager schema. If schemas load on demand, confirmation still has to happen (in `run` args, elicitation, or a host permission prompt). High/critical runbooks should not become easier to fire because they were missing from `tools/list`. Keep `--allow-risk` as a **server-side** filter so hidden tools cannot be guessed-and-called.

### 8. Design for clients that still eager-load

**Impact: tokens (high), adoption (high).** Claude Code tool search (default 2026) defers schemas; many hosts do not. A server with 200 eager tools is unusable in Cursor-without-search and in naive CI agents. Server-side search tools work everywhere. Client-side deferral is a bonus, not a plan.

### 9. File-shaped catalogs beat string-shaped catalogs for code-mode agents

**Impact: tokens (high), adoption (medium).** Anthropic's 2025-11-04 post: agents navigate filesystems well. A catalog layout like `catalogs/<name>/<runbook>/{runbook.yaml,script.sh,SKILL.md}` lets a code-mode agent `list` / `read` / `exec` without any MCP tools. dops already has this on disk. Preserve it; make it the API for agents that prefer CLI over MCP (Scalekit: GitHub CLI 1,365 vs MCP 44,026 tokens on one task).

### 10. Install UX is a one-liner or it loses

**Impact: adoption (high).** Winners: `npx … setup` (Desktop Commander), `claude mcp add`, `npx nx configure-ai-agents`, `curl | sh` (dops, mise, Atuin). dops-next needs: binary + `mcp serve` snippet **and** a skill installer that writes `.claude/skills/` and `.agents/skills/` from the same catalog. Smithery/official registry listing is distribution, not the core loop.

### 11. Secrets never belong in `tools/list` or in resources

**Impact: safety (critical).** Keep dops's "omit secret params from schema." Do **not** copy mise's `mise://env` (returns real secret values). Atuin's `secrets_filter` is the right default for any history/audit log. Vault stays local (age or equivalent); hosted gateways (Smithery) are a different trust model.

### 12. Human discoverability still matters (TUI/list/docs)

**Impact: adoption (medium).** just `--list`, `mise generate task-docs`, Warp palette, dops TUI. Agents will not be the only user. Generate an agent-readable index (`llms.txt` / task-docs / runbook list JSON) from the same metadata the TUI uses. One catalog, three projections: human TUI, agent index, CI CLI.

### 13. Mine history into named scripts

**Impact: adoption (medium), tokens (medium, delayed).** Atuin Scripts (`--last N`) and Warp "save as workflow" close the loop from a successful ad-hoc command to a parameterized, shareable unit. dops-next should make "promote this invocation to a runbook" trivial. That is how catalogs stay cheaper than reasoning: the good argv gets frozen.

### 14. Effect labels are not enforcement; tags can be

**Impact: safety (medium).** mise `list_commands` effects are hints. `task-mcp` `allow-agent` is a boundary. dops risk levels are a boundary if the server enforces them. Keep enforcement on the server. Expose effects/risk in the **index** so the model can choose, then refuse on the backend anyway.

### 15. License and supply chain are product constraints

**Impact: adoption (medium).** Prefer permissive licenses (dops is MIT; just CC0; Task/mise/Atuin/Nx MIT; Runme Apache-2.0). Do not take a GPL-3.0 dependency as the script runner (mcp-shell). Do not auto-install arbitrary MCP servers from a marketplace into an operator tool (Smithery scan, 2026-05-22). Publish **to** the official registry; do not become one.

---

## 8. What dops-next should not copy

- Desktop Commander's "one server is your computer" surface (token bomb + bypassable guardrails).
- Eager one-tool-per-task MCP (Taskfile server, current dops, GitHub MCP full set).
- mise `mise://env` dumping secrets as a resource.
- Warp lock-in as the only way to run workflows.
- Skill bodies that inline huge procedures (Codex/Claude both warn: keep SKILL.md small; split references).
- Treating client-side tool search as sufficient server design.

---

## 9. Deviations

- Web search and page fetches worked. No competitor was marked unverified for lack of network.
- Star counts and registry totals are snapshots on 2026-09-11 and will drift; they are cited as snapshots.
- Some token figures are third-party estimates (chars/4) labeled in §5.
- `atuin mcp` is documented in a 2026-08-11 community cheatsheet; treat first-party MCP status as possibly in flux versus Atuin AI's local `atuin_history` tool.
- Warp YAML workflows are legacy relative to Warp Drive; both are described.
- This document does not propose product names.

---

## 10. Sources (collected)

Publication or retrieval dates in parentheses.

**dops / protocol**

- [dops README / MCP](https://github.com/rundops/dops) (retrieved 2026-09-11)
- [rundops.dev MCP guide](https://rundops.dev/guides/mcp) (retrieved 2026-09-11)
- [MCP specification](https://modelcontextprotocol.io/) (retrieved 2026-09-11)
- [Official MCP Registry about](https://modelcontextprotocol.io/registry/about) (2026-09-08 docs update)
- [Introducing the MCP Registry](https://modelcontextprotocol.info/blog/mcp-registry-preview/) (2025-09-08)

**MCP servers**

- [sonirico/mcp-shell](https://github.com/sonirico/mcp-shell) and [LICENSE (GPL-3.0)](https://github.com/sonirico/mcp-shell/blob/master/LICENSE) (2026-09-11)
- [g0t4/mcp-server-commands](https://github.com/g0t4/mcp-server-commands) (2026-09-11)
- [DesktopCommanderMCP](https://github.com/wonderwhy-er/DesktopCommanderMCP) and [SECURITY.md](https://github.com/wonderwhy-er/DesktopCommanderMCP/blob/main/SECURITY.md) (2026-09-11)
- [Smithery CLI](https://smithery.ai/docs/concepts/cli) (2026-05-02 docs)
- [Bawbel Smithery scan](https://dev.to/bawbel/we-scanned-500-mcp-servers-on-smithery-here-is-what-we-found-4g8i) (2026-05-22)
- [PromptExecution/just-mcp](https://github.com/PromptExecution/just-mcp) (2026-09-11)
- [ewired/just-mcp](https://github.com/ewired/just-mcp) (2025-04-25)
- [toolprint/just-mcp](https://github.com/toolprint/just-mcp) (2026-09-11)
- [task-mcp](https://lib.rs/crates/task-mcp) (2026-04-28)
- [rsclarke/mcp-taskfile-server](https://github.com/rsclarke/mcp-taskfile-server) (2026-09-11)
- [github/github-mcp-server configuration](https://github.com/github/github-mcp-server/blob/main/docs/server-configuration.md) (2026-09-11)

**Task runners**

- [casey/just](https://github.com/casey/just) (2026-09-11)
- [taskfile.dev](https://taskfile.dev) (2026-09-11)
- [go-task/task](https://github.com/go-task/task) (2026-09-11)
- [mise tasks](https://mise.jdx.dev/tasks/) (2026-09-07)
- [mise MCP](https://mise.jdx.dev/mcp.html) (2026-09-07)
- [mise#9479 skills bridging](https://github.com/jdx/mise/discussions/9479) (2026-09-11)

**Notebooks / terminals / history**

- [runme.dev](https://runme.dev) (2026-09-11)
- [Runme ExecuteCode tool](https://pkg.go.dev/github.com/runmedev/runme/v3/api/gen/proto/go/agent/tools/v1/toolsv1mcp) (2026-08-22)
- [Warp YAML workflows](https://docs.warp.dev/terminal/entry/yaml-workflows/) (2026-09-09)
- [Warp Drive workflows](https://docs.warp.dev/knowledge-and-collaboration/warp-drive/workflows/) (2026-09-10)
- [Warp Agents](https://docs.warp.dev/agents/) (2026-09-11)
- [Warp Agent CLI launch](https://www.warp.dev/blog/introducing-the-warp-agent-cli-coding-agent) (2026-08-04)
- [Atuin Scripts](https://blog.atuin.sh/atuin-scripts-shareable-syncable-shell-snippets/) (2025-04-09)
- [Atuin agent hooks](https://docs.atuin.sh/latest/guide/agent-hooks/) (2026-09-11)
- [Atuin Desktop OSS](https://blog.atuin.sh/atuin-desktop-open-source/) (2025-09-30)
- [crates.io/atuin](https://crates.io/crates/atuin) (2026-08-31)

**Skills / Nx**

- [Agent Skills specification](https://agentskills.io/specification) (2026-09-11)
- [Claude Code skills](https://code.claude.com/docs/en/skills) (2026-09-11)
- [Steering Claude Code](https://claude.com/blog/steering-claude-code-skills-hooks-rules-subagents-and-more) (2026-06-18)
- [Codex / ChatGPT build skills](https://learn.chatgpt.com/docs/build-skills) (2026-09-11)
- [Nx: why we deleted most MCP tools](https://nx.dev/blog/why-we-deleted-most-of-our-mcp-tools) (2026-02-17)
- [Nx agent skills](https://nx.dev/blog/nx-ai-agent-skills) (2026-09-11)
- [Nx MCP reference](https://nx.dev/docs/reference/nx-mcp) (2026-09-11)

**Token cost**

- [Anthropic: Code execution with MCP](https://www.anthropic.com/engineering/code-execution-with-mcp) (2025-11-04)
- [GitHub MCP token autopsy](https://getunblocked.com/blog/github-mcp-token-cost/) (2026-05-15 / 2026-09-10)
- [The MCP tax](https://okaneland.com/study/the-mcp-context-tax/) (2026-09-01)
- [What an MCP server costs in tokens](https://www.ismaelramos.dev/blog/what-an-mcp-server-costs-you-in-tokens/) (2026-06-17)
- [mcp-tool-search](https://github.com/KGT24k/mcp-tool-search) (2026-09-11)
- [CLI vs MCP token savings](https://ubos.tech/news/cli-vs-mcp-token-cost-savings-and-lazy-loading-explained/) (2026-02-25)
- [CLI scripts as agent tools](https://agentpatterns.ai/tool-engineering/cli-scripts-as-agent-tools/) (2026-09-11)
