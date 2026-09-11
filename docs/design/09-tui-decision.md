# kadou TUI decision: is a screen still required?

**Date:** 2026-09-11
**Follows:** `08-shape-review.md` (single-file kata, folders, no registry). Mason answered its six open questions and then asked: *"I am not sure a TUI is really required any longer, especially if I am going to use this to be driven by AI Agents."*
**Reviewed against:** `05-prd.md` §1, §2 row 10, §3, §6.3, §6.4, §6.7, §6.8, §7, §8, §9 slice 8, §11; `03-principles.md` §4, §5, charter 5 and 13, non-goals; `06-session-mining.md` §2.9, §4.5; `08` §1, §2.4, §2.5, §7.
**Role:** decision, not a menu. Defaults are picked. Mason can overrule any line; the recommendation stands until he does.

---

## 0. Decisions recorded from the shape review

These close `08` §8. The PRD revision cites this table, not the review's question list.

| # | Question (`08` §8) | Decision | Consequence the PRD carries |
|---|---|---|---|
| D1 | Name of the unit | **kata**, singular and plural. Tool names `list_kata`, `describe_kata`, `run_kata`, `propose_kata`. Folder form is `<name>/kata.sh`. | `08` §5.2 gloss goes into `kadou --help` and the README once. "runbook" and "catalog" leave the vocabulary. |
| D2 | Runtime | **The shebang is the runtime; no shebang means `/bin/sh`.** `kadou check` warns when the interpreter is not on `PATH`. | `03` §1 rule 5 widens from "POSIX sh" to "the shebang, `/bin/sh` by default". The starter stays `#!/bin/sh`. |
| D3 | Project-local trust | **`kadou trust` pins the folder path** (direnv `allow` without the hash). `kadou trust --forget` removes it. Agents never see an untrusted `./kata`. | `[trust] paths` in `kadou.toml`; `08` §4.2 rules verbatim. |
| D4 | Sesami repository after conversion | **Kata at the repository root** (`cc4-aaa.sh`, `scripts/`, `device-log-metrics/`). `kadou get` needs no `--root`. | The `--root` symlink survives only for foreign layouts; Sesami does not use it. |
| D5 | Last-used args | **Interactive prefill only; CLI with args on the command line and MCP use header defaults.** Go's runbook-scope saving is dropped. | With no TUI, the interactive surface is the prompt (§3.4 below). A last-used value fills a prompt; it never changes what a non-interactive run or an agent run does. |
| D6 | Starter on disk | **`kata/starter/` is materialized on first run** if `kata/` does not exist; deletable; `kadou get starter` restores it. | `08` §2.2 rule as written. |

**D7 (this document, pending Mason):** no full-screen TUI in v1. The human surface is a styled CLI with one built-in picker and inline prompts. Section 5 scores it; section 6 lists the edits.

---

## 1. Verdict

**Drop the full-screen TUI. Keep one interactive component: a built-in picker with a header preview, plus inline prompts and confirms.** Everything else the TUI carried is already a CLI command or becomes one command done well.

The reason is not cost, although the cost is real. It is that the product's shape changed under it. In `05` the human had to browse a registry of catalogs, open a wizard to fill nine parameter types across four scopes, and watch a pane. In `08` the human has a folder of files. `ls` is the catalog. `$EDITOR` is the editor. The header is the documentation. A screen that re-implements `ls`, `less`, and a form on top of a folder of scripts is a second product, and it is the product that drifted last time (`01` §2.1: MCP confirm ≠ TUI confirm).

The agent angle finishes the argument. When agents start most runs, the human's job is to **review**: approve a grant, accept a draft, read a mined proposal, trust a folder, set a vault entry. Each of those is a one-line command with a diff and a `y/N`, and each already exists in the command tree. What the TUI added for review was a badge in a footer. A line in `kadou`'s output and a desktop notification do that better, because the human is not sitting in kadou; they are sitting in Claude Code or Cursor.

What is genuinely lost: a persistent two-pane browse. Section 7 says how to get it back if it turns out to matter.

---

## 2. What the TUI carries today, flow by flow

Sources: `05` §7.2 keybindings, §6.3 confirm table, §6.4 step 4, §6.7 palette entry; `08` §2.4 mocks, §2.5 key table; `03` §4 rules 3 and 4.

