#!/usr/bin/env bash
# Monta a árvore de arquivos de um pacote do Mosaic (sem DEBIAN e sem o binário).
# Uso: scripts/stage-package.sh <core|plugins> <destino>
#   core:    documentação e a skill do protocolo (mosaic-widget-authoring)
#   plugins: os plugins de serviço (pastas de plugins/ que têm SKILL.md), cada um com o
#            código, o README, a skill (SKILL.md) e o roteiro do mosaic plugins setup (setup.toml)
# MOSAIC_REPO aponta para outra raiz de repositório (usado nos testes).
set -euo pipefail
shopt -s nullglob

USAGE="uso: $0 <core|plugins> <destino>"
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
PLUGINS_DESTINATION="$DESTINATION/usr/share/mosaic/plugins"

stage_core() {
  local documents="$DESTINATION/usr/share/doc/mosaic"
  install -Dm644 "$REPO/README.md" "$documents/README.md"
  install -Dm644 "$REPO/PROTOCOL.md" "$documents/PROTOCOL.md"
  install -Dm644 "$REPO/schema/protocol.schema.json" "$documents/protocol.schema.json"
  install -Dm644 "$REPO/LICENSE" "$documents/copyright"
  mkdir -p "$SKILLS_DESTINATION"
  cp -R "$SKILLS_SOURCE/$AUTHORING_SKILL" "$SKILLS_DESTINATION/"
}

service_plugins() {
  local folder name
  for folder in "$REPO"/plugins/*/; do
    name=$(basename "$folder")
    [ -f "$folder/SKILL.md" ] && echo "$name"
  done
}

copy_plugin() {
  local name=$1 file
  mkdir -p "$PLUGINS_DESTINATION/$name"
  for file in "$REPO/plugins/$name"/*.py "$REPO/plugins/$name"/*.js "$REPO/plugins/$name/README.md" "$REPO/plugins/$name/SKILL.md" "$REPO/plugins/$name/setup.toml"; do
    if [ -f "$file" ] && [ ! -L "$file" ]; then
      cp -p "$file" "$PLUGINS_DESTINATION/$name/"
    fi
  done
}

stage_plugins() {
  local name documents="$DESTINATION/usr/share/doc/mosaic-plugins"
  for name in $(service_plugins); do
    copy_plugin "$name"
  done
  install -Dm644 "$REPO/plugins/VERSION" "$documents/VERSION"
  install -Dm644 "$REPO/LICENSE" "$documents/copyright"
}

normalize_permissions() {
  chmod -R u=rwX,go=rX "$DESTINATION"
}

case "$PACKAGE" in
  core) stage_core ;;
  plugins) stage_plugins ;;
  *)
    echo "$USAGE" >&2
    exit 2
    ;;
esac
normalize_permissions
