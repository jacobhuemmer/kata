//! `kadou`: the CLI entrypoint. Wires the command tree from
//! `docs/design/05-prd.md` §7.1; every command besides `--help` and `version` is a stub in
//! this slice (§9 slice 1 "kadou --help lists the command tree stubs").

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use clap::{Args, Parser, Subcommand};

mod commands;
mod confirm;
mod starter;
mod ui;

/// kadou: a script library that is also an MCP server for AI agents.
#[derive(Debug, Parser)]
#[command(name = "kadou", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Force plain output even on a TTY (§7 rule 3; `NO_COLOR` does the same).
    #[arg(long, global = true)]
    plain: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the kadou version.
    Version,
    /// Run a kata (picker when no id is given).
    Run(RunArgs),
    /// List kata.
    List(ListArgs),
    /// Show a kata's header.
    Show { id: Option<String> },
    /// Write a new kata and open it.
    New {
        id: String,
        #[arg(long)]
        from: Option<String>,
    },
    /// Open a kata in $EDITOR.
    Edit { id: Option<String> },
    /// Validate kata headers (or a single path).
    Check {
        #[arg(value_name = "ID_OR_PATH")]
        folder_or_path: Option<String>,
        #[arg(short = 'v', long)]
        verbose: bool,
    },
    /// Clone a team's kata from git.
    Get {
        url: String,
        #[arg(long = "as", value_name = "NAME")]
        as_folder: Option<String>,
        #[arg(long = "ref", value_name = "REF")]
        git_ref: Option<String>,
        #[arg(long, value_name = "SUB")]
        root: Option<String>,
    },
    /// Pull git remotes for installed kata.
    Update {
        #[arg(value_name = "NAME")]
        folder: Option<String>,
    },
    /// Uninstall kata by id prefix (starter, sesami, …).
    Remove {
        #[arg(value_name = "NAME")]
        folder: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        force: bool,
    },
    /// Convert an old dops catalog into kata.
    Import {
        dir: String,
        #[arg(long = "as", value_name = "NAME")]
        as_folder: String,
    },
    /// Accept a proposed or mined draft.
    Accept {
        id: String,
        #[arg(long, value_name = "NAME")]
        into: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Trust (or untrust) project-local `./kata/`.
    Trust {
        #[arg(long)]
        forget: bool,
    },
    /// Manage the age-encrypted secret vault.
    Vault(VaultCommand),
    /// Show run history.
    History {
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        json: bool,
    },
    /// Manage agent grants.
    Grant(GrantCommand),
    /// Session mining.
    Mine(MineCommand),
    /// The MCP server.
    Mcp(McpCommand),
    /// Print a shell completion script.
    Completion { shell: String },
}

#[derive(Debug, Args)]
struct RunArgs {
    id: Option<String>,
    /// `key=value` args for the kata, e.g. `name=world`.
    #[arg(trailing_var_arg = true)]
    kv: Vec<String>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long, value_name = "ID")]
    confirm: Option<String>,
    #[arg(long)]
    ask: bool,
}

#[derive(Debug, Args)]
struct ListArgs {
    query: Option<String>,
    /// Restrict to one id prefix (`starter`, `sesami`, …).
    #[arg(long = "prefix", alias = "folder", value_name = "NAME")]
    folder: Option<String>,
    #[arg(long, value_name = "R")]
    risk: Option<String>,
}

#[derive(Debug, Args)]
struct VaultCommand {
    #[command(subcommand)]
    action: VaultAction,
}

#[derive(Debug, Subcommand)]
enum VaultAction {
    Set {
        name: String,
        #[arg(long)]
        plain: bool,
    },
    List,
    Rm {
        name: String,
    },
}

#[derive(Debug, Args)]
struct GrantCommand {
    #[command(subcommand)]
    action: GrantAction,
}

#[derive(Debug, Subcommand)]
enum GrantAction {
    List,
    Show {
        pending_id: String,
    },
    Approve {
        pending_id: String,
        /// Required non-interactively for a high/critical kata (§6.3, §6.4).
        #[arg(long, value_name = "ID")]
        confirm: Option<String>,
    },
    Deny {
        pending_id: String,
    },
    Allow {
        id: String,
        #[arg(long)]
        any_version: bool,
    },
}

#[derive(Debug, Args)]
struct MineCommand {
    #[command(subcommand)]
    action: MineAction,
}

#[derive(Debug, Subcommand)]
enum MineAction {
    Run {
        #[arg(long)]
        once: bool,
        #[arg(long)]
        watch: bool,
        #[arg(long, value_name = "ISO")]
        since: Option<String>,
        /// Overrides `~/Documents/Sessions/index.jsonl` (or `AGENT_SESSION_LEDGER_DIR`) --
        /// mainly for pointing at a fixture directory (`06` §2.1).
        #[arg(long, value_name = "PATH")]
        index: Option<String>,
    },
    Status,
    List,
    Show {
        fingerprint: String,
    },
    /// Lists queued drafts and how to act on each; `--dump <fingerprint>` prints one draft's
    /// full (already-redacted) source (`06` §2.9, §4.5). Approve/reject/skip are the
    /// dedicated subcommands below, not sub-actions of `review` itself.
    Review {
        #[arg(long, value_name = "FP")]
        dump: Option<String>,
        #[arg(long)]
        redacted: bool,
    },
    Approve {
        fingerprint: String,
        /// The name the draft gets under `mined/`; defaults to the slug `kadou mine run`
        /// derived from the automation's first step.
        #[arg(long, value_name = "NAME")]
        into: Option<String>,
    },
    Reject {
        fingerprint: String,
        #[arg(long)]
        reason: String,
    },
    /// Records `skipped` and leaves the draft queued (`06` §2.9 "skip: leave queued") --
    /// unlike approve/reject, never terminal: a skipped fingerprint can still be approved or
    /// rejected later.
    Skip {
        fingerprint: String,
    },
    InstallSchedule {
        /// Also runs `launchctl load` on macOS (never done by the writer itself, and never in
        /// tests).
        #[arg(long)]
        load: bool,
    },
}

