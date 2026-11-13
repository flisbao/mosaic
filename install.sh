#!/bin/sh
# Installs Mosaic into ~/.local/bin (no sudo).
#   curl -fsSL https://raw.githubusercontent.com/flisbao/mosaic/main/install.sh | sh
#
# Variables:
#   MOSAIC_VERSION        version to install, for example v0.0.11 (default: the latest)
#   MOSAIC_INSTALL_DIR    destination folder (default: $HOME/.local/bin)
#   MOSAIC_DOWNLOAD_BASE  base URL of the releases (default: https://github.com/flisbao/mosaic/releases)
set -eu

DEFAULT_DOWNLOAD_BASE="https://github.com/flisbao/mosaic/releases"
EXECUTABLE_NAME="mosaic"
EXECUTABLE_MODE=755
VERSION_COMPONENT_COUNT=3
INTERRUPTED_EXIT_STATUS=130

fail() {
  printf 'install.sh: %s\n' "$1" >&2
  exit 1
}

say() {
  printf '%s\n' "$1"
}

has_command() {
  command -v "$1" >/dev/null 2>&1
}

detect_target() {
  system=$(uname -s)
  machine=$(uname -m)
  case "$system" in
    Linux)
      case "$machine" in
        x86_64 | amd64) target="x86_64-unknown-linux-musl" ;;
        aarch64 | arm64) target="aarch64-unknown-linux-musl" ;;
        *) fail "unsupported architecture: $machine (Linux x86_64 and aarch64)" ;;
      esac
      ;;
    Darwin)
      case "$machine" in
        arm64 | aarch64) target="aarch64-apple-darwin" ;;
        x86_64) target="x86_64-apple-darwin" ;;
        *) fail "unsupported architecture: $machine (macOS arm64 and x86_64)" ;;
      esac
      ;;
    *) fail "unsupported system: $system (Linux and macOS)" ;;
  esac
}

choose_downloader() {
  if has_command curl; then
    downloader=curl
  elif has_command wget; then
    downloader=wget
  else
    fail "curl or wget is required to download Mosaic"
  fi
}

curl_download() {
  if [ -z "${MOSAIC_DOWNLOAD_BASE:-}" ]; then
    curl --proto '=https' --tlsv1.2 "$@"
  else
    curl "$@"
  fi
}

download_file() {
  url=$1
  destination=$2
  if [ "$downloader" = curl ]; then
    curl_download -fsSL -o "$destination" "$url" || fail "could not download $url"
  else
    wget -q -O "$destination" "$url" || fail "could not download $url"
  fi
}

latest_release_location() {
  url="$download_base/latest"
  if [ "$downloader" = curl ]; then
    curl_download -fsSL -o /dev/null -w '%{url_effective}' "$url" ||
      fail "could not find the latest version at $url"
  else
    wget -q -S -O /dev/null --max-redirect=0 "$url" 2>&1 |
      sed -n 's/^ *[Ll]ocation: *//p' | tr -d '\r' | tail -n 1 || true
  fi
}

is_release_version() {
  case "$1" in
    v*) ;;
    *) return 1 ;;
  esac
  numbers=${1#v}
  case "$numbers" in
    '' | *[!0-9.]* | .* | *. | *..*) return 1 ;;
  esac
  previous_separator=$IFS
  IFS=.
  # shellcheck disable=SC2086
  set -- $numbers
  IFS=$previous_separator
  [ "$#" -eq "$VERSION_COMPONENT_COUNT" ]
}

resolve_version() {
  if [ -n "${MOSAIC_VERSION:-}" ]; then
    requested=$MOSAIC_VERSION
  else
    location=$(latest_release_location)
    requested=${location##*/}
  fi
  case "$requested" in
    v*) version=$requested ;;
    *) version="v$requested" ;;
  esac
  if ! is_release_version "$version"; then
    fail "invalid version: $requested (use something like v0.0.11)"
  fi
}

sha256_of() {
  if has_command sha256sum; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif has_command shasum; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  else
    fail "sha256sum or shasum is required to verify the download"
  fi
}

verify_checksum() {
  archive=$1
  checksum_file=$2
  expected=$(cut -d ' ' -f 1 "$checksum_file" | tr 'A-F' 'a-f')
  actual=$(sha256_of "$archive" | tr 'A-F' 'a-f')
  if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
    fail "SHA-256 checksum does not match the published one; nothing was installed"
  fi
}

cleanup() {
  if [ -n "${work_directory:-}" ]; then
    rm -rf "$work_directory"
  fi
  if [ -n "${staged_executable:-}" ]; then
    rm -f "$staged_executable"
  fi
}

install_executable() {
  source_executable=$1
  mkdir -p "$install_directory" || fail "could not create $install_directory"
  staged_executable="$install_directory/.$EXECUTABLE_NAME.new.$$"
  install -m "$EXECUTABLE_MODE" "$source_executable" "$staged_executable" ||
    fail "could not write to $install_directory"
  mv -f "$staged_executable" "$install_directory/$EXECUTABLE_NAME" ||
    fail "could not install into $install_directory"
  staged_executable=""
}

warn_if_outside_path() {
  case ":$PATH:" in
    *":$install_directory:"*) ;;
    *)
      say ""
      say "$install_directory is not in your PATH. Add this to your shell:"
      say "  export PATH=\"$install_directory:\$PATH\""
      ;;
  esac
}

main() {
  download_base=${MOSAIC_DOWNLOAD_BASE:-$DEFAULT_DOWNLOAD_BASE}
  download_base=${download_base%/}
  home_directory=${HOME:-}
  install_directory=${MOSAIC_INSTALL_DIR:-}
  if [ -z "$install_directory" ]; then
    [ -n "$home_directory" ] || fail "HOME is not set; set MOSAIC_INSTALL_DIR"
    install_directory="$home_directory/.local/bin"
  fi

  detect_target
  choose_downloader
  resolve_version

  package="$EXECUTABLE_NAME-$version-$target"
  archive_name="$package.tar.gz"
  say "Installing Mosaic $version ($target) into $install_directory"

  work_directory=$(mktemp -d) || fail "could not create a temporary folder"
  trap cleanup EXIT
  trap 'exit "$INTERRUPTED_EXIT_STATUS"' INT TERM HUP

  release_url="$download_base/download/$version"
  download_file "$release_url/$archive_name" "$work_directory/$archive_name"
  download_file "$release_url/$archive_name.sha256" "$work_directory/$archive_name.sha256"
  verify_checksum "$work_directory/$archive_name" "$work_directory/$archive_name.sha256"

  tar -xzf "$work_directory/$archive_name" -C "$work_directory" ||
    fail "could not extract $archive_name"
  extracted="$work_directory/$package/$EXECUTABLE_NAME"
  [ -f "$extracted" ] || fail "$archive_name does not contain $package/$EXECUTABLE_NAME"
  install_executable "$extracted"

  installed_version=$("$install_directory/$EXECUTABLE_NAME" --version 2>/dev/null || true)
  say "Installed: ${installed_version:-$install_directory/$EXECUTABLE_NAME}"
  warn_if_outside_path
  say ""
  say "To get started, run: $EXECUTABLE_NAME --help"
}

main
