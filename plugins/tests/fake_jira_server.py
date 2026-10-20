#!/usr/bin/env python3
"""Servidor Jira falso para testes de integração do mosaic.

Imprime a porta na primeira linha do stdout e atende até ser encerrado.
Variáveis: FAKE_JIRA_EMAIL e FAKE_JIRA_TOKEN definem as credenciais aceitas.
"""

import base64
import json
import os
import sys
import urllib.parse
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ME = {"accountId": "acc-me", "displayName": "Eu"}
ANA = {"accountId": "acc-ana", "displayName": "Ana Souza"}
BRUNO = {"accountId": "acc-bruno", "displayName": "Bruno Lima"}
TO_DO = {"name": "To Do", "statusCategory": {"key": "new"}}
IN_PROGRESS = {"name": "In Progress", "statusCategory": {"key": "indeterminate"}}
DONE = {"name": "Done", "statusCategory": {"key": "done"}}


def timestamp(moment):
    return moment.strftime("%Y-%m-%dT%H:%M:%S.000+0000")


def build_issues(now):
    def issue(key, summary, status, assignee, created, updated, resolved=None):
        return {
            "key": key,
            "fields": {
                "summary": summary,
                "status": status,
                "assignee": assignee,
                "creator": ANA,
                "priority": {"name": "High"},
                "created": timestamp(created),
                "updated": timestamp(updated),
            },
        }

    hours = lambda count: timedelta(hours=count)
    return [
        issue("VHPJ-1", "Ticket novo sem dono", TO_DO, None, now - hours(5), now - hours(5)),
        issue("VHPJ-2", "Corrigir login", IN_PROGRESS, ANA, now - hours(48), now - timedelta(minutes=30)),
        issue("VHPJ-3", "Ajustar relatório", IN_PROGRESS, BRUNO, now - hours(24), now - hours(6)),
        issue("VHPJ-4", "Migrar banco", DONE, ANA, now - hours(72), now - hours(30), now - hours(30)),
        issue("VHPJ-5", "Meu ticket em andamento", IN_PROGRESS, ME, now - hours(30), now - hours(2)),
        issue("VHPJ-6", "Meu ticket concluído", DONE, ME, now - hours(80), now - hours(48), now - hours(48)),
    ]


SEARCH_PATH = "/rest/api/3/search/jql"
DEFAULT_PAGE_SIZE = 50
FAILURE_STATUS_BY_MARKER = {"FAIL429": 429, "FAIL500": 500}
INVALID_JQL_MARKER = "FAIL400"
INVALID_JQL_MESSAGE = "Erro de sintaxe no JQL"


def matching_issues(issues, jql):
    if "currentUser()" in jql:
        return [i for i in issues if (i["fields"]["assignee"] or {}).get("accountId") == ME["accountId"]]
    return issues


def parse_offset(token):
    return int(token) if token and token.isdigit() else 0


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if not self._authorized():
            return self._send(401, {"message": "unauthorized"})
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path != SEARCH_PATH:
            return self._send(404, {})
        params = dict(urllib.parse.parse_qsl(parsed.query))
        jql = params.get("jql", "")
        if INVALID_JQL_MARKER in jql:
            return self._send(400, {"errorMessages": [INVALID_JQL_MESSAGE]})
        for marker, status in FAILURE_STATUS_BY_MARKER.items():
            if marker in jql:
                return self._send(status, {})
        issues = matching_issues(build_issues(datetime.now(timezone.utc)), jql)
        return self._send(200, self._page(issues, params))

    def _page(self, issues, params):
        offset = parse_offset(params.get("nextPageToken"))
        page_size = int(params.get("maxResults", DEFAULT_PAGE_SIZE))
        end = offset + page_size
        page = {"issues": issues[offset:end], "isLast": end >= len(issues)}
        if not page["isLast"]:
            page["nextPageToken"] = str(end)
        return page

    def _authorized(self):
        expected = f"{os.environ['FAKE_JIRA_EMAIL']}:{os.environ['FAKE_JIRA_TOKEN']}".encode()
        return self.headers.get("Authorization") == "Basic " + base64.b64encode(expected).decode()

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
