#!/bin/sh
#
# Clone or update all SES Bitbucket repositories to a local directory.
#
# Usage:
#   TARGET_DIR=/tmp/example-org/repos REF=release/26.4.1.0 ./script.sh
#
# Parameters (passed as environment variables by dops):
#   TARGET_DIR  - Local directory to clone repos into (default: /tmp/example-org/repos)
#   REF         - Branch, tag, or ref to checkout after cloning (optional)
#
set -eu

TARGET_DIR="${TARGET_DIR:-/tmp/example-org/repos}"
REF="${REF:-}"
BASE_URL="git@git.example.com:example-org"

REPOS="
  cc4-aaa
  cc4-admin
  cc4-biz
  cc4-cam
  cc4-cash
  cc4-cash-ops
  cc4-cms
  cc4-device
  cc4-device-fm
  cc4-device-rm
  cc4-edge
  cc4-export
  cc4-hall
  cc4-monitoring
  cc4-prestage
  cc4-ras
  cc4-reports
  cc4-retail
  cc4-system
"

failed=""
skipped=""
total=0
cloned=0
pulled=0

check_dependency() {
  if ! command -v "$1" > /dev/null 2>&1; then
    echo "Error: ${1} is not installed" >&2
    exit 1
  fi
}

main() {
  check_dependency "git"

  echo "==> Stage 1/2: Prepare"
  echo "    Target: ${TARGET_DIR}"
  if [ -n "${REF}" ]; then
    echo "    Ref:    ${REF}"
  fi
  mkdir -p "${TARGET_DIR}"
  echo "    Directory ready."

  echo ""
  echo "==> Stage 2/2: Clone / Update"

  for repo in ${REPOS}; do
    total=$((total + 1))
    dest="${TARGET_DIR}/${repo}"

    if [ -d "${dest}/.git" ]; then
      printf "    Pulling  %s...\n" "${repo}"
      if ! git -C "${dest}" fetch > /dev/null 2>&1; then
        echo "    ✗ Fetch failed: ${repo}" >&2
        failed="${failed} ${repo}"
        continue
      fi
      if ! git -C "${dest}" pull --ff-only > /dev/null 2>&1; then
        echo "    ✗ Pull failed: ${repo}" >&2
        failed="${failed} ${repo}"
        continue
      fi
      pulled=$((pulled + 1))
    else
      printf "    Cloning  %s...\n" "${repo}"
      if ! git clone "${BASE_URL}/${repo}.git" "${dest}" > /dev/null 2>&1; then
        echo "    ✗ Clone failed: ${repo}" >&2
        failed="${failed} ${repo}"
        continue
      fi
      cloned=$((cloned + 1))
    fi

    if [ -n "${REF}" ]; then
      if git -C "${dest}" checkout "${REF}" > /dev/null 2>&1; then
        printf "    ✓ Checked out %s @ %s\n" "${repo}" "${REF}"
      else
        printf "    ! Ref '%s' not found in %s - skipping\n" "${REF}" "${repo}" >&2
        skipped="${skipped} ${repo}"
      fi
    fi
  done

  echo ""
  echo "========================================="
  echo "  Summary"
  echo "========================================="
  printf "  Total:   %d\n" "${total}"
  printf "  Cloned:  %d\n" "${cloned}"
  printf "  Updated: %d\n" "${pulled}"
  if [ -n "${skipped}" ]; then
    echo "  Ref not found (skipped):${skipped}"
  fi
  if [ -n "${failed}" ]; then
    echo "  Failed:${failed}"
    echo "  Status:  PARTIAL"
    echo "========================================="
    exit 1
  fi
  echo "  Status:  SUCCESS"
  echo "========================================="
}

main "$@"
