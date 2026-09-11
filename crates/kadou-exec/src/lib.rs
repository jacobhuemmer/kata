//! Process execution engine for kata: shebang runtime, cwd, env injection, timeouts, and
//! cancellation via process groups (`docs/design/05-prd.md` §6.1, §9 slice 3).
//!
//! The shebang is the runtime: `argv` is `<interpreter-from-shebang> <abs-kata-path>`, or
//! `/bin/sh <abs-kata-path>` when there is no shebang. `stdin` is always `/dev/null`;
//! `stdout`/`stderr` are always piped (never inherited) and merged into one line stream.
//! Cancellation and `[exec] timeout` both terminate the child's whole process group: `SIGTERM`
//! first, `SIGKILL` after a 5s grace (§6.1 "Timeout and lifecycle").
//!
//! This crate does not decide *what* env a child gets — the MCP allowlist (§6.1 "MCP child
//! environment") lands in slice 5. [`RunSpec::env`] is set on top of whatever the caller's own
//! process env already is (inherited by default, exactly the "CLI keeps the full parent
//! environment" rule, §6.1).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use std::collections::BTreeMap;

use kadou_core::{Kata, ResolvedNeed, ResolvedVar};
use tokio::io::{AsyncBufReadExt as _, AsyncRead};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

/// How long to wait after `SIGTERM` before escalating to `SIGKILL` (§6.1).
const KILL_GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("failed to spawn {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to wait for child: {0}")]
    Wait(#[source] std::io::Error),
    #[error("failed to start the exec runtime: {0}")]
    Runtime(#[source] std::io::Error),
}

/// `KADOU_ID`/`KADOU_FILE`/`KADOU_DIR`/`KADOU_ROOT` for one kata (§6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KataContext {
    pub id: String,
    pub file: PathBuf,
    /// cwd for the exec: the kata's directory (folder form) or the folder containing the
    /// file.
    pub dir: PathBuf,
    /// The top-level folder under `kata/` — the git checkout root for a `kadou get` folder,
    /// regardless of how deeply the kata itself is nested.
    pub root: PathBuf,
}

/// Builds the `KADOU_*` context from a resolved [`Kata`] and the `kata/` root it was found
/// under.
pub fn kata_context(kata_dir: &Path, kata: &Kata) -> KataContext {
    let dir = kata
        .path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| kata_dir.to_path_buf());
    let top_level = kata.id.split('/').next().unwrap_or(&kata.id);
    KataContext {
        id: kata.id.clone(),
        file: kata.path.clone(),
        dir,
        root: kata_dir.join(top_level),
    }
}

/// `KADOU_ID`/`KADOU_FILE`/`KADOU_DIR`/`KADOU_ROOT` as env pairs, ready to fold into
/// [`RunSpec::env`] (§6.1).
pub fn context_env(ctx: &KataContext) -> Vec<(String, String)> {
    vec![
        ("KADOU_ID".to_string(), ctx.id.clone()),
        ("KADOU_FILE".to_string(), ctx.file.display().to_string()),
        ("KADOU_DIR".to_string(), ctx.dir.display().to_string()),
        ("KADOU_ROOT".to_string(), ctx.root.display().to_string()),
    ]
}

/// Splits a shebang line into its argv words, or `["/bin/sh"]` when there is none (§6.1). Not
/// a full POSIX shebang parser — kadou's own header grammar never needs quoted interpreter
/// arguments, so whitespace splitting is enough.
pub fn interpreter_words(shebang: Option<&str>) -> Vec<String> {
    let words: Vec<String> = shebang
        .map(|line| line.strip_prefix("#!").unwrap_or(line))
        .map(|rest| rest.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    if words.is_empty() {
        vec!["/bin/sh".to_string()]
    } else {
        words
    }
}

/// `argv`: the interpreter words followed by the absolute kata path (§6.1).
pub fn resolve_argv(shebang: Option<&str>, file: &Path) -> Vec<String> {
    let mut argv = interpreter_words(shebang);
    argv.push(file.display().to_string());
    argv
}

/// The interpreter a shebang actually declares, unwrapping `env <name>` to `<name>` — that is
/// the program whose absence `kadou check` should warn about, not `env` itself.
fn declared_interpreter(shebang: Option<&str>) -> String {
    let words = interpreter_words(shebang);
    let base = Path::new(&words[0])
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(&words[0]);
    if base == "env" && words.len() > 1 {
        words[1].clone()
    } else {
        words[0].clone()
    }
}

/// `Some(name)` when the declared interpreter is missing from disk (an absolute/relative
/// path) or `PATH` (a bare name) — the check `kadou check` runs at load time (§6.1 "`kadou
/// check` warns when the declared interpreter is not on `PATH`").
pub fn interpreter_missing(shebang: Option<&str>) -> Option<String> {
    let name = declared_interpreter(shebang);
    let found = if name.contains('/') {
        Path::new(&name).is_file()
    } else {
        on_path(&name)
    };
    (!found).then_some(name)
}

fn on_path(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(cmd).is_file()))
        .unwrap_or(false)
}

