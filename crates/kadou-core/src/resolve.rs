//! Needs/args resolution for a run (`docs/design/05-prd.md` §4.4, §9 slice 3).
//!
//! The vault lands in slice 4: needs resolve from header defaults only here. A need with no
//! default is `missing` — not prompted, not silently skipped — and the caller (`kadou run`)
//! turns a missing need into the fix line `kadou vault set <name>` (§4.4 "Missing needs").
//! Args always resolve here: a required arg with no value is a hard error, same as `describe_kata`
//! would report `invalid_args`.

use std::collections::BTreeMap;

use crate::kata::{Arg, ArgDefault, ArgType, Kata, Need};

/// One resolved `args:` entry: the env var kadou-exec will set on the child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedVar {
    pub name: String,
    pub env_name: String,
    pub value: String,
}

/// One resolved `needs:` entry. `value` is `None` when the header gave no default and there
/// is (yet) no vault to ask — that is `missing`, not an empty string (§4.4).
///
/// `secret` mirrors [`Need::is_secret`]: a need with no header default is secret-shaped until
/// a vault entry says otherwise (slice 4). This is the same rule `describe_kata`'s
/// `secret_env_names` will use once the vault exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNeed {
    pub name: String,
    pub env_name: String,
    pub value: Option<String>,
    pub secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("unknown arg \"{0}\"; kata do not take a shell string")]
    UnknownArg(String),
    #[error("required arg `{0}` has no value; pass {0}=<value>")]
    MissingArg(String),
    #[error("`{0}` is a need, supplied by the vault; needs can never be passed as args")]
    ArgNamesNeed(String),
    #[error("invalid value for arg `{name}`: {message}")]
    InvalidArg { name: String, message: String },
}

/// Resolves every `args:` entry: `provided` (CLI `k=v` or MCP `run_kata.args`) wins, else the
/// header default, else `MissingArg` (§4.3 "no default → required. That is the whole rule.").
/// An entry in `provided` naming a need, or naming nothing on the kata at all, is rejected —
/// the args/needs split is structural, checked here rather than left to the shell (§4.4).
pub fn resolve_args(
    kata: &Kata,
    provided: &BTreeMap<String, String>,
) -> Result<Vec<ResolvedVar>, ResolveError> {
    for key in provided.keys() {
        if kata.needs.iter().any(|n| &n.name == key) {
            return Err(ResolveError::ArgNamesNeed(key.clone()));
        }
        if !kata.args.iter().any(|a| &a.name == key) {
            return Err(ResolveError::UnknownArg(key.clone()));
        }
    }

    kata.args
        .iter()
        .map(|arg| resolve_one_arg(arg, provided.get(&arg.name)))
        .collect()
}

fn resolve_one_arg(arg: &Arg, raw: Option<&String>) -> Result<ResolvedVar, ResolveError> {
    let value = match (raw, &arg.default) {
        (Some(raw), _) => coerce(arg, raw)?,
        (None, Some(default)) => serialize_default(default),
        (None, None) => return Err(ResolveError::MissingArg(arg.name.clone())),
    };
    Ok(ResolvedVar {
        name: arg.name.clone(),
        env_name: arg.env_name(),
        value,
    })
}

/// Env serialization, per type (§4.3, §6.1): `text` as-is, `int` decimal, `bool`
/// `true`/`false`, `select` the chosen option.
fn serialize_default(default: &ArgDefault) -> String {
    match default {
        ArgDefault::Text(s) => s.clone(),
        ArgDefault::Int(i) => i.to_string(),
        ArgDefault::Bool(b) => b.to_string(),
        ArgDefault::Select(s) => s.clone(),
    }
}

fn coerce(arg: &Arg, raw: &str) -> Result<String, ResolveError> {
    let invalid = |message: String| ResolveError::InvalidArg {
        name: arg.name.clone(),
        message,
    };
    match &arg.ty {
        ArgType::Text => Ok(raw.to_string()),
        ArgType::Int => raw
            .parse::<i64>()
            .map(|i| i.to_string())
            .map_err(|_| invalid(format!("`{raw}` is not an integer"))),
        ArgType::Bool => match raw {
            "true" => Ok("true".to_string()),
            "false" => Ok("false".to_string()),
            _ => Err(invalid(format!("`{raw}` is not true or false"))),
        },
        ArgType::Select { options } => {
            if options.iter().any(|o| o == raw) {
                Ok(raw.to_string())
            } else {
                Err(invalid(format!(
                    "`{raw}` is not one of the declared options: {}",
                    options.join(", ")
                )))
            }
        }
    }
}

/// Resolves every `needs:` entry from the header alone (no vault in this slice, §4.4).
pub fn resolve_needs(kata: &Kata) -> Vec<ResolvedNeed> {
    kata.needs.iter().map(resolve_one_need).collect()
}

