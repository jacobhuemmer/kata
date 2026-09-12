//! `kadou run` (§7.1, §6.1, §9 slice 3).

use std::collections::BTreeMap;
use std::io::IsTerminal as _;
use std::path::Path;
use std::process::ExitCode;

use kadou_core::{Config, KadouPaths, Kata, LastArgsStore, Vault, VaultStore};

use crate::ui;

use super::picker::{PickOutcome, resolve_id_or_pick};
use super::{
    auto_import_go_vault, check_human_ceiling, cli_confirm, find_kata_or_report, load_config,
    materialize_starter, print_run_report_and_exit_code, read_vault_value, resolve_paths,
    run_frame_header, run_one_cli, styled_for_stdout, vault_store,
};

/// Parses `k=v` positional args into a map, per the shape `kadou run <id> [k=v…]` already
/// declares in `main.rs`'s `RunArgs` (§7.1).
fn parse_kv(pairs: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for pair in pairs {
        let Some((key, value)) = pair.split_once('=') else {
            return Err(pair.clone());
        };
        out.insert(key.to_string(), value.to_string());
    }
    Ok(out)
}

/// `kadou run <id> [k=v…] [--dry-run]` (§7.1, §6.1, §9 slice 3). No picker: a missing id is a
/// stub for slice 8, same as the bare `kadou` frame.
/// Prints `--dry-run`'s env-name report (§6.1 "`dry_run` does not spawn. It returns env
/// names, not a command line.").
fn print_dry_run(kata_id: &str, result: &kadou_core::runner::DryRunResult) {
    println!("{kata_id}");
    println!("env_names: {}", result.env_names.join(", "));
    if result.env_public.is_empty() {
        println!("env_public: (none)");
    } else {
        for (name, value) in &result.env_public {
            println!("env_public: {name}={value}");
        }
    }
    if result.secret_env_names.is_empty() {
        println!("secret_env_names: (none)");
    } else {
        println!("secret_env_names: {}", result.secret_env_names.join(", "));
    }
}

/// Prompts for every missing need on a TTY and saves them to the vault before returning the
/// freshly re-resolved needs; off a TTY, reports the fix line and stops (§4.4 "Missing
/// needs"). Re-resolving after the save (rather than patching the in-memory list) keeps this
/// the same code path `kadou vault set` itself goes through.
fn resolve_missing_needs_interactively(
    kata: &Kata,
    vault_store_handle: &VaultStore,
    vault: &mut Vault,
    missing_needs: &[String],
) -> Result<Vec<kadou_core::ResolvedNeed>, ExitCode> {
    if std::io::stdin().is_terminal() {
        for name in missing_needs {
            let value = read_vault_value(name, false).map_err(|err| {
                eprintln!("error: failed to read a value for `{name}`: {err}");
                ExitCode::from(2)
            })?;
            vault.set(name.clone(), value, true);
        }
        vault_store_handle.save(vault).map_err(|err| {
            eprintln!("error: failed to save the vault: {err}");
            ExitCode::FAILURE
        })?;
        Ok(kadou_core::resolve_needs(kata, vault))
    } else {
        eprintln!(
            "error: {} needs {} but the vault isn't set up yet",
            kata.id,
            missing_needs.join(", ")
        );
        for name in missing_needs {
            eprintln!("  = kadou vault set {name}");
        }
        Err(ExitCode::from(2))
    }
}

/// Everything `run_run` needs before it can gate/spawn: the looked-up kata, its resolved
/// args/needs, and the vault handles the missing-needs prompt (if any) reuses.
struct RunPreparation {
    kata: Kata,
    resolved_args: Vec<kadou_core::ResolvedVar>,
    resolved_needs: Vec<kadou_core::ResolvedNeed>,
    vault_store_handle: VaultStore,
    vault: Vault,
}