#[derive(Debug, Args)]
struct McpCommand {
    #[command(subcommand)]
    action: McpAction,
}

#[derive(Debug, Subcommand)]
enum McpAction {
    Serve {
        #[arg(long, value_name = "stdio|http", default_value = "stdio")]
        transport: String,
        #[arg(long, value_name = "HOST:PORT")]
        bind: Option<String>,
        #[arg(long, value_name = "LEVEL")]
        max_risk: Option<String>,
    },
    /// Print the served `tools/list` JSON and its byte count.
    Schema {
        #[arg(long)]
        bytes: bool,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let plain = cli.plain;

    match cli.command {
        None => commands::run_bare(plain),
        Some(Command::Version) => {
            println!("kadou {}", env!("CARGO_PKG_VERSION"));
            std::process::ExitCode::SUCCESS
        }
        Some(Command::List(args)) => commands::run_list(args.query, args.folder, args.risk),
        Some(Command::Check {
            folder_or_path,
            verbose,
        }) => commands::run_check(folder_or_path, verbose, plain),
        Some(Command::Import { dir, as_folder }) => commands::run_import(dir, as_folder),
        Some(Command::Run(args)) => commands::run_run(
            args.id,
            args.kv,
            args.dry_run,
            args.confirm,
            args.ask,
            plain,
        ),
        Some(Command::Show { id }) => commands::run_show(id, plain),
        Some(Command::Vault(cmd)) => dispatch_vault(cmd.action),
        Some(Command::Grant(cmd)) => dispatch_grant(cmd.action),
        Some(Command::Mcp(cmd)) => dispatch_mcp(cmd.action),
        // A4: each still-stubbed variant names its own (label, slice) right in its own arm,
        // rather than in a second match that has to be kept in sync with this one by hand —
        // an already-dispatched variant added here would be a normal "duplicate match arm"
        // compile error, never a live `unreachable!()`.
        Some(Command::Get {
            url,
            as_folder,
            git_ref,
            root,
        }) => commands::run_get(url, as_folder, git_ref, root),
        Some(Command::Update { folder }) => commands::run_update(folder),
        Some(Command::Remove { folder, yes, force }) => commands::run_remove(folder, yes, force),
        Some(Command::Accept { id, into, yes }) => commands::run_accept(id, into, yes),
        Some(Command::New { id, from }) => commands::run_new(id, from),
        Some(Command::Edit { id }) => commands::run_edit(id, plain),
        Some(Command::History { limit, json }) => commands::run_history(limit, json),
        Some(Command::Completion { shell }) => commands::run_completion(&shell),
        Some(cmd @ Command::Trust { .. }) => stub("trust", 5, Some(&cmd)),
        Some(Command::Mine(cmd)) => dispatch_mine(cmd.action),
    }
}

fn dispatch_vault(action: VaultAction) -> std::process::ExitCode {
    match action {
        VaultAction::Set { name, plain } => commands::run_vault_set(name, plain),
        VaultAction::List => commands::run_vault_list(),
        VaultAction::Rm { name } => commands::run_vault_rm(name),
    }
}

fn dispatch_grant(action: GrantAction) -> std::process::ExitCode {
    match action {
        GrantAction::List => commands::run_grant_list(),
        GrantAction::Show { pending_id } => commands::run_grant_show(pending_id),
        GrantAction::Approve {
            pending_id,
            confirm,
        } => commands::run_grant_approve(pending_id, confirm),
        GrantAction::Deny { pending_id } => commands::run_grant_deny(pending_id),
        GrantAction::Allow { id, any_version } => commands::run_grant_allow(id, any_version),
    }
}

fn dispatch_mine(action: MineAction) -> std::process::ExitCode {
    match action {
        MineAction::Run {
            once,
            watch,
            since,
            index,
        } => commands::run_mine_run(once, watch, since, index),
        MineAction::Status => commands::run_mine_status(),
        MineAction::List => commands::run_mine_list(),
        MineAction::Show { fingerprint } => commands::run_mine_show(fingerprint),
        MineAction::Review { dump, redacted } => commands::run_mine_review(dump, redacted),
        MineAction::Approve { fingerprint, into } => commands::run_mine_approve(fingerprint, into),
        MineAction::Reject {
            fingerprint,
            reason,
        } => commands::run_mine_reject(fingerprint, reason),
        MineAction::Skip { fingerprint } => commands::run_mine_skip(fingerprint),
        MineAction::InstallSchedule { load } => commands::run_mine_install_schedule(load),
    }
}

fn dispatch_mcp(action: McpAction) -> std::process::ExitCode {
    match action {
        McpAction::Serve {
            transport,
            bind,
            max_risk,
        } => commands::run_mcp_serve(transport, bind, max_risk),
        McpAction::Schema { bytes } => commands::run_mcp_schema(bytes),
    }
}

/// Every command besides `--help` and `version` prints this and exits 2 until its slice
/// lands (`docs/design/05-prd.md` §9). `parsed` is echoed (debug-formatted) so the clap
/// shape itself stays exercised even though nothing acts on it yet.
fn stub(command: &str, slice: u8, parsed: Option<&Command>) -> ! {
    eprintln!("error: '{command}' is not yet implemented (slice {slice})");
    if let Some(parsed) = parsed {
        eprintln!("  parsed: {parsed:?}");
    }
    std::process::exit(2);
}
