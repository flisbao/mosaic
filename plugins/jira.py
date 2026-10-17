#!/usr/bin/env python3
"""Plugin do panel-tui: lê um Jira Cloud (somente leitura) nos modos manager e personal."""

from __future__ import annotations

import base64
import http.client
import json
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path

MODES = ("manager", "personal")
DEFAULT_UNASSIGNED_HOURS = 2
DEFAULT_BUDGET_SECONDS = 40
DEFAULT_STATE_DIRECTORY = "~/.local/state/panel-tui/jira"
SITE_PATTERN = re.compile(r"^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$")
PROJECT_PATTERN = re.compile(r"^[A-Z][A-Z0-9_]*$")
MAXIMUM_SITE_LENGTH = 253
REQUEST_TIMEOUT_SECONDS = 20
HTTP_UNAUTHORIZED = 401
HTTP_FORBIDDEN = 403
HTTP_TOO_MANY_REQUESTS = 429
HTTP_ERROR_FLOOR = 400


class ConfigError(Exception):
    pass


@dataclass(frozen=True)
class JiraConfig:
    site: str
    email: str
    token: str
    mode: str
    board: int | None
    project: str | None
    team: list
    unassigned_hours: int
    state_dir: Path
    budget_seconds: int
    base_url: str


def load_config(environ):
    site = _read_site(environ)
    email = environ.get("JIRA_EMAIL", "").strip()
    if "@" not in email or not email.isprintable():
        raise ConfigError("JIRA_EMAIL precisa ser um e-mail válido")
    mode = environ.get("JIRA_MODE", "")
    if mode not in MODES:
        raise ConfigError("JIRA_MODE precisa ser 'manager' ou 'personal'")
    board = _read_board(environ)
    project = _read_project(environ)
    if board is None and project is None:
        raise ConfigError("defina JIRA_BOARD e/ou JIRA_PROJECT")
    return JiraConfig(
        site=site,
        email=email,
        token=_read_token(environ),
        mode=mode,
        board=board,
        project=project,
        team=_read_team(environ),
        unassigned_hours=_read_positive(environ, "JIRA_UNASSIGNED_HOURS", DEFAULT_UNASSIGNED_HOURS),
        state_dir=Path(environ.get("JIRA_STATE_DIR", DEFAULT_STATE_DIRECTORY)).expanduser(),
        budget_seconds=_read_positive(environ, "JIRA_BUDGET_SECONDS", DEFAULT_BUDGET_SECONDS),
        base_url=environ.get("JIRA_BASE_URL", "").strip() or f"https://{site}",
    )


def _read_site(environ):
    site = environ.get("JIRA_SITE", "")
    if not site or len(site) > MAXIMUM_SITE_LENGTH or not SITE_PATTERN.match(site):
        raise ConfigError("JIRA_SITE precisa ser só o nome do host (ex.: empresa.atlassian.net)")
    return site


def _read_board(environ):
    raw = environ.get("JIRA_BOARD")
    if raw is None or raw == "":
        return None
    if not raw.isascii() or not raw.isdigit() or int(raw) == 0:
        raise ConfigError("JIRA_BOARD precisa ser um número inteiro positivo")
    return int(raw)


def _read_project(environ):
    raw = environ.get("JIRA_PROJECT")
    if raw is None or raw == "":
        return None
    if not PROJECT_PATTERN.match(raw):
        raise ConfigError("JIRA_PROJECT precisa ser um prefixo como VHPJ")
    return raw


def _read_positive(environ, name, default):
    raw = environ.get(name)
    if raw is None or raw == "":
        return default
    if not raw.isascii() or not raw.isdigit() or int(raw) == 0:
        raise ConfigError(f"{name} precisa ser um número inteiro positivo")
    return int(raw)


def _read_team(environ):
    entries = []
    for entry in environ.get("JIRA_TEAM", "").split(","):
        entry = entry.strip()
        if entry and entry.isprintable():
            entries.append(entry)
    return entries


def _read_token(environ):
    variable = environ.get("JIRA_TOKEN_ENV", "").strip()
    if variable:
        value = environ.get(variable, "").strip()
        if not value:
            raise ConfigError(f"a variável {variable} (JIRA_TOKEN_ENV) está vazia ou ausente")
        return value
    path = environ.get("JIRA_TOKEN_FILE", "").strip()
    if path:
        try:
            value = Path(path).expanduser().read_text(encoding="utf-8").strip()
        except OSError:
            raise ConfigError("não foi possível ler o arquivo de JIRA_TOKEN_FILE") from None
        if not value:
            raise ConfigError("o arquivo de JIRA_TOKEN_FILE está vazio")
        return value
    raise ConfigError("defina JIRA_TOKEN_ENV (nome da variável com o token) ou JIRA_TOKEN_FILE")


class JiraError(Exception):
    pass


class AuthError(JiraError):
    pass


class RateLimitError(JiraError):
    pass


class NetworkError(JiraError):
    pass


class UnexpectedResponseError(JiraError):
    pass


def urllib_http(url, method, headers, timeout):
    request = urllib.request.Request(url, method=method, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


class JiraClient:
    def __init__(self, config, http):
        self._config = config
        self._http = http
        credentials = f"{config.email}:{config.token}".encode("utf-8")
        self._headers = {
            "Authorization": "Basic " + base64.b64encode(credentials).decode("ascii"),
            "Accept": "application/json",
        }

    def get_json(self, path, params=None):
        url = self._config.base_url + path
        if params:
            url += "?" + urllib.parse.urlencode(params)
        try:
            status, body = self._http(url, "GET", self._headers, REQUEST_TIMEOUT_SECONDS)
        except (OSError, http.client.HTTPException) as error:
            raise NetworkError(
                f"falha de rede ao falar com {self._config.site} ({type(error).__name__})"
            ) from None
        return self._decode(status, body)

    def _decode(self, status, body):
        if status in (HTTP_UNAUTHORIZED, HTTP_FORBIDDEN):
            raise AuthError(f"credenciais inválidas para {self._config.site}")
        if status == HTTP_TOO_MANY_REQUESTS:
            raise RateLimitError("limite de requisições do Jira; tentando de novo mais tarde")
        if status >= HTTP_ERROR_FLOOR:
            raise UnexpectedResponseError(f"resposta inesperada do Jira (HTTP {status})")
        try:
            return json.loads(body)
        except ValueError:
            raise UnexpectedResponseError("a resposta do Jira não é JSON") from None


if __name__ == "__main__":
    sys.exit(0)
