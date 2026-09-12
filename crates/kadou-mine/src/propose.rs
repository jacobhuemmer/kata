//! Draft writer: turns a ranked cluster into a single-file kata draft plus its `meta.json`
//! (`docs/design/05-prd.md` §6.8 "[shape] The miner writes a single-file draft with a header,
//! not `format_version: 2` YAML"; `docs/design/06-session-mining.md` §2.8).
//!
//! Every string that reaches [`Proposal`] has passed through [`crate::redact::redact_all`]
//! first; if the fail-closed high-entropy check still trips afterward, the whole candidate is
//! dropped (`06` §2.7, §4.1) rather than written anywhere.

use std::collections::BTreeMap;

use kadou_core::RiskLevel;

use crate::cluster::Cluster;

#[cfg(test)]
mod tests;

/// One normalized step plus the concrete values its placeholders stood for, as recorded when
/// the orchestrator first normalized this exact template (`06` §2.4/§2.8).
#[derive(Debug, Clone)]
pub struct StepWithParams {
    pub template: String,
    pub params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposeError {
    /// Redaction could not be proven safe -- fail closed, drop the candidate (`06` §4.1).
    RedactionUnproven,
}

#[derive(Debug, Clone)]
pub struct Proposal {
    pub fingerprint: String,
    pub slug: String,
    pub kata_source: String,
    pub meta_json: String,
}

/// Builds a redacted, single-file kata draft plus its `meta.json` from a ranked cluster
/// (`06` §2.7-2.8). `steps` carries the medoid's own normalized templates and the concrete
/// values recorded when the orchestrator first normalized them.
pub fn build_proposal(
    cluster: &Cluster,
    steps: &[StepWithParams],
    score: f64,
    risk: RiskLevel,
    redact_extra: &[String],
) -> Result<Proposal, ProposeError> {
    todo!("{cluster:?} {steps:?} {score} {risk:?} {redact_extra:?}")
}
