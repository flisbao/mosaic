#!/bin/sh
# Instala o Mosaic em ~/.local/bin (sem sudo).
#   curl -fsSL https://raw.githubusercontent.com/flisbao/mosaic/main/install.sh | sh
#
# Variáveis:
#   MOSAIC_VERSION        versão a instalar, por exemplo v0.0.11 (padrão: a mais recente)
#   MOSAIC_INSTALL_DIR    pasta de destino (padrão: $HOME/.local/bin)
#   MOSAIC_DOWNLOAD_BASE  URL base das releases (padrão: https://github.com/flisbao/mosaic/releases)
set -eu

DEFAULT_DOWNLOAD_BASE="https://github.com/flisbao/mosaic/releases"
EXECUTABLE_NAME="mosaic"
EXECUTABLE_MODE=755

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
        *) fail "arquitetura não suportada: $machine (Linux x86_64 e aarch64)" ;;
      esac
      ;;
    Darwin)
      case "$machine" in
        arm64 | aarch64) target="aarch64-apple-darwin" ;;
        x86_64) target="x86_64-apple-darwin" ;;
        *) fail "arquitetura não suportada: $machine (macOS arm64 e x86_64)" ;;
      esac
      ;;
    *) fail "sistema não suportado: $system (Linux e macOS)" ;;
  esac
}

choose_downloader() {
  if has_command curl; then
    downloader=curl
  elif has_command wget; then
    downloader=wget
  else
    fail "precisa de curl ou wget para baixar o Mosaic"
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
    curl_download -fsSL -o "$destination" "$url" || fail "não consegui baixar $url"
  else
    wget -q -O "$destination" "$url" || fail "não consegui baixar $url"
  fi
}

latest_release_location() {
  url="$download_base/latest"
  if [ "$downloader" = curl ]; then
    curl_download -fsSL -o /dev/null -w '%{url_effective}' "$url" ||
      fail "não consegui descobrir a versão mais recente em $url"
  else
    wget -q -S -O /dev/null --max-redirect=0 "$url" 2>&1 |
      sed -n 's/^ *[Ll]ocation: *//p' | tr -d '\r' | tail -n 1 || true
  fi
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
  if ! printf '%s' "$version" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
    fail "versão inválida: $requested (use algo como v0.0.11)"
  fi
}

sha256_of() {
  if has_command sha256sum; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif has_command shasum; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  else
    fail "precisa de sha256sum ou shasum para conferir o download"
  fi
}

verify_checksum() {
  archive=$1
  checksum_file=$2
  expected=$(cut -d ' ' -f 1 "$checksum_file" | tr 'A-F' 'a-f')
  actual=$(sha256_of "$archive" | tr 'A-F' 'a-f')
  if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
    fail "checksum SHA-256 diferente do publicado; nada foi instalado"
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
  mkdir -p "$install_directory" || fail "não consegui criar $install_directory"
  staged_executable="$install_directory/.$EXECUTABLE_NAME.new.$$"
  install -m "$EXECUTABLE_MODE" "$source_executable" "$staged_executable" ||
    fail "não consegui gravar em $install_directory"
  mv -f "$staged_executable" "$install_directory/$EXECUTABLE_NAME" ||
    fail "não consegui instalar em $install_directory"
  staged_executable=""
}

warn_if_outside_path() {
  case ":$PATH:" in
    *":$install_directory:"*) ;;
    *)
      say ""
      say "$install_directory não está no seu PATH. Acrescente ao seu shell:"
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
    [ -n "$home_directory" ] || fail "HOME não está definido; informe MOSAIC_INSTALL_DIR"
    install_directory="$home_directory/.local/bin"
  fi

  detect_target
  choose_downloader
  resolve_version

  package="$EXECUTABLE_NAME-$version-$target"
  archive_name="$package.tar.gz"
  say "Instalando o Mosaic $version ($target) em $install_directory"

  work_directory=$(mktemp -d) || fail "não consegui criar uma pasta temporária"
  trap cleanup EXIT INT TERM HUP

  release_url="$download_base/download/$version"
  download_file "$release_url/$archive_name" "$work_directory/$archive_name"
  download_file "$release_url/$archive_name.sha256" "$work_directory/$archive_name.sha256"
  verify_checksum "$work_directory/$archive_name" "$work_directory/$archive_name.sha256"

  tar -xzf "$work_directory/$archive_name" -C "$work_directory" ||
    fail "não consegui extrair $archive_name"
  extracted="$work_directory/$package/$EXECUTABLE_NAME"
  [ -f "$extracted" ] || fail "$archive_name não contém $package/$EXECUTABLE_NAME"
  install_executable "$extracted"

  installed_version=$("$install_directory/$EXECUTABLE_NAME" --version 2>/dev/null || true)
  say "Instalado: ${installed_version:-$install_directory/$EXECUTABLE_NAME}"
  warn_if_outside_path
  say ""
  say "Para começar, rode: $EXECUTABLE_NAME --help"
}

main
