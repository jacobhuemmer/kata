//! `kadou import`: converts an old dops catalog (`<dir>/<name>/{runbook.yaml,script.sh}`)
//! into a kadou folder, once (`docs/design/05-prd.md` §4.6). `serde-yaml-ng` is used only
//! on this path — the product's own loader never parses YAML again (§3).

use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::header::render_header;
use crate::kata::{Arg, ArgDefault, ArgType, Need};
use crate::risk::RiskLevel;

const GLOBAL_SCOPE: &str = "global";

#[derive(Debug, Deserialize)]
struct DopsRunbook {
    #[serde(default)]
    description: String,
    risk_level: String,
    #[serde(default = "default_script")]
    script: String,
    #[serde(default)]
    parameters: Vec<DopsParam>,
}

fn default_script() -> String {
    "script.sh".to_string()
}

#[derive(Debug, Deserialize)]
struct DopsParam {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    description: String,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    options: Vec<String>,
    #[serde(default)]
    secret: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("source catalog {0} does not exist or is not a directory")]
    SourceNotFound(PathBuf),
    #[error("{0} already exists; kadou import refuses to overwrite an existing folder")]
    TargetExists(PathBuf),
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: {source}")]
    Yaml {
        path: PathBuf,
        #[source]
        source: Box<serde_yaml_ng::Error>,
    },
    #[error("{kata}: unknown risk_level `{risk_level}`")]
    UnknownRisk { kata: String, risk_level: String },
    #[error("{kata}: parameter `{param}`: {message}")]
    Param {
        kata: String,
        param: String,
        message: String,
    },
}

/// One converted kata, before it is written to disk — kept so `kadou import` can print a
/// per-file diff (§4.6 "prints per-file diffs").
pub struct ConvertedKata {
    /// Path the file is written to, relative to the new folder's own directory.
    pub relative_path: PathBuf,
    pub content: String,
    /// Extra files copied verbatim alongside a multi-file kata (source path -> bytes).
    pub extra_files: Vec<(PathBuf, Vec<u8>)>,
}

#[derive(Debug, Default)]
pub struct ImportSummary {
    pub kata_written: usize,
    pub booleans_coerced: usize,
    pub integers_coerced: usize,
    /// Unified diffs, one per converted kata file, in the order written.
    pub diffs: Vec<String>,
}

/// Converts every `<src_dir>/<name>/runbook.yaml` into `<kata_dir>/<folder>/...` (§4.6).
/// Refuses to run at all if `<kata_dir>/<folder>` already exists.
pub fn import_catalog(
    src_dir: &Path,
    kata_dir: &Path,
    folder: &str,
) -> Result<ImportSummary, ImportError> {
    if !src_dir.is_dir() {
        return Err(ImportError::SourceNotFound(src_dir.to_path_buf()));
    }
    let target_dir = kata_dir.join(folder);
    if target_dir.exists() {
        return Err(ImportError::TargetExists(target_dir));
    }

    let mut kata_names: Vec<String> = Vec::new();
    for entry in read_dir_sorted(src_dir)? {
        if !entry.path().is_dir() {
            continue;
        }
        if entry.path().join("runbook.yaml").is_file() {
            kata_names.push(entry.file_name().to_string_lossy().to_string());
        }
    }

    let mut summary = ImportSummary::default();
    let mut converted: Vec<(String, ConvertedKata)> = Vec::new();

    for name in &kata_names {
        let kata_src = src_dir.join(name);
        let converted_kata = convert_one(&kata_src, name, &mut summary)?;
        converted.push((name.clone(), converted_kata));
    }

    // Only write once every kata has converted cleanly, so a bad file in the batch never
    // leaves a half-written folder.
    fs_err_create_dir_all(&target_dir)?;
    for (_, kata) in &converted {
        let dest = target_dir.join(&kata.relative_path);
        if let Some(parent) = dest.parent() {
            fs_err_create_dir_all(parent)?;
        }
        write_file(&dest, kata.content.as_bytes())?;
        summary
            .diffs
            .push(unified_diff(&kata.relative_path, &kata.content));
        for (rel, bytes) in &kata.extra_files {
            let extra_dest = target_dir.join(rel);
            if let Some(parent) = extra_dest.parent() {
                fs_err_create_dir_all(parent)?;
            }
            write_file(&extra_dest, bytes)?;
        }
        summary.kata_written += 1;
    }

    // The shared helper scripts the 29 wrappers reach via $KADOU_ROOT (§4.6) live one
    // level up from `<dops-catalog-dir>/src`, not inside it.
    if let Some(catalog_root) = src_dir.parent() {
        let shared_scripts = catalog_root.join("scripts");
        if shared_scripts.is_dir() {
            copy_dir_recursive(&shared_scripts, &target_dir.join("scripts"))?;
        }
    }

    Ok(summary)
}