| Flow | What the TUI does (`05`/`08`) | CLI today | Verdict |
|---|---|---|---|
| **Browse** | Sidebar grouped by folder, detail pane with about, risk, needs, args, file, last run, source | `kadou list`, `kadou show <id>` exist (`08` §7 §7.1) | **CLI covers it** once `kadou` with no args prints the library styled (§3.2) and `kadou show` prints the detail pane as text. Shell completion of ids (`kadou completion`) is the keyboard browse a TUI never had. |
| **Run with prefill** | `Enter` opens a wizard; fields prefilled from last-used args (D5) | `kadou run <id> k=v…`; missing required args are an error | **One well-designed command:** on a TTY, `kadou run` prompts for each missing required arg with the last-used value as the default (§3.4). Non-TTY stays an error with the exact line to copy. |
| **Confirm high / critical** | Overlay `y/N`; critical types the id | `--confirm <id>` (`05` §6.3) | **CLI already covers it.** On a TTY the prompt *is* the confirm; `--confirm` remains the non-interactive form. One protocol, two faces instead of three. |
| **Pending grants** | Footer badge `p pending 1`, palette entry, list, approve/deny | `kadou grant list/approve/deny/allow` (`05` §6.4) | **CLI covers it** plus two additions: a "needs you" block in `kadou`'s no-arg output (§3.2) and a desktop notification when a pending record is written (§4.3). The badge was the only thing the TUI had that the CLI lacked. |
| **Accept drafts** | Palette "Accept proposed runbook" | `kadou accept <id>` prints the diff and asks `y/N` (`05` §6.7; `08` §6.2) | **CLI already covers it.** The palette entry was a shortcut to the same command. |
| **Mining review** | `06` §2.9: "TUI optional later; CLI is enough for v1" | `kadou mine review` walks the queue with approve / edit / reject / skip (`06` §4.5) | **CLI already covers it,** by the mining design's own statement. |
| **Edit** | `e` opens `$EDITOR`, re-checks on return (`08` §2.5) | `kadou edit <id>` is in the `08` §7 command tree | **CLI already covers it.** The TUI key was a shell-out. |
| **New** | `n` writes a template in the selected folder | `kadou new <folder/name>` (`08` §2.7) | **CLI already covers it.** |
| **Check** | `c` runs the linter, shows diagnostics in a pane | `kadou check` (`08` §2.7) | **CLI already covers it.** Diagnostics are cargo-shaped text; a pane adds nothing. |
| **Pull a folder** | `g` runs `git pull` when the folder has `.git` | `kadou update [folder]` | **CLI already covers it.** |
| **Pick a kata by typing** | `/` fuzzy find across id, about, alias | Nothing: `kadou list <query>` filters but does not select | **Genuinely needs an interactive component.** Not a screen: an inline picker (§3.3). This is the one thing worth building. |
| **Live output while browsing** | Output pane streams the run under the list | `kadou run` streams to the terminal | **CLI covers it.** A run is the foreground; nobody browses during a Jenkins trigger. |
| **First-run beauty** | Finished two-pane frame on first paint (`03` §4 rule 3) | `kadou` with no args | **CLI can cover it** if the no-arg frame is designed (§3.2): starter listed, one tip, no empty panel. This is a design obligation, not a screen. |
| **Theme, palette, help overlay** | `Ctrl+p` palette, `?` overlay, `theme = …` | `kadou --help`, `kadou.toml` | **CLI covers it.** The theme still exists; it colors the CLI. The palette listed seven actions that are seven subcommands. |

Thirteen flows. One needs an interactive component. None needs a screen.

---

## 3. Beautiful by default and keyboard-first, without a screen

### 3.1 What the words mean now

Omarchy's principles restated for a CLI-plus-MCP product (`03` §1–§7, `08` §2):

| Principle | With a TUI (`03`) | Without one |
|---|---|---|
| Omakase | one theme, one layout, one key map | one theme, one output grammar, one prompt style; nothing to configure about how output looks except `theme` and `NO_COLOR` |
| Batteries | starter in the sidebar | starter in `kadou`'s first frame; picker built in, not `fzf` |
| One-line install | `kadou` opens a finished screen | `kadou` prints a finished frame; the next command is on it |
| Beautiful | first TUI frame is a product | every frame is a product: `kadou`, `kadou run`, `kadou check`, errors. Styled on a TTY, plain when piped, silent about color when `NO_COLOR` is set |
| Keyboard-first | every action has a key | every action is a command a shell completes; every interactive step is a keystroke (type to filter, `↵`, `esc`, `y`/`N`, type the id); no mouse anywhere; no chord to learn |
| Dotfiles | `[keys]` map in config | no key map to configure; `theme`, `[notify]` |
| Convention | `Enter` runs, `/` finds | a command with an id runs; a command without one picks |

Rules every human-facing frame obeys:

