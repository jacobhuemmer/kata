#!/bin/sh
# kadou install one-liner (docs/design/05-prd.md §7.5).
#
#   curl -fsSL https://<stable-install-url>/install.sh | sh
#
# POSIX sh, no bashisms (CI runs this under both dash and bash-in-POSIX-mode, per §9
# "Cross-cutting"). Detects OS/arch, downloads the release tarball and its SHA256SUMS,
# verifies the tarball's checksum before touching disk, then installs the single `kadou`
# binary. Idempotent: only the binary is replaced -- config, vault, and kata folders under
# KADOU_INSTALL_DIR's sibling XDG dirs are never touched.
#
# Stable URL and GitHub release publishing are gated (repo README Gates) -- this script
# specifies the shape only; KADOU_INSTALL_BASE_URL must be set to a real (or, in tests, a
# `file://`) location.
set -eu

# `file://` is the only scheme every test in this tree uses (no real network, per the repo's
# no-network test gate); anything else goes through curl.
fetch() {
  url="$1"
  dest="$2"
  case "$url" in
    file://*)
      cp "${url#file://}" "$dest"
      ;;
    *)
      curl -fsSL "$url" -o "$dest"
      ;;
  esac
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{ print $1 }'
  else
    echo "install.sh: need sha256sum or shasum on PATH to verify the download" >&2
    exit 1
  fi
}

main() {
  version="${KADOU_VERSION:-latest}"
  base_url="${KADOU_INSTALL_BASE_URL:-https://REPLACE-WITH-STABLE-INSTALL-URL}"
  install_dir="${KADOU_INSTALL_DIR:-$HOME/.local/bin}"
  dry_run=0

  for arg in "$@"; do
    case "$arg" in
      --dry-run)
        dry_run=1
        ;;
      *)
        echo "install.sh: unknown argument: $arg" >&2
        exit 2
        ;;
    esac
  done

  os=$(uname -s | tr '[:upper:]' '[:lower:]')
  arch=$(uname -m)
  asset="kadou-${version}-${os}-${arch}.tar.gz"
  asset_url="${base_url%/}/${version}/${asset}"
  sums_url="${base_url%/}/${version}/SHA256SUMS"

  work_dir=$(mktemp -d)
  trap 'rm -rf "$work_dir"' EXIT INT TERM

  fetch "$asset_url" "$work_dir/asset.tar.gz"
  fetch "$sums_url" "$work_dir/SHA256SUMS"

  expected_sum=$(awk -v name="$asset" '$2 == name { print $1 }' "$work_dir/SHA256SUMS")
  if [ -z "$expected_sum" ]; then
    echo "install.sh: no checksum entry for ${asset} in SHA256SUMS" >&2
    exit 1
  fi

  actual_sum=$(sha256_of "$work_dir/asset.tar.gz")
  if [ "$actual_sum" != "$expected_sum" ]; then
    echo "install.sh: checksum mismatch for ${asset}" >&2
    echo "  expected sha256:${expected_sum}" >&2
    echo "  actual   sha256:${actual_sum}" >&2
    exit 1
  fi

  if [ "$dry_run" -eq 1 ]; then
    echo "kadou ${version} → ${install_dir}/kadou   sha256 ok (dry run)"
    return 0
  fi

  tar -xzf "$work_dir/asset.tar.gz" -C "$work_dir"
  mkdir -p "$install_dir"
  mv "$work_dir/kadou" "$install_dir/kadou"
  chmod +x "$install_dir/kadou"

  echo "kadou ${version} → ${install_dir}/kadou   sha256 ok"
  echo "  next:  kadou            open the library"
  echo "         kadou mcp serve  for agents"
}

main "$@"
