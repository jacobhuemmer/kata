//! The one shared run pipeline (`docs/design/05-prd.md` §3 "one engine … so confirm/risk/vault
//! cannot drift", `docs/design/11-code-review.md` R2/C6): env → spec → exec → history, used by
//! `kadou-mcp`'s `run_kata`, the CLI's `kadou run`, and `kadou grant approve` alike.
//!
//! [`begin`] and [`finish`] are the two halves a caller composes: `begin` resolves the
//! `KADOU_*` context, builds the env, and writes the initial `status: running` history record
//! (so a caller has `history_id`/`log_path` to hand back before the run finishes); `finish`
//! runs the exec to completion (or accepts an already-completed [`kadou_exec::RunOutcome`])
//! and writes the terminal history record. [`run_one_blocking`] composes both for a caller
//! that just wants to block until done, exactly what the CLI needs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::history::{FinishOutcome, HistoryError, HistoryRecord, HistoryStore};
use crate::kata::Kata;
use crate::resolve::{ResolvedNeed, ResolvedVar};

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
/// under. For a project-local kata (id `./name`, §4.5), `kata_dir` is already that single
/// project's own kata directory — there is no per-folder subdivision to join, unlike a
/// library kata's `sesami/…` (`root` is the folder's own git checkout root).
pub fn kata_context(kata_dir: &Path, kata: &Kata) -> KataContext {
    let dir = kata
        .path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| kata_dir.to_path_buf());
    let root = if kata.id.starts_with("./") {
        kata_dir.to_path_buf()
    } else {
        let top_level = kata.id.split('/').next().unwrap_or(&kata.id);
        kata_dir.join(top_level)
    };
    KataContext {
        id: kata.id.clone(),
        file: kata.path.clone(),
        dir,
        root,
    }
}

/// `KADOU_ID`/`KADOU_FILE`/`KADOU_DIR`/`KADOU_ROOT` as env pairs, ready to fold into
/// [`kadou_exec::RunSpec::env`] (§6.1).
pub fn context_env(ctx: &KataContext) -> Vec<(String, String)> {
    vec![
        ("KADOU_ID".to_string(), ctx.id.clone()),
        ("KADOU_FILE".to_string(), ctx.file.display().to_string()),
        ("KADOU_DIR".to_string(), ctx.dir.display().to_string()),
        ("KADOU_ROOT".to_string(), ctx.root.display().to_string()),
    ]
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
/// [`crate::resolve_needs`]/[`crate::resolve_args`] — this just applies the public/secret
/// split (§5.5) that a real run's env-building step also needs.
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

/// Everything [`begin`] needs to build one run's env, [`kadou_exec::RunSpec`], and history
/// record. `resolved_needs` must already have every value present — the caller checks
/// `missing_needs` and stops before reaching here (§4.4); `begin` does not re-check.
pub struct RunOneRequest<'a> {
    pub kata: &'a Kata,
    pub folder: &'a str,
    pub resolved_args: &'a [ResolvedVar],
    pub resolved_needs: &'a [ResolvedNeed],
    /// The `kata/` directory this kata was found under — for a project-local kata, its own
    /// project root, not the library's `kata/` (§4.5).
    pub kata_dir_root: &'a Path,
    /// `"cli" | "mcp"` (§6.6, no `tui` value).
    pub interface: &'static str,
    pub initiator: &'a str,
    pub mcp_client: Option<&'a str>,
    /// Env set before `KADOU_*`/args/needs. Empty for the CLI (which inherits the parent
    /// process env via `env_clear: false`); the allowlisted set for MCP (§6.1).
    pub base_env: Vec<(String, String)>,
    pub env_clear: bool,
    /// `[exec] timeout`, already parsed (R6 moves parsing to config load).
    pub config_exec_timeout: std::time::Duration,
}

/// A run ready to exec: the history record already written (`status: running`, §6.6) and the
/// [`kadou_exec::RunSpec`] with its redaction closure and log sink wired up.
pub struct PreparedRun {
    pub history_store: HistoryStore,
    pub record: HistoryRecord,
    pub spec: kadou_exec::RunSpec,
}