/// `[exec] timeout`, lowered or raised by the kata's own `timeout:` header, up to the 24h max
/// the header parser already enforces (§4.3, §6.1).
pub fn effective_timeout(kata_timeout: Option<Duration>, config_timeout_raw: &str) -> Duration {
    kata_timeout.unwrap_or_else(|| {
        humantime::parse_duration(config_timeout_raw).unwrap_or(Duration::from_secs(30 * 60))
    })
}

/// One exec request: everything [`run`] needs, already resolved (§6.1).
#[derive(Debug, Clone)]
pub struct RunSpec {
    pub shebang: Option<String>,
    /// Absolute path to the kata file.
    pub file: PathBuf,
    pub cwd: PathBuf,
    /// Set on top of the caller's own process env (inherited by default — "CLI keeps the
    /// full parent environment", §6.1). Includes `KADOU_*` plus resolved args and needs.
    pub env: Vec<(String, String)>,
    /// `[exec] timeout`, lowered or raised by the kata's own `timeout:` header (§6.1).
    pub timeout: Duration,
    /// `true` starts the child from an empty environment (only `env` is set) instead of
    /// inheriting this process's own environment first. MCP sets this — its `env` is already
    /// the complete allowlisted set (§6.1 "MCP child environment"); the CLI leaves it `false`
    /// ("CLI keeps the full parent environment", §6.1).
    pub env_clear: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Success,
    Failed,
    TimedOut,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    /// The merged stdout/stderr line stream (§6.1 "merged line stream").
    pub output: Vec<String>,
    pub duration: Duration,
}

/// `dry_run`'s result: every env name a real run would set, split into the public values an
/// agent may see and the names that are secret-shaped and never get a value shown (§5.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunResult {
    pub env_names: Vec<String>,
    pub env_public: BTreeMap<String, String>,
    pub secret_env_names: Vec<String>,
}

/// Resolves env names without spawning (§6.1 "`dry_run` does not spawn. It returns env
/// names, not a command line."). Needs and args are already resolved by
/// `kadou_core::resolve_needs`/`resolve_args` — this just applies the public/secret split
/// (§5.5) that `run`'s real env-building step also needs.
pub fn dry_run(args: &[ResolvedVar], needs: &[ResolvedNeed]) -> DryRunResult {
    let mut env_names = Vec::with_capacity(args.len() + needs.len());
    let mut env_public = BTreeMap::new();
    let mut secret_env_names = Vec::new();

    for need in needs {
        env_names.push(need.env_name.clone());
        if need.secret {
            secret_env_names.push(need.env_name.clone());
        } else if let Some(value) = &need.value {
            env_public.insert(need.env_name.clone(), value.clone());
        }
    }
    for arg in args {
        env_names.push(arg.env_name.clone());
        env_public.insert(arg.env_name.clone(), arg.value.clone());
    }

    DryRunResult {
        env_names,
        env_public,
        secret_env_names,
    }
}

/// Execs one kata to completion (`docs/design/05-prd.md` §6.1). Builds a private
/// single-threaded tokio runtime per call — the CLI is not otherwise async, and one run
/// blocks the terminal either way.
pub fn run_blocking(spec: RunSpec) -> Result<RunOutcome, ExecError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(ExecError::Runtime)?;
    rt.block_on(run(spec, None))
}

