use std::path::PathBuf;
use std::time::Duration;

use crate::risk::RiskLevel;

/// A vault name a kata declares under `needs:` (`docs/design/05-prd.md` §4.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Need {
    pub name: String,
    /// A plain (non-secret) fallback given as `name=default` in the header. A need with a
    /// default is never secret by construction (§4.4) — only a bare name may resolve to a
    /// vault-held secret.
    pub default: Option<String>,
}

impl Need {
    /// A need is secret exactly when the header gave it no plain default.
    pub fn is_secret(&self) -> bool {
        self.default.is_none()
    }

    /// The environment variable name the resolved value is injected under.
    pub fn env_name(&self) -> String {
        self.name.to_ascii_uppercase()
    }
}

/// One of the four arg types a header may declare (§4.3). Deliberately four, not nine —
/// `float`, `file_path`, `resource_id`, `multi_select` are unused across every real catalog
/// on disk and are not header types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgType {
    Text,
    Int,
    Bool,
    Select { options: Vec<String> },
}

/// An arg's default value, typed per its `ArgType` (§4.3 "Env serialization, per type").
#[derive(Debug, Clone, PartialEq)]
pub enum ArgDefault {
    Text(String),
    Int(i64),
    Bool(bool),
    Select(String),
}

/// One `args:` line (§4.3): `<name>: <type>[ <options>][ = <default>][  # <help>]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    pub name: String,
    pub ty: ArgType,
    pub default: Option<ArgDefault>,
    pub help: Option<String>,
}

impl Arg {
    /// "no default → required. That is the whole rule." (§4.3)
    pub fn is_required(&self) -> bool {
        self.default.is_none()
    }

    /// The environment variable name the resolved value is injected under.
    pub fn env_name(&self) -> String {
        self.name.to_ascii_uppercase()
    }
}

/// A parsed kata: one script file plus its header (§4.3). The header parser that produces
/// this type lands in slice 2; this crate only carries the shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Kata {
    /// Path under `kata/`, no extension, `/`-separated: `sesami/cc4-aaa`, `starter/hello`,
    /// `./deploy` for a project-local kata (§4.2).
    pub id: String,
    /// Absolute path to the kata file: `<name>.sh` or `<name>/kata.sh`.
    pub path: PathBuf,
    /// The description everywhere: list, `kata show`, MCP. There is no separate
    /// display-name field (§4.2).
    pub about: String,
    pub risk: RiskLevel,
    pub needs: Vec<Need>,
    pub args: Vec<Arg>,
    /// Short names, unique across all folders (§4.2).
    pub alias: Vec<String>,
    /// Overrides `[exec] timeout` for this kata; never above 24h (§4.3).
    pub timeout: Option<Duration>,
    /// The first comment paragraph after the closing `# ---`, shown by `kata show` (§4.3).
    pub notes: Option<String>,
    /// Line 1 when it is a shebang (`#!...`); absent means the exec contract falls back to
    /// `/bin/sh` (§6.1). The runtime, not a display concern — carried on `Kata` because
    /// `kata-exec` execs by id, not by `ParsedHeader`.
    pub shebang: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_arg(name: &str, default: Option<&str>) -> Arg {
        Arg {
            name: name.to_string(),
            ty: ArgType::Text,
            default: default.map(|d| ArgDefault::Text(d.to_string())),
            help: None,
        }
    }

    #[test]
    fn arg_without_default_is_required() {
        assert!(text_arg("target_dir", None).is_required());
        assert!(!text_arg("target_dir", Some(".")).is_required());
    }

    #[test]
    fn env_names_are_uppercase() {
        assert_eq!(text_arg("target_dir", None).env_name(), "TARGET_DIR");
        let need = Need {
            name: "jenkins_token".to_string(),
            default: None,
        };
        assert_eq!(need.env_name(), "JENKINS_TOKEN");
        assert!(need.is_secret());
    }

    #[test]
    fn need_with_default_is_never_secret() {
        let need = Need {
            name: "jenkins_url".to_string(),
            default: Some("https://ci.example.com".to_string()),
        };
        assert!(!need.is_secret());
    }
}
