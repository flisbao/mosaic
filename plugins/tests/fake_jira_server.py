#!/usr/bin/env python3
"""Servidor Jira falso para testes de integração do mosaic.

Imprime a porta na primeira linha do stdout e atende até ser encerrado.
Variáveis: FAKE_JIRA_EMAIL e FAKE_JIRA_TOKEN definem as credenciais aceitas.
"""

import base64
import json
import os
import re
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
                "resolutiondate": timestamp(resolved) if resolved else None,
            },
        }

    hours = lambda count: timedelta(hours=count)
    return [
        issue("VHPJ-1", "Ticket novo sem dono", TO_DO, None, now - hours(5), now - hours(5)),
        issue("VHPJ-2", "Corrigir login", IN_PROGRESS, ANA, now - hours(48), now - timedelta(minutes=30)),
        issue("VHPJ-3", "Ajustar relatório", IN_PROGRESS, BRUNO, now - hours(24), now - hours(6)),
        issue("VHPJ-4", "Migrar banco", DONE, ANA, now - hours(72), now - hours(24), now - hours(24)),
        issue("VHPJ-5", "Meu ticket em andamento", IN_PROGRESS, ME, now - hours(30), now - hours(2)),
        issue("VHPJ-6", "Meu ticket concluído", DONE, ME, now - hours(80), now - hours(48), now - hours(48)),
    ]


def changelog_for(issue):
    resolved = issue["fields"]["resolutiondate"]
    if not resolved:
        return {"changelog": {"histories": []}}
    finished = datetime.strptime(resolved, "%Y-%m-%dT%H:%M:%S.000+0000").replace(tzinfo=timezone.utc)
    started = finished - timedelta(hours=24)
    return {
        "changelog": {
            "histories": [
                {"created": timestamp(started), "items": [{"field": "status", "toString": "In Progress"}]},
                {"created": timestamp(finished), "items": [{"field": "status", "toString": "Done"}]},
            ]
        }
    }


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if not self._authorized():
            return self._send(401, {"message": "unauthorized"})
        parsed = urllib.parse.urlparse(self.path)
        params = dict(urllib.parse.parse_qsl(parsed.query))
        now = datetime.now(timezone.utc)
        issues = build_issues(now)
        if "currentUser()" in params.get("jql", ""):
            issues = [i for i in issues if (i["fields"]["assignee"] or {}).get("accountId") == ME["accountId"]]
        path = parsed.path
        if path == "/rest/api/3/search/jql":
            return self._send(200, {"issues": issues, "isLast": True})
        if re.fullmatch(r"/rest/agile/1\.0/board/\d+/issue", path):
            return self._send(200, {"startAt": 0, "maxResults": 100, "total": len(issues), "issues": issues})
        if path == "/rest/api/3/status":
            return self._send(200, [TO_DO | {"id": "1"}, IN_PROGRESS | {"id": "2"}, DONE | {"id": "3"}])
        if path == "/rest/api/3/myself":
            return self._send(200, ME)
        if path == "/rest/api/3/user/search":
            return self._send(200, [])
        match = re.fullmatch(r"/rest/api/3/issue/([A-Z]+-\d+)", path)
        if match:
            found = next((i for i in build_issues(now) if i["key"] == match.group(1)), None)
            return self._send(200, changelog_for(found)) if found else self._send(404, {})
        return self._send(404, {})

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
