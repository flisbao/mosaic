#!/usr/bin/env python3
"""Servidor falso da API de builds do Azure DevOps para testes de integração do mosaic.

Imprime a porta na primeira linha do stdout e atende até ser encerrado.
Variável: FAKE_PR_TOKEN define o token aceito (Bearer).
Rotas (organização "org"): /org/_apis/projects, /org/<projeto>/_apis/build/builds
e /org/<projeto>/_apis/build/builds/<id>/timeline, com os dados de pipeline_fixtures.
"""

import json
import os
import re
import sys
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from pipeline_fixtures import builds_of, raw_projects, raw_timeline

PROJECTS_PATH = "/org/_apis/projects"
BUILDS_PATH = re.compile(r"/org/([^/]+)/_apis/build/builds")
TIMELINE_PATH = re.compile(r"/org/([^/]+)/_apis/build/builds/(\d+)/timeline")


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if not self._authorized():
            return self._send(401, {"message": "unauthorized"})
        path = urllib.parse.unquote(urllib.parse.urlparse(self.path).path)
        if path == PROJECTS_PATH:
            return self._send(200, raw_projects())
        timeline = TIMELINE_PATH.fullmatch(path)
        if timeline:
            return self._send(200, raw_timeline(int(timeline.group(2))))
        builds = BUILDS_PATH.fullmatch(path)
        if builds:
            items = builds_of(builds.group(1))
            return self._send(200, {"count": len(items), "value": items})
        return self._send(404, {})

    def _authorized(self):
        return self.headers.get("Authorization", "") == f"Bearer {os.environ['FAKE_PR_TOKEN']}"

    def _send(self, status, payload):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *arguments):
        pass


def main():
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    print(server.server_address[1], flush=True)
    server.serve_forever()


if __name__ == "__main__":
    sys.exit(main())
