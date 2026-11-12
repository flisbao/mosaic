#!/usr/bin/env python3
"""Plugin do mosaic: kanban em pastas, um arquivo .md por tarefa."""

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
LANGUAGE_VARIABLE = "MOSAIC_LANGUAGE"
LANGUAGE_ENGLISH = "en"
LANGUAGE_PORTUGUESE = "pt-BR"
PORTUGUESE_PREFIX = "pt"

MESSAGES = {
    LANGUAGE_PORTUGUESE: {
        "move_label": "Mover para {column}",
        "move_description": "Mover o cartão selecionado para {column}",
        "invalid_card_id": "identificador de cartão inválido: {item_id!r}",
        "unknown_column": "coluna desconhecida: {column!r}",
        "invalid_card_name_detailed": "nome de cartão inválido: {name!r}",
        "invalid_card_name": "nome de cartão inválido",
        "unknown_destination": "coluna de destino desconhecida: {destination!r}",
        "card_not_found": "cartão não encontrado: {item_id}",
        "already_in_column": "o cartão já está em {destination}",
        "outside_kanban_folder": "cartão fora da pasta do kanban",
        "name_already_taken": "já existe um cartão com esse nome em {destination}",
        "card_moved": "Cartão movido para {destination}",
        "error_prefix": "erro",
        "usage": "uso: kanban.py snapshot | action move (recebido: {arguments})",
    },
    LANGUAGE_ENGLISH: {
        "move_label": "Move to {column}",
        "move_description": "Move the selected card to {column}",
        "invalid_card_id": "invalid card identifier: {item_id!r}",
        "unknown_column": "unknown column: {column!r}",
        "invalid_card_name_detailed": "invalid card name: {name!r}",
        "invalid_card_name": "invalid card name",
        "unknown_destination": "unknown destination column: {destination!r}",
        "card_not_found": "card not found: {item_id}",
        "already_in_column": "the card is already in {destination}",
        "outside_kanban_folder": "card outside the kanban folder",
        "name_already_taken": "a card with that name already exists in {destination}",
        "card_moved": "Card moved to {destination}",
        "error_prefix": "error",
        "usage": "usage: kanban.py snapshot | action move (received: {arguments})",
    },
}


def resolve_language(environ):
    requested = environ.get(LANGUAGE_VARIABLE, "").strip().lower()
    if requested.startswith(PORTUGUESE_PREFIX):
        return LANGUAGE_PORTUGUESE
    return LANGUAGE_ENGLISH


def messages_for(language):
    return MESSAGES[language]


def kanban_root(environ):
    return Path(environ.get("KANBAN_DIR", DEFAULT_ROOT)).expanduser()


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


def build_snapshot(root, messages):
    cards_by_column = {column: list_cards(root, column) for column in COLUMNS}
    return {
        "summary": {
            "title": "Kanban",
            "blocks": [
                {
                    "type": "lines",
                    "lines": [
                        {"text": f"{column} {len(cards_by_column[column])}"}
                        for column in COLUMNS
                    ],
                }
            ],
        },
        "detail": [
            {
                "heading": column,
                "items": [card_item(column, card) for card in cards_by_column[column]],
            }
            for column in COLUMNS
        ],
        "actions": [
            {
                "id": "move",
                "label": messages["move_label"].format(column=column),
                "description": messages["move_description"].format(column=column),
                "applies_to": "item",
                "params": {"to": column},
            }
            for column in COLUMNS
        ],
    }


def parse_item_id(item_id, messages):
    parts = item_id.split("/")
    if len(parts) != 2:
        raise ValueError(messages["invalid_card_id"].format(item_id=item_id))
    column, name = parts
    if column not in COLUMNS:
        raise ValueError(messages["unknown_column"].format(column=column))
    if not name.endswith(CARD_SUFFIX) or name in (CARD_SUFFIX, ".", "..") or "\\" in name:
        raise ValueError(messages["invalid_card_name_detailed"].format(name=name))
    if "\x00" in name:
        raise ValueError(messages["invalid_card_name"])
    return column, name


def move_card(root, item_id, destination, messages):
    if destination not in COLUMNS:
        raise ValueError(messages["unknown_destination"].format(destination=destination))
    column, name = parse_item_id(item_id, messages)
    source = root / column / name
    if not source.is_file():
        raise ValueError(messages["card_not_found"].format(item_id=item_id))
    if column == destination:
        raise ValueError(messages["already_in_column"].format(destination=destination))
    root_resolved = root.resolve()
    if source.resolve().parent != (root_resolved / column):
        raise ValueError(messages["outside_kanban_folder"])
    target_folder = root / destination
    target_folder.mkdir(exist_ok=True)
    target = target_folder / name
    if target.exists():
        raise ValueError(messages["name_already_taken"].format(destination=destination))
    os.rename(source, target)


def main(arguments, environ=None):
    environ = os.environ if environ is None else environ
    messages = messages_for(resolve_language(environ))
    root = kanban_root(environ)
    if arguments == ["snapshot"]:
        print(json.dumps(build_snapshot(root, messages), ensure_ascii=False))
        return 0
    if arguments == ["action", "move"]:
        try:
            params = json.loads(sys.stdin.read() or "{}")
            move_card(root, str(params.get("item_id", "")), str(params.get("to", "")), messages)
        except (ValueError, OSError) as error:
            print(f"{messages['error_prefix']}: {error}", file=sys.stderr)
            return 1
        print(messages["card_moved"].format(destination=params["to"]))
        return 0
    print(messages["usage"].format(arguments=arguments), file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
