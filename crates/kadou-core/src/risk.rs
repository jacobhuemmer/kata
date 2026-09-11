use std::fmt;

use serde::{Deserialize, Serialize};

/// The four risk words a kata header may declare (`docs/design/05-prd.md` §4.3).
///
/// Declaration order is significant: derived `Ord`/`PartialOrd` give
/// `Low < Medium < High < Critical`, matching the ceiling comparisons in §2 decision 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    /// All four levels, lowest first.
    pub const ALL: [RiskLevel; 4] = [Self::Low, Self::Medium, Self::High, Self::Critical];

    /// The header/CLI spelling: `low`, `medium`, `high`, `critical`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Default for RiskLevel {
    /// The lowest level, matching the default agent ceiling (§2 decision 5).
    fn default() -> Self {
        Self::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_is_low_medium_high_critical() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);

        let mut levels = RiskLevel::ALL;
        levels.sort();
        assert_eq!(levels, RiskLevel::ALL);
    }

    #[test]
    fn serializes_lowercase() {
        #[derive(Serialize)]
        struct Wrapper {
            risk: RiskLevel,
        }

        let toml = toml::to_string(&Wrapper {
            risk: RiskLevel::High,
        })
        .unwrap();
        assert_eq!(toml.trim(), "risk = \"high\"");
        assert_eq!(RiskLevel::Critical.as_str(), "critical");
    }

    #[test]
    fn default_is_low() {
        assert_eq!(RiskLevel::default(), RiskLevel::Low);
    }
}
