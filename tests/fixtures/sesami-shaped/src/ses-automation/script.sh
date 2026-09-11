#!/bin/sh
set -eu

JENKINS_URL="${JENKINS_URL:?jenkins_url is required}"
JENKINS_USER="${JENKINS_USER:?jenkins_user is required}"
JENKINS_TOKEN="${JENKINS_TOKEN:?jenkins_token is required}"
TEST_SUITE="${TEST_SUITE:-uatCompleteUiAndAPI}"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TRIGGER="${REPO_ROOT}/scripts/trigger-pipeline.sh"
PIPELINE="SES/SES Automation"

main() {
  echo "==> Stage 1/3: Validate"
  echo "Pipeline   : ${PIPELINE}"
  echo "TEST_SUITE : ${TEST_SUITE}"
  echo "Server     : ${JENKINS_URL}"

  if [ ! -f "${TRIGGER}" ]; then
    echo "✗ trigger-pipeline.sh not found at ${TRIGGER}" >&2
    exit 1
  fi

  echo ""
  echo "==> Stage 2/3: Run"
  JENKINS_URL="${JENKINS_URL}" \
  JENKINS_USER="${JENKINS_USER}" \
  JENKINS_PASS="${JENKINS_TOKEN}" \
    "${TRIGGER}" "${PIPELINE}" \
      --param "TEST_SUITE=${TEST_SUITE}"

  echo ""
  echo "✓ Done"
}

main "$@"
