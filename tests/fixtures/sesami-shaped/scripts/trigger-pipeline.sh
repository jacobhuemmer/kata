#!/usr/bin/env bash
# trigger-pipeline.sh
#
# Trigger any Jenkins pipeline at ci.example.com, then stream its console log.
# Supports standalone pipelines and multi-branch pipelines (via --branch).
#
# Usage:
#   scripts/trigger-pipeline.sh <pipeline-path> [OPTIONS]
#
# Examples:
#   scripts/trigger-pipeline.sh CCO/Master_Release
#   scripts/trigger-pipeline.sh SES/CC4/cc4-aaa --branch dev
#   scripts/trigger-pipeline.sh SES/CC4/cc4-aaa --branch release/26.4.1.0
#   scripts/trigger-pipeline.sh "SES/SES Deploy" --param VERSION=26.4.1.0 --param OKE_CLUSTER=prod
#   scripts/trigger-pipeline.sh SES/CC4/cc4-aaa --branch PR-85 --dry-run

set -euo pipefail

# Merge stderr into stdout so dops sees a single stream (no red text),
# while still letting the Jenkins console log stream unbuffered via stdout.
exec 2>&1

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

JENKINS_URL="${JENKINS_URL:-https://ci.example.com}"
QUEUE_TIMEOUT="${QUEUE_TIMEOUT:-300}"        # seconds to wait for a build executor
LOG_POLL_INTERVAL="${LOG_POLL_INTERVAL:-3}"  # seconds between progressive-log polls

# ---------------------------------------------------------------------------
# Logger  (stderr → stdout via exec above; Jenkins console log → stdout)
# ---------------------------------------------------------------------------

if [ -t 2 ]; then
  _RST=$'\033[0m'
  _BOLD=$'\033[1m'
  _CYAN=$'\033[0;36m'
  _GREEN=$'\033[0;32m'
  _YELLOW=$'\033[0;33m'
  _RED=$'\033[0;31m'
else
  _RST='' _BOLD='' _CYAN='' _GREEN='' _YELLOW='' _RED=''
fi

_log() {
  local level="$1" color="$2"
  shift 2
  printf '%s[%s] %-5s%s %s\n' "${color}" "$(date -u '+%H:%M:%S')" "${level}" "${_RST}" "$*"
} >&2

log_info()  { _log INFO  "${_CYAN}"   "$@"; }
log_ok()    { _log OK    "${_GREEN}"  "$@"; }
log_warn()  { _log WARN  "${_YELLOW}" "$@"; }
log_error() { _log ERROR "${_RED}"    "$@"; }

_sep() { printf '%s%s%s\n' "${_BOLD}" '─────────────────────────────────────────────────────' "${_RST}"; } >&2

# ---------------------------------------------------------------------------
# Credentials — only source ~/.bashrc if vars not already in the environment
# (allows runbooks to inject JENKINS_USER/JENKINS_PASS via dops global vars)
# ---------------------------------------------------------------------------

if [ -z "${JENKINS_USER:-}" ] || [ -z "${JENKINS_PASS:-}" ]; then
  if [ -f "${HOME}/.bashrc" ]; then
    # shellcheck disable=SC1090
    eval "$(grep -E '^export JENKINS_(USER|PASS)=' "${HOME}/.bashrc" | head -2)"
  fi
fi

: "${JENKINS_USER:?JENKINS_USER is not set. Export it or add to ~/.bashrc}"
: "${JENKINS_PASS:?JENKINS_PASS is not set. Export it or add to ~/.bashrc}"

CURL_OPTS=(-gsk -u "${JENKINS_USER}:${JENKINS_PASS}" --max-time 30 --connect-timeout 10)

# ---------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------

