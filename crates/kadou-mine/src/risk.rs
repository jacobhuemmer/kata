//! Risk defaulting before a human ever sees the draft (`docs/design/06-session-mining.md`
//! §4.4). The miner never assigns `low` or `critical` -- a mined script is unreviewed, and
//! `critical` is a human's own choice.

use kadou_core::RiskLevel;

#[cfg(test)]
mod tests;

/// Defaults a mined cluster's risk from its template's verb shape (`06` §4.4). Never returns
/// `Low` or `Critical`. An unparseable or mixed-signal template defaults to `High`, the same
/// as the doc's own catch-all row.
pub fn default_risk(template: &str) -> RiskLevel {
    todo!("{template}")
}
