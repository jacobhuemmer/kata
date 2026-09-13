//! Fingerprinting and near-duplicate clustering (`docs/design/06-session-mining.md` §2.5).
//! A cluster stores only a template, step count, and member refs (agent/session/when) --
//! never a raw command (§4.1's never-written list).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use sha2::{Digest, Sha256};

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
        self.members
            .iter()
            .map(|m| m.session_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }

    pub fn unique_agents(&self) -> usize {
        self.members
            .iter()
            .map(|m| m.agent.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }
}

/// SHA-256 of the steps joined by newline (`06` §2.5: argv0 + flag names + normalized
/// subcommands, or the ordered step fingerprints for a sequence) -- the normalized template
/// already carries exactly that information, having stripped every literal value already.
pub fn fingerprint(steps: &[String]) -> String {
    let joined = steps.join("\n");
    let digest = Sha256::digest(joined.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest.as_slice() {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn token_set(template: &str) -> std::collections::BTreeSet<&str> {
    template.split_whitespace().collect()
}

/// `true` when two templates are near-duplicates: Jaccard similarity of their token sets is
/// at least 0.85, or one is a whitespace-token prefix of the other (`06` §2.5 "same argv0 +
/// flags, extra `| tail`").
fn near_duplicate(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let ta = token_set(a);
    let tb = token_set(b);
    let intersection = ta.intersection(&tb).count();
    let union = ta.union(&tb).count();
    if union > 0 && (intersection as f64 / union as f64) >= 0.85 {
        return true;
    }
    let wa: Vec<&str> = a.split_whitespace().collect();
    let wb: Vec<&str> = b.split_whitespace().collect();
    let (shorter, longer) = if wa.len() <= wb.len() {
        (&wa, &wb)
    } else {
        (&wb, &wa)
    };
    !shorter.is_empty() && longer.starts_with(shorter.as_slice())
}

/// Groups candidates into clusters: exact-template groups first, then merges near-duplicate
/// groups via a small union-find (`06` §2.5). Cluster id is the fingerprint of the medoid --
/// the most frequent exact template in the merged group -- so it stays stable across runs
/// that see the same dominant shape.
pub fn cluster_candidates(candidates: &[Candidate]) -> Vec<Cluster> {
    let mut groups: BTreeMap<String, Vec<&Candidate>> = BTreeMap::new();
    for candidate in candidates {
        let key = candidate.steps.join("\n");
        groups.entry(key).or_default().push(candidate);
    }

    let keys: Vec<String> = groups.keys().cloned().collect();
    let mut parent: Vec<usize> = (0..keys.len()).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }
    for i in 0..keys.len() {
        for j in (i + 1)..keys.len() {
            if near_duplicate(&keys[i], &keys[j]) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    let mut merged: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..keys.len() {
        let root = find(&mut parent, i);
        merged.entry(root).or_default().push(i);
    }

    merged
        .into_values()
        .map(|group_indices| build_cluster(&keys, &groups, &group_indices))
        .collect()
}

fn build_cluster(
    keys: &[String],
    groups: &BTreeMap<String, Vec<&Candidate>>,
    group_indices: &[usize],
) -> Cluster {
    let medoid_key = group_indices
        .iter()
        .max_by_key(|&&i| groups[&keys[i]].len())
        .map(|&i| keys[i].clone())
        .unwrap_or_default();
    let medoid_steps: Vec<String> = medoid_key.split('\n').map(str::to_string).collect();

    let mut members = Vec::new();
    let mut whens = Vec::new();
    for &i in group_indices {
        for candidate in &groups[&keys[i]] {
            members.push(Member {
                agent: candidate.agent.clone(),
                session_id: candidate.session_id.clone(),
                when: candidate.when.clone(),
            });
            whens.push(candidate.when.clone());
        }
    }
    whens.sort();
    let first_seen = whens.first().cloned().unwrap_or_default();
    let last_seen = whens.last().cloned().unwrap_or_default();

    Cluster {
        fingerprint: fingerprint(&medoid_steps),
        step_count: medoid_steps.len(),
        template: medoid_steps.join("\n"),
        members,
        first_seen,
        last_seen,
    }
}