/// Async form of [`run_blocking`], with an optional cancellation signal (`notifications/cancelled`
/// in MCP terms, §6.1) alongside the timeout every run already carries.
pub async fn run(
    spec: RunSpec,
    cancel: Option<oneshot::Receiver<()>>,
) -> Result<RunOutcome, ExecError> {
    let start = Instant::now();

    let mut argv = resolve_argv(spec.shebang.as_deref(), &spec.file);
    let program = argv.remove(0);

    let mut cmd = Command::new(&program);
    cmd.args(&argv)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if spec.env_clear {
        cmd.env_clear();
    }
    for (key, value) in &spec.env {
        cmd.env(key, value);
    }
    #[cfg(unix)]
    {
        // A new process group, pgid == the child's own pid, so a cancel/timeout can signal
        // the whole tree the kata spawned, not just the immediate child (§6.1).
        cmd.process_group(0);
    }

    let mut child = cmd.spawn().map_err(|source| ExecError::Spawn {
        program: program.clone(),
        source,
    })?;
    let pid = child.id();

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (tx, mut rx) = mpsc::unbounded_channel();
    let out_task = tokio::spawn(pump_lines(stdout, tx.clone()));
    let err_task = tokio::spawn(pump_lines(stderr, tx.clone()));
    drop(tx);
    let collector = tokio::spawn(async move {
        let mut lines = Vec::new();
        while let Some(line) = rx.recv().await {
            lines.push(line);
        }
        lines
    });

    let cancel_fut = async move {
        match cancel {
            Some(rx) => {
                let _ = rx.await;
            }
            None => std::future::pending::<()>().await,
        }
    };

    enum Race {
        Exited(std::io::Result<std::process::ExitStatus>),
        TimedOut,
        Cancelled,
    }

    let race = tokio::select! {
        res = child.wait() => Race::Exited(res),
        () = tokio::time::sleep(spec.timeout) => Race::TimedOut,
        () = cancel_fut => Race::Cancelled,
    };

    let (status, exit_code) = match race {
        Race::Exited(Ok(exit_status)) => {
            let status = if exit_status.success() {
                RunStatus::Success
            } else {
                RunStatus::Failed
            };
            (status, exit_code_of(exit_status))
        }
        Race::Exited(Err(source)) => return Err(ExecError::Wait(source)),
        Race::TimedOut => {
            terminate(pid, &mut child).await;
            (RunStatus::TimedOut, None)
        }
        Race::Cancelled => {
            terminate(pid, &mut child).await;
            (RunStatus::Cancelled, None)
        }
    };

    let _ = out_task.await;
    let _ = err_task.await;
    let output = collector.await.unwrap_or_default();

    Ok(RunOutcome {
        status,
        exit_code,
        output,
        duration: start.elapsed(),
    })
}

#[cfg(unix)]
fn exit_code_of(status: std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt as _;
    status.code().or_else(|| status.signal().map(|s| -s))
}

#[cfg(not(unix))]
fn exit_code_of(status: std::process::ExitStatus) -> Option<i32> {
    status.code()
}

async fn pump_lines<R: AsyncRead + Unpin>(reader: R, tx: mpsc::UnboundedSender<String>) {
    let mut lines = tokio::io::BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let _ = tx.send(line);
    }
}

/// `SIGTERM` the process group, then `SIGKILL` after [`KILL_GRACE`] if it hasn't exited
/// (§6.1). Best-effort on non-Unix: no process groups there, so this falls back to killing
/// just the direct child.
#[cfg(unix)]
async fn terminate(pid: Option<u32>, child: &mut Child) {
    use nix::sys::signal::{self, Signal};
    use nix::unistd::Pid;

    let Some(pid) = pid else {
        let _ = child.kill().await;
        return;
    };
    let pgid = Pid::from_raw(pid as i32);
    let _ = signal::killpg(pgid, Signal::SIGTERM);
    if tokio::time::timeout(KILL_GRACE, child.wait())
        .await
        .is_err()
    {
        let _ = signal::killpg(pgid, Signal::SIGKILL);
        let _ = child.wait().await;
    }
}

