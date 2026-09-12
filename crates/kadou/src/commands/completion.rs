//! `kadou completion` (§7.1, §7.3, §9 slice 8).

use std::process::ExitCode;

/// `kadou completion <shell>` (§7.1, §7.3, §9 slice 8) via `clap_complete`.
pub fn run_completion(shell: &str) -> ExitCode {
    let Ok(shell) = shell.parse::<clap_complete::Shell>() else {
        eprintln!("error: unknown shell `{shell}`");
        eprintln!("  = one of: bash, elvish, fish, powershell, zsh");
        return ExitCode::from(2);
    };
    let mut cmd = <crate::Cli as clap::CommandFactory>::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
    ExitCode::SUCCESS
}
