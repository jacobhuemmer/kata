#!/bin/sh
set -eu

JENKINS_URL="${JENKINS_URL:?jenkins_url is required}"
JENKINS_USER="${JENKINS_USER:?jenkins_user is required}"
JENKINS_TOKEN="${JENKINS_TOKEN:?jenkins_token is required}"
RELEASE_BRANCH="${RELEASE_BRANCH:?release_branch is required}"
VERSION="${VERSION:-}"
ALLOW_IMAGE_OVERRIDE="${ALLOW_IMAGE_OVERRIDE:-false}"
SEND_EMAIL="${SEND_EMAIL:-true}"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TRIGGER="${REPO_ROOT}/scripts/trigger-pipeline.sh"
PIPELINE="SES/SES-Release Build"

main() {
  echo "==> Stage 1/3: Validate"
  echo "Pipeline             : ${PIPELINE}"
  echo "RELEASE_BRANCH       : ${RELEASE_BRANCH}"
  echo "VERSION              : ${VERSION:-<same as RELEASE_BRANCH>}"
  echo "ALLOW_IMAGE_OVERRIDE : ${ALLOW_IMAGE_OVERRIDE}"
  echo "SEND_EMAIL           : ${SEND_EMAIL}"
  echo "Server               : ${JENKINS_URL}"

  if [ ! -f "${TRIGGER}" ]; then
    echo "✗ trigger-pipeline.sh not found at ${TRIGGER}" >&2
    exit 1
  fi

  echo ""
  echo "==> Stage 2/3: Run"

  set -- "${PIPELINE}" \
    --param "RELEASE_BRANCH=${RELEASE_BRANCH}" \
    --param "ALLOW_IMAGE_OVERRIDE=${ALLOW_IMAGE_OVERRIDE}" \
    --param "SEND_EMAIL=${SEND_EMAIL}"

  if [ -n "${VERSION}" ]; then
    set -- "$@" --param "VERSION=${VERSION}"
  fi

  JENKINS_URL="${JENKINS_URL}" \
  JENKINS_USER="${JENKINS_USER}" \
  JENKINS_PASS="${JENKINS_TOKEN}" \
    "${TRIGGER}" "$@"

  echo ""
  echo "✓ Done"
}

main "$@"