usage() {
  cat <<EOF
Usage: $(basename "$0") <pipeline-path> [OPTIONS]

Arguments:
  <pipeline-path>   Jenkins job path (e.g. "CCO/Master_Release" or "SES/CC4/cc4-aaa")
                    For multi-branch pipelines, pass the parent path and use --branch.

Options:
  --branch <ref>    Branch, release label, or PR (e.g. "dev", "release/26.4.1.0", "PR-85")
  --param KEY=VAL   Build parameter (repeatable; e.g. --param VERSION=26.4.1.0)
  --no-log          Trigger only — skip console log streaming
  --dry-run         Print the trigger command without executing it
  --help            Show this help

Examples:
  $(basename "$0") CCO/Master_Release
  $(basename "$0") SES/CC4/cc4-aaa --branch dev
  $(basename "$0") SES/CC4/cc4-aaa --branch release/26.4.1.0
  $(basename "$0") "SES/SES Deploy" --param VERSION=26.4.1.0 --param OKE_CLUSTER=prod
  $(basename "$0") SES/CC4/cc4-aaa --branch PR-85 --dry-run
EOF
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# URL-encode a single segment (spaces → %20, etc.); does NOT encode slashes.
urlencode_segment() {
  python3 -c "import urllib.parse, sys; print(urllib.parse.quote(sys.argv[1], safe=''), end='')" "$1"
}

# Convert "SES/CC4/cc4-aaa" → "/job/SES/job/CC4/job/cc4-aaa"
path_to_job_url() {
  local path="$1" job_path="" IFS='/'
  # shellcheck disable=SC2086
  set -- $path
  for segment; do
    job_path="${job_path}/job/$(urlencode_segment "${segment}")"
  done
  printf '%s' "${job_path}"
}

get_crumb() {
  local crumb_json field value
  if ! crumb_json=$(curl "${CURL_OPTS[@]}" -s "${JENKINS_URL}/crumbIssuer/api/json" 2>/dev/null); then
    log_error "Could not reach ${JENKINS_URL}/crumbIssuer/api/json"
    return 1
  fi
  field=$(printf '%s' "${crumb_json}" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['crumbRequestField'])")
  value=$(printf '%s' "${crumb_json}" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['crumb'])")
  printf '%s:%s' "${field}" "${value}"
}

# Poll the queue item until Jenkins assigns the build an executor.
# Prints the build URL on success.
wait_for_build() {
  local queue_url="$1"
  local elapsed=0 item_json build_url

  log_info "Waiting for executor (queue: ${queue_url})..."

  while [ "${elapsed}" -lt "${QUEUE_TIMEOUT}" ]; do
    item_json=$(curl "${CURL_OPTS[@]}" -s "${queue_url}api/json" 2>/dev/null) || true

    # Cancelled?
    if printf '%s' "${item_json}" | python3 -c \
        "import sys,json; d=json.load(sys.stdin); sys.exit(0 if d.get('cancelled') else 1)" \
        2>/dev/null; then
      log_error "Build was cancelled in queue"
      return 1
    fi

    # Started?
    build_url=$(printf '%s' "${item_json}" | python3 -c "
import sys, json
d = json.load(sys.stdin)
if d.get('executable'):
    print(d['executable']['url'])
" 2>/dev/null || true)

    if [ -n "${build_url}" ]; then
      printf '%s' "${build_url}"
      return 0
    fi

    sleep 2
    elapsed=$((elapsed + 2))
  done

  log_error "Timed out after ${QUEUE_TIMEOUT}s waiting for build to start"
  return 1
}

# Stream the Jenkins progressive console log to stdout until the build completes.
stream_log() {
  local build_url="$1"
  local log_url="${build_url}logText/progressiveText"
  local start=0 more_data="true"
  local header_file new_start new_more

  header_file=$(mktemp)
  # shellcheck disable=SC2064
  trap "rm -f '${header_file}'" RETURN

  while [ "${more_data}" = "true" ]; do
    curl "${CURL_OPTS[@]}" \
      -s \
      -D "${header_file}" \
      "${log_url}?start=${start}" 2>/dev/null

    new_start=$(grep -i '^X-Text-Size:' "${header_file}" | head -1 | awk '{print $2}' | tr -d '\r') || true
    new_more=$(grep -i '^X-More-Data:' "${header_file}" | head -1 | awk '{print $2}' | tr -d '\r' | tr '[:upper:]' '[:lower:]') || true

    start="${new_start:-${start}}"
    more_data="${new_more:-false}"

    if [ "${more_data}" = "true" ]; then
      sleep "${LOG_POLL_INTERVAL}"
    fi
  done
}

# Query the finished build for result, duration, and number.
# Outputs: "RESULT|Xm Ys|BUILD_NUMBER"
get_build_result() {
  local build_url="$1"
  curl "${CURL_OPTS[@]}" -s "${build_url}api/json?tree=result,duration,number" 2>/dev/null | \
    python3 -c "
import sys, json
d        = json.load(sys.stdin)
result   = d.get('result') or 'UNKNOWN'
duration = d.get('duration', 0)
number   = d.get('number', 0)
mins     = duration // 60000
secs     = (duration % 60000) // 1000
print(f'{result}|{mins}m {secs}s|{number}')
"
}

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------

if [ $# -eq 0 ]; then
  usage; exit 1
fi

PIPELINE_PATH=""
BRANCH=""
DRY_RUN=false
NO_LOG=false
declare -a PARAMS=()

while [ $# -gt 0 ]; do
  case "$1" in
    --help|-h)  usage; exit 0 ;;
    --branch)   shift; BRANCH="${1:?--branch requires a value}" ;;
    --param)    shift; PARAMS+=("${1:?--param requires KEY=VALUE}") ;;
    --dry-run)  DRY_RUN=true ;;
    --no-log)   NO_LOG=true ;;
    -*)
      printf 'ERROR: unknown option: %s\n' "$1" >&2
      usage >&2; exit 1
      ;;
    *)
      if [ -n "${PIPELINE_PATH}" ]; then
        PIPELINE_PATH="${PIPELINE_PATH} $1"
      else
        PIPELINE_PATH="$1"
      fi
      ;;
  esac
  shift
