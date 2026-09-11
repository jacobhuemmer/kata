#!/bin/sh
set -eu

JENKINS_URL="${JENKINS_URL:?jenkins_url is required}"
JENKINS_USER="${JENKINS_USER:?jenkins_user is required}"
JENKINS_TOKEN="${JENKINS_TOKEN:?jenkins_token is required}"
BRANCH="${BRANCH:-dev}"
VERSION="${VERSION:-}"
SEND_EMAIL="${SEND_EMAIL:-}"
PUBLISH_IMAGE="${PUBLISH_IMAGE:-}"
PUBLISH_API="${PUBLISH_API:-}"
ALLOW_IMAGE_OVERRIDE="${ALLOW_IMAGE_OVERRIDE:-}"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TRIGGER="${REPO_ROOT}/scripts/trigger-pipeline.sh"
PIPELINE="SES/CC4/cc4-ras"

main() {
  echo "==> Stage 1/3: Validate"
  echo "Pipeline             : ${PIPELINE}"
  echo "Branch               : ${BRANCH}"
  if [ -n "${VERSION}" ]; then
    echo "VERSION              : ${VERSION}"
  fi
  if [ -n "${SEND_EMAIL}" ]; then
    echo "SEND_EMAIL           : ${SEND_EMAIL}"
  fi
  if [ -n "${PUBLISH_IMAGE}" ]; then
    echo "PUBLISH_IMAGE        : ${PUBLISH_IMAGE}"
  fi
  if [ -n "${PUBLISH_API}" ]; then
    echo "PUBLISH_API          : ${PUBLISH_API}"
  fi
  if [ -n "${ALLOW_IMAGE_OVERRIDE}" ]; then
    echo "ALLOW_IMAGE_OVERRIDE : ${ALLOW_IMAGE_OVERRIDE}"
  fi
  echo "Server               : ${JENKINS_URL}"

  if [ ! -f "${TRIGGER}" ]; then
    echo "✗ trigger-pipeline.sh not found at ${TRIGGER}" >&2
    exit 1
  fi

  echo ""
  echo "==> Stage 2/3: Run"

  set -- "${PIPELINE}" --branch "${BRANCH}"

  if [ -n "${VERSION}" ]; then
    set -- "$@" --param "VERSION=${VERSION}"
  fi
  if [ -n "${SEND_EMAIL}" ]; then
    set -- "$@" --param "SEND_EMAIL=${SEND_EMAIL}"
  fi
  if [ -n "${PUBLISH_IMAGE}" ]; then
    set -- "$@" --param "PUBLISH_IMAGE=${PUBLISH_IMAGE}"
  fi
  if [ -n "${PUBLISH_API}" ]; then
    set -- "$@" --param "PUBLISH_API=${PUBLISH_API}"
  fi
  if [ -n "${ALLOW_IMAGE_OVERRIDE}" ]; then
    set -- "$@" --param "ALLOW_IMAGE_OVERRIDE=${ALLOW_IMAGE_OVERRIDE}"
  fi

  JENKINS_URL="${JENKINS_URL}" \
  JENKINS_USER="${JENKINS_USER}" \
  JENKINS_PASS="${JENKINS_TOKEN}" \
    "${TRIGGER}" "$@"

  echo ""
  echo "✓ Done"
}

main "$@"