#[cfg(not(unix))]
async fn terminate(_pid: Option<u32>, child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_script(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    fn spec(shebang: Option<&str>, file: PathBuf, cwd: PathBuf) -> RunSpec {
        RunSpec {
            shebang: shebang.map(str::to_string),
            file,
            cwd,
            env: Vec::new(),
            timeout: Duration::from_secs(10),
            env_clear: false,
        }
    }

    #[test]
    fn resolve_argv_defaults_to_bin_sh_with_no_shebang() {
        let argv = resolve_argv(None, Path::new("/x/kata.sh"));
        assert_eq!(argv, vec!["/bin/sh".to_string(), "/x/kata.sh".to_string()]);
    }

    #[test]
    fn resolve_argv_uses_the_declared_shebang() {
        let argv = resolve_argv(Some("#!/usr/bin/env python3"), Path::new("/x/kata.sh"));
        assert_eq!(
            argv,
            vec![
                "/usr/bin/env".to_string(),
                "python3".to_string(),
                "/x/kata.sh".to_string(),
            ]
        );
    }

    #[test]
    fn interpreter_missing_is_none_for_bin_sh() {
        assert_eq!(interpreter_missing(Some("#!/bin/sh")), None);
    }

    #[test]
    fn interpreter_missing_names_an_absent_absolute_interpreter() {
        let missing = interpreter_missing(Some("#!/no/such/interpreter"));
        assert_eq!(missing.as_deref(), Some("/no/such/interpreter"));
    }

    #[test]
    fn interpreter_missing_unwraps_env_to_the_real_interpreter() {
        let missing = interpreter_missing(Some("#!/usr/bin/env kadou-test-nonexistent-xyz"));
        assert_eq!(missing.as_deref(), Some("kadou-test-nonexistent-xyz"));
        // A real interpreter behind `env` is not missing.
        assert_eq!(interpreter_missing(Some("#!/usr/bin/env python3")), None);
    }

    #[tokio::test]
    async fn fixture_kata_echoes_an_env_var() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(dir.path(), "kata.sh", "#!/bin/sh\necho \"$FOO\"\n");
        let mut s = spec(Some("#!/bin/sh"), file, dir.path().to_path_buf());
        s.env.push(("FOO".to_string(), "bar".to_string()));

        let outcome = run(s, None).await.unwrap();
        assert_eq!(outcome.status, RunStatus::Success);
        assert_eq!(outcome.output, vec!["bar".to_string()]);
    }

    #[tokio::test]
    async fn env_clear_starts_the_child_from_an_empty_environment() {
        // `CARGO_MANIFEST_DIR` is always set in this test process's own environment (cargo
        // sets it), but never appears in `spec.env` here — it must not reach a child that
        // asked for `env_clear: true` (§6.1 "MCP child environment" — the allowlist, not
        // blanket inheritance, decides what's visible). This reads real ambient state rather
        // than mutating it, avoiding the `unsafe`/parallel-test-race concerns `set_var` carries
        // (edition 2024 made it `unsafe` for exactly that reason).
        assert!(
            std::env::var("CARGO_MANIFEST_DIR").is_ok(),
            "sanity: cargo sets this"
        );

        let dir = tempfile::tempdir().unwrap();
        let file = write_script(
            dir.path(),
            "kata.sh",
            "#!/bin/sh\necho \"leak=${CARGO_MANIFEST_DIR:-none}\"\n",
        );
        let mut s = spec(Some("#!/bin/sh"), file, dir.path().to_path_buf());
        s.env_clear = true;

        let outcome = run(s, None).await.unwrap();
        assert_eq!(outcome.status, RunStatus::Success);
        assert_eq!(outcome.output, vec!["leak=none".to_string()]);
    }

    #[tokio::test]
    async fn secret_shaped_env_never_needs_the_process_to_print_it() {
        // kadou-exec only ever sets what it's told; a secret name that's never in `env`
        // simply never reaches the child. This is the exec-side half of "secret-shaped names
        // appear in env_names not env_public" — the naming/redaction split itself is
        // kadou-core's `resolve` module (§4.4).
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(
            dir.path(),
            "kata.sh",
            "#!/bin/sh\nenv | grep -c JENKINS_TOKEN || true\n",
        );
        let s = spec(Some("#!/bin/sh"), file, dir.path().to_path_buf());
        let outcome = run(s, None).await.unwrap();
        assert_eq!(outcome.output, vec!["0".to_string()]);
    }

    #[tokio::test]
    async fn stdin_is_dev_null_a_cat_script_does_not_hang() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(dir.path(), "kata.sh", "#!/bin/sh\ncat\necho done\n");
        let s = spec(Some("#!/bin/sh"), file, dir.path().to_path_buf());
        let start = Instant::now();
        let outcome = run(s, None).await.unwrap();
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "cat should hit EOF on /dev/null immediately, took {:?}",
            start.elapsed()
        );
        assert_eq!(outcome.status, RunStatus::Success);
        assert_eq!(outcome.output, vec!["done".to_string()]);
    }

    #[tokio::test]
    async fn stdout_and_stderr_are_both_piped_and_merged() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(
            dir.path(),
            "kata.sh",
            "#!/bin/sh\necho out-line\necho err-line 1>&2\n",
        );
        let s = spec(Some("#!/bin/sh"), file, dir.path().to_path_buf());
        let outcome = run(s, None).await.unwrap();
        assert!(outcome.output.contains(&"out-line".to_string()));
        assert!(outcome.output.contains(&"err-line".to_string()));
    }

    #[tokio::test]
    async fn no_shebang_defaults_to_bin_sh() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(dir.path(), "kata.sh", "echo no-shebang-ran\n");
        let s = spec(None, file, dir.path().to_path_buf());
        let outcome = run(s, None).await.unwrap();
        assert_eq!(outcome.status, RunStatus::Success);
        assert_eq!(outcome.output, vec!["no-shebang-ran".to_string()]);
    }

    #[tokio::test]
    async fn env_bash_shebang_actually_runs_as_bash() {
        // A helper reached via `env bash` must run under bash, not fall back to `/bin/sh` —
        // proven with a bash-only array, which a POSIX `sh` (dash, or macOS's sh) rejects.
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(
            dir.path(),
            "kata.sh",
            "#!/usr/bin/env bash\narr=(one two three)\necho \"${arr[1]}\"\n",
        );
        let s = spec(Some("#!/usr/bin/env bash"), file, dir.path().to_path_buf());
        let outcome = run(s, None).await.unwrap();
        assert_eq!(outcome.status, RunStatus::Success);
        assert_eq!(outcome.output, vec!["two".to_string()]);
    }

    #[tokio::test]
    async fn python_shebang_runs_under_python() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(
            dir.path(),
            "kata.sh",
            "#!/usr/bin/env python3\nprint(\"py-ok\")\n",
        );
        let s = spec(
            Some("#!/usr/bin/env python3"),
            file,
            dir.path().to_path_buf(),
        );
        let outcome = run(s, None).await.unwrap();
        assert_eq!(outcome.status, RunStatus::Success);
        assert_eq!(outcome.output, vec!["py-ok".to_string()]);
    }

    #[tokio::test]
    async fn failing_kata_reports_failed_status_and_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(dir.path(), "kata.sh", "#!/bin/sh\nexit 7\n");
        let s = spec(Some("#!/bin/sh"), file, dir.path().to_path_buf());
        let outcome = run(s, None).await.unwrap();
        assert_eq!(outcome.status, RunStatus::Failed);
        assert_eq!(outcome.exit_code, Some(7));
    }

    #[tokio::test]
    async fn timeout_terminates_a_sleeping_kata_via_sigterm() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(dir.path(), "kata.sh", "#!/bin/sh\nsleep 30\n");
        let mut s = spec(Some("#!/bin/sh"), file, dir.path().to_path_buf());
        s.timeout = Duration::from_millis(200);

        let start = Instant::now();
        let outcome = run(s, None).await.unwrap();
        assert_eq!(outcome.status, RunStatus::TimedOut);
        assert!(
            start.elapsed() < KILL_GRACE,
            "a plain `sleep` should die on SIGTERM alone, not need the SIGKILL grace: took {:?}",
            start.elapsed()
        );
    }

    #[tokio::test]
    async fn cancel_terminates_the_process_group_via_sigterm() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(dir.path(), "kata.sh", "#!/bin/sh\nsleep 30\n");
        let s = spec(Some("#!/bin/sh"), file, dir.path().to_path_buf());
        let (tx, rx) = oneshot::channel();
        tx.send(()).unwrap();

        let start = Instant::now();
        let outcome = run(s, Some(rx)).await.unwrap();
        assert_eq!(outcome.status, RunStatus::Cancelled);
        assert!(
            start.elapsed() < KILL_GRACE,
            "SIGTERM alone should stop `sleep`: took {:?}",
            start.elapsed()
        );
    }

    #[tokio::test]
    async fn timeout_escalates_to_sigkill_when_sigterm_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(dir.path(), "kata.sh", "#!/bin/sh\ntrap '' TERM\nsleep 30\n");
        let mut s = spec(Some("#!/bin/sh"), file, dir.path().to_path_buf());
        s.timeout = Duration::from_millis(200);

        let start = Instant::now();
        let outcome = run(s, None).await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(outcome.status, RunStatus::TimedOut);
        assert!(
            elapsed >= KILL_GRACE,
            "a SIGTERM-ignoring script must wait out the SIGKILL grace: took {elapsed:?}"
        );
        assert!(
            elapsed < KILL_GRACE + Duration::from_secs(3),
            "SIGKILL should land promptly after the grace: took {elapsed:?}"
        );
    }

    #[test]
    fn dry_run_puts_secret_shaped_needs_in_secret_names_not_public() {
        let args = vec![ResolvedVar {
            name: "branch".to_string(),
            env_name: "BRANCH".to_string(),
            value: "dev".to_string(),
        }];
        let needs = vec![
            ResolvedNeed {
                name: "jenkins_url".to_string(),
                env_name: "JENKINS_URL".to_string(),
                value: Some("https://ci.example.com".to_string()),
                secret: false,
            },
            ResolvedNeed {
                name: "jenkins_token".to_string(),
                env_name: "JENKINS_TOKEN".to_string(),
                value: None,
                secret: true,
            },
        ];

        let result = dry_run(&args, &needs);
        assert_eq!(
            result.env_names,
            vec![
                "JENKINS_URL".to_string(),
                "JENKINS_TOKEN".to_string(),
                "BRANCH".to_string(),
            ]
        );
        assert_eq!(result.secret_env_names, vec!["JENKINS_TOKEN".to_string()]);
        assert!(!result.env_public.contains_key("JENKINS_TOKEN"));
        assert_eq!(
            result.env_public.get("JENKINS_URL"),
            Some(&"https://ci.example.com".to_string())
        );
        assert_eq!(result.env_public.get("BRANCH"), Some(&"dev".to_string()));
    }

    #[test]
    fn effective_timeout_prefers_the_kata_header_override() {
        assert_eq!(
            effective_timeout(Some(Duration::from_secs(10)), "30m"),
            Duration::from_secs(10)
        );
        assert_eq!(effective_timeout(None, "30m"), Duration::from_secs(30 * 60));
        assert_eq!(
            effective_timeout(None, "not-a-duration"),
            Duration::from_secs(30 * 60)
        );
    }

    #[tokio::test]
    async fn kadou_context_env_is_set() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_script(
            dir.path(),
            "kata.sh",
            "#!/bin/sh\necho \"$KADOU_ID $KADOU_FILE $KADOU_DIR $KADOU_ROOT\"\n",
        );
        let kata_dir = dir.path().join("kata");
        std::fs::create_dir_all(kata_dir.join("starter")).unwrap();
        std::fs::rename(&file, kata_dir.join("starter/hello.sh")).unwrap();

        let kata = Kata {
            id: "starter/hello".to_string(),
            path: kata_dir.join("starter/hello.sh"),
            about: "Test".to_string(),
            risk: kadou_core::RiskLevel::Low,
            needs: Vec::new(),
            args: Vec::new(),
            alias: Vec::new(),
            timeout: None,
            notes: None,
            shebang: Some("#!/bin/sh".to_string()),
        };
        let ctx = kata_context(&kata_dir, &kata);
        assert_eq!(ctx.root, kata_dir.join("starter"));
        assert_eq!(ctx.dir, kata_dir.join("starter"));

        let mut s = spec(Some("#!/bin/sh"), ctx.file.clone(), ctx.dir.clone());
        s.env = context_env(&ctx);
        let outcome = run(s, None).await.unwrap();
        assert_eq!(
            outcome.output,
            vec![format!(
                "starter/hello {} {} {}",
                ctx.file.display(),
                ctx.dir.display(),
                ctx.root.display()
            )]
        );
    }
}