1. **Risk is a colored dot plus a word.** Never only a color (`08` §2.4).
2. **Success is one line. Errors are a sentence and a fix line.** The fix line is a command the reader can paste.
3. **Styled on a TTY only.** When stdout is not a terminal, output is plain, tab-separated where it is a list, and stable for scripts. `NO_COLOR` and `--plain` force plain on a TTY. `TERM=dumb` disables the picker and prompts (they become errors with the non-interactive form).
4. **One palette.** `theme = "doop"` in `kadou.toml` colors everything: dots, the `▸` marker, `✓`/`✗`, muted text. A theme file has eight keys. The twenty embedded Go themes shrink to the handful that people actually pick; the format is not the Go JSON.
5. **The library frame fits on one screen for the starter and scrolls like `ls` after that.** Nothing paginates. Nothing is interactive unless the command is missing an id.
6. **Interactive means inline.** The picker and prompts draw below the shell prompt and erase themselves when done. No alternate screen, no lost scrollback, `Ctrl+c` always exits with a one-line "cancelled".

### 3.2 `kadou` with no arguments

First run, empty config, starter materialized (D6):

```
$ kadou
 kadou 稼働   1 folder · 5 kata

 starter
   hello         ● low   Print a greeting
   disk-usage    ● low   Disk usage of a directory
   git-status    ● low   git status -sb in a repo
   health        ● low   Resolve a host
   list-path     ● low   List a directory

 run     kadou run                 pick one, or:  kadou run starter/hello
 new     kadou new <folder/name>   write a kata and open it
 team    kadou get <git-url>       add your team's kata as a folder
 agents  kadou mcp serve
```

After `kadou get` of the converted Sesami folder (D4), with `[folder.sesami] max_risk = "critical"`, one grant waiting and two drafts. The mock elides 30 rows; the real frame lists every kata:

```
$ kadou
 kadou 稼働   2 folders · 37 kata                  needs you: 1 grant · 2 drafts

 starter
   hello               ● low    Print a greeting
   …
 sesami                git ✓ main @ 3f9e1c2
   cc4-aaa             ● med    Trigger a SES/CC4/cc4-aaa branch pipeline
   cc4-admin           ● med    Trigger a SES/CC4/cc4-admin branch pipeline
   …
   ses-deploy          ● crit   Trigger the SES Deploy pipeline
   ses-release-build   ● high   Build a SES release

 needs you
   grant  sesami/ses-deploy  version=25.6.1.2 oke_cluster=uat   claude-code · 4m ago
          kadou grant approve 7c1e   ·   kadou grant deny 7c1e
   draft  proposed/ops/argocd-sync                kadou accept ops/argocd-sync
   draft  mined/k8s-pod-logs                      kadou mine review

 run  kadou run   ·   help  kadou --help
```

Rules: the `needs you` block and the header count appear only when something is waiting (`08` §2.4: badges that are always present are noise). A folder that failed `kadou check` shows `✗ 2 errors` where `git ✓` is and its kata are listed dimmed. Piped, the same command prints one kata per line:

```
$ kadou | grep crit
sesami/ses-deploy	critical	Trigger the SES Deploy pipeline
```

### 3.3 The picker: any id-taking command with no id

`kadou run`, `kadou show`, `kadou edit`, and `kadou history` with no id open the picker on a TTY. It is one component, built in, no `fzf` on the machine required (`03` charter 3). Type to filter across id, about, and alias; the highlighted kata's header shows below the list, which is the `08` detail pane without the chrome.

```
$ kadou run
 ▸ run which kata?  ses dep█
   sesami/ses-deploy           ● critical   Trigger the SES Deploy pipeline
   sesami/ses-release-build    ● high       Build a SES release
   sesami/ses-argocd-sync      ● medium     Force-refresh an ArgoCD application
   sesami/ses-automation       ● low        Trigger the SES automation pipeline
 ──────────────────────────────────────────────────────────────────────
 sesami/ses-deploy   ● critical   confirm by typing the id
 needs  jenkins_url  jenkins_user  jenkins_token   ✓ vault
 args   version      text   required    Image tag to deploy
        oke_cluster  text   required    Target OKE cluster
        namespace    text   = ""
        modules      text   = "aaa,admin,…"
 file   kata/sesami/ses-deploy.sh   sha 4b1c…
 last   2026-09-10 14:02  ✓ 1m12s  mason (cli)
 ↑↓ move   ↵ run   tab show   e edit   esc cancel
```

Keys, complete: printable characters filter; `↑`/`↓` and `Ctrl+k`/`Ctrl+j` move; `↵` selects and continues the command; `tab` prints the full `kadou show` for the highlighted kata and returns; `e` opens it in `$EDITOR` and returns; `esc` or `Ctrl+c` cancels with exit 130. The list shows at most twelve rows and scrolls. Match scoring is fuzzy with id matches ranked above about matches. With no matches the list says `no kata matches "xyz"   kadou new sesami/xyz`.

