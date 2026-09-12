//! `kadou`: the CLI entrypoint. Wires the command tree from
//! `docs/design/05-prd.md` §7.1; every command besides `--help` and `version` is a stub in
//! this slice (§9 slice 1 "kadou --help lists the command tree stubs").

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use clap::{Args, Parser, Subcommand};

mod commands;
mod confirm;
mod starter;
mod ui;

/// kadou (稼働): a script library that is also an MCP server for AI agents.
#[derive(Debug, Parser)]
#[command(name = "kadou", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
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
    /// Validate every header in a folder (or a single path).
    Check {
        folder_or_path: Option<String>,
        #[arg(short = 'v', long)]
        verbose: bool,
    },
    /// Clone a folder from git.
    Get {
        url: String,
        #[arg(long = "as", value_name = "FOLDER")]
        as_folder: Option<String>,
        #[arg(long = "ref", value_name = "REF")]
        git_ref: Option<String>,
        #[arg(long, value_name = "SUB")]
        root: Option<String>,
    },
    /// Pull a folder's git remote (every folder if none is given).
    Update { folder: Option<String> },
    /// Remove a folder.
    Remove {
        folder: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        force: bool,
    },
    /// Convert an old dops catalog into a folder.
    Import {
        dir: String,
        #[arg(long = "as", value_name = "FOLDER")]
        as_folder: String,
    },
    /// Accept a proposed or mined draft into a folder.
    Accept {
        id: String,
        #[arg(long, value_name = "FOLDER")]
        into: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Trust (or untrust) the project-local `./kata/` folder.
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
    #[arg(long, value_name = "F")]
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
    },
    Status,
    List,
    Show {
        fingerprint: String,
    },
    Review,
    Approve {
        fingerprint: String,
        #[arg(long, value_name = "F")]
        into: Option<String>,
    },
    Reject {
        fingerprint: String,
        #[arg(long)]
        reason: String,
    },
    InstallSchedule,
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

    match cli.command {
        // Decision D6 (§7.4, §9 slice 6): the bare `kadou` frame is a stub until slice 8, but
        // it still scans kata (the "1 folder · 5 kata" preview it'll print), so it still
        // materializes the starter kata on a fresh home.
        None => {
            commands::materialize_starter_for_bare_invocation();
            stub("kadou", 8, None)
        }
        Some(Command::Version) => {
            println!("kadou {}", env!("CARGO_PKG_VERSION"));
            std::process::ExitCode::SUCCESS
        }
        Some(Command::List(args)) => commands::run_list(args.query, args.folder, args.risk),
        Some(Command::Check {
            folder_or_path,
            verbose,
        }) => commands::run_check(folder_or_path, verbose),
        Some(Command::Import { dir, as_folder }) => commands::run_import(dir, as_folder),
        Some(Command::Run(args)) => {
            commands::run_run(args.id, args.kv, args.dry_run, args.confirm, args.ask)
        }
        Some(Command::Show { id }) => commands::run_show(id),
        Some(Command::Vault(cmd)) => match cmd.action {
            VaultAction::Set { name, plain } => commands::run_vault_set(name, plain),
            VaultAction::List => commands::run_vault_list(),
            VaultAction::Rm { name } => commands::run_vault_rm(name),
        },
        Some(Command::Grant(cmd)) => match cmd.action {
            GrantAction::List => commands::run_grant_list(),
            GrantAction::Show { pending_id } => commands::run_grant_show(pending_id),
            GrantAction::Approve {
                pending_id,
                confirm,
            } => commands::run_grant_approve(pending_id, confirm),
            GrantAction::Deny { pending_id } => commands::run_grant_deny(pending_id),
            GrantAction::Allow { id, any_version } => commands::run_grant_allow(id, any_version),
        },
        Some(Command::Mcp(cmd)) => match cmd.action {
            McpAction::Serve {
                transport,
                bind,
                max_risk,
            } => commands::run_mcp_serve(transport, bind, max_risk),
            McpAction::Schema { bytes } => commands::run_mcp_schema(bytes),
        },
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
        Some(cmd @ Command::New { .. }) => stub("new", 8, Some(&cmd)),
        Some(cmd @ Command::Edit { .. }) => stub("edit", 8, Some(&cmd)),
        Some(cmd @ Command::Trust { .. }) => stub("trust", 5, Some(&cmd)),
        Some(cmd @ Command::History { .. }) => stub("history", 5, Some(&cmd)),
        Some(cmd @ Command::Mine(_)) => stub("mine", 9, Some(&cmd)),
        Some(cmd @ Command::Completion { .. }) => stub("completion", 8, Some(&cmd)),
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
