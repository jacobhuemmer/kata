//! `kata history` (§7.1, §9 slice 8).

use std::process::ExitCode;

use super::resolve_paths;

/// `kata history [--limit N] [--json]` (§7.1, §9 slice 8): the newest records, newest first.
pub fn run_history(limit: Option<u32>, json: bool) -> ExitCode {
    let paths = resolve_paths();
    let store = kata_core::history::HistoryStore::new(&paths.state_dir);
    let limit = limit.unwrap_or(20) as usize;
    let records = store.list_recent(limit);

    if json {
        match serde_json::to_string(&records) {
            Ok(text) => println!("{text}"),
            Err(err) => {
                eprintln!("error: failed to serialize history: {err}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }

    if records.is_empty() {
        println!("no run history");
        return ExitCode::SUCCESS;
    }
    for record in &records {
        println!(
            "{:<36} {:<28} {:<9} {} ({})",
            record.history_id, record.id, record.status, record.start_time, record.interface
        );
    }
    ExitCode::SUCCESS
}