fn prepare_run(
    paths: &KadouPaths,
    kata: Kata,
    provided: &BTreeMap<String, String>,
) -> Result<RunPreparation, ExitCode> {
    let resolved_args = kadou_core::resolve_args(&kata, provided).map_err(|err| {
        eprintln!("error: {err}");
        ExitCode::from(2)
    })?;
    let vault_store_handle = vault_store(paths);
    auto_import_go_vault(&vault_store_handle);
    let vault = vault_store_handle.load().unwrap_or_else(|err| {
        eprintln!("warning: failed to load the vault: {err}");
        Vault::default()
    });
    let resolved_needs = kadou_core::resolve_needs(&kata, &vault);
    Ok(RunPreparation {
        kata,
        resolved_args,
        resolved_needs,
        vault_store_handle,
        vault,
    })
}

/// The non-interactive replay line §6.3's confirm protocol prints on `NeedsConfirm`: the same
/// invocation with `--confirm <id>` appended. Built from the final `provided` map (kv pairs
/// plus anything a TTY prompt just filled in) so a replayed run carries everything the human
/// just typed, not only what was on the original command line.
fn build_replay_line(id: &str, provided: &BTreeMap<String, String>) -> String {
    let mut replay_line = format!("kadou run {id}");
    for (name, value) in provided {
        replay_line.push_str(&format!(" {name}={value}"));
    }
    replay_line.push_str(&format!(" --confirm {id}"));
    replay_line
}

fn int_validator(input: &str) -> Result<inquire::validator::Validation, inquire::CustomUserError> {
    if ui::prompt::is_valid_int(input) {
        Ok(inquire::validator::Validation::Valid)
    } else {
        Ok(inquire::validator::Validation::Invalid(
            "must be an integer".into(),
        ))
    }
}

/// The real `inquire` widget for one arg, chosen by its `ArgType` (§7.3 "A `bool` prompts
/// `y/n`; an `int` rejects non-digits before `↵`; a `select` is a four-row picker").
fn prompt_for_arg(
    arg: &kadou_core::Arg,
    prefill: Option<&str>,
) -> Result<String, inquire::InquireError> {
    use kadou_core::ArgType;
    match &arg.ty {
        ArgType::Bool => {
            let default = prefill.and_then(|s| s.parse().ok()).unwrap_or(false);
            inquire::Confirm::new(&arg.name)
                .with_default(default)
                .prompt()
                .map(|value| value.to_string())
        }
        ArgType::Int => {
            let mut text = inquire::Text::new(&arg.name).with_validator(int_validator);
            if let Some(p) = prefill {
                text = text.with_default(p);
            }
            text.prompt()
        }
        ArgType::Select { options } => {
            let mut select = inquire::Select::new(&arg.name, options.clone());
            if let Some(pos) = prefill.and_then(|p| options.iter().position(|o| o == p)) {
                select = select.with_starting_cursor(pos);
            }
            select.prompt()
        }
        ArgType::Text => {
            let mut text = inquire::Text::new(&arg.name);
            if let Some(p) = prefill {
                text = text.with_default(p);
            }
            text.prompt()
        }
    }
}

/// Prompts on a TTY for every arg named in `targets`, in header order, prefilled per D5
/// (§6.6, §7.3): the header default when the arg has one, else the last-used value. A
/// cancelled prompt (`esc`/Ctrl+c) is "cancelled", exit 130, same as every other interactive
/// cancel in this CLI.
fn prompt_for_args(
    state_dir: &Path,
    kata_id: &str,
    targets: &[&kadou_core::Arg],
    provided: &mut BTreeMap<String, String>,
) -> Result<(), ExitCode> {
    let last_used = LastArgsStore::new(state_dir).read(kata_id);
    for arg in targets {
        let prefill = ui::prompt::prefill(arg, last_used.get(&arg.name).map(String::as_str));
        match prompt_for_arg(arg, prefill.as_deref()) {
            Ok(value) => {
                provided.insert(arg.name.clone(), value);
            }
            Err(_) => {
                eprintln!("cancelled");
                return Err(ExitCode::from(130));
            }
        }
    }
    Ok(())
}

