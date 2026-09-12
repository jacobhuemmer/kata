//! Ranking and the proposal cutoff (`docs/design/06-session-mining.md` §2.6).

use std::time::SystemTime;

use crate::cluster::Cluster;

#[cfg(test)]
mod tests;

/// `score = log(1 + freq) * recency * unique_sessions * unique_agents * sequence_bonus *
/// catalog_penalty` (`06` §2.6). `now` and `catalog_conflict` (an existing kata already covers
/// this cluster's argv0 + subcommand) are caller-supplied so the formula stays pure and
/// testable without a wall clock or a real kata library.
pub fn score(cluster: &Cluster, now: SystemTime, catalog_conflict: bool) -> f64 {
    todo!("{cluster:?} {now:?} {catalog_conflict}")
}

/// `unique_sessions >= 3` or (`freq >= 8` and `unique_sessions >= 2`) -- cross-session reuse is
/// required; a noisy single session cannot mint a draft (`06` §2.6).
pub fn passes_cutoff(cluster: &Cluster) -> bool {
    todo!("{cluster:?}")
}