When stdin or stdout is not a TTY, `kadou run` with no id exits 2: `error: no kata id given and no terminal to pick one   = kadou run <id>, or kadou list`.

### 3.4 `kadou run`: prompts, then the frame

An id with every required arg on the command line runs immediately; that frame is `08` §2.7 unchanged. An id with a required arg missing, on a TTY, prompts once per missing arg. The default shown is the last-used value for that kata when one exists (D5), else nothing; `↵` accepts it, typing replaces it, `esc` cancels.

```
$ kadou run sesami/ses-deploy
 sesami/ses-deploy  ● critical   Trigger the SES Deploy pipeline
 version      Image tag to deploy       › 25.6.1.2
 oke_cluster  Target OKE cluster        › uat
 ● critical   type the id to run  › sesami/ses-deploy
 ▶ sesami/ses-deploy  ● critical
   version=25.6.1.2  oke_cluster=uat  namespace=""  modules="aaa,admin,…"
   needs  jenkins_url jenkins_user jenkins_token   ✓ vault
 ─────────────────────────────────────────────────────────────────────
 ==> Stage 1/3: Validate
 …
 ✓ sesami/ses-deploy  exit 0  1m12s   kadou history 91aa
```

D5 holds exactly: optional args always take the header default; a required arg has no header default by definition, so the last-used value can only fill a blank. `kadou run <id> --ask` prompts for every arg, showing the header default (or the last-used value when one exists) so a human can walk the whole form when they want to. A `bool` prompts as `y/n`, an `int` rejects non-digits before `↵`, a `select` is a four-row picker. High risk prompts `run? [y/N]`; critical prompts for the id, as in `05` §6.3, with `--confirm <id>` as the non-interactive form.

Non-TTY, missing required arg or missing confirm, unchanged from `05` §6.3: exit 2 and the exact line to copy.

```
$ kadou run sesami/ses-deploy version=25.6.1.2 oke_cluster=uat </dev/null
error: sesami/ses-deploy is critical and needs confirmation
  = kadou run sesami/ses-deploy version=25.6.1.2 oke_cluster=uat --confirm sesami/ses-deploy
```

### 3.5 `kadou check`

Unchanged from `08` §2.7: cargo-shaped diagnostics, a line per error, a fix line per error, one summary line. It was never a TUI flow; it is repeated here only because it is the frame people will see most after `kadou run`.

```
$ kadou check sesami
error: unknown arg type `string`
  --> kata/sesami/ses-argocd-sync.sh:8:14
   |
 8 | #   app_name: string                  # ArgoCD application name
   |               ^^^^^^ use `text`
   = arg types are text, int, bool, select

checked 32 kata in sesami   1 error  0 warnings
```

### 3.6 Errors as sentences

The `08` §2.7 examples stand. Two more that the missing TUI creates:

```
$ kadou edit
error: no kata id given and no terminal to pick one
  = kadou edit <id>, or kadou list
```

```
$ kadou grant approve 7c1e
error: kata/sesami/ses-deploy.sh changed since the agent asked (sha 4b1c… → 9d02…)
  = read the diff:   kadou grant show 7c1e
  = then approve again, or deny:   kadou grant deny 7c1e
```

### 3.7 Implementation shape

