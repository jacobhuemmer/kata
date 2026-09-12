//! CLI integration tests: drives the real, built `kadou` binary through `assert_cmd`
//! (`docs/design/05-prd.md` §9), moved out of `src/main.rs` (R15, `docs/design/
//! 11-code-review.md` G3/C7/G4). `CARGO_BIN_EXE_kadou` is set by Cargo for a genuine
//! integration test target the way it never reliably is for a bin's own unit-test harness --
//! see `docs/design/10-mutation-baseline.md`'s "known blocker".

use std::collections::BTreeMap;
use std::path::Path;

use assert_cmd::Command;
use kadou_core::RiskLevel;
use kadou_mcp::pending::PendingStore;
use predicates::prelude::*;

fn kadou() -> Command {
    Command::cargo_bin("kadou").unwrap()
}

/// Every stateful test's command builder: a fresh `KADOU_HOME`, the real `$HOME` always
/// removed (`clean-code-testing/SKILL.md`, FIRST: "never depend on the real $HOME" -- G4, half
/// the old call sites forgot this), and notifications forced to their no-op path so a test
/// that reaches a `pending_grant` or `propose_kata` never pops a real desktop notification.
fn kadou_in(home: &Path) -> Command {
    let mut cmd = kadou();
    cmd.env("KADOU_HOME", home)
        .env_remove("HOME")
        .env("KADOU_NOTIFY_TEST_NOOP", "1");
    cmd
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
fn run_ask_refuses_as_not_yet_implemented() {
    // §7.3: "kadou run <id> --ask prompts for every arg" -- unimplemented until slice 8.
    // Parsed-and-silently-dropped (I-19) is worse than an explicit refusal.
    let home = tempfile::tempdir().unwrap();
    kadou_in(home.path())
        .args(["run", "starter/hello", "--ask"])
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
    kadou_in(home.path())
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
    kadou_in(home.path())
        .args(["run", "starter/hello"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello, world"));

    kadou_in(home.path())
        .args(["run", "starter/hello", "name=mason"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello, mason"));
}

#[test]
fn run_unknown_kata_is_a_clean_error() {
    let home = tempfile::tempdir().unwrap();
    kadou_in(home.path())
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

    kadou_in(home.path())
        .args(["run", "team/secret-task"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("kadou vault set api_token"));
}

#[test]
fn show_prints_header_fields_resolved_args_env_names_path_and_sha256() {
    let home = tempfile::tempdir().unwrap();
    kadou_in(home.path())
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
fn sesami_dry_run_reports_default_less_needs_as_missing_not_guessed_secret_without_a_vault() {
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

    kadou_in(home.path())
        .args([
            "import",
            home.path().join("catalog/src").to_str().unwrap(),
            "--as",
            "sesami",
        ])
        .assert()
        .success();

    // Without any vault set up, jenkins_user and jenkins_token (no header default) are
    // `missing` — reported missing, not guessed secret (§4.4, §9 slice 4 replacing the
    // slice-3 stopgap): they appear in `env_names` but in neither `env_public` nor
    // `secret_env_names`. Only `kadou vault set` gives them a resolved value at all,
    // and only then does their secret bit show up (see
    // `needs_resolve_vault_before_default_and_dry_run_reflects_it`).
    kadou_in(home.path())
        .args(["run", "sesami/cc4-aaa", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "env_names: JENKINS_URL, JENKINS_USER, JENKINS_TOKEN, BRANCH",
        ))
        .stdout(predicate::str::contains("secret_env_names: (none)"))
        .stdout(predicate::str::contains("env_public: BRANCH=dev"))
        .stdout(predicate::str::contains(
            "env_public: JENKINS_URL=https://ci.example.com",
        ))
        .stdout(predicate::str::contains("JENKINS_USER=").not())
        .stdout(predicate::str::contains("JENKINS_TOKEN=").not());
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

    kadou_in(home.path())
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
    kadou_in(home.path()).arg("check").assert().success();
}

#[test]
fn list_on_a_fresh_home_shows_the_five_starter_kata() {
    // Decision D6 (§7.4, §9 slice 6): `list` materializes the starter kata on scan, so an
    // empty home is never truly empty — it replaces the old slice-2 expectation of
    // "no kata found".
    let home = tempfile::tempdir().unwrap();
    kadou_in(home.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("starter/hello"))
        .stdout(predicate::str::contains("starter/disk-usage"))
        .stdout(predicate::str::contains("starter/git-status"))
        .stdout(predicate::str::contains("starter/health"))
        .stdout(predicate::str::contains("starter/list-path"));
}

#[test]
fn import_then_list_still_shows_the_starter_kata() {
    // Reproduces the slice-5 defect (D6): importing a team folder first used to
    // permanently prevent the starter kata from ever materializing, since `kata_dir`
    // already existed the first time anything scanned it.
    let home = tempfile::tempdir().unwrap();
    let src = home.path().join("catalog/src/widget");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("runbook.yaml"),
        "name: widget\ndescription: Say hello\nrisk_level: low\nscript: script.sh\nparameters: []\n",
    )
    .unwrap();
    std::fs::write(src.join("script.sh"), "#!/bin/sh\necho hi\n").unwrap();

    kadou_in(home.path())
        .args([
            "import",
            home.path().join("catalog/src").to_str().unwrap(),
            "--as",
            "sesami",
        ])
        .assert()
        .success();

    kadou_in(home.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("sesami/widget"))
        .stdout(predicate::str::contains("starter/hello"));
}

#[test]
fn mcp_serve_on_a_fresh_home_materializes_the_starter_kata() {
    // §9 slice 6 D6 fix: a home that only ever ran `kadou mcp serve` (never `kadou list`
    // directly) must still get the starter kata — materialization happens at server
    // startup, before the transport ever completes an `initialize` handshake, so this
    // holds even for a peer that connects and immediately disconnects (closing stdin here
    // is EOF with no client on the other end at all, which the server reports as a
    // "connection closed" error — the exit code is not the point of this test, the
    // materialized files are).
    let home = tempfile::tempdir().unwrap();
    let _ = kadou_in(home.path())
        .args(["mcp", "serve"])
        .write_stdin("")
        .timeout(std::time::Duration::from_secs(10))
        .output();

    assert!(
        home.path()
            .join(".config/kadou/kata/starter/hello.sh")
            .is_file()
    );
    assert!(
        home.path()
            .join(".config/kadou/kata/starter/disk-usage.sh")
            .is_file()
    );
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

    kadou_in(home.path())
        .args([
            "import",
            home.path().join("catalog/src").to_str().unwrap(),
            "--as",
            "sesami",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported 1 kata into sesami"));

    kadou_in(home.path())
        .args(["check", "sesami"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checked 1 kata in sesami   0 errors  0 warnings",
        ));

    kadou_in(home.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("sesami/widget"));
}

#[test]
fn vault_list_with_no_entries_says_so() {
    let home = tempfile::tempdir().unwrap();
    kadou_in(home.path())
        .args(["vault", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no vault entries"));
}

#[test]
fn vault_set_plain_and_secret_round_trip_via_stdin_never_argv() {
    let home = tempfile::tempdir().unwrap();

    kadou_in(home.path())
        .args(["vault", "set", "jenkins_user", "--plain"])
        .write_stdin("ci-user")
        .assert()
        .success()
        .stdout(predicate::str::contains("saved jenkins_user"));

    kadou_in(home.path())
        .args(["vault", "set", "jenkins_token"])
        .write_stdin("not-a-real-token")
        .assert()
        .success()
        .stdout(predicate::str::contains("saved jenkins_token"));

    let assert = kadou_in(home.path())
        .args(["vault", "list"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(stdout.contains("jenkins_user") && stdout.contains("plain"));
    assert!(stdout.contains("jenkins_token") && stdout.contains("secret"));
    assert!(
        !stdout.contains("not-a-real-token"),
        "vault list must never print values: {stdout}"
    );

    // Vault files are 0600, keys/data dirs 0700 (§6.5).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let vault_file = home.path().join(".local/share/kadou/vault.json");
        let mode = std::fs::metadata(&vault_file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    kadou_in(home.path())
        .args(["vault", "rm", "jenkins_user"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed jenkins_user"));

    kadou_in(home.path())
        .args(["vault", "rm", "jenkins_user"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no vault entry named"));
}

#[test]
fn needs_resolve_vault_before_default_and_dry_run_reflects_it() {
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
"#,
    )
    .unwrap();
    std::fs::write(src.join("script.sh"), "#!/bin/sh\necho hi\n").unwrap();

    kadou_in(home.path())
        .args([
            "import",
            home.path().join("catalog/src").to_str().unwrap(),
            "--as",
            "sesami",
        ])
        .assert()
        .success();

    kadou_in(home.path())
        .args(["vault", "set", "jenkins_user", "--plain"])
        .write_stdin("ci-user")
        .assert()
        .success();
    kadou_in(home.path())
        .args(["vault", "set", "jenkins_token"])
        .write_stdin("not-a-real-token")
        .assert()
        .success();

    // check's missing-need warning consults the vault: only unset needs still warn.
    kadou_in(home.path())
        .args(["check", "sesami"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checked 1 kata in sesami   0 errors  0 warnings",
        ));

    // jenkins_url also has a header default, but a vault entry (§4.4) wins over it: this
    // is the case the operator hits when `kadou import` writes a default= into every
    // header and `kadou vault set jenkins_url` must not be a silent no-op (I-1).
    kadou_in(home.path())
        .args(["vault", "set", "jenkins_url", "--plain"])
        .write_stdin("https://vault.example.com")
        .assert()
        .success();

    kadou_in(home.path())
        .args(["run", "sesami/cc4-aaa", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "env_public: JENKINS_URL=https://vault.example.com",
        ))
        .stdout(predicate::str::contains("env_public: JENKINS_USER=ci-user"))
        .stdout(predicate::str::contains("secret_env_names: JENKINS_TOKEN"));
}

#[test]
fn show_applies_the_human_ceiling_like_list_and_run() {
    // §6.2: `visible(k,f) = rank(k.risk) <= human_ceiling(f)`, "[CLI]" — `show` is the CLI
    // projection of `describe_kata`, which already gates on the ceiling; `show` did not
    // (I-10). Default max_risk is medium, so this critical kata must stay invisible.
    let home = tempfile::tempdir().unwrap();
    let kata_dir = home.path().join(".config/kadou/kata/sesami");
    std::fs::create_dir_all(&kata_dir).unwrap();
    std::fs::write(
        kata_dir.join("ses-deploy.sh"),
        "#!/bin/sh\n# ---\n# about: Deploy\n# risk:  critical\n# ---\necho hi\n",
    )
    .unwrap();

    kadou_in(home.path())
        .args(["show", "sesami/ses-deploy"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("above the max_risk ceiling"));

    kadou_in(home.path())
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ses-deploy").not());
}

#[test]
fn run_critical_without_confirm_still_works_as_dry_run_only() {
    let home = critical_ceiling_home("ses-deploy", "critical");

    kadou_in(home.path())
        .args(["run", "sesami/ses-deploy", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sesami/ses-deploy"));
}

/// Writes a home with `max_risk = "critical"` (so the human ceiling never interferes) and
/// one kata at `risk` under `sesami/<id>.sh` — the fixture every §6.3 confirm-protocol test
/// below shares.
fn critical_ceiling_home(id: &str, risk: &str) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let kata_dir = home.path().join(".config/kadou/kata/sesami");
    std::fs::create_dir_all(&kata_dir).unwrap();
    std::fs::write(
        kata_dir.join(format!("{id}.sh")),
        format!(
            "#!/bin/sh\n# ---\n# about: Test kata\n# risk:  {risk}\n# args:\n#   version: text = 1.0\n# ---\necho ran-{id}\n"
        ),
    )
    .unwrap();
    std::fs::write(
        home.path().join(".config/kadou/kadou.toml"),
        "max_risk = \"critical\"\n",
    )
    .unwrap();
    home
}

#[test]
fn non_tty_high_without_confirm_exits_2_with_the_copyable_confirm_line() {
    let home = critical_ceiling_home("ses-release-build", "high");

    kadou_in(home.path())
        .args(["run", "sesami/ses-release-build"])
        .write_stdin("")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "sesami/ses-release-build is high and needs confirmation",
        ))
        .stderr(predicate::str::contains(
            "= kadou run sesami/ses-release-build --confirm sesami/ses-release-build",
        ));
}

#[test]
fn non_tty_critical_without_confirm_exits_2_with_the_copyable_confirm_line_including_args() {
    let home = critical_ceiling_home("ses-deploy", "critical");

    kadou_in(home.path())
        .args(["run", "sesami/ses-deploy", "version=1.2.3"])
        .write_stdin("")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "sesami/ses-deploy is critical and needs confirmation",
        ))
        .stderr(predicate::str::contains(
            "= kadou run sesami/ses-deploy version=1.2.3 --confirm sesami/ses-deploy",
        ));
}

#[test]
fn matching_confirm_flag_runs_a_high_risk_kata_non_interactively() {
    let home = critical_ceiling_home("ses-release-build", "high");

    kadou_in(home.path())
        .args([
            "run",
            "sesami/ses-release-build",
            "--confirm",
            "sesami/ses-release-build",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("ran-ses-release-build"));
}

#[test]
fn a_mismatched_confirm_flag_is_rejected() {
    let home = critical_ceiling_home("ses-deploy", "critical");

    kadou_in(home.path())
        .args(["run", "sesami/ses-deploy", "--confirm", "sesami/wrong-id"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("does not match"));
}

#[test]
fn the_human_ceiling_still_applies_even_with_a_matching_confirm_flag() {
    // §6.3: "The human ceiling still applies" — `--confirm` satisfies the confirm
    // protocol, but a kata above `max_risk` is refused regardless.
    let home = tempfile::tempdir().unwrap();
    let kata_dir = home.path().join(".config/kadou/kata/sesami");
    std::fs::create_dir_all(&kata_dir).unwrap();
    std::fs::write(
        kata_dir.join("ses-deploy.sh"),
        "#!/bin/sh\n# ---\n# about: Deploy SES\n# risk:  critical\n# ---\necho would-deploy\n",
    )
    .unwrap();
    // Default `max_risk` (medium) is left in place — no kadou.toml override.

    kadou_in(home.path())
        .args(["run", "sesami/ses-deploy", "--confirm", "sesami/ses-deploy"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("max_risk"));
}

#[test]
fn stopgap_gate_refuses_a_kata_above_the_folder_ceiling() {
    let home = tempfile::tempdir().unwrap();
    let kata_dir = home.path().join(".config/kadou/kata/team");
    std::fs::create_dir_all(&kata_dir).unwrap();
    std::fs::write(
        kata_dir.join("medium-task.sh"),
        "#!/bin/sh\n# ---\n# about: A medium task\n# risk:  medium\n# ---\necho ran\n",
    )
    .unwrap();
    std::fs::write(
        home.path().join(".config/kadou/kadou.toml"),
        "max_risk = \"low\"\n",
    )
    .unwrap();

    kadou_in(home.path())
        .args(["run", "team/medium-task"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("max_risk"));
}

#[test]
fn mcp_serve_refuses_an_unbuilt_http_transport() {
    kadou()
        .args(["mcp", "serve", "--transport", "http"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("not available in this build"));
}

#[test]
fn mcp_schema_prints_the_tools_list_and_byte_count() {
    kadou()
        .args(["mcp", "schema", "--bytes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"list_kata\""))
        .stdout(predicate::str::contains("\"describe_kata\""))
        .stdout(predicate::str::contains("\"run_kata\""))
        .stdout(predicate::str::contains("\"propose_kata\""))
        .stdout(predicate::str::contains("bytes: 2028"));
}

// -----------------------------------------------------------------------
// kadou grant * (§6.4, §7.1, §9 slice 6)
// -----------------------------------------------------------------------

/// Writes `<home>/.config/kadou/kata/<folder>/<name>.sh` with the given risk/body and
/// returns its `folder/name` id and current sha256 — the fixture kata every grant test
/// below runs against.
fn write_grant_kata(
    home: &std::path::Path,
    folder: &str,
    name: &str,
    risk: &str,
    body: &str,
) -> (String, String) {
    let kata_dir = home.join(".config/kadou/kata").join(folder);
    std::fs::create_dir_all(&kata_dir).unwrap();
    let source = format!("#!/bin/sh\n# ---\n# about: Test kata\n# risk:  {risk}\n# ---\n{body}");
    let path = kata_dir.join(format!("{name}.sh"));
    std::fs::write(&path, &source).unwrap();
    let sha256 = kadou_core::file_sha256(&path).unwrap();
    (format!("{folder}/{name}"), sha256)
}

fn pending_store(home: &std::path::Path) -> PendingStore {
    PendingStore::new(&home.join(".local/state/kadou"))
}

/// Simulates what `run_kata` would have written for a visible, not-allow-listed high/
/// critical kata (§6.4 item 1), without needing a live MCP server subprocess.
fn write_pending(
    home: &std::path::Path,
    id: &str,
    folder: &str,
    risk: RiskLevel,
    sha256: &str,
    source: &str,
    args: &[(&str, &str)],
) -> String {
    let store = pending_store(home);
    let args: BTreeMap<String, String> = args
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let record = store
        .create(
            id,
            folder,
            risk,
            args,
            "mcp",
            Some("claude-code"),
            sha256,
            source,
            None,
        )
        .unwrap();
    record.pending_id
}

#[test]
fn grant_list_says_so_when_empty() {
    let home = tempfile::tempdir().unwrap();
    kadou_in(home.path())
        .arg("grant")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("no pending grants"));
}

#[test]
fn grant_list_and_show_a_pending_record() {
    let home = tempfile::tempdir().unwrap();
    let (id, sha256) = write_grant_kata(
        home.path(),
        "sesami",
        "ses-deploy",
        "critical",
        "echo would-deploy\n",
    );
    let source =
        std::fs::read_to_string(home.path().join(".config/kadou/kata/sesami/ses-deploy.sh"))
            .unwrap();
    let pending_id = write_pending(
        home.path(),
        &id,
        "sesami",
        RiskLevel::Critical,
        &sha256,
        &source,
        &[("version", "1.2.3")],
    );

    kadou_in(home.path())
        .arg("grant")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(pending_id.as_str()))
        .stdout(predicate::str::contains("sesami/ses-deploy"))
        .stdout(predicate::str::contains("pending"))
        .stdout(predicate::str::contains("version=1.2.3"));

    kadou_in(home.path())
        .args(["grant", "show", &pending_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("sesami/ses-deploy"))
        .stdout(predicate::str::contains("critical"))
        .stdout(predicate::str::contains("claude-code"))
        .stdout(predicate::str::contains("version = 1.2.3"))
        .stdout(predicate::str::contains(
            "no changes to the kata since the request",
        ));
}

#[test]
fn grant_show_prints_a_diff_when_the_kata_changed_since_the_request() {
    let home = tempfile::tempdir().unwrap();
    let (id, sha256) =
        write_grant_kata(home.path(), "sesami", "ses-deploy", "critical", "echo v1\n");
    let pending_id = write_pending(
        home.path(),
        &id,
        "sesami",
        RiskLevel::Critical,
        &sha256,
        "#!/bin/sh\n# ---\n# about: Test kata\n# risk:  critical\n# ---\necho v1\n",
        &[],
    );
    write_grant_kata(home.path(), "sesami", "ses-deploy", "critical", "echo v2\n");

    kadou_in(home.path())
        .args(["grant", "show", &pending_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("-echo v1"))
        .stdout(predicate::str::contains("+echo v2"));
}

#[test]
fn grant_show_unknown_pending_id_is_a_clean_error() {
    let home = tempfile::tempdir().unwrap();
    kadou_in(home.path())
        .args(["grant", "show", "does-not-exist"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no pending grant"));
}

#[test]
fn grant_deny_removes_the_record() {
    let home = tempfile::tempdir().unwrap();
    let (id, sha256) =
        write_grant_kata(home.path(), "sesami", "ses-deploy", "critical", "echo hi\n");
    let pending_id = write_pending(
        home.path(),
        &id,
        "sesami",
        RiskLevel::Critical,
        &sha256,
        "src",
        &[],
    );

    kadou_in(home.path())
        .args(["grant", "deny", &pending_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("denied"));

    assert!(pending_store(home.path()).get(&pending_id).is_none());

    kadou_in(home.path())
        .args(["grant", "deny", &pending_id])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no pending grant"));
}

#[test]
fn grant_approve_runs_a_low_risk_kata_and_writes_back_history() {
    let home = tempfile::tempdir().unwrap();
    let (id, sha256) = write_grant_kata(
        home.path(),
        "team",
        "low-task",
        "low",
        "echo approved-and-ran\n",
    );
    let source =
        std::fs::read_to_string(home.path().join(".config/kadou/kata/team/low-task.sh")).unwrap();
    let pending_id = write_pending(
        home.path(),
        &id,
        "team",
        RiskLevel::Low,
        &sha256,
        &source,
        &[],
    );

    kadou_in(home.path())
        .args(["grant", "approve", &pending_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("approved-and-ran"));

    let record = pending_store(home.path()).get(&pending_id).unwrap();
    assert_eq!(record.status.as_deref(), Some("success"));
    assert!(record.history_id.is_some());
    let log_path = record.log_path.unwrap();
    assert!(log_path.is_file());
    assert!(
        std::fs::read_to_string(&log_path)
            .unwrap()
            .contains("approved-and-ran")
    );

    // A second approve on the same (now-completed) record is refused.
    kadou_in(home.path())
        .args(["grant", "approve", &pending_id])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("already approved"));
}

#[test]
fn grant_approve_refuses_when_the_kata_changed_since_the_request() {
    let home = tempfile::tempdir().unwrap();
    let (id, sha256) = write_grant_kata(home.path(), "team", "task", "low", "echo v1\n");
    let pending_id = write_pending(
        home.path(),
        &id,
        "team",
        RiskLevel::Low,
        &sha256,
        "src",
        &[],
    );
    // The kata changes on disk after the grant was requested.
    write_grant_kata(home.path(), "team", "task", "low", "echo v2\n");

    kadou_in(home.path())
        .args(["grant", "approve", &pending_id])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("sha256"));

    assert!(
        pending_store(home.path())
            .get(&pending_id)
            .unwrap()
            .history_id
            .is_none()
    );
}

#[test]
fn grant_approve_refuses_an_expired_record() {
    let home = tempfile::tempdir().unwrap();
    let (id, sha256) = write_grant_kata(home.path(), "team", "task", "low", "echo hi\n");
    let pending_id = write_pending(
        home.path(),
        &id,
        "team",
        RiskLevel::Low,
        &sha256,
        "src",
        &[],
    );
    let store = pending_store(home.path());
    let mut record = store.get(&pending_id).unwrap();
    record.expires = humantime::format_rfc3339_seconds(
        std::time::SystemTime::now() - std::time::Duration::from_secs(60),
    )
    .to_string();
    store.save(&record).unwrap();

    kadou_in(home.path())
        .args(["grant", "approve", &pending_id])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("expired"));
}

#[test]
fn grant_approve_runs_in_the_cli_environment_not_an_mcp_allowlisted_one() {
    // §6.4 item 5: "Approval runs in the human CLI's environment (full parent env, not
    // the MCP server's allowlisted one)". `FOO_CLI_ONLY` is not on the MCP allowlist
    // (`crates/kadou-mcp/src/env.rs`) — it must still reach an approved run because
    // approval never goes through that allowlist at all.
    let home = tempfile::tempdir().unwrap();
    let (id, sha256) = write_grant_kata(
        home.path(),
        "team",
        "env-check",
        "low",
        "echo \"leak=${FOO_CLI_ONLY:-none}\"\n",
    );
    let source =
        std::fs::read_to_string(home.path().join(".config/kadou/kata/team/env-check.sh")).unwrap();
    let pending_id = write_pending(
        home.path(),
        &id,
        "team",
        RiskLevel::Low,
        &sha256,
        &source,
        &[],
    );

    kadou_in(home.path())
        .env("FOO_CLI_ONLY", "visible-in-cli")
        .args(["grant", "approve", &pending_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("leak=visible-in-cli"));
}

#[test]
fn grant_approve_of_a_critical_kata_needs_confirm_non_interactively() {
    let home = tempfile::tempdir().unwrap();
    let (id, sha256) = write_grant_kata(
        home.path(),
        "sesami",
        "ses-deploy",
        "critical",
        "echo deployed\n",
    );
    std::fs::write(
        home.path().join(".config/kadou/kadou.toml"),
        "max_risk = \"critical\"\n",
    )
    .unwrap();
    let source =
        std::fs::read_to_string(home.path().join(".config/kadou/kata/sesami/ses-deploy.sh"))
            .unwrap();
    let pending_id = write_pending(
        home.path(),
        &id,
        "sesami",
        RiskLevel::Critical,
        &sha256,
        &source,
        &[],
    );

    kadou_in(home.path())
        .args(["grant", "approve", &pending_id])
        .write_stdin("")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("needs confirmation"))
        .stderr(predicate::str::contains(format!(
            "kadou grant approve {pending_id} --confirm sesami/ses-deploy"
        )));

    kadou_in(home.path())
        .args([
            "grant",
            "approve",
            &pending_id,
            "--confirm",
            "sesami/ses-deploy",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("deployed"));
}

#[test]
fn grant_allow_pins_to_the_current_sha256_by_default() {
    let home = tempfile::tempdir().unwrap();
    let (id, sha256) =
        write_grant_kata(home.path(), "sesami", "ses-deploy", "critical", "echo hi\n");

    kadou_in(home.path())
        .args(["grant", "allow", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains(sha256.as_str()));

    let config = kadou_core::Config::load(&home.path().join(".config/kadou/kadou.toml")).unwrap();
    assert_eq!(config.agent.allow, vec![format!("{id}@{sha256}")]);
}

#[test]
fn grant_allow_any_version_pins_to_nothing() {
    let home = tempfile::tempdir().unwrap();
    let (id, _sha256) =
        write_grant_kata(home.path(), "sesami", "ses-deploy", "critical", "echo hi\n");

    kadou_in(home.path())
        .args(["grant", "allow", &id, "--any-version"])
        .assert()
        .success();

    let config = kadou_core::Config::load(&home.path().join(".config/kadou/kadou.toml")).unwrap();
    assert_eq!(config.agent.allow, vec![id.clone()]);
}

#[test]
fn grant_allow_preserves_comments_and_does_not_write_unset_defaults() {
    // PRD §3.1: "Comment-preserving kadou trust / kadou grant allow config edits" (toml_edit).
    // A whole-config toml::to_string_pretty rewrite loses the comment and freezes every
    // default explicitly into the file (A1/I-5) -- EMPTY_TEMPLATE's whole point is that a
    // later default change is not frozen out by an old file.
    let home = tempfile::tempdir().unwrap();
    let (id, sha256) =
        write_grant_kata(home.path(), "sesami", "ses-deploy", "critical", "echo hi\n");
    let config_path = home.path().join(".config/kadou/kadou.toml");
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    std::fs::write(
        &config_path,
        "# a human's own comment, must survive\nmax_risk = \"high\"\n",
    )
    .unwrap();

    kadou_in(home.path())
        .args(["grant", "allow", &id])
        .assert()
        .success();

    let text = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        text.contains("# a human's own comment, must survive"),
        "comment lost: {text}"
    );
    assert!(
        !text.contains("[mcp]"),
        "an untouched table must not be materialized: {text}"
    );

    let config = kadou_core::Config::load(&config_path).unwrap();
    assert_eq!(config.max_risk, kadou_core::RiskLevel::High);
    assert_eq!(config.agent.allow, vec![format!("{id}@{sha256}")]);
}

#[test]
fn grant_allow_is_idempotent_replacing_a_prior_entry_for_the_same_id() {
    let home = tempfile::tempdir().unwrap();
    let (id, _sha256) =
        write_grant_kata(home.path(), "sesami", "ses-deploy", "critical", "echo v1\n");
    kadou_in(home.path())
        .args(["grant", "allow", &id])
        .assert()
        .success();

    write_grant_kata(home.path(), "sesami", "ses-deploy", "critical", "echo v2\n");
    kadou_in(home.path())
        .args(["grant", "allow", &id])
        .assert()
        .success();

    let config = kadou_core::Config::load(&home.path().join(".config/kadou/kadou.toml")).unwrap();
    assert_eq!(config.agent.allow.len(), 1, "must replace, not accumulate");
}