fn resolve_one_need(need: &Need) -> ResolvedNeed {
    ResolvedNeed {
        name: need.name.clone(),
        env_name: need.env_name(),
        value: need.default.clone(),
        secret: need.is_secret(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::risk::RiskLevel;
    use std::path::PathBuf;

    fn kata(args: Vec<Arg>, needs: Vec<Need>) -> Kata {
        Kata {
            id: "t/kata".to_string(),
            path: PathBuf::from("/tmp/t/kata.sh"),
            about: "Test".to_string(),
            risk: RiskLevel::Low,
            needs,
            args,
            alias: Vec::new(),
            timeout: None,
            notes: None,
            shebang: Some("#!/bin/sh".to_string()),
        }
    }

    fn text_arg(name: &str, default: Option<&str>) -> Arg {
        Arg {
            name: name.to_string(),
            ty: ArgType::Text,
            default: default.map(|d| ArgDefault::Text(d.to_string())),
            help: None,
        }
    }

    #[test]
    fn default_is_used_when_not_provided() {
        let k = kata(vec![text_arg("name", Some("world"))], vec![]);
        let resolved = resolve_args(&k, &BTreeMap::new()).unwrap();
        assert_eq!(resolved[0].env_name, "NAME");
        assert_eq!(resolved[0].value, "world");
    }

    #[test]
    fn provided_value_overrides_default() {
        let k = kata(vec![text_arg("name", Some("world"))], vec![]);
        let mut provided = BTreeMap::new();
        provided.insert("name".to_string(), "mason".to_string());
        let resolved = resolve_args(&k, &provided).unwrap();
        assert_eq!(resolved[0].value, "mason");
    }

    #[test]
    fn missing_required_arg_is_an_error() {
        let k = kata(vec![text_arg("name", None)], vec![]);
        let err = resolve_args(&k, &BTreeMap::new()).unwrap_err();
        assert_eq!(err, ResolveError::MissingArg("name".to_string()));
    }

    #[test]
    fn unknown_arg_is_rejected() {
        let k = kata(vec![], vec![]);
        let mut provided = BTreeMap::new();
        provided.insert("cmd".to_string(), "rm -rf /".to_string());
        let err = resolve_args(&k, &provided).unwrap_err();
        assert_eq!(err, ResolveError::UnknownArg("cmd".to_string()));
    }

    #[test]
    fn arg_naming_a_need_is_rejected() {
        let k = kata(
            vec![],
            vec![Need {
                name: "jenkins_token".to_string(),
                default: None,
            }],
        );
        let mut provided = BTreeMap::new();
        provided.insert("jenkins_token".to_string(), "hunter2".to_string());
        let err = resolve_args(&k, &provided).unwrap_err();
        assert_eq!(err, ResolveError::ArgNamesNeed("jenkins_token".to_string()));
    }

    #[test]
    fn select_outside_options_is_rejected() {
        let arg = Arg {
            name: "mode".to_string(),
            ty: ArgType::Select {
                options: vec!["a".to_string(), "b".to_string()],
            },
            default: Some(ArgDefault::Select("a".to_string())),
            help: None,
        };
        let k = kata(vec![arg], vec![]);
        let mut provided = BTreeMap::new();
        provided.insert("mode".to_string(), "zzz".to_string());
        let err = resolve_args(&k, &provided).unwrap_err();
        assert!(matches!(err, ResolveError::InvalidArg { name, .. } if name == "mode"));
    }

    #[test]
    fn int_and_bool_coerce_or_reject() {
        let args = vec![
            Arg {
                name: "retries".to_string(),
                ty: ArgType::Int,
                default: None,
                help: None,
            },
            Arg {
                name: "send_email".to_string(),
                ty: ArgType::Bool,
                default: None,
                help: None,
            },
        ];
        let k = kata(args, vec![]);
        let mut provided = BTreeMap::new();
        provided.insert("retries".to_string(), "3".to_string());
        provided.insert("send_email".to_string(), "true".to_string());
        let resolved = resolve_args(&k, &provided).unwrap();
        assert_eq!(resolved[0].value, "3");
        assert_eq!(resolved[1].value, "true");

        let mut bad = BTreeMap::new();
        bad.insert("retries".to_string(), "abc".to_string());
        bad.insert("send_email".to_string(), "true".to_string());
        assert!(resolve_args(&k, &bad).is_err());
    }

    #[test]
    fn need_with_default_resolves_and_is_not_secret() {
        let k = kata(
            vec![],
            vec![Need {
                name: "jenkins_url".to_string(),
                default: Some("https://ci.example.com".to_string()),
            }],
        );
        let resolved = resolve_needs(&k);
        assert_eq!(resolved[0].env_name, "JENKINS_URL");
        assert_eq!(resolved[0].value.as_deref(), Some("https://ci.example.com"));
        assert!(!resolved[0].secret);
    }

    #[test]
    fn need_without_default_is_missing_and_secret_shaped() {
        let k = kata(
            vec![],
            vec![Need {
                name: "jenkins_token".to_string(),
                default: None,
            }],
        );
        let resolved = resolve_needs(&k);
        assert_eq!(resolved[0].value, None);
        assert!(resolved[0].secret);
    }
}
