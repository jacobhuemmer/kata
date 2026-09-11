#!/bin/sh
set -eu

MODE="${MODE:-report}"
ENV="${ENV:?env is required}"
COMPARE_ENV="${COMPARE_ENV:-}"
SINCE="${SINCE:-24h}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
COLLECT="${SCRIPT_DIR}/collect.sh"
COMPARE="${SCRIPT_DIR}/compare.py"
REPORT="${SCRIPT_DIR}/report.py"

resolve_env_file() {
  _env_file="${SCRIPT_DIR}/envs/${1}.env"
  if [ ! -f "${_env_file}" ]; then
    echo "✗ Environment file not found: ${_env_file}" >&2
    exit 1
  fi
  printf '%s' "${_env_file}"
}

main() {
  echo "==> Stage 1: Validate"
  echo "    Mode  : ${MODE}"
  echo "    Env   : ${ENV}"
  if [ "${MODE}" = "compare" ]; then
    echo "    Compare: ${COMPARE_ENV}"
  fi
  echo "    Since : ${SINCE}"

  if [ ! -f "${COLLECT}" ]; then
    echo "✗ collect.sh not found at ${COLLECT}" >&2
    exit 1
  fi

  if [ "${MODE}" = "compare" ] && [ -z "${COMPARE_ENV}" ]; then
    echo "✗ compare_env is required when mode=compare" >&2
    exit 1
  fi

  if [ "${MODE}" = "compare" ] && [ "${ENV}" = "${COMPARE_ENV}" ]; then
    echo "✗ env and compare_env must be different" >&2
    exit 1
  fi

  env_file="$(resolve_env_file "${ENV}")"

  echo ""
  echo "==> Stage 2: Collect ${ENV}"
  snapshot_a="$("${COLLECT}" --env "${env_file}" --since "${SINCE}")"
  echo "    Snapshot: ${snapshot_a}"

  if [ "${MODE}" = "compare" ]; then
    compare_env_file="$(resolve_env_file "${COMPARE_ENV}")"

    echo ""
    echo "==> Stage 3: Collect ${COMPARE_ENV}"
    snapshot_b="$("${COLLECT}" --env "${compare_env_file}" --since "${SINCE}")"
    echo "    Snapshot: ${snapshot_b}"

    echo ""
    echo "==> Stage 4: Compare"
    python3 "${COMPARE}" "${snapshot_a}" "${snapshot_b}"

    echo ""
    echo "==> Stage 5: Report"
    # compare.py writes comparison JSON; find the latest one
    latest_cmp="$(ls -t "${SCRIPT_DIR}/snapshots/comparison_"*.json 2>/dev/null | head -1)"
    if [ -n "${latest_cmp}" ]; then
      python3 "${REPORT}" --mode compare "${latest_cmp}"
    fi
  else
    echo ""
    echo "==> Stage 3: Report"
    python3 "${REPORT}" "${snapshot_a}"
  fi

  echo ""
  echo "========================================="
  echo "  Summary"
  echo "========================================="
  echo "  Mode   : ${MODE}"
  echo "  Env    : ${ENV}"
  if [ "${MODE}" = "compare" ]; then
    echo "  Compare: ${COMPARE_ENV}"
  fi
  echo "  Status : SUCCESS"
  echo "========================================="
}

main "$@"
