use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::fsutil::write_atomic_0600;
use crate::risk::RiskLevel;

/// `humantime`-shaped `Duration` fields (`"30m"`, `"50s"`) that fail loudly at config load
/// instead of silently falling back to a default (R6, A3): `[exec] timeout` and `[mcp]
/// max_wait` are safety-relevant bounds, so a typo like `"30 minuts"` should be a `ConfigError`,
/// not a quietly-ignored 30-minute default.
mod duration_toml {
    use std::time::Duration;

    use serde::{Deserialize as _, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&humantime::format_duration(*value).to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        let raw = String::deserialize(deserializer)?;
        humantime::parse_duration(&raw).map_err(|err| {
            serde::de::Error::custom(format!("invalid duration `{raw}`: {err}"))
        })
    }
}

/// The literal first-run file content (`docs/design/05-prd.md` §7.6, §7.4 item 1). Writing
/// this rather than a fully-populated file keeps every key absent until a human sets it, so
/// a later change to a default is not frozen out by an old file.
pub const EMPTY_TEMPLATE: &str = "# Missing keys mean defaults. This file may be empty.\n";

/// `kadou.toml`: every key is optional. A missing file, an empty file, and a fully
/// populated file that repeats every default all parse to the same `Config`
/// (§7.6, §9 slice 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    /// Human ceiling for every folder, overridable per folder (§2 decision 5).
    pub max_risk: RiskLevel,
    pub agent: AgentConfig,
    /// `[folder.<name>]`: optional per-folder policy. Nothing here needs to be "registered" —
    /// a folder exists because it is on disk (§2 decision 5).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub folder: BTreeMap<String, FolderConfig>,
    pub trust: TrustConfig,
    pub exec: ExecConfig,
    pub mcp: McpConfig,
    pub vault: VaultConfig,
    pub notify: NotifyConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "doop".to_string(),
            max_risk: RiskLevel::Medium,
            agent: AgentConfig::default(),
            folder: BTreeMap::new(),
            trust: TrustConfig::default(),
            exec: ExecConfig::default(),
            mcp: McpConfig::default(),
            vault: VaultConfig::default(),
            notify: NotifyConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub max_risk: RiskLevel,
    /// Ids the agent may run above `max_risk`, e.g. `"sesami/ses-deploy"`.
    pub allow: Vec<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_risk: RiskLevel::Low,
            allow: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderConfig {
    pub max_risk: Option<RiskLevel>,
    pub agent_max_risk: Option<RiskLevel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrustConfig {
    /// Project-local kata folders that may use the vault and be seen by agents (§4.5).
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecConfig {
    /// Parsed from a `humantime`-shaped TOML string (`"30m"`) at load time (R6, A3): an
    /// unparseable value is a load-time `ConfigError`, not a silently-ignored default.
    #[serde(with = "duration_toml")]
    pub timeout: Duration,
    pub pass_env: Vec<String>,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30 * 60),
            pass_env: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    pub max_output_lines: u32,
    /// Parsed from a `humantime`-shaped TOML string (`"50s"`) at load time (R6, A3).
    #[serde(with = "duration_toml")]
    pub max_wait: Duration,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            max_output_lines: 50,
            max_wait: Duration::from_secs(50),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultConfig {
    pub keyring: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyConfig {
    pub enabled: bool,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    TomlEdit {
        path: PathBuf,
        #[source]
        source: toml_edit::TomlError,
    },
}

impl Config {
    /// Loads `path`. A missing file is not an error: it parses the same as an empty
    /// document, i.e. every field takes its default (§7.6 "missing keys mean defaults").
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source: Box::new(source),
        })
    }

    /// Loads `path`, first writing [`EMPTY_TEMPLATE`] at mode `0600` if nothing exists there
    /// yet (§7.4 "Missing config → write defaults"). Returns the loaded config either way.
    pub fn load_or_init(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            write_atomic_0600(path, EMPTY_TEMPLATE.as_bytes()).map_err(|source| {
                ConfigError::Write {
                    path: path.to_path_buf(),
                    source,
                }
            })?;
        }
        Self::load(path)
    }

    /// Serializes and atomically writes `self` to `path` (write-to-temp, fsync, rename —
    /// `tempfile`, §3.1), creating parent directories as needed and setting mode `0600`
    /// (§7.6 "Config mode 0600").
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = toml::to_string_pretty(self)?;
        write_atomic_0600(path, text.as_bytes()).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Applies `edit` to the TOML document at `path` (an empty document if nothing exists
    /// there yet) and writes the result back atomically at 0600 -- comments and every key
    /// `edit` doesn't touch survive, and an untouched default is never materialized (§3.1:
    /// "Comment-preserving `kadou trust` / `kadou grant allow` config edits"). Unlike
    /// [`Config::save`], this never re-serializes the whole `Config`.
    pub fn edit(
        path: &Path,
        edit: impl FnOnce(&mut toml_edit::DocumentMut),
    ) -> Result<(), ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(source) if source.kind() == io::ErrorKind::NotFound => String::new(),
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let mut doc: toml_edit::DocumentMut =
            text.parse().map_err(|source| ConfigError::TomlEdit {
                path: path.to_path_buf(),
                source,
            })?;
        edit(&mut doc);
        write_atomic_0600(path, doc.to_string().as_bytes()).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_toml_is_all_defaults() {
        assert_eq!(toml::from_str::<Config>("").unwrap(), Config::default());
    }

    #[test]
    fn full_example_matches_documented_defaults_and_overrides() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/config/kadou.full-example.toml"
        ))
        .unwrap();
        let config: Config = toml::from_str(&text).unwrap();

        assert_eq!(config.theme, "doop");
        assert_eq!(config.max_risk, RiskLevel::Medium);
        assert_eq!(config.agent.max_risk, RiskLevel::Low);
        assert!(config.agent.allow.is_empty());
        assert_eq!(
            config.folder.get("sesami").unwrap(),
            &FolderConfig {
                max_risk: Some(RiskLevel::Critical),
                agent_max_risk: None
            }
        );
        assert!(config.trust.paths.is_empty());
        assert_eq!(config.exec.timeout, Duration::from_secs(30 * 60));
        assert!(config.exec.pass_env.is_empty());
        assert_eq!(config.mcp.max_output_lines, 50);
        assert_eq!(config.mcp.max_wait, Duration::from_secs(50));
        assert!(!config.vault.keyring);
        assert!(config.notify.enabled);
    }

    #[test]
    fn missing_keys_in_a_present_table_still_default() {
        let config: Config = toml::from_str("[agent]\nallow = [\"sesami/ses-deploy\"]\n").unwrap();
        assert_eq!(config.agent.max_risk, RiskLevel::Low);
        assert_eq!(config.agent.allow, vec!["sesami/ses-deploy".to_string()]);
    }

    #[test]
    fn an_unparseable_exec_timeout_is_a_load_error_not_a_silent_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kadou.toml");
        std::fs::write(&path, "[exec]\ntimeout = \"30 minuts\"\n").unwrap();
        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn an_unparseable_mcp_max_wait_is_a_load_error_not_a_silent_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kadou.toml");
        std::fs::write(&path, "[mcp]\nmax_wait = \"nope\"\n").unwrap();
        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn load_missing_file_returns_defaults_without_creating_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kadou.toml");
        assert_eq!(Config::load(&path).unwrap(), Config::default());
        assert!(!path.exists());
    }

    #[test]
    fn load_or_init_writes_empty_template_at_0600() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kadou.toml");

        let config = Config::load_or_init(&path).unwrap();
        assert_eq!(config, Config::default());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), EMPTY_TEMPLATE);
        assert_mode_0600(&path);

        // Second call must not clobber a since-edited file.
        std::fs::write(&path, "theme = \"custom\"\n").unwrap();
        let config = Config::load_or_init(&path).unwrap();
        assert_eq!(config.theme, "custom");
    }

    #[test]
    fn save_round_trips_and_sets_0600() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("kadou.toml");

        let config = Config {
            theme: "custom".to_string(),
            ..Config::default()
        };
        config.save(&path).unwrap();

        assert_mode_0600(&path);
        assert_eq!(Config::load(&path).unwrap(), config);
    }

    #[cfg(unix)]
    fn assert_mode_0600(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");
    }

    #[cfg(not(unix))]
    fn assert_mode_0600(_path: &Path) {}
}
