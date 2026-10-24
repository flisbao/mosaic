#!/usr/bin/env bash
# Monta o .deb do pacote mosaic-plugins (todas as arquiteturas).
# Uso: scripts/build-plugins-deb.sh <versão> <pasta-de-saída>
set -euo pipefail
if [ "$#" -ne 2 ]; then
  echo "uso: $0 <versão> <pasta-de-saída>" >&2
  exit 2
fi
VERSION=${1#plugins-v}
VERSION=${VERSION#v}
OUTPUT=$2
REPO=$(cd "$(dirname "$0")/.." && pwd)
ARCHITECTURE=all

ROOT=$(mktemp -d)
trap 'rm -rf "$ROOT"' EXIT
chmod 755 "$ROOT"

"$REPO/scripts/stage-package.sh" plugins "$ROOT"

mkdir -p "$ROOT/DEBIAN"
cat > "$ROOT/DEBIAN/control" <<CONTROL
Package: mosaic-plugins
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCHITECTURE
Depends: mosaic
Recommends: python3
Maintainer: Lisbao <flisbao@gmail.com>
Homepage: https://github.com/flisbao/mosaic
Description: Service widgets for the Mosaic terminal dashboard
 Widgets de Jira, GitHub, GitLab, Azure DevOps, Azure Pipelines e agenda,
 com as skills do Claude Code de cada um. Rode "mosaic plugins install"
 para copiá-los para ~/.config/mosaic/plugins e "mosaic skills install"
 para ligar as skills ao seu ~/.claude/skills.
CONTROL

mkdir -p "$OUTPUT"
FILE="$OUTPUT/mosaic-plugins_${VERSION}_${ARCHITECTURE}.deb"
dpkg-deb --root-owner-group --build "$ROOT" "$FILE" >/dev/null
echo "$FILE"
