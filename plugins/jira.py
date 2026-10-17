#!/usr/bin/env python3
"""Plugin do panel-tui: lê um Jira Cloud (somente leitura) nos modos manager e personal."""

from __future__ import annotations

import base64
import http.client
import json
import os
import re
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass
from datetime import datetime, timedelta, timezone
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
STATUS_CATEGORIES = ("new", "indeterminate", "done")
UNKNOWN_CATEGORY = "unknown"
TIMESTAMP_FORMATS = ("%Y-%m-%dT%H:%M:%S.%f%z", "%Y-%m-%dT%H:%M:%S%z")
SECONDS_PER_MINUTE = 60
MINUTES_PER_HOUR = 60
HOURS_PER_DAY = 24
HOURS_SHOWN_BEFORE_DAYS = 48
ELLIPSIS = "…"
MAXIMUM_PAGES = 10
MAXIMUM_ISSUES = 500
PAGE_SIZE = 100
ISSUE_FIELDS = "summary,status,assignee,creator,created,updated,resolutiondate,priority"
SEARCH_PATH = "/rest/api/3/search/jql"
BOARD_ISSUES_PATH = "/rest/agile/1.0/board/{board}/issue"
DIRECTORY_MODE = 0o700
FILE_MODE = 0o600
FIRST_SEEN_LOOKBACK_HOURS = 24
STATE_FILE = "state.json"
HISTORY_FILE = "history.json"
SNAPSHOT_FILE = "snapshot-{mode}.json"
UNSAFE_KEY_CHARACTERS = re.compile(r"[^A-Za-z0-9._-]")


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


@dataclass(frozen=True)
class Issue:
    key: str
    summary: str
    status: str
    status_category: str
    assignee_id: str | None
    assignee_name: str | None
    creator_name: str | None
    priority: str | None
    created: datetime
    updated: datetime
    resolved: datetime | None

    def to_raw(self):
        raw = asdict(self)
        for name in ("created", "updated", "resolved"):
            raw[name] = raw[name].isoformat() if raw[name] is not None else None
        return raw

    @staticmethod
    def from_raw(raw):
        values = dict(raw)
        for name in ("created", "updated", "resolved"):
            values[name] = datetime.fromisoformat(values[name]) if values[name] else None
        return Issue(**values)


def parse_timestamp(text):
    if not isinstance(text, str) or not text:
        return None
    candidate = text[:-1] + "+0000" if text.endswith("Z") else text
    for timestamp_format in TIMESTAMP_FORMATS:
        try:
            return datetime.strptime(candidate, timestamp_format).astimezone(timezone.utc)
        except ValueError:
            continue
    return None


def parse_issue(payload):
    if not isinstance(payload, dict):
        return None
    key = payload.get("key")
    fields = payload.get("fields")
    if not isinstance(key, str) or not key or not isinstance(fields, dict):
        return None
    created = parse_timestamp(fields.get("created"))
    updated = parse_timestamp(fields.get("updated"))
    if created is None or updated is None:
        return None
    assignee = fields.get("assignee") or {}
    creator = fields.get("creator") or {}
    status = fields.get("status") or {}
    priority = fields.get("priority") or {}
    category = ((status.get("statusCategory") or {}).get("key")) if isinstance(status, dict) else None
    return Issue(
        key=key,
        summary=str(fields.get("summary") or ""),
        status=str(status.get("name") or "") if isinstance(status, dict) else "",
        status_category=category if category in STATUS_CATEGORIES else UNKNOWN_CATEGORY,
        assignee_id=assignee.get("accountId"),
        assignee_name=assignee.get("displayName"),
        creator_name=creator.get("displayName"),
        priority=priority.get("name"),
        created=created,
        updated=updated,
        resolved=parse_timestamp(fields.get("resolutiondate")),
    )


