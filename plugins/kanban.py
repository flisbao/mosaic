#!/usr/bin/env python3
"""Plugin do panel-tui: kanban em pastas, um arquivo .md por tarefa."""

import json
import os
import sys
from pathlib import Path

COLUMNS = ["inbox", "backlog", "in progress", "acompanhar_jira", "done"]
CARD_SUFFIX = ".md"
HEADING_PREFIX = "# "
FRONT_MATTER_FENCE = "---"
DEFAULT_ROOT = "~/kanban"
META_KEYS = ("jira", "responsavel", "prioridade")


def kanban_root():
    return Path(os.environ.get("KANBAN_DIR", DEFAULT_ROOT)).expanduser()


def list_cards(root, column):
    folder = root / column
    if not folder.is_dir():
        return []
    return sorted(
        path for path in folder.iterdir() if path.is_file() and path.suffix == CARD_SUFFIX
    )


def strip_front_matter(lines):
    if not lines or lines[0].strip() != FRONT_MATTER_FENCE:
        return lines
    for position in range(1, len(lines)):
        if lines[position].strip() == FRONT_MATTER_FENCE:
            return lines[position + 1 :]
    return lines


def card_title(path):
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return path.stem
    for line in strip_front_matter(text.splitlines()):
        if line.startswith(HEADING_PREFIX):
            title = line[len(HEADING_PREFIX) :].strip()
            if title:
                return title
    return path.stem


def front_matter_fields(path):
    try:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return {}
    if not lines or lines[0].strip() != FRONT_MATTER_FENCE:
        return {}
    fields = {}
    for line in lines[1:]:
        if line.strip() == FRONT_MATTER_FENCE:
            return fields
        key, separator, value = line.partition(":")
        key, value = key.strip(), value.strip()
        if separator and key in META_KEYS and value:
            fields[key] = value
    return {}


def card_item(column, card):
    item = {"id": f"{column}/{card.name}", "text": card_title(card)}
    meta = front_matter_fields(card)
    if meta:
        item["meta"] = meta
    return item


def build_snapshot(root):
    cards_by_column = {column: list_cards(root, column) for column in COLUMNS}
    return {
        "summary": {
            "title": "Kanban",
            "lines": [
                {"text": f"{column} {len(cards_by_column[column])}"} for column in COLUMNS
            ],
        },
        "detail": {
            "sections": [
                {
                    "heading": column,
                    "items": [card_item(column, card) for card in cards_by_column[column]],
                }
                for column in COLUMNS
            ]
        },
        "actions": [
            {
                "id": "move",
                "label": f"Mover para {column}",
                "description": f"Mover o cartão selecionado para {column}",
                "applies_to": "item",
                "params": {"to": column},
            }
            for column in COLUMNS
        ],
    }


def parse_item_id(item_id):
    parts = item_id.split("/")
    if len(parts) != 2:
        raise ValueError(f"identificador de cartão inválido: {item_id!r}")
    column, name = parts
    if column not in COLUMNS:
        raise ValueError(f"coluna desconhecida: {column!r}")
    if not name.endswith(CARD_SUFFIX) or name in (CARD_SUFFIX, ".", "..") or "\\" in name:
        raise ValueError(f"nome de cartão inválido: {name!r}")
    if "\x00" in name:
        raise ValueError("nome de cartão inválido")
    return column, name


def move_card(root, item_id, destination):
    if destination not in COLUMNS:
        raise ValueError(f"coluna de destino desconhecida: {destination!r}")
    column, name = parse_item_id(item_id)
    source = root / column / name
    if not source.is_file():
        raise ValueError(f"cartão não encontrado: {item_id}")
    if column == destination:
        raise ValueError(f"o cartão já está em {destination}")
    root_resolved = root.resolve()
    if source.resolve().parent != (root_resolved / column):
        raise ValueError("cartão fora da pasta do kanban")
    target_folder = root / destination
    target_folder.mkdir(exist_ok=True)
    target = target_folder / name
    if target.exists():
        raise ValueError(f"já existe um cartão com esse nome em {destination}")
    os.rename(source, target)


def main(arguments):
    root = kanban_root()
    if arguments == ["snapshot"]:
        print(json.dumps(build_snapshot(root), ensure_ascii=False))
        return 0
    if arguments == ["action", "move"]:
        try:
            params = json.loads(sys.stdin.read() or "{}")
            move_card(root, str(params.get("item_id", "")), str(params.get("to", "")))
        except (ValueError, OSError) as error:
            print(f"erro: {error}", file=sys.stderr)
            return 1
        print(f"Cartão movido para {params['to']}")
        return 0
    print(f"uso: kanban.py snapshot | action move (recebido: {arguments})", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
