#!/usr/bin/env bash
# scan-jenkins-pipelines.sh
#
# Discovers all Jenkins pipelines at ci.example.com and writes one markdown file
# per pipeline to ./plans/jenkins/<safe-name>.md.
#
# Usage: bash scripts/scan-jenkins-pipelines.sh
# Run from the repo root. Requires: curl, jq.

set -euo pipefail

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

JENKINS_URL="https://ci.example.com"
OUT_DIR="plans/jenkins"
SUMMARY_FILE="plans/jenkins/_summary.txt"

# ---------------------------------------------------------------------------
# Load credentials from ~/.bashrc
# (Standard bashrc exits early in non-interactive shells, so we extract the
#  JENKINS export lines directly instead of sourcing the whole file.)
# ---------------------------------------------------------------------------

if [ -f "${HOME}/.bashrc" ]; then
  # shellcheck disable=SC1090
  eval "$(grep -E '^export JENKINS_(USER|PASS)=' "${HOME}/.bashrc" | head -2)"
fi

: "${JENKINS_USER:?JENKINS_USER is not set. Check ~/.bashrc}"
: "${JENKINS_PASS:?JENKINS_PASS is not set. Check ~/.bashrc}"

CURL_OPTS=(-gsk -u "${JENKINS_USER}:${JENKINS_PASS}" --max-time 30 --connect-timeout 10)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# Classes that act as containers (recurse into them)
is_folder() {
  case "$1" in
    com.cloudbees.hudson.plugins.folder.Folder|\
    jenkins.branch.OrganizationFolder|\
    org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject)
      return 0 ;;
    *) return 1 ;;
  esac
}

# Convert a job path like "ProjectA/my pipeline" to a safe filename
safe_name() {
  printf '%s' "$1" \
    | tr '[:upper:]' '[:lower:]' \
    | sed 's|/|--|g; s|[^a-z0-9-]|-|g; s|-\+|-|g; s|^-||; s|-$||'
}

# Format a Unix millisecond timestamp to human-readable date (requires GNU date)
format_ts() {
  local ts_ms="$1"
  if [ "${ts_ms}" = "null" ] || [ -z "${ts_ms}" ]; then
    printf '%s' "—"
  else
    local ts_s
    ts_s=$(( ts_ms / 1000 ))
    date -d "@${ts_s}" '+%Y-%m-%d %H:%M UTC' 2>/dev/null || printf '%s' "${ts_ms}"
  fi
}

# ---------------------------------------------------------------------------
# Write one markdown file for a buildable pipeline
# ---------------------------------------------------------------------------

