#!/usr/bin/env bash
# collect.sh — Collect ES write volume metrics into a JSON snapshot
#
# Usage:
#   ./collect.sh --env envs/preprod.env [OPTIONS]
#
# Options:
#   --env <file>     Environment config file (required)
#   --since <spec>   Time window start: ISO 8601 or "2h", "24h", "7d" (default: 24h)
#   --label <name>   Override ENV_LABEL from env file
#   -h, --help       Show this message
#
# Output:
#   Writes snapshot JSON to snapshots/{label}_{timestamp}.json
#   Prints the snapshot file path to stdout

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --- defaults ---
ENV_FILE=""
SINCE=""
LABEL_OVERRIDE=""

usage() {
  grep '^#' "$0" | grep -v '#!/' | sed 's/^# \?//'
  exit 0
}

# --- parse args ---
while [[ $# -gt 0 ]]; do
  case $1 in
    --env)    ENV_FILE="$2";       shift 2 ;;
    --since)  SINCE="$2";         shift 2 ;;
    --label)  LABEL_OVERRIDE="$2"; shift 2 ;;
    -h|--help) usage ;;
    *) echo "Unknown option: $1" >&2; usage ;;
  esac
done

if [[ -z "$ENV_FILE" ]]; then
  echo "ERROR: --env <file> is required" >&2
  exit 1
fi

# --- load environment ---
# shellcheck source=/dev/null
source "$ENV_FILE"

if [[ -n "$LABEL_OVERRIDE" ]]; then
  ENV_LABEL="$LABEL_OVERRIDE"
fi

# --- resolve --since to ISO 8601 UTC ---
if [[ -z "$SINCE" ]]; then
  SINCE_TS=$(date -u -d "24 hours ago" +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null \
    || date -u -v-24H +"%Y-%m-%dT%H:%M:%SZ")
elif [[ "$SINCE" =~ ^[0-9]+[hHdDmM]$ ]]; then
  NUM="${SINCE//[^0-9]/}"
  UNIT="${SINCE//[0-9]/}"
  case "${UNIT,,}" in
    h) ARG="${NUM} hours ago" ;;
    d) ARG="${NUM} days ago"  ;;
    m) ARG="${NUM} minutes ago" ;;
  esac
  SINCE_TS=$(date -u -d "$ARG" +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null \
    || date -u -v-"${NUM}${UNIT}" +"%Y-%m-%dT%H:%M:%SZ")
else
  SINCE_TS="$SINCE"
fi

NOW_TS=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# --- connect to ES ---
# shellcheck source=lib/es_connect.sh
source "${SCRIPT_DIR}/lib/es_connect.sh"

# --- ES aggregation query builder ---
run_agg() {
  local index_pattern="$1"
  local device_type_field="$2"

  curl -s -u "${ES_USER}:${ES_PASS}" \
    "${ES}/${index_pattern}/_search" \
    -H 'Content-Type: application/json' \
    -d "{
      \"size\": 0,
      \"query\": {
        \"range\": {
          \"@timestamp\": {
            \"gte\": \"${SINCE_TS}\",
            \"lte\": \"${NOW_TS}\"
          }
        }
      },
      \"aggs\": {
        \"by_device_type\": {
          \"terms\": { \"field\": \"${device_type_field}\", \"size\": 50 },
          \"aggs\": {
            \"sample\": {
              \"top_hits\": { \"size\": 20, \"_source\": true }
            }
          }
        }
      }
    }"
}

run_cardinality() {
  local index_pattern="$1"

  curl -s -u "${ES_USER}:${ES_PASS}" \
    "${ES}/${index_pattern}/_search" \
    -H 'Content-Type: application/json' \
    -d "{
      \"size\": 0,
      \"query\": {
        \"range\": {
          \"@timestamp\": { \"gte\": \"${SINCE_TS}\", \"lte\": \"${NOW_TS}\" }
        }
      },
      \"aggs\": {
        \"by_device_type\": {
          \"terms\": { \"field\": \"deviceType\", \"size\": 50 },
          \"aggs\": {
            \"unique_devices\": {
              \"cardinality\": {
                \"field\": \"deviceId.keyword\",
                \"precision_threshold\": 40000
              }
            }
          }
        }
      }
    }"
}

# --- collect per tenant ---
# For now, collect across all tenants using wildcard indices.
# TENANT_IDS is available for future per-tenant collection.
echo "▶ Querying evt streams ..." >&2
EVT_JSON=$(run_agg "${INDEX_PREFIX}-evt-*" "deviceType")

echo "▶ Querying com streams ..." >&2
COM_JSON=$(run_agg "${INDEX_PREFIX}-com-*" "deviceType")

echo "▶ Counting distinct devices per type (cardinality) ..." >&2
DEVICE_COUNT_JSON=$(run_cardinality "${INDEX_PREFIX}-evt-*,${INDEX_PREFIX}-com-*")

# --- build raw input for compute_metrics.py ---
TMPDIR_COLLECT=$(mktemp -d)
_collect_cleanup() {
  rm -rf "$TMPDIR_COLLECT"
  _es_connect_cleanup 2>/dev/null || true
}
trap '_collect_cleanup' EXIT

# Write raw JSON parts to temp files to avoid quoting issues
echo "$EVT_JSON"          > "$TMPDIR_COLLECT/evt.json"
echo "$COM_JSON"          > "$TMPDIR_COLLECT/com.json"
echo "$DEVICE_COUNT_JSON" > "$TMPDIR_COLLECT/devices.json"

# Assemble the input payload using Python (avoids jq dependency)
python3 -c "
import json, sys

with open('$TMPDIR_COLLECT/evt.json') as f: evt = json.load(f)
with open('$TMPDIR_COLLECT/com.json') as f: com = json.load(f)
with open('$TMPDIR_COLLECT/devices.json') as f: dev = json.load(f)

payload = {
    'env_label': '$ENV_LABEL',
    'since': '$SINCE_TS',
    'until': '$NOW_TS',
    'evt': evt,
    'com': com,
    'device_counts': dev,
}
json.dump(payload, sys.stdout)
" | python3 "${SCRIPT_DIR}/lib/compute_metrics.py" > "$TMPDIR_COLLECT/snapshot.json"

# --- write snapshot ---
TIMESTAMP=$(date -u +"%Y%m%d_%H%M%S")
SAFE_LABEL=$(echo "$ENV_LABEL" | tr ' ' '_')
SNAPSHOT_PATH="${SCRIPT_DIR}/snapshots/${SAFE_LABEL}_${TIMESTAMP}.json"

cp "$TMPDIR_COLLECT/snapshot.json" "$SNAPSHOT_PATH"

echo "▶ Snapshot written to: $SNAPSHOT_PATH" >&2
echo "$SNAPSHOT_PATH"
