#!/usr/bin/env python3
"""Galeria do mosaic: um widget com todos os blocos do protocolo v1, a agenda completa no
detalhe (Enter) e um aviso por hora.

Não usa rede. Serve de modelo para escrever o seu widget; valide com
`mosaic check python3 plugins/gallery.py`.
"""

import json
import os
import sys
from datetime import datetime

PROTOCOL_VERSION = 1
ALERT_ID_FORMAT = "gallery-%Y%m%d%H"
LANGUAGE_VARIABLE = "MOSAIC_LANGUAGE"
LANGUAGE_ENGLISH = "en"
LANGUAGE_PORTUGUESE = "pt-BR"
PORTUGUESE_PREFIX = "pt"

MESSAGES = {
    LANGUAGE_PORTUGUESE: {
        "title": "Galeria",
        "agenda_daily": "Daily do time",
        "agenda_code_review": "Revisão de código",
        "agenda_now_tag": "agora",
        "agenda_one_on_one": "1:1 com a gestão",
        "agenda_room_tag": "sala 3",
        "agenda_remote_tag": "remoto",
        "agenda_detail_heading": "Agenda de hoje",
        "full_agenda": [
            "09:00 Daily do time",
            "10:30 Revisão de código",
            "12:00 Almoço",
            "14:00 1:1 com a gestão",
            "15:30 Planejamento da sprint",
            "16:30 Estudo de ownership em Rust",
            "17:30 Fechar o dia",
        ],
        "table_person": "Pessoa",
        "table_in_progress": "Em andamento",
        "table_state": "Estado",
        "state_ok": "ok",
        "state_stuck": "parado",
        "state_review": "revisão",
        "stat_meetings": "reuniões",
        "stat_free": "livre",
        "stat_tasks": "tarefas",
        "stat_hint": "dados de exemplo, sem rede",
        "line_all_good": "tudo em dia",
        "line_see_protocol": "veja PROTOCOL.md",
        "alert_title": "Aviso da galeria",
        "alert_body": "Um aviso novo por hora, só para demonstrar.",
        "snapshot_only": "a galeria só implementa 'snapshot'",
    },
    LANGUAGE_ENGLISH: {
        "title": "Gallery",
        "agenda_daily": "Team daily",
        "agenda_code_review": "Code review",
        "agenda_now_tag": "now",
        "agenda_one_on_one": "1:1 with management",
        "agenda_room_tag": "room 3",
        "agenda_remote_tag": "remote",
        "agenda_detail_heading": "Today's agenda",
        "full_agenda": [
            "09:00 Team daily",
            "10:30 Code review",
            "12:00 Lunch",
            "14:00 1:1 with management",
            "15:30 Sprint planning",
            "16:30 Rust ownership study",
            "17:30 Wrap up the day",
        ],
        "table_person": "Person",
        "table_in_progress": "In progress",
        "table_state": "State",
        "state_ok": "ok",
        "state_stuck": "stuck",
        "state_review": "review",
        "stat_meetings": "meetings",
        "stat_free": "free",
        "stat_tasks": "tasks",
        "stat_hint": "sample data, no network",
        "line_all_good": "all caught up",
        "line_see_protocol": "see PROTOCOL.md",
        "alert_title": "Gallery alert",
        "alert_body": "A new alert every hour, just for demonstration.",
        "snapshot_only": "the gallery only implements 'snapshot'",
    },
}


def resolve_language(environ):
    requested = environ.get(LANGUAGE_VARIABLE, "").strip().lower()
    if requested.startswith(PORTUGUESE_PREFIX):
        return LANGUAGE_PORTUGUESE
    return LANGUAGE_ENGLISH


def agenda_rows(messages):
    return [
        {"time": "09:00", "text": messages["agenda_daily"], "tone": "muted"},
        {
            "time": "10:30",
            "text": messages["agenda_code_review"],
            "tone": "current",
            "tags": [{"text": messages["agenda_now_tag"], "tone": "ok"}],
        },
        {
            "time": "14:00",
            "text": messages["agenda_one_on_one"],
            "flag": "updated",
            "tags": [
                {"text": messages["agenda_room_tag"]},
                {"text": messages["agenda_remote_tag"], "tone": "muted"},
            ],
        },
    ]


def agenda_detail(messages):
    return [
        {
            "heading": messages["agenda_detail_heading"],
            "items": [{"text": entry} for entry in messages["full_agenda"]],
        }
    ]


def team_table(messages):
    return {
        "type": "table",
        "columns": [
            {"title": messages["table_person"]},
            {"title": messages["table_in_progress"], "align": "right"},
            {"title": messages["table_state"]},
        ],
        "rows": [
            ["Ana", "3", {"text": messages["state_ok"], "tone": "ok"}],
            [
                "Bruno",
                "5",
                {"text": messages["state_stuck"], "tone": "danger", "flag": "attention"},
            ],
            ["Carla", "2", {"text": messages["state_review"], "tone": "warn"}],
        ],
    }


def build_snapshot(now, messages):
    return {
        "protocol": PROTOCOL_VERSION,
        "summary": {
            "title": messages["title"],
            "blocks": [
                {"type": "rows", "rows": agenda_rows(messages)},
                {
                    "type": "stat",
                    "items": [
                        {"value": "3", "label": messages["stat_meetings"], "tone": "warn"},
                        {"value": "2h", "label": messages["stat_free"], "tone": "ok"},
                        {"value": "7", "label": messages["stat_tasks"]},
                    ],
                    "hint": messages["stat_hint"],
                },
                team_table(messages),
                {
                    "type": "lines",
                    "lines": [
                        {"text": messages["line_all_good"], "tone": "ok"},
                        {"text": messages["line_see_protocol"], "tone": "muted"},
                    ],
                },
            ],
        },
        "detail": agenda_detail(messages),
        "alerts": [
            {
                "id": now.strftime(ALERT_ID_FORMAT),
                "title": messages["alert_title"],
                "body": messages["alert_body"],
                "level": "info",
            }
        ],
    }


def main(arguments, environ=None):
    environ = os.environ if environ is None else environ
    messages = MESSAGES[resolve_language(environ)]
    if arguments != ["snapshot"]:
        print(messages["snapshot_only"], file=sys.stderr)
        return 2
    print(json.dumps(build_snapshot(datetime.now(), messages), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