fn convert_one(
    kata_src: &Path,
    name: &str,
    summary: &mut ImportSummary,
) -> Result<ConvertedKata, ImportError> {
    let yaml_path = kata_src.join("runbook.yaml");
    let yaml_text = read_to_string(&yaml_path)?;
    let runbook: DopsRunbook =
        serde_yaml_ng::from_str(&yaml_text).map_err(|source| ImportError::Yaml {
            path: yaml_path.clone(),
            source: Box::new(source),
        })?;

    let risk = match runbook.risk_level.as_str() {
        "low" => RiskLevel::Low,
        "medium" => RiskLevel::Medium,
        "high" => RiskLevel::High,
        "critical" => RiskLevel::Critical,
        other => {
            return Err(ImportError::UnknownRisk {
                kata: name.to_string(),
                risk_level: other.to_string(),
            });
        }
    };

    let about = runbook.description.trim().to_string();

    let mut needs: Vec<Need> = Vec::new();
    let mut args: Vec<Arg> = Vec::new();

    for param in &runbook.parameters {
        if param.scope == GLOBAL_SCOPE {
            let default = param.default.as_ref().filter(|d| !d.is_empty()).cloned();
            needs.push(Need {
                name: param.name.clone(),
                default,
            });
            continue;
        }

        if param.secret {
            return Err(ImportError::Param {
                kata: name.to_string(),
                param: param.name.clone(),
                message: "secret: true on a non-global parameter would become a plain, \
                    agent-settable arg; re-declare it with scope: global so it imports as a \
                    need instead"
                    .to_string(),
            });
        }

        let arg = convert_arg(param, name, summary)?;
        args.push(arg);
    }

    let script_path = kata_src.join(&runbook.script);
    let script_body = read_to_string(&script_path)?;
    let (shebang, body) = split_shebang(&script_body);
    let body = rewrite_repo_root_idiom(body);

    let rendered = render_header(shebang, &about, risk, &needs, &args, &[], None, &body);

    // Multi-file kata: anything in the source directory besides runbook.yaml and the
    // script itself travels along, opaque (§4.6 device-log-metrics).
    let mut extra_files = Vec::new();
    let mut has_extra = false;
    for entry in read_dir_sorted(kata_src)? {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name == "runbook.yaml" || file_name == runbook.script {
            continue;
        }
        has_extra = true;
        collect_extra(
            &entry.path(),
            &Path::new(name).join(&file_name),
            &mut extra_files,
        )?;
    }

    let relative_path = if has_extra {
        PathBuf::from(name).join("kata.sh")
    } else {
        PathBuf::from(format!("{name}.sh"))
    };

    Ok(ConvertedKata {
        relative_path,
        content: rendered,
        extra_files,
    })
}

fn convert_arg(
    param: &DopsParam,
    kata: &str,
    summary: &mut ImportSummary,
) -> Result<Arg, ImportError> {
    let param_err = |message: String| ImportError::Param {
        kata: kata.to_string(),
        param: param.name.clone(),
        message,
    };

    let help = if param.description.trim().is_empty() {
        None
    } else {
        Some(param.description.trim().to_string())
    };

    // "required: true, default: """ becomes a required arg with no default — that is what
    // "required with empty default" meant (§4.6). Any other combination keeps the default.
    let raw_default = param.default.as_deref();
    let drop_default = param.required && raw_default.is_some_and(str::is_empty);
    let effective_default = if drop_default { None } else { raw_default };

    match param.ty.as_str() {
        "string" => {
            let default = effective_default.map(|d| ArgDefault::Text(d.to_string()));
            Ok(Arg {
                name: param.name.clone(),
                ty: ArgType::Text,
                default,
                help,
            })
        }
        "boolean" => {
            let default = match effective_default {
                None => None,
                Some("true") => Some(ArgDefault::Bool(true)),
                Some("false") => Some(ArgDefault::Bool(false)),
                Some(other) => {
                    return Err(param_err(format!(
                        "cannot coerce boolean default `{other}` to true/false"
                    )));
                }
            };
            if default.is_some() {
                summary.booleans_coerced += 1;
            }
            Ok(Arg {
                name: param.name.clone(),
                ty: ArgType::Bool,
                default,
                help,
            })
        }
        "number" => {
            let default = match effective_default {
                None => None,
                Some(raw) => Some(ArgDefault::Int(raw.parse::<i64>().map_err(|_| {
                    param_err(format!(
                        "cannot coerce number default `{raw}` to an integer"
                    ))
                })?)),
            };
            if default.is_some() {
                summary.integers_coerced += 1;
            }
            Ok(Arg {
                name: param.name.clone(),
                ty: ArgType::Int,
                default,
                help,
            })
        }
        "select" => {
            if param.options.is_empty() {
                return Err(param_err("select parameter has no options".to_string()));
            }
            let default = match effective_default {
                None => None,
                Some(raw) => {
                    if !param.options.iter().any(|o| o == raw) {
                        return Err(param_err(format!(
                            "default `{raw}` is not one of the declared options"
                        )));
                    }
                    Some(ArgDefault::Select(raw.to_string()))
                }
            };
            Ok(Arg {
                name: param.name.clone(),
                ty: ArgType::Select {
                    options: param.options.clone(),
                },
                default,
                help,
            })
        }
        other => Err(param_err(format!(
            "unsupported dops parameter type `{other}`"
        ))),
    }
}

