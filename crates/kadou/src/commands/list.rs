//! `kadou list` (§7.1, §9 slice 2).

use std::process::ExitCode;

use kadou_core::{Config, RiskLevel};

use super::{load_config, materialize_starter, resolve_paths};

/// `kadou list [--folder F] [--risk R] [query]` — the CLI projection of `list_kata` (§7.1).
/// A human's own ceiling applies (§6.2 `visible(k,f) = rank(k.risk) ≤ human_ceiling(f)
/// [CLI]`); untrusted project-local folders are out of scope for this slice.
type ScannedFolders = Vec<(String, Vec<kadou_core::ScannedFile>)>;

/// One folder's visible-and-matching rows: gated on the human ceiling (§6.2 [CLI]), then the
/// `--risk` and free-text query filters.
fn list_rows_for_folder(
    files: &[kadou_core::ScannedFile],
    ceiling: RiskLevel,
    risk_filter: Option<RiskLevel>,
    query_lower: Option<&str>,
) -> Vec<(String, RiskLevel, String)> {
    let mut rows = Vec::new();
    for file in files {
        let Some(header) = &file.header else {
            continue;
        };
        if header.risk > ceiling {
            continue;
        }
        if let Some(want) = risk_filter
            && header.risk != want
        {
            continue;
        }
        if let Some(q) = query_lower {
            let haystack = format!(
                "{} {} {}",
                file.id.to_lowercase(),
                header.about.to_lowercase(),
                header.alias.join(" ").to_lowercase()
            );
            if !haystack.contains(q) {
                continue;
            }
        }
        rows.push((file.id.clone(), header.risk, header.about.clone()));
    }
    rows
}

fn list_rows(
    scanned: &ScannedFolders,
    folder: Option<&str>,
    risk_filter: Option<RiskLevel>,
    query_lower: Option<&str>,
    config: &Config,
) -> Vec<(String, RiskLevel, String)> {
    let mut rows = Vec::new();
    for (name, files) in scanned {
        if let Some(want) = folder
            && name != want
        {
            continue;
        }
        let ceiling = kadou_core::visibility::human_ceiling(config, name);
        rows.extend(list_rows_for_folder(
            files,
            ceiling,
            risk_filter,
            query_lower,
        ));
    }
    rows.sort();
    rows
}

pub fn run_list(query: Option<String>, folder: Option<String>, risk: Option<String>) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let config = load_config(&paths);

    let risk_filter = match risk.as_deref() {
        None => None,
        Some(s) => match s.parse::<RiskLevel>() {
            Ok(r) => Some(r),
            Err(_) => {
                eprintln!("error: unknown risk level `{s}`");
                eprintln!("  = risk is one of low, medium, high, critical");
                return ExitCode::from(2);
            }
        },
    };

    let scanned = match kadou_core::scan_kata_dir(&paths.kata_dir()) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    let query_lower = query.map(|q| q.to_lowercase());
    let rows = list_rows(
        &scanned,
        folder.as_deref(),
        risk_filter,
        query_lower.as_deref(),
        &config,
    );

    if rows.is_empty() {
        println!("no kata found");
    } else {
        for (id, risk, about) in &rows {
            println!("{id:<28} \u{25cf} {risk:<8} {about}");
        }
    }

    ExitCode::SUCCESS
}