/// `kadou run <id>`'s missing-required-arg prompts and `--ask`'s prompt-every-arg walk (§7.3):
/// on a TTY, fills `provided` in place; off a TTY, `--ask` is a hard error (there is nothing to
/// prompt with) while a missing required arg is left for `resolve_args`'s own `MissingArg`
/// error to report, unchanged from before this slice.
fn fill_args_interactively(
    paths: &KadouPaths,
    kata: &Kata,
    provided: &mut BTreeMap<String, String>,
    ask: bool,
) -> Result<(), ExitCode> {
    let targets: Vec<&kadou_core::Arg> = if ask {
        kata.args.iter().collect()
    } else {
        kata.args
            .iter()
            .filter(|arg| arg.is_required() && !provided.contains_key(&arg.name))
            .collect()
    };
    if targets.is_empty() {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        if ask {
            eprintln!("error: --ask needs a terminal");
            eprintln!("  = kadou run {} [k=v...] [--dry-run]", kata.id);
            return Err(ExitCode::from(2));
        }
        return Ok(());
    }
    prompt_for_args(&paths.state_dir, &kata.id, &targets, provided)
}

pub fn run_run(
    id: Option<String>,
    kv: Vec<String>,
    dry_run: bool,
    confirm_flag: Option<String>,
    ask: bool,
    plain: bool,
) -> ExitCode {
    let id = match id {
        Some(id) => id,
        None => match resolve_id_or_pick("run", plain) {
            PickOutcome::Use(id) => id,
            PickOutcome::Done(code) => return code,
        },
    };

    let mut provided = match parse_kv(&kv) {
        Ok(m) => m,
        Err(bad) => {
            eprintln!("error: invalid arg `{bad}`, expected key=value");
            return ExitCode::from(2);
        }
    };

    let paths = resolve_paths();
    materialize_starter(&paths);
    let config = load_config(&paths);

    let kata = match find_kata_or_report(&paths.kata_dir(), &id) {
        Ok(k) => k,
        Err(code) => return code,
    };
    if let Err(code) = fill_args_interactively(&paths, &kata, &mut provided, ask) {
        return code;
    }

    let mut prep = match prepare_run(&paths, kata, &provided) {
        Ok(p) => p,
        Err(code) => return code,
    };

    if dry_run {
        let result = kadou_core::runner::dry_run(&prep.resolved_args, &prep.resolved_needs);
        print_dry_run(&prep.kata.id, &result);
        return ExitCode::SUCCESS;
    }

    if let Err(code) = check_human_ceiling(&prep.kata, &config) {
        return code;
    }
    let replay_line = build_replay_line(&id, &provided);
    if let Err(code) = cli_confirm(&prep.kata, confirm_flag.as_deref(), &replay_line) {
        return code;
    }

    if let Err(code) = fill_missing_needs_interactively(&mut prep) {
        return code;
    }

    execute_run(&paths, &config, &prep, plain)
}

/// Prints the run header, spawns via `run_one_cli`, and prints the report/exit code -- the
/// shared tail of `run_run` after every gate (ceiling, confirm, needs) has passed.
fn execute_run(
    paths: &KadouPaths,
    config: &Config,
    prep: &RunPreparation,
    plain: bool,
) -> ExitCode {
    let folder = prep
        .kata
        .id
        .split('/')
        .next()
        .unwrap_or(&prep.kata.id)
        .to_string();
    let styled = styled_for_stdout(plain);
    print!(
        "{}",
        run_frame_header(
            &prep.kata,
            &prep.resolved_args,
            &prep.resolved_needs,
            styled
        )
    );

    match run_one_cli(
        paths,
        config,
        &prep.kata,
        &folder,
        &prep.resolved_args,
        &prep.resolved_needs,
        None,
    ) {
        Ok(report) => print_run_report_and_exit_code(
            &report,
            &prep.kata,
            &prep.resolved_args,
            config,
            &paths.state_dir,
            styled,
        ),
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Re-resolves `prep.resolved_needs` after prompting for any missing ones, if there are any
/// (a no-op otherwise) — the shared tail of `run_run`'s missing-needs handling.
fn fill_missing_needs_interactively(prep: &mut RunPreparation) -> Result<(), ExitCode> {
    let missing_needs: Vec<String> = prep
        .resolved_needs
        .iter()
        .filter(|n| n.value.is_none())
        .map(|n| n.name.clone())
        .collect();
    if missing_needs.is_empty() {
        return Ok(());
    }
    prep.resolved_needs = resolve_missing_needs_interactively(
        &prep.kata,
        &prep.vault_store_handle,
        &mut prep.vault,
        &missing_needs,
    )?;
    Ok(())
}
