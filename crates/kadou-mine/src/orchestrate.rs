//! `kadou mine run`'s engine: wires ingest -> parse -> extract -> normalize -> cluster -> rank
//! -> redact -> propose into one bounded run (`docs/design/06-session-mining.md` §2-3).
//!
//! Idempotency rule 1 (same event path + bytes -> skip, `06` §3.4) does almost all of the
//! idempotency work by itself: a second run against unchanged input reads zero new
//! transcripts, so it extracts zero new candidates and queues nothing new, without this
//! module needing to merge new evidence into an existing `meta.json` (`06` §3.4 rule 4,
//! deliberately not implemented this slice -- see the handoff's interpretation calls).
//!
//! `ingest_row` streams each transcript with a `BufReader`, line by line, rather than reading
//! the whole file into memory first: `06` §2.1 measured a real Codex store at ~2.8 GB, and
//! §3.5 requires the scan to stop mid-file once [`MineConfig::max_bytes_scanned`] is hit,
//! which is only possible if bytes are charged to the bound as each line is actually read.

use std::collections::BTreeMap;
use std::io::BufRead as _;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use kadou_core::RiskLevel;

use crate::cluster::{self, Cluster};
use crate::model::Agent;
use crate::propose::StepWithParams;
use crate::store::MineHome;
use crate::{extract, ingest, normalize, parse, propose, rank, store};

/// 20 minutes (`06` §3.5).
const MAX_WALL_TIME: Duration = Duration::from_secs(20 * 60);
/// 2 GB (`06` §3.5), the default for [`MineConfig::max_bytes_scanned`].
const DEFAULT_MAX_BYTES_SCANNED: u64 = 2 * 1024 * 1024 * 1024;
/// A lock older than this is treated as an abandoned prior run, not a live one, and is
/// reclaimed rather than blocking forever (`06` §3.4 rule 5).
const LOCK_STALE_AFTER: Duration = Duration::from_secs(25 * 60);

pub struct MineConfig {
    pub index_path: PathBuf,
    pub state_dir: PathBuf,
    pub redact_extra: Vec<String>,
    pub home_prefix: Option<String>,
    pub root_prefix: Option<String>,
    /// The run-wide scanned-bytes bound (`06` §3.5). Defaults to 2 GB; a test lowers it to
    /// prove the mid-transcript stop without needing a real multi-gigabyte fixture (B2).
    pub max_bytes_scanned: u64,
}