write_pipeline_md() {
  local job_url="$1"   # e.g. https://ci.example.com/job/Folder/job/my-pipeline/
  local job_path="$2"  # e.g. Folder/my-pipeline

  local api_url="${job_url}api/json?depth=1&tree=name,url,description,buildable,property[parameterDefinitions[name,type,defaultParameterValue[value],description]],lastBuild[number,result,timestamp],healthReport[score,description]"

  local json
  if ! json=$(curl "${CURL_OPTS[@]}" "${api_url}" 2>/dev/null); then
    printf 'WARN: curl failed for %s\n' "${job_path}" >&2
    return
  fi

  if ! echo "${json}" | jq -e '.name' >/dev/null 2>&1; then
    printf 'WARN: unexpected response for %s\n' "${job_path}" >&2
    return
  fi

  # Basic fields
  local name description last_build_num last_build_result last_build_ts health_text
  name=$(echo "${json}"           | jq -r '.name // "unknown"')
  description=$(echo "${json}"    | jq -r '.description // ""')
  last_build_num=$(echo "${json}" | jq -r '.lastBuild.number // "—"')
  last_build_result=$(echo "${json}" | jq -r '.lastBuild.result // "—"')
  local raw_ts
  raw_ts=$(echo "${json}" | jq -r '.lastBuild.timestamp // empty')
  last_build_ts=$(format_ts "${raw_ts:-}")
  health_text=$(echo "${json}" | jq -r '[.healthReport[]? | .description] | if length > 0 then join("; ") else "—" end')

  # Parameters — flatten across all property entries that carry parameterDefinitions
  local params_md
  params_md=$(echo "${json}" | jq -r '
    [.property[]? | .parameterDefinitions[]?] as $params |
    if ($params | length) == 0 then
      "_No parameters_"
    else
      (
        ["| Name | Type | Default | Description |",
         "|------|------|---------|-------------|"] +
        [
          $params[] |
          "| `" + .name + "` | " +
          (.type | split(".")[-1]) + " | " +
          ((.defaultParameterValue.value // "—") | tostring) + " | " +
          (.description // "—") + " |"
        ]
      ) | join("\n")
    end
  ')

  local safe_filename
  safe_filename=$(safe_name "${job_path}")
  local md_file="${OUT_DIR}/${safe_filename}.md"

  cat > "${md_file}" <<MDEOF
# Pipeline: ${name}

| Field | Value |
|-------|-------|
| Project / Path | \`${job_path}\` |
| URL | ${job_url} |
| Description | ${description:-—} |
| Last Build | #${last_build_num} — ${last_build_result} (${last_build_ts}) |

## Parameters

${params_md}

## Health

${health_text}

## Notes

<!-- Add context here -->
MDEOF

  printf '  Wrote: %s\n' "${md_file}"

  # Append to summary (tab-separated: path <TAB> file)
  printf '%s\t%s\n' "${job_path}" "${md_file}" >> "${SUMMARY_FILE}"
}

# ---------------------------------------------------------------------------
# Recursive job scanner
# ---------------------------------------------------------------------------

scan_jobs() {
  local base_url="$1"
  local path_prefix="$2"

  local api_url="${base_url}api/json?depth=1&tree=jobs[_class,name,url,buildable]"
  local json
  if ! json=$(curl "${CURL_OPTS[@]}" "${api_url}" 2>/dev/null); then
    printf 'WARN: curl failed for %s\n' "${base_url}" >&2
    return
  fi

  local job_count
  job_count=$(echo "${json}" | jq '.jobs | length')
  printf 'Scanning: %-50s  (%s jobs)\n' "${path_prefix:-/}" "${job_count}"

  while IFS= read -r job_json; do
    local job_class job_name job_url job_buildable
    job_class=$(echo "${job_json}"    | jq -r '._class // ""')
    job_name=$(echo "${job_json}"     | jq -r '.name')
    job_url=$(echo "${job_json}"      | jq -r '.url')
    job_buildable=$(echo "${job_json}" | jq -r '.buildable // false')

    local job_path
    if [ -n "${path_prefix}" ]; then
      job_path="${path_prefix}/${job_name}"
    else
      job_path="${job_name}"
    fi

    if is_folder "${job_class}"; then
      printf '  [folder]   %s\n' "${job_path}"
      scan_jobs "${job_url}" "${job_path}"
    elif [ "${job_buildable}" = "true" ]; then
      printf '  [pipeline] %s\n' "${job_path}"
      write_pipeline_md "${job_url}" "${job_path}"
    else
      printf '  [skip]     %s  (%s)\n' "${job_path}" "${job_class}"
    fi
  done < <(echo "${json}" | jq -c '.jobs[]?')
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

mkdir -p "${OUT_DIR}"
: > "${SUMMARY_FILE}"  # reset summary

printf '==> Scanning Jenkins at %s\n\n' "${JENKINS_URL}"
scan_jobs "${JENKINS_URL}/" ""

echo ""
printf '==> Done. Pipelines discovered:\n'
if [ -s "${SUMMARY_FILE}" ]; then
  while IFS=$'\t' read -r path file; do
    printf '    %-60s  %s\n' "${path}" "${file}"
  done < "${SUMMARY_FILE}"
  printf '\nTotal: %d pipeline(s)\n' "$(wc -l < "${SUMMARY_FILE}")"
else
  printf '    (none)\n'
fi
