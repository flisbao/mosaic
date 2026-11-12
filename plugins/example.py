#!/usr/bin/env python3
"""Plugin de exemplo em Python: imprime um snapshot JSON no stdout."""

import json
import os
import sys

LANGUAGE_VARIABLE = "MOSAIC_LANGUAGE"
LANGUAGE_ENGLISH = "en"
LANGUAGE_PORTUGUESE = "pt-BR"
PORTUGUESE_PREFIX = "pt"

MESSAGES = {
    LANGUAGE_PORTUGUESE: {
        "title": "Exemplo Python",
        "greeting": "olá do plugin em Python",
        "replace_hint": "troque este script pelo seu",
        "snapshot_only": "este exemplo só implementa 'snapshot'",
    },
    LANGUAGE_ENGLISH: {
        "title": "Python example",
        "greeting": "hello from the Python plugin",
        "replace_hint": "replace this script with yours",
        "snapshot_only": "this example only implements 'snapshot'",
    },
}


def resolve_language(environ):
    requested = environ.get(LANGUAGE_VARIABLE, "").strip().lower()
    if requested.startswith(PORTUGUESE_PREFIX):
        return LANGUAGE_PORTUGUESE
    return LANGUAGE_ENGLISH


def build_snapshot(messages):
    return {
        "summary": {
            "title": messages["title"],
            "blocks": [
                {
                    "type": "lines",
                    "lines": [
                        {"text": messages["greeting"], "flag": "updated"},
                        {"text": messages["replace_hint"]},
                    ],
                }
            ],
        }
    }


def main(arguments, environ=None):
    environ = os.environ if environ is None else environ
    messages = MESSAGES[resolve_language(environ)]
    if arguments != ["snapshot"]:
        print(messages["snapshot_only"], file=sys.stderr)
        return 2
    print(json.dumps(build_snapshot(messages), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
