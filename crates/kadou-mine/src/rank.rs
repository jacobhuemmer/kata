//! Ranking and the proposal cutoff (`docs/design/06-session-mining.md` §2.6).

use std::time::SystemTime;

use crate::cluster::Cluster;

#[cfg(test)]
mod tests;

const SECONDS_PER_DAY: f64 = 86_400.0;
const RECENCY_HALF_LIFE_DAYS: f64 = 30.0;
const SEQUENCE_BONUS_THRESHOLD: usize = 3;
const SEQUENCE_BONUS: f64 = 1.3;
const CATALOG_PENALTY: f64 = 0.1;

/// Age of `cluster.last_seen` relative to `now`, in days. Unparseable timestamps (should not
/// happen for a value this crate produced itself) are treated as maximally old rather than
/// panicking, so a malformed record scores low instead of crashing a run.
fn age_days(last_seen: &str, now: SystemTime) -> f64 {
    let Ok(last_seen_time) = humantime::parse_rfc3339_weak(last_seen) else {
        return f64::MAX;
    };
    match now.duration_since(last_seen_time) {
        Ok(elapsed) => elapsed.as_secs_f64() / SECONDS_PER_DAY,
        Err(_) => 0.0, // `last_seen` is in the future relative to `now`: treat as fresh.
    }
}

/// `score = log(1 + freq) * recency * unique_sessions * unique_agents * sequence_bonus *
/// catalog_penalty` (`06` §2.6). `now` and `catalog_conflict` (an existing kata already covers
/// this cluster's argv0 + subcommand) are caller-supplied so the formula stays pure and
/// testable without a wall clock or a real kata library.
pub fn score(cluster: &Cluster, now: SystemTime, catalog_conflict: bool) -> f64 {
    let freq_term = (1.0 + cluster.freq() as f64).ln();
    let recency = (-age_days(&cluster.last_seen, now) / RECENCY_HALF_LIFE_DAYS).exp();
    let unique_sessions = cluster.unique_sessions() as f64;
    let unique_agents = 1.0 + 0.25 * (cluster.unique_agents() as f64 - 1.0);
    let sequence_bonus = if cluster.step_count >= SEQUENCE_BONUS_THRESHOLD {
        SEQUENCE_BONUS
    } else {
        1.0
    };
    let catalog_penalty = if catalog_conflict {
        CATALOG_PENALTY
    } else {
        1.0
    };

    freq_term * recency * unique_sessions * unique_agents * sequence_bonus * catalog_penalty
}

/// `unique_sessions >= 3` or (`freq >= 8` and `unique_sessions >= 2`) -- cross-session reuse is
/// required; a noisy single session cannot mint a draft (`06` §2.6).
pub fn passes_cutoff(cluster: &Cluster) -> bool {
    let sessions = cluster.unique_sessions();
    sessions >= 3 || (cluster.freq() >= 8 && sessions >= 2)
}