done

if [ -z "${PIPELINE_PATH}" ]; then
  printf 'ERROR: <pipeline-path> is required\n' >&2
  usage >&2; exit 1
fi

# ---------------------------------------------------------------------------
# Build the Jenkins job URL
# ---------------------------------------------------------------------------

JOB_PATH=$(path_to_job_url "${PIPELINE_PATH}")

if [ -n "${BRANCH}" ]; then
  JOB_PATH="${JOB_PATH}/job/$(urlencode_segment "${BRANCH}")"
fi

if [ "${#PARAMS[@]}" -gt 0 ]; then
  QUERY_STRING=""
  for p in "${PARAMS[@]}"; do
    encoded_key=$(urlencode_segment "${p%%=*}")
    encoded_val=$(urlencode_segment "${p#*=}")
    if [ -z "${QUERY_STRING}" ]; then
      QUERY_STRING="${encoded_key}=${encoded_val}"
    else
      QUERY_STRING="${QUERY_STRING}&${encoded_key}=${encoded_val}"
    fi
  done
  FULL_URL="${JENKINS_URL}${JOB_PATH}/buildWithParameters?${QUERY_STRING}"
else
  FULL_URL="${JENKINS_URL}${JOB_PATH}/build"
fi

# ---------------------------------------------------------------------------
# Dry run
# ---------------------------------------------------------------------------

if "${DRY_RUN}"; then
  printf '==> DRY RUN — would execute:\n\n'
  printf 'curl %s \\\n' "${CURL_OPTS[*]}"
  printf '     -X POST \\\n'
  printf '     -H "<crumb-field>:<crumb>" \\\n'
  printf '     "%s"\n' "${FULL_URL}"
  exit 0
fi

# ---------------------------------------------------------------------------
# Trigger
# ---------------------------------------------------------------------------

HEADER_FILE=$(mktemp)
trap 'rm -f "${HEADER_FILE}"' EXIT

log_info "Fetching CSRF crumb..."
CRUMB=$(get_crumb)

log_info "Triggering ${PIPELINE_PATH}${BRANCH:+ @ ${BRANCH}}..."

