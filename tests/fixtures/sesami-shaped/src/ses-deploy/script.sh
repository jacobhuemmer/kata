#!/bin/sh
set -eu

JENKINS_URL="${JENKINS_URL:?jenkins_url is required}"
JENKINS_USER="${JENKINS_USER:?jenkins_user is required}"
JENKINS_TOKEN="${JENKINS_TOKEN:?jenkins_token is required}"
VERSION="${VERSION:?version is required}"
OKE_CLUSTER="${OKE_CLUSTER:?oke_cluster is required}"
NAMESPACE="${NAMESPACE:-}"
MODULES="${MODULES:-aaa,admin,assistant,biz,cam,cash,cash-ops,cms,device,device-fm,device-rm,edge,export,ext,hall,monitoring,prestage,ras,reports,retail,system}"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TRIGGER="${REPO_ROOT}/scripts/trigger-pipeline.sh"
PIPELINE="SES/SES Deploy"

main() {
  echo "==> Stage 1/3: Validate"
  echo "Pipeline    : ${PIPELINE}"
  echo "VERSION     : ${VERSION}"
  echo "OKE_CLUSTER : ${OKE_CLUSTER}"
  echo "NAMESPACE   : ${NAMESPACE:-<default>}"
  echo "MODULES     : ${MODULES}"
  echo "Server      : ${JENKINS_URL}"

  if [ ! -f "${TRIGGER}" ]; then
    echo "✗ trigger-pipeline.sh not found at ${TRIGGER}" >&2
    exit 1
  fi

  echo ""
  echo "==> Stage 2/3: Run"
  if [ -n "${NAMESPACE}" ]; then
    JENKINS_URL="${JENKINS_URL}" \
    JENKINS_USER="${JENKINS_USER}" \
    JENKINS_PASS="${JENKINS_TOKEN}" \
      "${TRIGGER}" "${PIPELINE}" \
        --param "VERSION=${VERSION}" \
        --param "OKE_CLUSTER=${OKE_CLUSTER}" \
        --param "NAMESPACE=${NAMESPACE}" \
        --param "MODULES=${MODULES}"
  else
    JENKINS_URL="${JENKINS_URL}" \
    JENKINS_USER="${JENKINS_USER}" \
    JENKINS_PASS="${JENKINS_TOKEN}" \
      "${TRIGGER}" "${PIPELINE}" \
        --param "VERSION=${VERSION}" \
        --param "OKE_CLUSTER=${OKE_CLUSTER}" \
        --param "MODULES=${MODULES}"
  fi

  echo ""
  echo "✓ Done"
}

main "$@"