def format_duration(delta):
    minutes = int(delta.total_seconds() // SECONDS_PER_MINUTE)
    if minutes <= 0:
        return "0m"
    if minutes < MINUTES_PER_HOUR:
        return f"{minutes}m"
    hours = minutes // MINUTES_PER_HOUR
    if hours < HOURS_SHOWN_BEFORE_DAYS:
        return f"{hours}h"
    return f"{hours // HOURS_PER_DAY}d"


def flatten_text(text, limit):
    flattened = " ".join(str(text).split())
    if len(flattened) <= limit:
        return flattened
    if limit <= 0:
        return ""
    return flattened[: limit - 1] + ELLIPSIS


def build_scope_jql(config, window_days, assignee_only):
    clauses = []
    if config.project:
        clauses.append(f"project = {config.project}")
    if assignee_only:
        clauses.append("assignee = currentUser()")
    clauses.append(f"(statusCategory != Done OR resolutiondate >= -{window_days}d)")
    return " AND ".join(clauses)


def fetch_scope(client, config, window_days, now, clock, assignee_only=False):
    jql = build_scope_jql(config, window_days, assignee_only)
    deadline = clock() + config.budget_seconds
    collected = {}
    if config.board is not None:
        _read_board_pages(client, config, jql, clock, deadline, collected)
    else:
        _read_search_pages(client, jql, clock, deadline, collected)
    return list(collected.values())[:MAXIMUM_ISSUES]


def _absorb(page_issues, collected):
    added = 0
    for payload in page_issues or []:
        issue = parse_issue(payload)
        if issue is not None and issue.key not in collected:
            collected[issue.key] = issue
            added += 1
    return added


def _read_search_pages(client, jql, clock, deadline, collected):
    token = None
    seen_tokens = set()
    for _ in range(MAXIMUM_PAGES):
        if clock() > deadline:
            return
        params = {"jql": jql, "fields": ISSUE_FIELDS, "maxResults": PAGE_SIZE}
        if token:
            params["nextPageToken"] = token
        page = client.get_json(SEARCH_PATH, params)
        page = page if isinstance(page, dict) else {}
        added = _absorb(page.get("issues"), collected)
        token = page.get("nextPageToken")
        if not token or page.get("isLast") or token in seen_tokens or added == 0:
            return
        if len(collected) >= MAXIMUM_ISSUES:
            return
        seen_tokens.add(token)


def _read_board_pages(client, config, jql, clock, deadline, collected):
    path = BOARD_ISSUES_PATH.format(board=config.board)
    start_at = 0
    for _ in range(MAXIMUM_PAGES):
        if clock() > deadline:
            return
        params = {"jql": jql, "fields": ISSUE_FIELDS, "startAt": start_at, "maxResults": PAGE_SIZE}
        page = client.get_json(path, params)
        page = page if isinstance(page, dict) else {}
        page_issues = page.get("issues") or []
        added = _absorb(page_issues, collected)
        reported_start = page.get("startAt")
        ignored_paging = reported_start is not None and reported_start != start_at
        if len(page_issues) < PAGE_SIZE or added == 0 or ignored_paging:
            return
        if len(collected) >= MAXIMUM_ISSUES:
            return
        start_at += len(page_issues)


def state_key(config):
    parts = [config.site]
    if config.project:
        parts.append(config.project)
    if config.board is not None:
        parts.append(f"board{config.board}")
    return UNSAFE_KEY_CHARACTERS.sub("_", "-".join(parts))


def last_seen(state, mode, now):
    default = now - timedelta(hours=FIRST_SEEN_LOOKBACK_HOURS)
    raw = state.get(f"last_seen_{mode}") if isinstance(state, dict) else None
    if not isinstance(raw, str):
        return default
    try:
        parsed = datetime.fromisoformat(raw)
    except ValueError:
        return default
    return parsed if parsed.tzinfo else parsed.replace(tzinfo=timezone.utc)


class StateStore:
    def __init__(self, directory):
        self._directory = Path(directory)

    def load_state(self):
        return self._read_dict(STATE_FILE)

    def save_state(self, state):
        return self._write(STATE_FILE, state)

    def load_history(self):
        return self._read_dict(HISTORY_FILE)

    def save_history(self, history):
        return self._write(HISTORY_FILE, history)

    def load_snapshot(self, mode):
        data = self._read_dict(SNAPSHOT_FILE.format(mode=mode))
        try:
            saved_at = datetime.fromisoformat(data["saved_at"])
            issues = [Issue.from_raw(raw) for raw in data["issues"]]
        except (KeyError, TypeError, ValueError):
            return None
        return issues, saved_at

    def save_snapshot(self, mode, issues, saved_at):
        payload = {"saved_at": saved_at.isoformat(), "issues": [issue.to_raw() for issue in issues]}
        return self._write(SNAPSHOT_FILE.format(mode=mode), payload)

    def _read_dict(self, name):
        try:
            data = json.loads((self._directory / name).read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return {}
        return data if isinstance(data, dict) else {}

    def _write(self, name, payload):
        try:
            self._directory.mkdir(parents=True, exist_ok=True, mode=DIRECTORY_MODE)
            os.chmod(self._directory, DIRECTORY_MODE)
            descriptor, temporary = tempfile.mkstemp(dir=self._directory, prefix=".tmp-")
            try:
                with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
                    json.dump(payload, handle, ensure_ascii=False)
                os.chmod(temporary, FILE_MODE)
                os.replace(temporary, self._directory / name)
            except BaseException:
                if os.path.exists(temporary):
                    os.unlink(temporary)
                raise
        except (OSError, TypeError, ValueError):
            return False
        return True


if __name__ == "__main__":
    sys.exit(0)
