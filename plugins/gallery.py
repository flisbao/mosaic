#!/usr/bin/env python3
"""Galeria do mosaic: um widget com todos os blocos do protocolo v1 e um aviso por hora.

Não usa rede. Serve de modelo para escrever o seu widget; valide com
`mosaic check python3 plugins/gallery.py`.
"""

import json
import sys
from datetime import datetime

PROTOCOL_VERSION = 1
ALERT_ID_FORMAT = "gallery-%Y%m%d%H"


def agenda_rows():
    return [
        {"time": "09:00", "text": "Daily do time", "tone": "muted"},
        {
            "time": "10:30",
            "text": "Revisão de código",
            "tone": "current",
            "tags": [{"text": "agora", "tone": "ok"}],
        },
        {
            "time": "14:00",
            "text": "1:1 com a gestão",
            "flag": "updated",
            "tags": [{"text": "sala 3"}, {"text": "remoto", "tone": "muted"}],
        },
    ]


def team_table():
    return {
        "type": "table",
        "columns": [
            {"title": "Pessoa"},
            {"title": "Em andamento", "align": "right"},
            {"title": "Estado"},
        ],
        "rows": [
            ["Ana", "3", {"text": "ok", "tone": "ok"}],
            ["Bruno", "5", {"text": "parado", "tone": "danger", "flag": "attention"}],
            ["Carla", "2", {"text": "revisão", "tone": "warn"}],
        ],
    }


def build_snapshot(now):
    return {
        "protocol": PROTOCOL_VERSION,
        "summary": {
            "title": "Galeria",
            "blocks": [
                {"type": "rows", "rows": agenda_rows()},
                {
                    "type": "stat",
                    "items": [
                        {"value": "3", "label": "reuniões", "tone": "warn"},
                        {"value": "2h", "label": "livre", "tone": "ok"},
                        {"value": "7", "label": "tarefas"},
                    ],
                    "hint": "dados de exemplo, sem rede",
                },
                team_table(),
                {
                    "type": "lines",
                    "lines": [
                        {"text": "tudo em dia", "tone": "ok"},
                        {"text": "veja PROTOCOL.md", "tone": "muted"},
                    ],
                },
            ],
        },
        "alerts": [
            {
                "id": now.strftime(ALERT_ID_FORMAT),
                "title": "Aviso da galeria",
                "body": "Um aviso novo por hora, só para demonstrar.",
                "level": "info",
            }
        ],
    }


def main(arguments):
    if arguments != ["snapshot"]:
        print("a galeria só implementa 'snapshot'", file=sys.stderr)
        return 2
    print(json.dumps(build_snapshot(datetime.now()), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