/// Builds the env, the redaction closure, and the exec spec, and writes the initial
/// `status: running` history record (§6.6, §9 slice 5) — the record already carries
/// `history_id`/`log_path` a caller can hand back to a polling agent before the run finishes
/// (I-7).
pub fn begin(state_dir: &Path, req: &RunOneRequest<'_>) -> Result<PreparedRun, HistoryError> {
    let ctx = kata_context(req.kata_dir_root, req.kata);
    let mut env = req.base_env.clone();
    env.extend(context_env(&ctx));
    for arg in req.resolved_args {
        env.push((arg.env_name.clone(), arg.value.clone()));
    }
    for need in req.resolved_needs {
        if let Some(value) = &need.value {
            env.push((need.env_name.clone(), value.clone()));
        }
    }

    let secrets: Vec<String> = req
        .resolved_needs
        .iter()
        .filter(|n| n.secret)
        .filter_map(|n| n.value.clone())
        .collect();
    let need_values: Vec<String> = req
        .resolved_needs
        .iter()
        .filter_map(|n| n.value.clone())
        .collect();

    let history_store = HistoryStore::new(state_dir);
    let public_args: BTreeMap<String, String> = req
        .resolved_args
        .iter()
        .map(|a| (a.name.clone(), a.value.clone()))
        .collect();
    let record = history_store.begin(
        &req.kata.id,
        req.folder,
        &public_args,
        req.interface,
        req.initiator,
        req.mcp_client,
    )?;

    let redact: kadou_exec::LineRedactor =
        Arc::new(move |line: &str| crate::redact::redact_all(line, &secrets, &need_values));

    let spec = kadou_exec::RunSpec {
        shebang: req.kata.shebang.clone(),
        file: req.kata.path.clone(),
        cwd: ctx.dir,
        env,
        timeout: kadou_exec::effective_timeout(req.kata.timeout, req.config_exec_timeout),
        env_clear: req.env_clear,
        redact: Some(redact),
        log_sink: Some(record.log_path.clone()),
    };

    Ok(PreparedRun {
        history_store,
        record,
        spec,
    })
}

/// The last non-empty line, capped at 200 characters (§5.5's `summary` field).
fn last_non_empty_line(lines: &[String]) -> String {
    let line = lines
        .iter()
        .rev()
        .find(|l| !l.trim().is_empty())
        .cloned()
        .unwrap_or_default();
    if line.chars().count() > 200 {
        line.chars().take(200).collect()
    } else {
        line
    }
}

/// One run's outcome, shared by every interface (`interface`-agnostic; a caller adds its own
/// wire/CLI shaping on top — MCP's `output` truncation, the CLI's plain `println!`s).
#[derive(Debug, Clone)]
pub struct RunReport {
    pub id: String,
    pub status: kadou_exec::RunStatus,
    pub exit_code: Option<i32>,
    /// The merged, already-redacted line stream (redaction happened inside `kadou-exec`'s own
    /// line stream, before this — §6.6).
    pub output: Vec<String>,
    pub duration_ms: u64,
    pub history_id: String,
    pub log_path: PathBuf,
    /// The last non-empty output line, capped at 200 characters (§5.5's `summary` field) —
    /// the same value written to the history record's own `output_summary`.
    pub output_summary: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RunOneError {
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error("failed to run: {0}")]
    Exec(#[from] kadou_exec::ExecError),
}

/// The status string §6.6 uses in a history record and a wire result alike.
fn status_str(status: kadou_exec::RunStatus) -> &'static str {
    match status {
        kadou_exec::RunStatus::Success => "success",
        kadou_exec::RunStatus::Failed | kadou_exec::RunStatus::TimedOut => "failed",
        kadou_exec::RunStatus::Cancelled => "cancelled",
    }
}

/// Writes the terminal history record from an already-completed exec result and returns the
/// shared [`RunReport`]. On an [`kadou_exec::ExecError`] (the run never produced an outcome at
/// all), the history record is still finished as a failed run before the error propagates —
/// an internal failure is still audited (§6.6).
pub fn finish(
    history_store: &HistoryStore,
    mut record: HistoryRecord,
    exec_result: Result<kadou_exec::RunOutcome, kadou_exec::ExecError>,
) -> Result<RunReport, RunOneError> {
    let outcome = match exec_result {
        Ok(outcome) => outcome,
        Err(err) => {
            let _ = history_store.finish(
                &mut record,
                FinishOutcome {
                    status: "failed",
                    exit_code: None,
                    output_lines: 0,
                    output_summary: "",
                    duration_ms: 0,
                },
            );
            return Err(RunOneError::Exec(err));
        }
    };

    let output = outcome.output;
    let summary = last_non_empty_line(&output);
    let duration_ms = u64::try_from(outcome.duration.as_millis()).unwrap_or(u64::MAX);
    let status = status_str(outcome.status);

    let _ = history_store.finish(
        &mut record,
        FinishOutcome {
            status,
            exit_code: outcome.exit_code,
            output_lines: output.len(),
            output_summary: &summary,
            duration_ms,
        },
    );

    Ok(RunReport {
        id: record.id,
        status: outcome.status,
        exit_code: outcome.exit_code,
        output,
        duration_ms,
        history_id: record.history_id,
        log_path: record.log_path,
        output_summary: summary,
    })
}

