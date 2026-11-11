#!/usr/bin/env bash
# Monta a árvore de arquivos de um pacote do Mosaic (sem DEBIAN e sem o binário).
# Uso: scripts/stage-package.sh core <destino>
#   core:    documentação e a skill do protocolo (mosaic-widget-authoring)
# MOSAIC_REPO aponta para outra raiz de repositório (usado nos testes).
set -euo pipefail

USAGE="uso: $0 core <destino>"
if [ "$#" -ne 2 ]; then
  echo "$USAGE" >&2
  exit 2
fi
PACKAGE=$1
DESTINATION=$2
REPO=${MOSAIC_REPO:-$(cd "$(dirname "$0")/.." && pwd)}
SKILLS_SOURCE="$REPO/.claude/skills"
AUTHORING_SKILL="mosaic-widget-authoring"
SKILLS_DESTINATION="$DESTINATION/usr/share/mosaic/skills"

stage_core() {
  local documents="$DESTINATION/usr/share/doc/mosaic"
  install -Dm644 "$REPO/README.md" "$documents/README.md"
  install -Dm644 "$REPO/PROTOCOL.md" "$documents/PROTOCOL.md"
  install -Dm644 "$REPO/schema/protocol.schema.json" "$documents/protocol.schema.json"
  install -Dm644 "$REPO/LICENSE" "$documents/copyright"
  mkdir -p "$SKILLS_DESTINATION"
  cp -R "$SKILLS_SOURCE/$AUTHORING_SKILL" "$SKILLS_DESTINATION/"
}

normalize_permissions() {
  chmod -R u=rwX,go=rX "$DESTINATION"
}

case "$PACKAGE" in
  core) stage_core ;;
  *)
    echo "$USAGE" >&2
    exit 2
    ;;
esac
normalize_permissions
