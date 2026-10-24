#!/usr/bin/env python3
"""Servidor falso das APIs de pull requests para testes de integração do mosaic.

Imprime a porta na primeira linha do stdout e atende até ser encerrado.
Variável: FAKE_PR_TOKEN define o token aceito (Bearer no GitHub, GitLab e
Azure DevOps).
"""

import json
import os
import sys
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from pull_request_fixtures import (
    azure_connection_data,
    azure_mine_page,
    azure_review_page,
    github_response,
    gitlab_response,
)

GITHUB_GRAPHQL_PATH = "/graphql"
GITLAB_GRAPHQL_PATH = "/api/graphql"
AZURE_CONNECTION_DATA_PATH = "/org/_apis/connectionData"
AZURE_PULL_REQUESTS_PATH = "/org/_apis/git/pullrequests"
REVIEWER_CRITERION = "searchCriteria.reviewerId"
CREATOR_CRITERION = "searchCriteria.creatorId"


def azure_wrap(pages):
    return {"value": pages}


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        if not self._authorized():
            return self._send(401, {"message": "unauthorized"})
        self._read_body()
        path = urllib.parse.urlparse(self.path).path
        if path == GITHUB_GRAPHQL_PATH:
            return self._send(200, github_response())
        if path == GITLAB_GRAPHQL_PATH:
            return self._send(200, gitlab_response())
        return self._send(404, {})

    def do_GET(self):
        if not self._authorized():
            return self._send(401, {"message": "unauthorized"})
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == AZURE_CONNECTION_DATA_PATH:
            return self._send(200, azure_connection_data())
        if parsed.path == AZURE_PULL_REQUESTS_PATH:
            return self._send(200, self._azure_pull_requests(parsed.query))
        return self._send(404, {})

    def _azure_pull_requests(self, query):
        criteria = urllib.parse.parse_qs(query)
        if REVIEWER_CRITERION in criteria:
            return azure_wrap(azure_review_page())
        if CREATOR_CRITERION in criteria:
            return azure_wrap(azure_mine_page())
        return azure_wrap([])

    def _read_body(self):
        length = int(self.headers.get("Content-Length", "0") or 0)
        if length:
            self.rfile.read(length)

    def _authorized(self):
        token = os.environ["FAKE_PR_TOKEN"]
        header = self.headers.get("Authorization", "")
        return header == f"Bearer {token}"

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