/// Splits off a leading shebang line, if any, returning `(shebang, rest)`.
fn split_shebang(text: &str) -> (Option<&str>, &str) {
    if let Some(first_line_end) = text.find('\n') {
        let first_line = &text[..first_line_end];
        if first_line.starts_with("#!") {
            return (Some(first_line), &text[first_line_end + 1..]);
        }
    }
    (None, text)
}

const REPO_ROOT_LINE: &str = "REPO_ROOT=\"$(cd \"$(dirname \"$0\")/../..\" && pwd)\"";
const TRIGGER_LINE_OLD: &str = "TRIGGER=\"${REPO_ROOT}/scripts/trigger-pipeline.sh\"";
const TRIGGER_LINE_NEW: &str = "TRIGGER=\"${KADOU_ROOT}/scripts/trigger-pipeline.sh\"";

/// Rewrites the verified-identical `REPO_ROOT`/`TRIGGER` idiom (§4.6). A single-file kata
/// sits one level shallower than `src/<name>/script.sh`, so the two-level `../..` climb
/// breaks; `$KADOU_ROOT` is the folder root regardless of nesting.
fn rewrite_repo_root_idiom(body: &str) -> String {
    body.lines()
        .filter(|line| line.trim_end() != REPO_ROOT_LINE)
        .map(|line| {
            if line.trim_end() == TRIGGER_LINE_OLD {
                TRIGGER_LINE_NEW
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if body.ends_with('\n') { "\n" } else { "" }
}

fn collect_extra(
    src: &Path,
    rel: &Path,
    out: &mut Vec<(PathBuf, Vec<u8>)>,
) -> Result<(), ImportError> {
    if src.is_dir() {
        for entry in read_dir_sorted(src)? {
            let child_rel = rel.join(entry.file_name());
            collect_extra(&entry.path(), &child_rel, out)?;
        }
    } else {
        let bytes = std::fs::read(src).map_err(|source| ImportError::Read {
            path: src.to_path_buf(),
            source,
        })?;
        out.push((rel.to_path_buf(), bytes));
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), ImportError> {
    fs_err_create_dir_all(dest)?;
    for entry in read_dir_sorted(src)? {
        let dest_path = dest.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            let bytes = std::fs::read(entry.path()).map_err(|source| ImportError::Read {
                path: entry.path(),
                source,
            })?;
            write_file(&dest_path, &bytes)?;
        }
    }
    Ok(())
}

/// A minimal unified diff against `/dev/null` — every converted kata is a new file
/// (`kadou import` refuses to overwrite an existing folder), so the diff is always an
/// addition (§4.6 "prints per-file diffs").
fn unified_diff(relative_path: &Path, content: &str) -> String {
    let path_str = relative_path.display();
    let mut out = format!("--- /dev/null\n+++ {path_str}\n");
    for line in content.lines() {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn read_dir_sorted(dir: &Path) -> Result<Vec<std::fs::DirEntry>, ImportError> {
    let map_err = |source: io::Error| ImportError::Read {
        path: dir.to_path_buf(),
        source,
    };
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)
        .map_err(map_err)?
        .collect::<Result<_, io::Error>>()
        .map_err(map_err)?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    Ok(entries)
}

fn read_to_string(path: &Path) -> Result<String, ImportError> {
    std::fs::read_to_string(path).map_err(|source| ImportError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), ImportError> {
    std::fs::write(path, bytes).map_err(|source| ImportError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn fs_err_create_dir_all(dir: &Path) -> Result<(), ImportError> {
    std::fs::create_dir_all(dir).map_err(|source| ImportError::Write {
        path: dir.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn converts_a_simple_wrapper_with_globals_and_bool_coercion() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "src/widget/runbook.yaml",
            r#"
name: widget
version: 1.0.0
description: Trigger the widget pipeline
risk_level: medium
script: script.sh
parameters:
  - name: jenkins_url
    type: string
    required: true
    scope: global
    default: "https://ci.example.com"
    secret: false
  - name: jenkins_user
    type: string
    required: true
    scope: global
    secret: false
  - name: jenkins_token
    type: string
    required: true
    scope: global
    secret: true
  - name: branch
    type: string
    required: true
    default: "dev"
    scope: runbook
    secret: false
  - name: send_email
    type: boolean
    required: false
    default: "true"
    scope: runbook
    secret: false
"#,
        );
        write(
            dir.path(),
            "src/widget/script.sh",
            "#!/bin/sh\nset -eu\nREPO_ROOT=\"$(cd \"$(dirname \"$0\")/../..\" && pwd)\"\nTRIGGER=\"${REPO_ROOT}/scripts/trigger-pipeline.sh\"\necho hi\n",
        );

        let kata_dir = dir.path().join("kata");
        let summary = import_catalog(&dir.path().join("src"), &kata_dir, "sesami").unwrap();
        assert_eq!(summary.kata_written, 1);
        assert_eq!(summary.booleans_coerced, 1);
        assert_eq!(summary.integers_coerced, 0);

        let content = std::fs::read_to_string(kata_dir.join("sesami/widget.sh")).unwrap();
        assert!(
            content
                .contains("needs: jenkins_url=https://ci.example.com jenkins_user jenkins_token")
        );
        assert!(content.contains("branch: text = dev"));
        assert!(content.contains("send_email: bool = true"));
        assert!(content.contains("KADOU_ROOT"));
        assert!(!content.contains("REPO_ROOT"));

        let (header, diags) = crate::header::parse_header(&content);
        assert!(
            !diags.iter().any(crate::header::Diagnostic::is_error),
            "{diags:?}"
        );
        assert!(header.is_some());
    }

    #[test]
    fn required_true_with_empty_default_becomes_required_no_default() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "src/sync/runbook.yaml",
            r#"
name: sync
description: Sync a thing
risk_level: medium
script: script.sh
parameters:
  - name: app_name
    type: string
    required: true
    default: ""
    scope: local
"#,
        );
        write(dir.path(), "src/sync/script.sh", "#!/bin/sh\necho hi\n");

        let kata_dir = dir.path().join("kata");
        import_catalog(&dir.path().join("src"), &kata_dir, "sesami").unwrap();
        let content = std::fs::read_to_string(kata_dir.join("sesami/sync.sh")).unwrap();
        let (header, _) = crate::header::parse_header(&content);
        let header = header.unwrap();
        assert_eq!(header.args.len(), 1);
        assert!(header.args[0].is_required());
    }

    #[test]
    fn a_runbook_scoped_secret_param_is_an_import_error_not_an_arg() {
        // Decision 13 / §4.4: a secret must never be agent-settable. scope decides need vs.
        // arg; a non-global param that also carries secret: true would otherwise convert
        // silently into a plain, agent-settable arg (I-15).
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "src/widget/runbook.yaml",
            r#"
name: widget
description: Trigger the widget pipeline
risk_level: medium
script: script.sh
parameters:
  - name: api_token
    type: string
    required: true
    scope: runbook
    secret: true
"#,
        );
        write(dir.path(), "src/widget/script.sh", "#!/bin/sh\necho hi\n");

        let kata_dir = dir.path().join("kata");
        let err = import_catalog(&dir.path().join("src"), &kata_dir, "sesami").unwrap_err();
        let ImportError::Param {
            kata,
            param,
            message,
        } = err
        else {
            panic!("expected ImportError::Param, got {err:?}");
        };
        assert_eq!(kata, "widget");
        assert_eq!(param, "api_token");
        assert!(
            message.contains("scope: global"),
            "message should tell the operator to make it a need: {message}"
        );
    }

    #[test]
    fn refuses_to_overwrite_an_existing_folder() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "src/widget/runbook.yaml",
            "name: widget\ndescription: X\nrisk_level: low\nscript: script.sh\nparameters: []\n",
        );
        write(dir.path(), "src/widget/script.sh", "#!/bin/sh\necho hi\n");
        let kata_dir = dir.path().join("kata");
        std::fs::create_dir_all(kata_dir.join("sesami")).unwrap();

        let err = import_catalog(&dir.path().join("src"), &kata_dir, "sesami").unwrap_err();
        assert!(matches!(err, ImportError::TargetExists(_)));
    }

    #[test]
    fn multi_file_kata_keeps_extra_files_opaque() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "src/metrics/runbook.yaml",
            "name: metrics\ndescription: Collect metrics\nrisk_level: low\nscript: script.sh\nparameters: []\n",
        );
        write(dir.path(), "src/metrics/script.sh", "#!/bin/sh\necho hi\n");
        write(dir.path(), "src/metrics/lib/helper.sh", "echo helper\n");

        let kata_dir = dir.path().join("kata");
        import_catalog(&dir.path().join("src"), &kata_dir, "sesami").unwrap();
        assert!(kata_dir.join("sesami/metrics/kata.sh").is_file());
        assert!(kata_dir.join("sesami/metrics/lib/helper.sh").is_file());
    }
}
