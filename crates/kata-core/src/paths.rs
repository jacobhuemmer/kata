use std::path::{Path, PathBuf};

use etcetera::BaseStrategy as _;

/// The three roots kadou uses on disk (§3, §7.6): user config (and, under it, `kata/` and
/// `themes/`), product data (vault, keys, cloned folders, drafts), and state (history,
/// pending, last-used args).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KataPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl KataPaths {
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("kata.toml")
    }

    pub fn kata_dir(&self) -> PathBuf {
        self.config_dir.join("catalog")
    }

    pub fn themes_dir(&self) -> PathBuf {
        self.config_dir.join("themes")
    }
}

/// The result of resolving [`KataPaths`], plus a deprecation notice when the legacy
/// `DOPS_HOME` variable is the one that decided the root (§7.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub paths: KataPaths,
    pub dops_home_warning: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("could not locate a home directory")]
    NoHomeDir,
    #[error("no XDG state directory is available on this platform")]
    NoStateDir,
}

/// Resolves paths from the real process environment (`KATA_HOME`, `DOPS_HOME`) and the
/// real home directory.
///
/// This is the only function in this module that touches the process environment. Tests
/// call [`resolve`] directly with explicit overrides instead of mutating real env vars:
/// edition 2024 made `std::env::set_var` `unsafe` precisely because it is a global mutation
/// that races other tests running in parallel.
pub fn discover() -> Result<Resolved, PathsError> {
    let kata_home = std::env::var_os("KATA_HOME").map(PathBuf::from);
    let kadou_home = std::env::var_os("KADOU_HOME").map(PathBuf::from);
    let dops_home = std::env::var_os("DOPS_HOME").map(PathBuf::from);
    if kata_home.is_some() {
        return resolve(kata_home.as_deref(), dops_home.as_deref());
    }
    if let Some(home) = kadou_home {
        let mut resolved = resolve(Some(home.as_path()), dops_home.as_deref())?;
        resolved.dops_home_warning = Some(
            "KADOU_HOME is deprecated and honored for one release only; set KATA_HOME instead"
                .into(),
        );
        return Ok(resolved);
    }
    resolve(None, dops_home.as_deref())
}

/// Pure resolution: `kadou_home` wins over `dops_home`, which wins over the platform's
/// native base strategy.
///
/// The native strategy is always `etcetera::choose_base_strategy()` — the XDG strategy on
/// macOS and Linux — never `choose_native_strategy()`, whose Apple strategy maps macOS to
/// `~/Library/Application Support` and has no state directory at all (B10, §3.1).
pub fn resolve(
    kadou_home: Option<&Path>,
    dops_home: Option<&Path>,
) -> Result<Resolved, PathsError> {
    if let Some(home) = kadou_home {
        return Ok(Resolved {
            paths: paths_under(home),
            dops_home_warning: None,
        });
    }

    if let Some(home) = dops_home {
        let warning = format!(
            "DOPS_HOME is deprecated and honored for one release only; set KATA_HOME instead (using DOPS_HOME={})",
            home.display()
        );
        return Ok(Resolved {
            paths: paths_under(home),
            dops_home_warning: Some(warning),
        });
    }

    let strategy = etcetera::choose_base_strategy().map_err(|_| PathsError::NoHomeDir)?;
    let state_dir = strategy.state_dir().ok_or(PathsError::NoStateDir)?;
    Ok(Resolved {
        paths: KataPaths {
            config_dir: strategy.config_dir().join("kata"),
            data_dir: strategy.data_dir().join("kata"),
            state_dir: state_dir.join("kata"),
        },
        dops_home_warning: None,
    })
}

/// `home` treated as a substitute `$HOME`: the same `.config` / `.local/share` /
/// `.local/state` layout `etcetera`'s XDG strategy derives from a real home directory, minus
/// any ambient `XDG_*` override — a fully isolated root is the point of `KATA_HOME`
/// (§7.6 "for tests/containers").
fn paths_under(home: &Path) -> KataPaths {
    KataPaths {
        config_dir: home.join(".config/kata"),
        data_dir: home.join(".local/share/kata"),
        state_dir: home.join(".local/state/kata"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kadou_home_isolates_all_three_roots() {
        let a = resolve(Some(Path::new("/tmp/kadou-test-a")), None).unwrap();
        let b = resolve(Some(Path::new("/tmp/kadou-test-b")), None).unwrap();

        assert_eq!(
            a.paths.config_dir,
            PathBuf::from("/tmp/kadou-test-a/.config/kata")
        );
        assert_eq!(
            a.paths.data_dir,
            PathBuf::from("/tmp/kadou-test-a/.local/share/kata")
        );
        assert_eq!(
            a.paths.state_dir,
            PathBuf::from("/tmp/kadou-test-a/.local/state/kata")
        );
        assert_ne!(a.paths, b.paths);
        assert!(a.dops_home_warning.is_none());
    }

    #[test]
    fn dops_home_fallback_warns() {
        let resolved = resolve(None, Some(Path::new("/tmp/kadou-test-dops"))).unwrap();

        assert_eq!(
            resolved.paths.config_dir,
            PathBuf::from("/tmp/kadou-test-dops/.config/kata")
        );
        let warning = resolved
            .dops_home_warning
            .expect("DOPS_HOME fallback must warn");
        assert!(
            warning.contains("DOPS_HOME"),
            "warning should name DOPS_HOME: {warning}"
        );
        assert!(
            warning.to_lowercase().contains("deprecat"),
            "warning should say deprecated: {warning}"
        );
    }

    #[test]
    fn kadou_home_wins_over_dops_home_silently() {
        let resolved = resolve(
            Some(Path::new("/tmp/kadou-test-k")),
            Some(Path::new("/tmp/kadou-test-d")),
        )
        .unwrap();

        assert_eq!(
            resolved.paths.config_dir,
            PathBuf::from("/tmp/kadou-test-k/.config/kata")
        );
        assert!(resolved.dops_home_warning.is_none());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_default_resolves_under_config_and_local_never_application_support() {
        let resolved = resolve(None, None).unwrap();

        for dir in [
            &resolved.paths.config_dir,
            &resolved.paths.data_dir,
            &resolved.paths.state_dir,
        ] {
            let text = dir.to_string_lossy();
            assert!(
                !text.contains("Application Support"),
                "must not resolve under Application Support: {text}"
            );
        }

        // Only assert the exact suffix when no ambient XDG_* override is active in this
        // process, so the test stays deterministic under a customized CI/dev environment.
        if std::env::var_os("XDG_CONFIG_HOME").is_none() {
            assert!(resolved.paths.config_dir.ends_with(".config/kata"));
        }
        if std::env::var_os("XDG_DATA_HOME").is_none() {
            assert!(resolved.paths.data_dir.ends_with(".local/share/kata"));
        }
        if std::env::var_os("XDG_STATE_HOME").is_none() {
            assert!(resolved.paths.state_dir.ends_with(".local/state/kata"));
        }
    }

    #[test]
    fn kadou_paths_helpers_join_correctly() {
        let paths = KataPaths {
            config_dir: PathBuf::from("/x/.config/kata"),
            data_dir: PathBuf::from("/x/.local/share/kata"),
            state_dir: PathBuf::from("/x/.local/state/kata"),
        };
        assert_eq!(
            paths.config_file(),
            PathBuf::from("/x/.config/kata/kata.toml")
        );
        assert_eq!(paths.kata_dir(), PathBuf::from("/x/.config/kata/catalog"));
        assert_eq!(paths.themes_dir(), PathBuf::from("/x/.config/kata/themes"));
    }
}
