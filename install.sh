#!/bin/sh
# Kata install one-liner (docs/design/05-prd.md §7.5, 13-kata-rename.md).
#
#   curl -fsSL https://raw.githubusercontent.com/masonhuemmer/kata/main/install.sh | sh
#
# POSIX sh, no bashisms (CI runs this under both dash and bash-in-POSIX-mode, per §9
# "Cross-cutting"). Detects OS/arch, downloads the release tarball and its SHA256SUMS,
# verifies the tarball's checksum before touching disk, then installs the single `kata`
# binary. Idempotent: only the binary is replaced -- config, vault, and catalog under
# KATA_INSTALL_DIR's sibling XDG dirs are never touched.
#
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
  version="${KATA_VERSION:-${KADOU_VERSION:-latest}}"
  base_url="${KATA_INSTALL_BASE_URL:-${KADOU_INSTALL_BASE_URL:-https://github.com/masonhuemmer/kata/releases/download}}"
  install_dir="${KATA_INSTALL_DIR:-${KADOU_INSTALL_DIR:-$HOME/.local/bin}}"
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

  if [ "$version" = latest ]; then
    latest_url=$(curl -fsSL -o /dev/null -w '%{url_effective}' https://github.com/masonhuemmer/kata/releases/latest)
    version=${latest_url##*/}
  fi
  version=${version#v}
  case "$version" in
    ''|*[!0-9A-Za-z.-]*) echo "install.sh: invalid release version" >&2; exit 2 ;;
  esac
  release_path=$version
  case "$base_url" in
    https://github.com/masonhuemmer/kata/releases/download) release_path=v$version ;;
  esac

  os=$(uname -s | tr '[:upper:]' '[:lower:]')
  arch=$(uname -m)
  asset="kata-${version}-${os}-${arch}.tar.gz"
  asset_url="${base_url%/}/${release_path}/${asset}"
  sums_url="${base_url%/}/${release_path}/SHA256SUMS"

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
    echo "kata ${version} → ${install_dir}/kata   sha256 ok (dry run)"
    return 0
  fi

  tar -xzf "$work_dir/asset.tar.gz" -C "$work_dir"
  mkdir -p "$install_dir"
  mv "$work_dir/kata" "$install_dir/kata"
  chmod +x "$install_dir/kata"

  echo "kata ${version} → ${install_dir}/kata   sha256 ok"
  echo "  next:  kata            open the library"
  echo "         kata mcp serve  for agents"
}

main "$@"
