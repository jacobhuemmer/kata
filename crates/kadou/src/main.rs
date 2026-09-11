//! `kadou`: the CLI entrypoint. Wires the command tree from
//! `docs/design/05-prd.md` §7.1; every command besides `--help` and `version` is a stub in
//! this slice (§9 slice 1 "kadou --help lists the command tree stubs").

use clap::{Args, Parser, Subcommand};

mod commands;
mod starter;

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
        Some(Command::Run(args)) => commands::run_run(args.id, args.kv, args.dry_run),
        Some(Command::Show { id }) => commands::run_show(id),
        Some(command) => {
            let (name, slice) = match &command {
                Command::Run(_) => unreachable!("handled above"),
                Command::List(_) => ("list", 2),
                Command::Show { .. } => unreachable!("handled above"),
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
    fn run_with_no_id_is_a_slice_8_stub() {
        kadou()
            .arg("run")
            .assert()
            .code(2)
            .stderr(predicate::str::contains("not yet implemented (slice 8)"));
    }

    #[test]
    fn show_with_no_id_is_a_slice_8_stub() {
        kadou()
            .arg("show")
            .assert()
            .code(2)
            .stderr(predicate::str::contains("not yet implemented (slice 8)"));
    }

    #[test]
    fn run_starter_hello_dry_run_resolves_without_executing() {
        let home = tempfile::tempdir().unwrap();
        kadou()
            .env("KADOU_HOME", home.path())
            .args(["run", "starter/hello", "--dry-run"])
            .assert()
            .success()
            .stdout(predicate::str::contains("starter/hello"))
            .stdout(predicate::str::contains("env_names: NAME"))
            .stdout(predicate::str::contains("env_public: NAME=world"));

        // Materialized on first use, but nothing was actually executed.
        assert!(
            home.path()
                .join(".config/kadou/kata/starter/hello.sh")
                .is_file()
        );
    }

    #[test]
    fn run_starter_hello_actually_executes() {
        let home = tempfile::tempdir().unwrap();
        kadou()
            .env("KADOU_HOME", home.path())
            .args(["run", "starter/hello"])
            .assert()
            .success()
            .stdout(predicate::str::contains("hello, world"));

        kadou()
            .env("KADOU_HOME", home.path())
            .args(["run", "starter/hello", "name=mason"])
            .assert()
            .success()
            .stdout(predicate::str::contains("hello, mason"));
    }

    #[test]
    fn run_unknown_kata_is_a_clean_error() {
        let home = tempfile::tempdir().unwrap();
        kadou()
            .env("KADOU_HOME", home.path())
            .args(["run", "starter/nope"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("no such kata"));
    }

    #[test]
    fn run_real_fails_cleanly_on_a_missing_need() {
        let home = tempfile::tempdir().unwrap();
        let kata_dir = home.path().join(".config/kadou/kata/team");
        std::fs::create_dir_all(&kata_dir).unwrap();
        std::fs::write(
            kata_dir.join("secret-task.sh"),
            "#!/bin/sh\n# ---\n# about: Needs a secret\n# risk:  low\n# needs: api_token\n# ---\necho \"$API_TOKEN\"\n",
        )
        .unwrap();

        kadou()
            .env("KADOU_HOME", home.path())
            .args(["run", "team/secret-task"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("kadou vault set api_token"));
    }

    #[test]
    fn show_prints_header_fields_resolved_args_env_names_path_and_sha256() {
        let home = tempfile::tempdir().unwrap();
        kadou()
            .env("KADOU_HOME", home.path())
            .args(["show", "starter/hello"])
            .assert()
            .success()
            .stdout(predicate::str::contains("starter/hello"))
            .stdout(predicate::str::contains("low"))
            .stdout(predicate::str::contains("Print a greeting"))
            .stdout(predicate::str::contains("file    "))
            .stdout(predicate::str::contains("sha256  sha256:"))
            .stdout(predicate::str::contains("name = world"))
            .stdout(predicate::str::contains("env     NAME"));
    }

    #[test]
    fn sesami_dry_run_lists_jenkins_token_as_secret_with_no_defaults_leaked() {
        let home = tempfile::tempdir().unwrap();
        let src = home.path().join("catalog/src/cc4-aaa");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("runbook.yaml"),
            r#"
name: cc4-aaa
description: Trigger a SES/CC4/cc4-aaa branch pipeline
risk_level: medium
script: script.sh
parameters:
  - name: jenkins_url
    type: string
    required: true
    scope: global
    default: "https://ci.example.com"
    secret: false
  - name: jenkins_user
    type: string
    required: true
    scope: global
    secret: false
  - name: jenkins_token
    type: string
    required: true
    scope: global
    secret: true
  - name: branch
    type: string
    required: true
    default: "dev"
    scope: runbook
    secret: false
"#,
        )
        .unwrap();
        std::fs::write(
            src.join("script.sh"),
            "#!/bin/sh\nset -eu\necho \"would trigger jenkins for $BRANCH\"\n",
        )
        .unwrap();

        kadou()
            .env("KADOU_HOME", home.path())
            .args([
                "import",
                home.path().join("catalog/src").to_str().unwrap(),
                "--as",
                "sesami",
            ])
            .assert()
            .success();

        kadou()
            .env("KADOU_HOME", home.path())
            .args(["run", "sesami/cc4-aaa", "--dry-run"])
            .assert()
            .success()
            .stdout(predicate::str::contains("JENKINS_TOKEN"))
            .stdout(predicate::str::contains(
                "secret_env_names: JENKINS_USER, JENKINS_TOKEN",
            ))
            .stdout(predicate::str::contains("env_public: BRANCH=dev"))
            .stdout(predicate::str::contains(
                "env_public: JENKINS_URL=https://ci.example.com",
            ));
    }

    #[test]
    fn check_warns_on_a_missing_interpreter() {
        let home = tempfile::tempdir().unwrap();
        let kata_dir = home.path().join(".config/kadou/kata/team");
        std::fs::create_dir_all(&kata_dir).unwrap();
        std::fs::write(
            kata_dir.join("x.sh"),
            "#!/no/such/interpreter\n# ---\n# about: X\n# risk:  low\n# ---\necho hi\n",
        )
        .unwrap();

        kadou()
            .env("KADOU_HOME", home.path())
            .args(["check", "team"])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "declared interpreter `/no/such/interpreter` is not on PATH",
            ));
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
