#!/usr/bin/env python3
"""Plugin de exemplo em Python: imprime um snapshot JSON no stdout."""

import json
import sys


def main(arguments):
    if arguments != ["snapshot"]:
        print("este exemplo só implementa 'snapshot'", file=sys.stderr)
        return 2
    snapshot = {
        "summary": {
            "title": "Exemplo Python",
            "lines": [
                {"text": "olá do plugin em Python", "flag": "updated"},
                {"text": "troque este script pelo seu"},
            ],
        }
    }
    print(json.dumps(snapshot, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