HTTP_CODE=$(curl "${CURL_OPTS[@]}" \
  -s \
  -o /dev/null \
  -w '%{http_code}' \
  -D "${HEADER_FILE}" \
  -X POST \
  -H "${CRUMB}" \
  "${FULL_URL}")

case "${HTTP_CODE}" in
  201|200|302) log_ok "Build queued (HTTP ${HTTP_CODE})" ;;
  403) log_error "403 Forbidden — check credentials or CSRF crumb"; exit 1 ;;
  404) log_error "404 Not Found — verify the pipeline path"; log_error "Tried: ${FULL_URL}"; exit 1 ;;
  *)   log_error "Unexpected HTTP ${HTTP_CODE} — URL: ${FULL_URL}"; exit 1 ;;
esac

# Normalize the queue Location header (may be relative or absolute)
QUEUE_URL=$(grep -i '^Location:' "${HEADER_FILE}" | head -1 | awk '{print $2}' | tr -d '\r')
case "${QUEUE_URL}" in
  http://*|https://*) ;;
  *) QUEUE_URL="${JENKINS_URL}${QUEUE_URL}" ;;
esac

if "${NO_LOG}"; then
  log_info "Monitor at: ${JENKINS_URL}${JOB_PATH}/"
  exit 0
fi

if [ -z "${QUEUE_URL}" ]; then
  log_warn "No queue URL in response — cannot stream log"
  log_info "Monitor at: ${JENKINS_URL}${JOB_PATH}/"
  exit 0
fi

# ---------------------------------------------------------------------------
# Wait for executor → stream log → summarise
# ---------------------------------------------------------------------------

BUILD_URL=$(wait_for_build "${QUEUE_URL}")
BUILD_URL="${BUILD_URL%/}/"  # ensure trailing slash

BUILD_NUM=$(printf '%s' "${BUILD_URL}" | grep -oE '[0-9]+/?$' | tr -d '/') || BUILD_NUM="?"
log_info "Build #${BUILD_NUM} started — streaming console log"

_sep
stream_log "${BUILD_URL}"
_sep

BUILD_INFO=$(get_build_result "${BUILD_URL}") || BUILD_INFO="UNKNOWN|0m 0s|0"
RESULT=$(printf '%s' "${BUILD_INFO}" | cut -d'|' -f1) || RESULT="UNKNOWN"
DURATION=$(printf '%s' "${BUILD_INFO}" | cut -d'|' -f2) || DURATION="?"

case "${RESULT}" in
  SUCCESS)  RESULT_COLOR="${_GREEN}"  ;;
  FAILURE)  RESULT_COLOR="${_RED}"    ;;
  ABORTED)  RESULT_COLOR="${_YELLOW}" ;;
  UNSTABLE) RESULT_COLOR="${_YELLOW}" ;;
  *)        RESULT_COLOR="${_BOLD}"   ;;
esac

{
  printf '\n==> Stage 3/3: Summary\n'
  _sep
  printf '%s  %-12s%s %s\n'    "${_BOLD}" "Pipeline" "${_RST}" "${PIPELINE_PATH}"
  if [ -n "${BRANCH}" ]; then
    printf '%s  %-12s%s %s\n'  "${_BOLD}" "Branch"   "${_RST}" "${BRANCH}"
  fi
  printf '%s  %-12s%s #%s\n'   "${_BOLD}" "Build"    "${_RST}" "${BUILD_NUM}"
  printf '%s  %-12s%s %s%s%s\n' "${_BOLD}" "Result"  "${_RST}" "${RESULT_COLOR}" "${RESULT}" "${_RST}"
  printf '%s  %-12s%s %s\n'    "${_BOLD}" "Duration" "${_RST}" "${DURATION}"
  printf '%s  %-12s%s %s\n'    "${_BOLD}" "URL"      "${_RST}" "${BUILD_URL}"
  _sep
  printf '\n'
} >&2

case "${RESULT}" in
  SUCCESS) exit 0 ;;
  *)       exit 1 ;;
esac
