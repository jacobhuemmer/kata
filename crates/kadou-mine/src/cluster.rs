//! Fingerprinting and near-duplicate clustering (`docs/design/06-session-mining.md` §2.5).
//! A cluster stores only a template, step count, and member refs (agent/session/when) --
//! never a raw command (§4.1's never-written list).

#[cfg(test)]
mod tests;

/// One normalized shell invocation or sequence, ready to fingerprint and cluster. `steps` is
/// one template per step (`06` §2.3's "consecutive shell invocations ... become one candidate
/// with `steps[]`"); a single command is a one-element `steps`.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub steps: Vec<String>,
    pub agent: String,
    pub session_id: String,
    pub when: String,
}

/// One cluster member: agent/session/when only, never a command (`06` §2.5 "member refs
/// (agent/session/when only)").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub agent: String,
    pub session_id: String,
    pub when: String,
}

#[derive(Debug, Clone)]
pub struct Cluster {
    pub fingerprint: String,
    pub template: String,
    pub step_count: usize,
    pub members: Vec<Member>,
    pub first_seen: String,
    pub last_seen: String,
}

impl Cluster {
    pub fn freq(&self) -> usize {
        self.members.len()
    }

    pub fn unique_sessions(&self) -> usize {
        todo!()
    }

    pub fn unique_agents(&self) -> usize {
        todo!()
    }
}

pub fn fingerprint(steps: &[String]) -> String {
    todo!("{steps:?}")
}

pub fn cluster_candidates(candidates: &[Candidate]) -> Vec<Cluster> {
    todo!("{candidates:?}")
}
