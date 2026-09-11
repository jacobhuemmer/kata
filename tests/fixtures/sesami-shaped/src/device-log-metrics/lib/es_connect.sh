#!/usr/bin/env bash
# es_connect.sh — Elasticsearch connection library
#
# Source this file after setting these variables (typically from an env file):
#   K8S_CONTEXT   — k8s context name (required)
#   NAMESPACE     — k8s namespace for ES (required)
#   LOCAL_PORT    — local port for port-forward (default: 9201)
#   SECRET        — k8s secret name (default: auto-detect)
#   INDEX_PREFIX  — index name prefix (default: derived from NAMESPACE)
#
# After sourcing, these are exported:
#   ES_USER       — ES username
#   ES_PASS       — ES password
#   ES            — Base URL (http://localhost:$LOCAL_PORT)
#   PF_PID        — Port-forward process PID
#   INDEX_PREFIX  — Index prefix for queries

set -euo pipefail

# --- validate required vars ---
: "${K8S_CONTEXT:?K8S_CONTEXT must be set before sourcing es_connect.sh}"
: "${NAMESPACE:?NAMESPACE must be set before sourcing es_connect.sh}"
LOCAL_PORT="${LOCAL_PORT:-9201}"

# --- credentials ---
if [[ "${NO_AUTH:-false}" == "true" ]]; then
  ES_USER=""
  ES_PASS=""
else
  # auto-detect secret
  if [[ -z "${SECRET:-}" ]]; then
    if kubectl --context "$K8S_CONTEXT" -n "$NAMESPACE" get secret admin-basic-auth >/dev/null 2>&1; then
      SECRET="admin-basic-auth"
    elif kubectl --context "$K8S_CONTEXT" -n "$NAMESPACE" get secret elasticsearch-master-credentials >/dev/null 2>&1; then
      SECRET="elasticsearch-master-credentials"
    else
      echo "ERROR: Could not find a known ES credentials secret in $NAMESPACE." >&2
      return 1 2>/dev/null || exit 1
    fi
  fi

  echo "▶ Getting credentials from secret $SECRET ..." >&2
  ES_USER=$(kubectl --context "$K8S_CONTEXT" -n "$NAMESPACE" \
    get secret "$SECRET" -o jsonpath='{.data.username}' | base64 -d)
  ES_PASS=$(kubectl --context "$K8S_CONTEXT" -n "$NAMESPACE" \
    get secret "$SECRET" -o jsonpath='{.data.password}' | base64 -d)
fi

# --- discover ES service ---
SVC=$(kubectl --context "$K8S_CONTEXT" -n "$NAMESPACE" get svc \
  --no-headers -o custom-columns=NAME:.metadata.name \
  | grep 'es-http$' | head -1 || true)

if [[ -z "$SVC" ]]; then
  SVC=$(kubectl --context "$K8S_CONTEXT" -n "$NAMESPACE" get svc \
    --no-headers -o custom-columns=NAME:.metadata.name \
    | grep 'elasticsearch-master$' | head -1 || true)
fi

if [[ -z "$SVC" ]]; then
  echo "ERROR: Could not find ES service in namespace $NAMESPACE" >&2
  return 1 2>/dev/null || exit 1
fi

# --- port-forward ---
echo "▶ Port-forwarding $NAMESPACE/$SVC -> localhost:$LOCAL_PORT ..." >&2
kubectl --context "$K8S_CONTEXT" -n "$NAMESPACE" \
  port-forward "svc/$SVC" "${LOCAL_PORT}:9200" >/tmp/es-pf.log 2>&1 &
PF_PID=$!

# Register cleanup — caller can override with their own trap
_es_connect_cleanup() {
  kill "$PF_PID" 2>/dev/null
  wait "$PF_PID" 2>/dev/null
}
trap '_es_connect_cleanup' EXIT

ES="http://localhost:${LOCAL_PORT}"

# --- derive index prefix ---
if [[ -z "${INDEX_PREFIX:-}" ]]; then
  INDEX_PREFIX="${NAMESPACE%-elastic}"
fi

# --- wait for readiness (up to 15s) ---
READY=0
for _ in $(seq 1 15); do
  sleep 1
  if curl -s -u "${ES_USER}:${ES_PASS}" "${ES}/_cluster/health" >/dev/null 2>&1; then
    READY=1
    break
  fi
done

if [[ $READY -eq 0 ]]; then
  echo "ERROR: Could not reach Elasticsearch at $ES after 15s" >&2
  echo "Port-forward log:" >&2
  cat /tmp/es-pf.log >&2
  return 1 2>/dev/null || exit 1
fi

echo "▶ Connected to Elasticsearch at $ES (prefix: $INDEX_PREFIX)" >&2

# --- export for caller ---
export ES_USER ES_PASS ES PF_PID INDEX_PREFIX