impl MineConfig {
    pub fn new(index_path: PathBuf, state_dir: PathBuf) -> Self {
        Self {
            index_path,
            state_dir,
            redact_extra: Vec::new(),
            home_prefix: None,
            root_prefix: None,
            max_bytes_scanned: DEFAULT_MAX_BYTES_SCANNED,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub events_seen: usize,
    pub transcripts_seen: usize,
    pub transcripts_missing: usize,
    pub candidates_emitted: usize,
    pub clusters: usize,
    pub queued: usize,
    pub already_running: bool,
    pub bounded_stop: bool,
}

/// `true` when a fresh lock is already held -- a concurrent run should exit immediately
/// (`06` §3.4 rule 5). A stale lock (older than [`LOCK_STALE_AFTER`]) is reclaimed instead.
fn lock_is_held(home: &MineHome) -> bool {
    let Ok(metadata) = std::fs::metadata(home.lock_path()) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age < LOCK_STALE_AFTER)
}

fn acquire_lock(home: &MineHome) -> std::io::Result<()> {
    if let Some(parent) = home.lock_path().parent() {
        kadou_core::fsutil::ensure_dir_0700(parent)?;
    }
    std::fs::write(home.lock_path(), b"")
}

fn release_lock(home: &MineHome) {
    let _ = std::fs::remove_file(home.lock_path());
}

/// A cluster's chosen medoid steps, still paired with the concrete param values recorded when
/// each exact template was first normalized in this run -- what `propose::build_proposal`
/// needs but `Cluster` itself doesn't carry (it only stores template text, `06` §2.5).
fn steps_with_params(
    cluster: &Cluster,
    template_params: &BTreeMap<String, Vec<BTreeMap<String, String>>>,
) -> Vec<StepWithParams> {
    let per_step_params = template_params.get(&cluster.template);
    cluster
        .template
        .split('\n')
        .enumerate()
        .map(|(i, template)| StepWithParams {
            template: template.to_string(),
            params: per_step_params
                .and_then(|v| v.get(i))
                .cloned()
                .unwrap_or_default(),
        })
        .collect()
}

/// Runs one bounded mining pass (`06` §2-3). Returns immediately with `already_running: true`
/// if another run's lock is still fresh.
pub fn run_once(config: &MineConfig) -> RunSummary {
    let home = MineHome::new(&config.state_dir);
    if lock_is_held(&home) {
        return RunSummary {
            already_running: true,
            ..RunSummary::default()
        };
    }
    if acquire_lock(&home).is_err() {
        return RunSummary {
            already_running: true,
            ..RunSummary::default()
        };
    }

    let summary = run_locked(config, &home);
    release_lock(&home);
    summary
}

/// Per-template example params, keyed by the exact normalized template (steps joined by
/// `"\n"`) -- recorded the first time this run sees that template so
/// `propose::build_proposal` has a concrete example value for each declared arg.
type TemplateParams = BTreeMap<String, Vec<BTreeMap<String, String>>>;

struct IngestOutcome {
    candidates: Vec<cluster::Candidate>,
    template_params: TemplateParams,
    events_seen: usize,
    transcripts_seen: usize,
    transcripts_missing: usize,
    candidates_emitted: usize,
    bounded_stop: bool,
}

/// Streams `transcript_path` line by line, charging each line's bytes to `bytes_scanned` as it
/// is read (§3.5) and stopping *before* reading a line that would push `bytes_scanned` past
/// `config.max_bytes_scanned` -- never reading the rest of the file. Returns the shell events
/// parsed from whatever was read, and whether the bound cut the file short.
fn stream_transcript(
    config: &MineConfig,
    agent: Agent,
    transcript_path: &std::path::Path,
    bytes_scanned: &mut u64,
) -> std::io::Result<(Vec<crate::model::RawShellEvent>, bool)> {
    let file = std::fs::File::open(transcript_path)?;
    let reader = std::io::BufReader::new(file);
    let mut raw_events = Vec::new();
    for line in reader.lines() {
        let line = line?;
        // +1 for the newline the reader stripped -- an approximation for the last,
        // possibly-unterminated line, which is fine: this bound only has to be close, not exact.
        let line_bytes = line.len() as u64 + 1;
        if *bytes_scanned + line_bytes > config.max_bytes_scanned {
            return Ok((raw_events, true));
        }
        *bytes_scanned += line_bytes;
        parse::parse_line(agent, &line, &mut raw_events);
    }
    Ok((raw_events, false))
}

/// One in-flight `ingest_new_candidates` run's own state (L9, `docs/design/12-mvp-review.md`
/// §4/§6): `home`/`index_base` are fixed for the run, `processed`/`started`/`bytes_scanned`
/// accumulate across rows, and `out` is the summary being built -- one logical unit, not eight
/// separate parameters threaded through a free function.
struct IngestRun<'a> {
    home: &'a MineHome,
    index_base: PathBuf,
    processed: Vec<ingest::ProcessedRecord>,
    started: Instant,
    bytes_scanned: u64,
    out: IngestOutcome,
}

impl IngestRun<'_> {
    /// One index row's contribution: streams its transcript (bounded, §3.5), parses,
    /// extracts, and normalizes into candidates, recording a processed-checkpoint row along
    /// the way. Returns `false` when the resource bounds were hit -- either before this row
    /// could be attempted at all, or mid-file while streaming it -- so the caller stops the
    /// whole loop. A row cut short mid-file is deliberately *not* checkpointed as processed,
    /// so the next run re-reads it from the start rather than skipping the part it never saw.
    fn ingest_row(&mut self, config: &MineConfig, row: &ingest::IndexRow) -> bool {
        if self.started.elapsed() >= MAX_WALL_TIME || self.bytes_scanned >= config.max_bytes_scanned
        {
            return false;
        }

        let event_md_path = ingest::resolve_relative(&self.index_base, &row.path);
        let Some(transcript_raw) = ingest::read_transcript_path(&event_md_path) else {
            self.record_processed(row, "skip: transcript_missing");
            self.out.transcripts_missing += 1;
            return true;
        };
        let transcript_path = ingest::resolve_relative(&self.index_base, &transcript_raw);

        let Some(agent) = Agent::parse(&row.agent) else {
            self.record_processed(row, "skip: unknown_agent");
            return true;
        };

        let Ok((raw_events, bounded_mid_file)) =
            stream_transcript(config, agent, &transcript_path, &mut self.bytes_scanned)
        else {
            self.record_processed(row, "skip: transcript_missing");
            self.out.transcripts_missing += 1;
            return true;
        };
        self.out.transcripts_seen += 1;

        for sequence in extract::extract_sequences(&raw_events) {
            let normalized: Vec<normalize::Normalized> = sequence
                .steps
                .iter()
                .map(|step| {
                    normalize::normalize_command(
                        step,
                        config.home_prefix.as_deref(),
                        config.root_prefix.as_deref(),
                    )
                })
                .collect();
            let templates: Vec<String> = normalized.iter().map(|n| n.template.clone()).collect();
            self.out
                .template_params
                .entry(templates.join("\n"))
                .or_insert_with(|| normalized.iter().map(|n| n.params.clone()).collect());

            self.out.candidates.push(cluster::Candidate {
                steps: templates,
                agent: agent.as_str().to_string(),
                session_id: row.session_id.clone(),
                when: sequence.when,
            });
            self.out.candidates_emitted += 1;
        }

        if bounded_mid_file {
            return false;
        }
        self.record_processed(row, "ok");
        true
    }

    fn record_processed(&mut self, row: &ingest::IndexRow, status: &str) {
        let record = ingest::ProcessedRecord {
            event_path: row.path.clone(),
            bytes: row.bytes,
            transcript_sha256: None,
            status: status.to_string(),
        };
        let _ = ingest::append_processed(self.home, &record);
        self.processed.push(record);
    }
}