No `kadou-tui` crate. A `ui` module inside the `kadou` bin crate holds four things: `style` (theme to ANSI, TTY and `NO_COLOR` detection), `frame` (the library, run, and check printers), `picker` (list plus preview over `crossterm` raw mode, fuzzy scoring), `prompt` (text, int, bool, select, confirm, secret). Candidates, to be version-checked on crates.io by the PRD revision the way `05` §3.1 was: `anstyle`/`anstream` for styled output that strips itself on a pipe (clap already depends on `anstyle`, so it is free); `nucleo-matcher` (Helix's fuzzy matcher) for picker scoring; `inquire` for the prompts, or a hand-rolled ~300 lines if its look cannot follow the theme. `crossterm` stays. `ratatui` goes. The desktop notification shells out to `osascript` on macOS and `notify-send` on Linux; no crate, and it is silently skipped when neither binary exists.

Snapshot tests replace visual tests: the `kadou` frame, a `kadou run` frame, a `kadou check` frame, and the picker's preview block are `insta` snapshots in both styled and plain forms, the same discipline `05` applies to `tools/list`.

---

## 4. The agent angle

### 4.1 What the human side must do when agents drive

Everything an agent cannot do for itself under `03` charter 10 and 11:

| Human act | Trigger | Command | Screen needed? |
|---|---|---|---|
| Approve or deny a grant | `run_kata` returned `pending_grant` | `kadou grant approve <id>` (diff since request, confirm, one-shot run); `kadou grant deny <id>` | No. A diff and a confirm. |
| Allow an id permanently | An agent keeps asking for the same high kata | `kadou grant allow <id>` | No. One line in `kadou.toml`. |
| Accept a draft | `propose_kata` wrote `proposed/…` | `kadou accept <id>` (diff, `y/N`) | No. |
| Review mined proposals | `kadou mine run` queued drafts | `kadou mine review` (approve / edit / reject / skip) | No, per `06` §2.9. |
| Trust a project folder | An agent's host cwd has `./kata` | `kadou trust` | No. |
| Supply a need | `describe_kata` says `needs_missing` | `kadou vault set <name>` | No. A secret prompt. |
| Notice that any of the above is waiting | The agent said so, or the human did not read it | `kadou` (the `needs you` block), a notification | This is the whole gap. |

The last row is the only one a TUI served, and it served it badly, because it required the human to be inside kadou to see the badge.

### 4.2 The host agent is the UI

When Claude Code or Cursor calls `run_kata` on a critical kata, the MCP result is the notification. The agent reads it and tells the human in the chat they are already looking at. The result therefore carries the exact human command, the same way `propose_kata` already carries `accept` (`08` §2.8):

```json
{"status":"pending_grant","id":"sesami/ses-deploy","risk":"critical",
 "pending_id":"7c1e…","pending_path":"/Users/mason/.local/state/kadou/pending/7c1e….json",
 "approve":"kadou grant approve 7c1e","expires":"2026-09-12T14:02:00Z",
 "reason":"critical; not in [agent] allow"}
```

`approve` and `expires` are new fields (§6, PRD §5.5). The agent's next message to the human is one line: "kadou needs a grant for `sesami/ses-deploy`; run `kadou grant approve 7c1e` in a terminal, then tell me." After approval the agent reads `pending_path` and sees `history_id` and `status` (`05` §6.4 step 5). No fifth tool, no polling protocol.

**MCP elicitation is rejected as the grant channel.** The 2025-06-18 MCP revision lets a server ask the client to collect input from the user. Two reasons not to use it for grants: host support is uneven and was not verified live here, and a grant answered inside the host's own dialog is a confirm string with better manners. `03` charter 10 says high and critical need "a human grant, not a confirm string in a schema"; the server cannot tell an elicitation answered by a person from one answered by an auto-approve setting. The grant stays out of band: a command typed in a terminal, with the file digest pinned. Elicitation may later be used for harmless things (choosing among `select` options) and nothing else.

### 4.3 The notification

`kadou mcp serve` writes a pending record, then posts one desktop notification:

```
kadou · grant wanted
sesami/ses-deploy (critical) from claude-code
kadou grant approve 7c1e
```

Same for `propose_kata` ("draft wanted: kadou accept ops/argocd-sync") and for `kadou mine run --once` when it queues a draft. `[notify] enabled = true` by default; `false` turns it off. No sound, no click action, no daemon: the server already exists at the moment the record is written, so there is nothing to keep alive. The `needs you` block in `kadou` is the durable copy for a human who missed the toast.

The smallest human surface that does the job is therefore: **five existing commands, one `needs you` block, one notification, and the agent's own chat.** A screen would add a fourth place to look.

---

## 5. Three options, scored

Scores 1–5, higher is better. "Agent-driven fit" is how well the option serves a human whose main job is reviewing what agents did. "Drift" is the risk that a human confirm path disagrees with the MCP path again, which `01` §2.1 names as the way the Go tree already broke.

| Criterion | A. Keep the full TUI (`05` slice 8) | B. Built-in picker + styled CLI | C. Drop all interactive UI |
|---|---|---|---|
| Agent-driven fit | 3 · the badge helps, the rest is a place to be instead of the chat | 5 · `needs you` block, notification, `approve` in the result | 4 · same commands, no notification, no block |
| Human review loop (grants, drafts, mining) | 4 · palette entries call the same commands | 5 · commands plus the two ways to notice | 3 · commands only; you must remember to look |
| Beautiful by default | 5 · the finished two-pane frame | 4 · a finished single frame; no live pane | 2 · plain listings |
| Keyboard-first | 5 · every action has a key | 5 · every action is a command or a keystroke; completion | 3 · commands only; no picker, no prompt |
| Browse and discover | 5 · persistent tree and detail pane | 4 · picker with preview, `kadou show`, completion | 2 · `kadou list` and `grep` |
| Build cost (inverse) | 1 · `ratatui` app: list, tree, detail, wizard, confirm, output pane, palette, help, themes, visual tests | 4 · one picker, five prompts, four printers, snapshots | 5 · printers only |
| Drift vs one engine (inverse) | 2 · a third confirm face, a wizard that re-implements arg types | 4 · prompts call the same arg parser; confirm is one function with a TTY branch | 5 · one face |
| Scripts, pipes, CI | 3 · TUI is irrelevant to them; CLI must still be complete | 5 · every frame has a plain form | 5 |
| Reversible later | 3 · removing a TUI strands the wizard code | 5 · a TUI later embeds the picker's list and preview model | 4 · a picker later is additive |
| **Total /45** | **31** | **41** | **33** |

**Recommendation: B.** It is the only option that scores at least 4 on every criterion. A wins beauty and browse by one point each and loses fourteen points on cost and drift. C is cheaper than B by two points and loses the two things Mason asked for in `08`: beauty and simplicity for people who are not agents.

The deciding sentence: **an operator who mostly reviews what agents did needs to be told, not to be somewhere.** B tells them.

---

## 6. File-by-file changes if B is taken

### 6.1 `03-principles.md`

| Where | Change |
|---|---|
| Intro paragraph | "Rust CLI + TUI + MCP server" → "Rust CLI + MCP server". |
| "How Omarchy maps" table | Row 2: "Starter catalog, TUI, CLI, MCP, …" → "starter folder, styled CLI with a built-in picker, MCP, …". Row 3 (`Super + Space`): "TUI is fully keyboard-operable; `?` and a command palette cover every action" → "every action is a completable command; the picker and prompts are keystrokes; no mouse path exists". Row 5: "One theme restyles TUI, CLI help, and errors" → "One theme restyles every CLI frame, help, and errors". |
| §4 Beautiful by default | "For dops-next" paragraph: "The first TUI frame should look like a product" → "The first `kadou` frame should look like a product." Rule 3: "Sidebar shows the starter catalog, metadata pane shows a selected runbook, footer shows key hints" → "`kadou` lists the starter folder and the next command; the picker shows a header preview; nothing is an empty panel." Rule 4: "`?` is a first-class overlay" → "`kadou --help` and the picker's key line are complete and styled." Rule 6: visual tests → snapshot tests of the styled and plain frames. |
| §5 Keyboard-first | Retitle the "For dops-next" paragraph: "A human operates kadou without touching the mouse because there is nothing to touch: every flow is a command the shell completes, and every interactive step is a keystroke." Rules 1–5 replaced by three: (1) every id-taking command without an id opens the picker on a TTY and errors with the non-interactive form otherwise; (2) picker and prompt keys are fixed (§3.3, §3.4 here) and documented in `--help`; (3) `kadou completion` completes ids, folders, and arg names. Rule 6 stays ("MCP and CLI are complete without a terminal UI") reworded as "MCP is complete without a terminal." Drop "Command palette is the menu" and "Bindings are data". |
| Convention vs configurable table | "Keybindings: product map … override map in config" → "Picker and prompt keys: fixed; not configurable." Add "Notifications: on; `[notify] enabled = false`." |
| Charter 3 | "Starter catalog, themes, TUI, CLI, MCP, vault, history, risk" → "starter folder, theme, CLI with picker, MCP, vault, history, risk". |
| **Charter 5** | "**Keyboard-first TUI.** Every human flow has a key. Palette + `?`. Mouse is extra. MCP/CLI are complete without a terminal UI." → "**Keyboard-first terminal.** Every human flow is one command the shell completes; every interactive step is a keystroke in an inline picker or prompt. No screen to learn, no mouse path. MCP is complete without a terminal." |
| Charter 11 | "Accept lives in the TUI/`dops catalog accept`" → "Accept lives in `kadou accept`, `kadou grant approve`, `kadou mine review`; the agent's result and a notification tell the human it is waiting." |
| **Charter 13** | "**One binary, three interfaces.** CLI, TUI, MCP. Same engine … A fourth interface is not implied." → "**One binary, two interfaces.** CLI and MCP. Same engine, same folders, same vault, same risk. The picker and prompts are the CLI on a TTY, not a third interface. A full-screen TUI is a later revision, not an implied one." |
| Non-goals | Web UI bullet: "dops-next's charter is CLI + TUI + MCP" → "kadou's charter is CLI + MCP". Add: "**A full-screen TUI in v1.** Alternate screen, panes, palette, and key maps are a later principles revision (`09` §7), taken only if humans start more runs interactively than agents start over MCP." |
| Scoreboard | Row 5 verdict text mentions "TUI is keyboard-capable"; leave, it describes dops today. Closing paragraph: "Port the runbook format, vault split, risk enum, keyboard TUI, and single-binary idea" → drop "keyboard TUI". |

### 6.2 `05-prd.md`

| § | Change |
|---|---|
| §1.1 interface table | Delete the TUI row. CLI row: entry becomes "`kadou` (library frame), `kadou run` (picker when no id), `kadou list/show/new/edit/check/get/update/accept/trust/vault/history/grant/mine`". "One binary exposes three interfaces" → "two interfaces". |
| §1.2 | "Humans at a keyboard (TUI) or in a shell (CLI)" → "Humans in a shell." Operators "accept proposed or mined runbooks" → "…are told by the agent, a notification, or `kadou` that a grant, draft, or proposal is waiting, and act with one command." |
| §1.3 | "≤ 10 s from a completed install to first TUI frame / first `list_runbooks`" → "first `kadou` frame / first `list_kata`". |
| §2 row 10 | "**CLI + MCP ship first** … **TUI is in MVP** on the same engine. **Web UI is a non-goal.**" → "**CLI + MCP are the product.** No TUI in v1; the CLI has a built-in picker and inline prompts on a TTY (`09` §3). Web UI is a non-goal." Reason: "three interfaces; four on day one is how today's MCP/TUI confirm drifted" → "two interfaces; a third face for confirm is how today's tree drifted." Cite `09` §5. |
| §3 workspace layout | Delete `kadou-tui/` line. `kadou/` bin crate: "clap CLI, wires TUI + MCP" → "clap CLI, `ui` module (style, frames, picker, prompts, notify), wires MCP". Crate table: delete the `kadou-tui` row; `kadou` row deps add the picker/prompt crates. "Why split this way: one engine … shared by CLI, TUI, and MCP" → "shared by CLI and MCP". |
| §3 MSRV sentence | "MSRV 1.88 (required by `rmcp` 3.3.0 and `ratatui`)" → "(required by `rmcp` 3.3.0)". |
| §3.1 dependencies | Delete `ratatui`. Keep `crossterm` with "why: picker raw mode and prompts". Add rows for the styled-output, fuzzy-matcher, and prompt crates named in `09` §3.7 **after** checking crates.io versions and licenses the way this table was built (`07` §5). Add them to the `cargo-deny` run. |
| §6.3 confirm table | Drop the TUI column. CLI column: "high: on a TTY `run? [y/N]`, default No; otherwise `--confirm <id>`. critical: on a TTY type the id; otherwise `--confirm <id>`." Sentence "CLI without `--confirm` on high/critical prints the summary and exits 2" gains "when stdin is not a TTY". |
| §6.4 step 4 | "Human sees pending in TUI (badge + palette "Pending grants") or `kadou grant list`" → "The agent's result carries `approve`; the server posts a desktop notification; `kadou` shows a `needs you` block; `kadou grant list` and `kadou grant show <id>` print the record and the diff since request." Step 5: "still subject to human ceiling + TUI/CLI confirm" → "+ the CLI confirm (TTY prompt or `--confirm`)". |
| §5.5 `run_kata` pending result | Add `approve` (the exact human command) and `expires` (ISO-8601). Regenerate the example. Sizes: two short strings, well inside the run budget. |
| §6.6 history | `interface` enum `tui\|cli\|mcp` → `cli\|mcp`. |
| §6.7 | Delete "TUI palette: "Accept proposed runbook"." |
| §6.8 | `06` §2.9 already says CLI; no change beyond the `08` renames. |
| §7 title | "TUI / CLI UX" → "CLI UX". |
| §7.1 command tree | Bare `kadou` = library frame, not TUI. Note: "`run`, `show`, `edit`, `history` with no id open the picker on a TTY." Add `run … --ask`, `grant show <pending_id>`. Take the `08` §7 tree otherwise. |
| §7.2 | Retitle "Picker and prompt keys". Replace the fourteen-row table with the keys in `09` §3.3 and §3.4. Delete the `[keys]` override sentence and the `Ctrl+Shift+p` revision note. |
| §7.3 first run | Step 2: "`kadou` shows starter catalog in the sidebar, first runbook selected, metadata pane filled, footer key hints" → "`kadou` prints the starter folder and the next command (`09` §3.2)". |
| §7.5 config | Delete `[keys]`. Add `[notify] enabled = true`. Keep `theme`; its comment becomes "colors every CLI frame". |
| §8.1 non-goals | Add "A full-screen TUI in v1 (`09` §7 names the condition to revisit)." |
| §8.2 risks | Row "Pending-grant queue ignored by operators": mitigation "TUI first-class pending list" → "`approve` in the MCP result, desktop notification, `needs you` block in `kadou`, 24 h TTL". Add row: "Inline picker misbehaves in a terminal (tmux, Terminal.app, Windows) → falls back to a numbered list; every command has a non-interactive form; `TERM=dumb` disables it." |
| §9 slice 8 | Retitle "Styled CLI, picker, prompts, notification, starter installer". Scope: `ui` module (§3.7 here), `kadou` frame, `run` prompts and TTY confirm, picker, `[notify]`, `install.sh`, rust-embed starter. Tests: styled and plain snapshots of the four frames; picker filter ranks id over about; `run` without a TTY and without an id exits 2; critical prompt requires the exact id; `NO_COLOR` strips; `osascript` missing is not an error. Done: "`kadou` after install is a finished frame; `kadou run` with no id picks." Delete "ratatui app", "VHS". |
| §11 decision 10 | "CLI+MCP first …, TUI after, no web" → "CLI+MCP only; picker and prompts inside the CLI; no TUI in v1; no web". Add decision 17: "Human review is told, not housed: `approve` in results, notification, `needs you` block (`09` §4)." |
| §12 | Add "Elicitation support in real MCP hosts was not verified; the design does not depend on it." |
| §13 | Add a "TUI decision (09)" table listing D1–D7 and the sections above. |

### 6.3 `08-shape-review.md`

`08` is a dated review, not a living spec; it is not edited. The PRD revision cites `09` §3 for every human frame and `08` only for the header, namespace, and naming specs. For the record, the lines in `08` that `09` supersedes: §1 "`e` in the TUI opens the kata" (now `kadou edit`); §2.4 both mock screens (now §3.2 and §3.3 here); §2.5 the key table (now §3.3, §3.4 here); §3.2 "shown by `kadou show` and the TUI"; §4.1 "The TUI groups by top folder"; §5.2 "the TUI title tooltip"; §6.1 "TUI-prefilled" (now prompt-prefilled, D5); §6.2 "The TUI badge and the run refusal carry that weight" (now the dimmed listing and the run refusal); §7 rows §7.2 and §9 slice 8.

---

## 7. Risks of the recommendation, and how to reverse it

| Risk | Mitigation |
|---|---|
| An operator who liked the Go TUI wants to sit in a screen and watch | The picker's preview is the detail pane; `kadou show` is the detail pane as text; `kadou run` is the output pane. If that is not enough, §7 reversal below. |
| "Beautiful" degrades into ANSI clutter across many printers | One `style` module, one theme, snapshot tests on every frame, `--plain` and `NO_COLOR` honored everywhere. No printer writes an escape code directly. |
| Inline picker in a hostile terminal | Numbered-list fallback; non-interactive form for every command; `TERM=dumb` opt-out. |
| Prompts become a second arg parser and drift from MCP | Prompts produce `key=value` strings and hand them to the same parser `kadou run` and `run_kata` use. One parser, tested once. |
| The desktop notification is missed or unwanted | It is a convenience on top of the agent's own message and the `needs you` block; off with one config key; never the only channel. |
| Losing the "live output under the list" feel | Accepted. A Jenkins trigger is a foreground task; nobody browses during it. |
| Mining review as a CLI walk is tedious at volume | `06` §2.9 accepted this for v1; `kadou mine review --since` and `--min-score` bound the walk. Volume is the signal to revisit. |

**Reversal.** Nothing in the engine changes for this decision; it removes a crate that does not exist yet. To add a TUI later: one new crate `kadou-tui` on `ratatui`, one slice, no format or config change, mounted as `kadou ui` (never as bare `kadou`, which stays the library frame). It embeds the picker's list and preview model and calls the same confirm function with a different face. The `08` §2.4 mocks remain the target for that day. The crossterm dependency and the theme format are kept now so that day costs one crate, not a re-plumb.

**When to revisit,** stated as a condition and not a feeling: after Sesami is running on kadou for a month, `kadou history` shows humans starting more runs interactively (`interface: cli` from a TTY) than agents start over MCP, or a second operator asks for browse. Either one reopens charter 13. Neither one is expected.

---

## Method and limits

- Read: `08` in full; `05` §1–3, §5.5, §6.3–6.8, §7–9, §11–13; `03` intro, §4, §5, convention table, charter, non-goals; `06` §2.9, §4.5; `07` §2 row 10, C19; `01` §2.1, §3.1 rows 14–15, §9 Q10; `02` §7.12.
- Mason's six answers are recorded as relayed in the assignment brief, not from a transcript; D5 is restated for a product without a TUI and the restatement is marked as such.
- Not verified: live elicitation support in Claude Code or Cursor; crates.io versions and licenses of the picker, prompt, and styled-output crates (the PRD revision must check them as `05` §3.1 did); the size of the Go product's TUI package. Scores in §5 are judgment on the criteria named, not measurements.
- No Rust was written. The mocks are hand-drawn targets, not screenshots. No secrets, internal hostnames, or workspace identifiers appear here.
