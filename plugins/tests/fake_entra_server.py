#!/usr/bin/env python3
"""Servidor falso do Microsoft Entra ID e do IMDS para testes de integração do mosaic.

Imprime a porta na primeira linha do stdout e atende até ser encerrado.
Variáveis: FAKE_PR_TOKEN é o token de acesso emitido; FAKE_ENTRA_SECRET é o
segredo aceito no fluxo client_credentials.
Rotas: POST /<tenant>/oauth2/v2.0/devicecode, POST /<tenant>/oauth2/v2.0/token
e GET /metadata/identity/oauth2/token (exige o cabeçalho Metadata: true).
"""

import json
import os
import sys
import threading
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DEVICE_CODE = "fake-device-code"
REFRESH_TOKEN = "fake-refresh-token"
DEVICE_CODE_PATH_SUFFIX = "/oauth2/v2.0/devicecode"
TOKEN_PATH_SUFFIX = "/oauth2/v2.0/token"
IMDS_PATH = "/metadata/identity/oauth2/token"
DEVICE_CODE_GRANT = "urn:ietf:params:oauth:grant-type:device_code"
POLL_INTERVAL_SECONDS = 5
DEVICE_CODE_LIFETIME_SECONDS = 900
TOKEN_LIFETIME_SECONDS = 3600
PENDING_POLLS_BEFORE_AUTHORIZATION = 1

pending_polls = {"remaining": PENDING_POLLS_BEFORE_AUTHORIZATION}
pending_lock = threading.Lock()


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        fields = self._read_form()
        path = urllib.parse.urlparse(self.path).path
        if path.endswith(DEVICE_CODE_PATH_SUFFIX):
            return self._send(200, self._device_code_grant())
        if path.endswith(TOKEN_PATH_SUFFIX):
            return self._token(fields)
        return self._send(404, {})

    def do_GET(self):
        if urllib.parse.urlparse(self.path).path != IMDS_PATH:
            return self._send(404, {})
        if self.headers.get("Metadata") != "true":
            return self._send(400, {"error": "invalid_request"})
        return self._send(200, self._access_token())

    def _token(self, fields):
        grant_type = fields.get("grant_type")
        if grant_type == "client_credentials":
            return self._client_credentials(fields)
        if grant_type == DEVICE_CODE_GRANT:
            return self._device_code(fields)
        if grant_type == "refresh_token":
            return self._refresh(fields)
        return self._send(400, {"error": "unsupported_grant_type"})

    def _client_credentials(self, fields):
        if fields.get("client_secret") != os.environ["FAKE_ENTRA_SECRET"]:
            return self._send(401, {"error": "invalid_client", "error_description": "AADSTS7000215"})
        return self._send(200, self._access_token())

    def _device_code(self, fields):
        if fields.get("device_code") != DEVICE_CODE:
            return self._send(400, {"error": "bad_verification_code"})
        with pending_lock:
            still_pending = pending_polls["remaining"] > 0
            pending_polls["remaining"] -= 1
        if still_pending:
            return self._send(400, {"error": "authorization_pending"})
        return self._send(200, self._access_token(with_refresh_token=True))

    def _refresh(self, fields):
        if fields.get("refresh_token") != REFRESH_TOKEN:
            return self._send(400, {"error": "invalid_grant", "error_description": "AADSTS70000"})
        return self._send(200, self._access_token(with_refresh_token=True))

    def _device_code_grant(self):
        return {
            "device_code": DEVICE_CODE,
            "user_code": "ABC123",
            "verification_uri": "https://microsoft.com/devicelogin",
            "message": "Abra https://microsoft.com/devicelogin e digite o código ABC123",
            "interval": POLL_INTERVAL_SECONDS,
            "expires_in": DEVICE_CODE_LIFETIME_SECONDS,
        }

    def _access_token(self, with_refresh_token=False):
        payload = {"access_token": os.environ["FAKE_PR_TOKEN"], "expires_in": TOKEN_LIFETIME_SECONDS}
        if with_refresh_token:
            payload["refresh_token"] = REFRESH_TOKEN
        return payload

    def _read_form(self):
        length = int(self.headers.get("Content-Length", "0") or 0)
        body = self.rfile.read(length).decode("utf-8") if length else ""
        return {key: values[0] for key, values in urllib.parse.parse_qs(body).items()}

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
