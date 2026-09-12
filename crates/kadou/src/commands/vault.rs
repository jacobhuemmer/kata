//! `kadou vault` (§6.5, §7.1, §9 slice 4).

use std::process::ExitCode;

use kadou_core::Vault;

use super::{auto_import_go_vault, load_vault, read_vault_value, resolve_paths, vault_store};

/// `kadou vault set [--plain] <name>` (§6.5, §7.1, §9 slice 4). Reads the value from a TTY
/// prompt or stdin, never argv — `main.rs`'s `VaultAction::Set` has no `value` field, so
/// there is nowhere on the command line a value could even go.
pub fn run_vault_set(name: String, plain: bool) -> ExitCode {
    let paths = resolve_paths();
    let store = vault_store(&paths);
    auto_import_go_vault(&store);

    let value = match read_vault_value(&name, plain) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("error: failed to read a value for `{name}`: {err}");
            return ExitCode::FAILURE;
        }
    };

    let mut vault = store.load().unwrap_or_else(|err| {
        eprintln!("warning: failed to load the existing vault: {err}; starting empty");
        Vault::default()
    });
    vault.set(name.clone(), value, !plain);

    match store.save(&vault) {
        Ok(()) => {
            println!("saved {name}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: failed to save the vault: {err}");
            ExitCode::FAILURE
        }
    }
}

/// `kadou vault list` (§6.5, §7.1): names and the secret bit, never values.
pub fn run_vault_list() -> ExitCode {
    let paths = resolve_paths();
    let vault = load_vault(&paths);

    if vault.is_empty() {
        println!("no vault entries");
        return ExitCode::SUCCESS;
    }

    let mut names: Vec<(&str, bool)> = vault.names().collect();
    names.sort_by(|a, b| a.0.cmp(b.0));
    for (name, secret) in names {
        let kind = if secret { "secret" } else { "plain" };
        println!("{name:<28} {kind}");
    }
    ExitCode::SUCCESS
}

/// `kadou vault rm <name>` (§7.1).
pub fn run_vault_rm(name: String) -> ExitCode {
    let paths = resolve_paths();
    let store = vault_store(&paths);
    let mut vault = store.load().unwrap_or_else(|err| {
        eprintln!("warning: failed to load the vault: {err}");
        Vault::default()
    });

    if !vault.remove(&name) {
        eprintln!("error: no vault entry named `{name}`");
        return ExitCode::from(2);
    }

    match store.save(&vault) {
        Ok(()) => {
            println!("removed {name}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: failed to save the vault: {err}");
            ExitCode::FAILURE
        }
    }
}
