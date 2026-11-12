#!/bin/sh
# Gera a fórmula do Homebrew (binário pré-compilado) a partir do SHA256SUMS de uma release.
# Uso: scripts/render-homebrew-formula.sh <versão> <SHA256SUMS> > Formula/mosaic.rb
set -eu

REPOSITORY="flisbao/mosaic"

if [ "$#" -ne 2 ]; then
  echo "uso: $0 <versão> <SHA256SUMS>" >&2
  exit 2
fi
version=${1#v}
checksums=$2

if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "versão inválida: $1" >&2
  exit 1
fi
if [ ! -r "$checksums" ]; then
  echo "não consegui ler $checksums" >&2
  exit 1
fi

checksum_for() {
  target=$1
  archive="mosaic-v$version-$target.tar.gz"
  hash=$(awk -v name="$archive" '$2 == name { print $1 }' "$checksums" | head -n 1)
  if ! printf '%s' "$hash" | grep -Eq '^[0-9a-fA-F]{64}$'; then
    echo "SHA-256 de $archive ausente ou inválido em $checksums (alvo $target)" >&2
    exit 1
  fi
  printf '%s' "$hash" | tr 'A-F' 'a-f'
}

macos_arm=$(checksum_for aarch64-apple-darwin)
macos_intel=$(checksum_for x86_64-apple-darwin)
linux_arm=$(checksum_for aarch64-unknown-linux-musl)
linux_intel=$(checksum_for x86_64-unknown-linux-musl)

release_url="https://github.com/$REPOSITORY/releases/download/v$version"

asset_block() {
  printf '      url "%s/mosaic-v%s-%s.tar.gz"\n      sha256 "%s"\n' \
    "$release_url" "$version" "$1" "$2"
}

cat <<FORMULA
class Mosaic < Formula
  desc "Terminal dashboard built from pluggable widgets"
  homepage "https://github.com/$REPOSITORY"
  version "$version"
  license "MIT"

  on_macos do
    on_arm do
$(asset_block aarch64-apple-darwin "$macos_arm")
    end
    on_intel do
$(asset_block x86_64-apple-darwin "$macos_intel")
    end
  end

  on_linux do
    on_arm do
$(asset_block aarch64-unknown-linux-musl "$linux_arm")
    end
    on_intel do
$(asset_block x86_64-unknown-linux-musl "$linux_intel")
    end
  end

  def install
    bin.install "mosaic"
    doc.install "README.md", "PROTOCOL.md"
    pkgshare.install "schema"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mosaic --version")
  end
end
FORMULA
