//! `kadou`: the CLI entrypoint. Wires the command tree from
//! `docs/design/05-prd.md` §7.1; every command besides `--help` and `version` is a stub in
//! this slice (§9 slice 1 "kadou --help lists the command tree stubs").

use clap::{Args, Parser, Subcommand};

mod commands;

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
    Remove { folder: String },
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
        None => stub("kadou", 8, None),
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
        Some(command) => {
            let (name, slice) = match &command {
                Command::Run(_) => ("run", 3),
                Command::List(_) => ("list", 2),
                Command::Show { .. } => ("show", 3),
                Command::New { .. } => ("new", 8),
                Command::Edit { .. } => ("edit", 8),
                Command::Check { .. } => ("check", 2),
                Command::Get { .. } => ("get", 7),
                Command::Update { .. } => ("update", 7),
                Command::Remove { .. } => ("remove", 7),
                Command::Import { .. } => ("import", 2),
                Command::Accept { .. } => ("accept", 7),
                Command::Trust { .. } => ("trust", 5),
                Command::Vault(_) => ("vault", 4),
                Command::History { .. } => ("history", 5),
                Command::Grant(_) => ("grant", 6),
                Command::Mine(_) => ("mine", 9),
                Command::Mcp(_) => ("mcp", 5),
                Command::Completion { .. } => ("completion", 8),
                Command::Version => unreachable!("handled above"),
            };
            stub(name, slice, Some(&command))
        }
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

#[cfg(test)]
mod tests {
    use assert_cmd::Command as AssertCommand;
    use predicates::prelude::*;

    fn kadou() -> AssertCommand {
        AssertCommand::cargo_bin("kadou").unwrap()
    }

    #[test]
    fn version_prints_the_crate_version() {
        kadou()
            .arg("version")
            .assert()
            .success()
            .stdout(format!("kadou {}\n", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn help_lists_the_full_command_tree() {
        let expected = [
            "version",
            "run",
            "list",
            "show",
            "new",
            "edit",
            "check",
            "get",
            "update",
            "remove",
            "import",
            "accept",
            "trust",
            "vault",
            "history",
            "grant",
            "mine",
            "mcp",
            "completion",
        ];
        let mut cmd = kadou();
        let mut assert = cmd.arg("--help").assert().success();
        for name in expected {
            assert = assert.stdout(predicate::str::contains(name));
        }
    }

    #[test]
    fn bare_invocation_is_a_stub_not_a_crash() {
        kadou()
            .assert()
            .code(2)
            .stderr(predicate::str::contains("not yet implemented"))
            .stderr(predicate::str::contains("slice 8"));
    }

    #[test]
    fn run_stub_names_its_slice() {
        kadou()
            .args(["run", "starter/hello"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("not yet implemented (slice 3)"));
    }

    #[test]
    fn check_on_an_empty_kata_dir_succeeds() {
        let home = tempfile::tempdir().unwrap();
        kadou()
            .env("KADOU_HOME", home.path())
            .arg("check")
            .assert()
            .success();
    }

    #[test]
    fn list_with_no_kata_says_so() {
        let home = tempfile::tempdir().unwrap();
        kadou()
            .env("KADOU_HOME", home.path())
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("no kata found"));
    }

    #[test]
    fn import_then_check_round_trips_through_the_real_cli() {
        let home = tempfile::tempdir().unwrap();
        let src = home.path().join("catalog/src/widget");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("runbook.yaml"),
            "name: widget\ndescription: Say hello\nrisk_level: low\nscript: script.sh\nparameters: []\n",
        )
        .unwrap();
        std::fs::write(src.join("script.sh"), "#!/bin/sh\necho hi\n").unwrap();

        kadou()
            .env("KADOU_HOME", home.path())
            .args([
                "import",
                home.path().join("catalog/src").to_str().unwrap(),
                "--as",
                "sesami",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("imported 1 kata into sesami"));

        kadou()
            .env("KADOU_HOME", home.path())
            .args(["check", "sesami"])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "checked 1 kata in sesami   0 errors  0 warnings",
            ));

        kadou()
            .env("KADOU_HOME", home.path())
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("sesami/widget"));
    }

    #[test]
    fn vault_stub_names_its_slice() {
        kadou()
            .args(["vault", "list"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("not yet implemented (slice 4)"));
    }

    #[test]
    fn mcp_serve_stub_names_its_slice() {
        kadou()
            .args(["mcp", "serve"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("not yet implemented (slice 5)"));
    }
}