fn ingest_new_candidates(config: &MineConfig, home: &MineHome) -> IngestOutcome {
    let index_base = config
        .index_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    let rows = ingest::read_index_rows(&config.index_path);
    let processed = ingest::load_processed(home);

    let mut run = IngestRun {
        home,
        index_base,
        processed,
        started: Instant::now(),
        bytes_scanned: 0,
        out: IngestOutcome {
            candidates: Vec::new(),
            template_params: TemplateParams::new(),
            events_seen: 0,
            transcripts_seen: 0,
            transcripts_missing: 0,
            candidates_emitted: 0,
            bounded_stop: false,
        },
    };

    for row in &rows {
        if ingest::already_processed(&run.processed, &row.path, row.bytes) {
            continue;
        }
        run.out.events_seen += 1;
        if !run.ingest_row(config, row) {
            run.out.bounded_stop = true;
            break;
        }
    }

    run.out
}

fn record_redaction_failure(home: &MineHome, fingerprint: &str) {
    let line = serde_json::json!({
        "fingerprint": fingerprint,
        "rule_id": "R14_HIGH_ENTROPY",
        "when": humantime::format_rfc3339_seconds(SystemTime::now()).to_string(),
    })
    .to_string();
    let _ = kadou_core::fsutil::append_line_0600(&home.redaction_failures_path(), &line);
}

/// One passing cluster's propose-and-queue attempt: `true` if it was newly queued.
fn propose_and_queue(
    config: &MineConfig,
    home: &MineHome,
    cl: &Cluster,
    template_params: &TemplateParams,
) -> bool {
    let score = rank::score(cl, SystemTime::now(), false);
    let risk = default_risk_for(cl);
    let steps = steps_with_params(cl, template_params);

    let Ok(proposal) = propose::build_proposal(cl, &steps, score, risk, &config.redact_extra)
    else {
        record_redaction_failure(home, &cl.fingerprint);
        return false;
    };
    if store::write_queue_entry(
        home,
        &cl.fingerprint,
        &proposal.kata_source,
        &proposal.meta_json,
    )
    .is_err()
    {
        return false;
    }
    let _ = store::append_audit(
        home,
        &store::AuditRow {
            when: humantime::format_rfc3339_seconds(SystemTime::now()).to_string(),
            action: "queued".to_string(),
            fingerprint: cl.fingerprint.clone(),
            actor: "schedule".to_string(),
            reason: None,
            catalog_id: None,
            script_sha256: None,
        },
    );
    true
}

/// Ranks, cuts off, and proposes every cluster not already queued or decided (`06` §2.6-2.9,
/// §3.4 rule 3). Returns how many were newly queued.
fn propose_ranked_clusters(
    config: &MineConfig,
    home: &MineHome,
    clusters: &[Cluster],
    template_params: &TemplateParams,
) -> usize {
    let already_queued = store::queued_fingerprints(home);
    let audit_rows = store::read_audit(home);

    clusters
        .iter()
        .filter(|cl| rank::passes_cutoff(cl))
        .filter(|cl| !already_queued.contains(&cl.fingerprint))
        .filter(|cl| {
            !matches!(
                store::latest_action(&audit_rows, &cl.fingerprint).as_deref(),
                Some("approved") | Some("rejected")
            )
        })
        .filter(|cl| propose_and_queue(config, home, cl, template_params))
        .count()
}

fn run_locked(config: &MineConfig, home: &MineHome) -> RunSummary {
    let ingested = ingest_new_candidates(config, home);
    let clusters = cluster::cluster_candidates(&ingested.candidates);
    let queued = propose_ranked_clusters(config, home, &clusters, &ingested.template_params);

    RunSummary {
        events_seen: ingested.events_seen,
        transcripts_seen: ingested.transcripts_seen,
        transcripts_missing: ingested.transcripts_missing,
        candidates_emitted: ingested.candidates_emitted,
        clusters: clusters.len(),
        queued,
        already_running: false,
        bounded_stop: ingested.bounded_stop,
    }
}

fn default_risk_for(cluster: &Cluster) -> RiskLevel {
    crate::risk::default_risk(&cluster.template)
}