/// Runs one kata to completion, blocking the calling thread (`kadou_exec::run_blocking`'s own
/// private single-threaded runtime) — the shape the CLI needs: `kadou run` and `kadou grant
/// approve` are both synchronous commands with no cancellation or `max_wait` race to run
/// (§9 slice 3/6). MCP's detach-capable flow composes [`begin`] and [`finish`] itself instead,
/// since it needs the record's `history_id`/`log_path` before the run is done.
pub fn run_one_blocking(
    state_dir: &Path,
    req: &RunOneRequest<'_>,
) -> Result<RunReport, RunOneError> {
    let prepared = begin(state_dir, req)?;
    let exec_result = kadou_exec::run_blocking(prepared.spec);
    finish(&prepared.history_store, prepared.record, exec_result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::risk::RiskLevel;

    #[test]
    fn last_non_empty_line_skips_trailing_blank_lines() {
        let lines = vec!["first".to_string(), "second".to_string(), String::new()];
        assert_eq!(last_non_empty_line(&lines), "second");
    }

    #[test]
    fn last_non_empty_line_truncates_at_200_characters() {
        let long = "x".repeat(250);
        let lines = vec![long.clone()];
        let summary = last_non_empty_line(&lines);
        assert_eq!(summary.chars().count(), 200);
        assert_eq!(summary, long.chars().take(200).collect::<String>());
    }

    #[test]
    fn last_non_empty_line_of_all_blank_lines_is_empty() {
        let lines = vec![String::new(), "   ".to_string()];
        assert_eq!(last_non_empty_line(&lines), "");
    }

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

    fn test_kata(path: PathBuf, id: &str) -> Kata {
        Kata {
            id: id.to_string(),
            path,
            about: "Test".to_string(),
            risk: RiskLevel::Low,
            needs: Vec::new(),
            args: Vec::new(),
            alias: Vec::new(),
            timeout: None,
            notes: None,
            shebang: Some("#!/bin/sh".to_string()),
        }
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
    fn kata_context_env_is_set() {
        let dir = tempfile::tempdir().unwrap();
        let kata_dir = dir.path().join("kata");
        std::fs::create_dir_all(kata_dir.join("starter")).unwrap();
        let kata = test_kata(kata_dir.join("starter/hello.sh"), "starter/hello");

        let ctx = kata_context(&kata_dir, &kata);
        assert_eq!(ctx.root, kata_dir.join("starter"));
        assert_eq!(ctx.dir, kata_dir.join("starter"));

        let env = context_env(&ctx);
        assert!(env.contains(&("KADOU_ID".to_string(), "starter/hello".to_string())));
        assert!(env.contains(&(
            "KADOU_ROOT".to_string(),
            kata_dir.join("starter").display().to_string()
        )));
    }

    #[test]
    fn project_local_kata_context_root_has_no_trailing_dot_component() {
        let dir = tempfile::tempdir().unwrap();
        let project_kata_dir = dir.path().join("myproject/kata");
        std::fs::create_dir_all(&project_kata_dir).unwrap();
        let kata = test_kata(project_kata_dir.join("deploy.sh"), "./deploy");

        let ctx = kata_context(&project_kata_dir, &kata);
        assert_eq!(ctx.root, project_kata_dir);
    }

    /// R2: the one shared pipeline redacts identically no matter which caller drives it — a
    /// table over `interface`, not two hand-copied assertions, so a future interface (e.g. a
    /// `tui`) is proven the same way for free.
    #[test]
    fn run_one_redacts_the_same_way_for_every_interface() {
        for interface in ["cli", "mcp"] {
            let dir = tempfile::tempdir().unwrap();
            let kata_dir = dir.path().join("kata/sesami");
            std::fs::create_dir_all(&kata_dir).unwrap();
            let file = write_script(
                &kata_dir,
                "leak.sh",
                "#!/bin/sh\necho \"token=hunter2ok\"\n",
            );
            let kata = test_kata(file, "sesami/leak");
            let state_dir = dir.path().join("state");

            let needs = vec![ResolvedNeed {
                name: "jenkins_token".to_string(),
                env_name: "JENKINS_TOKEN".to_string(),
                value: Some("hunter2ok".to_string()),
                secret: true,
            }];
            let req = RunOneRequest {
                kata: &kata,
                folder: "sesami",
                resolved_args: &[],
                resolved_needs: &needs,
                kata_dir_root: &dir.path().join("kata"),
                interface,
                initiator: "local",
                mcp_client: None,
                base_env: Vec::new(),
                env_clear: false,
                config_exec_timeout: std::time::Duration::from_secs(10),
            };

            let report = run_one_blocking(&state_dir, &req).unwrap();
            assert_eq!(
                report.status,
                kadou_exec::RunStatus::Success,
                "interface={interface}"
            );
            assert_eq!(
                report.output,
                vec!["token=****".to_string()],
                "interface={interface}"
            );

            // `finish` no longer rewrites `log_path` (D18) — its bytes are exactly what
            // `kadou-exec`'s own collector streamed, trailing newline included.
            let logged = std::fs::read_to_string(&report.log_path).unwrap();
            assert_eq!(logged, "token=****\n", "interface={interface}");
            assert!(!logged.contains("hunter2ok"), "interface={interface}");

            let record_path = state_dir
                .join("history/records")
                .join(format!("{}.json", report.history_id));
            let record_text = std::fs::read_to_string(&record_path).unwrap();
            assert!(
                record_text.contains(&format!("\"interface\":\"{interface}\"")),
                "history record must carry interface={interface}: {record_text}"
            );
        }
    }
}
