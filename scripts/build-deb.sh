#!/usr/bin/env bash
# Monta o .deb do mosaic (amd64) a partir de um binário já compilado.
# Uso: scripts/build-deb.sh <versão> <binário> <pasta-de-saída>
set -euo pipefail
if [ "$#" -ne 3 ]; then
  echo "uso: $0 <versão> <binário> <pasta-de-saída>" >&2
  exit 2
fi
VERSION=${1#v}
BINARY=$2
OUTPUT=$3
REPO=$(cd "$(dirname "$0")/.." && pwd)
ARCHITECTURE=amd64

MINIMUM_GLIBC=$(objdump -T "$BINARY" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -V | tail -1)
ROOT=$(mktemp -d)
trap 'rm -rf "$ROOT"' EXIT
chmod 755 "$ROOT"

install -Dm755 "$BINARY" "$ROOT/usr/bin/mosaic"
"$REPO/scripts/stage-package.sh" core "$ROOT"

mkdir -p "$ROOT/DEBIAN"
cat > "$ROOT/DEBIAN/control" <<CONTROL
Package: mosaic
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCHITECTURE
Depends: libc6 (>= ${MINIMUM_GLIBC:-2.17})
Recommends: python3
Suggests: mosaic-plugins
Maintainer: Lisbao <flisbao@gmail.com>
Homepage: https://github.com/flisbao/mosaic
Description: Terminal dashboard built from pluggable widgets
 Qualquer programa que imprime JSON vira um widget do painel.
 Os widgets de exemplo (kanban, example, gallery) são instalados em
 ~/.config/mosaic na primeira execução. Os widgets de serviço (Jira,
 GitHub, GitLab, Azure DevOps, agenda) vêm no pacote mosaic-plugins:
 rode "mosaic plugins install" para copiá-los.
 A skill do protocolo fica em /usr/share/mosaic/skills;
 rode "mosaic skills install" para ligá-la ao seu ~/.claude/skills.
CONTROL

mkdir -p "$OUTPUT"
FILE="$OUTPUT/mosaic_${VERSION}_${ARCHITECTURE}.deb"
dpkg-deb --root-owner-group --build "$ROOT" "$FILE" >/dev/null
echo "$FILE"
