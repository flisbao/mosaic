#!/usr/bin/env python3
"""Plugin do mosaic: lê um Jira Cloud (somente leitura) nos modos manager e personal."""

from __future__ import annotations

import base64
import http.client
import json
import os
import re
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path

MODES = ("manager", "personal")
DEFAULT_UNASSIGNED_HOURS = 2
DEFAULT_BUDGET_SECONDS = 40
DEFAULT_STATE_DIRECTORY = "~/.local/state/mosaic/jira"
SITE_PATTERN = re.compile(r"^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$")
PROJECT_PATTERN = re.compile(r"^[A-Z][A-Z0-9_]*$")
MAXIMUM_SITE_LENGTH = 253
LOCAL_HOSTS = ("127.0.0.1", "localhost")
HTTP_REDIRECT_FLOOR = 300
REQUEST_TIMEOUT_SECONDS = 20
MAXIMUM_RESPONSE_BYTES = 8 * 1024 * 1024
MAXIMUM_ERROR_BODY_BYTES = 8 * 1024
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
ISSUE_KEY_PATTERN = re.compile(r"^[A-Z][A-Z0-9_]*-[0-9]+$")
STATUS_PATH = "/rest/api/3/status"
ISSUE_PATH = "/rest/api/3/issue/{key}"
USER_SEARCH_PATH = "/rest/api/3/user/search"
SUMMARY_TEXT_LIMIT = 80
OTHERS_LABEL = "Outros"
NOBODY_LABEL = "ninguem"
MISSING_PRIORITY = "-"
UPDATED_FLAG = "updated"
PERSONAL_WINDOW_DAYS = 7
MANAGER_WINDOW_DAYS = 14
BROWSE_URL = "https://{site}/browse/{key}"
NO_MEDIAN = "—"
SECONDS_PER_HOUR = 3600
CYCLE_HOURS_DECIMALS = 1


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
        base_url=_read_base_url(environ, site),
    )


def _read_base_url(environ, site):
    override = environ.get("JIRA_BASE_URL", "").strip()
    if not override:
        return f"https://{site}"
    parsed = urllib.parse.urlparse(override)
    is_secure = parsed.scheme == "https" and bool(parsed.hostname)
    is_local = parsed.scheme == "http" and parsed.hostname in LOCAL_HOSTS
    if not (is_secure or is_local) or any(character.isspace() for character in override):
        raise ConfigError("JIRA_BASE_URL precisa ser https ou um endereço local de teste")
    return override.rstrip("/")


def _read_site(environ):
    site = environ.get("JIRA_SITE", "")
    if not site or len(site) > MAXIMUM_SITE_LENGTH or not SITE_PATTERN.fullmatch(site):
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
    if not PROJECT_PATTERN.fullmatch(raw):
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


class _RefuseRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, file_pointer, code, message, headers, new_url):
        return None


_OPENER = urllib.request.build_opener(_RefuseRedirects)


def urllib_http(url, method, headers, timeout):
    request = urllib.request.Request(url, method=method, headers=headers)
    try:
        with _OPENER.open(request, timeout=timeout) as response:
            return response.status, _read_limited(response, MAXIMUM_RESPONSE_BYTES)
    except urllib.error.HTTPError as error:
        return error.code, error.read(MAXIMUM_ERROR_BODY_BYTES)


def _read_limited(stream, limit):
    body = stream.read(limit + 1)
    if len(body) > limit:
        raise UnexpectedResponseError("a resposta do Jira excede o tamanho máximo permitido")
    return body


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
        if status >= HTTP_REDIRECT_FLOOR:
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
            parsed = datetime.fromisoformat(values[name]) if values[name] else None
            values[name] = parsed.replace(tzinfo=timezone.utc) if parsed and parsed.tzinfo is None else parsed
        return Issue(**values)


def _as_dict(value):
    return value if isinstance(value, dict) else {}


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
    assignee = _as_dict(fields.get("assignee"))
    creator = _as_dict(fields.get("creator"))
    status = _as_dict(fields.get("status"))
    priority = _as_dict(fields.get("priority"))
    category = _as_dict(status.get("statusCategory")).get("key")
    return Issue(
        key=key,
        summary=str(fields.get("summary") or ""),
        status=str(status.get("name") or ""),
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
        data = self._read_dict(HISTORY_FILE)
        return {key: value for key, value in data.items() if isinstance(value, dict)}

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


def fetch_status_categories(client):
    payload = client.get_json(STATUS_PATH)
    categories = {}
    for status in payload if isinstance(payload, list) else []:
        if not isinstance(status, dict):
            continue
        category = (status.get("statusCategory") or {}).get("key")
        if status.get("name") and category in STATUS_CATEGORIES:
            categories[status["name"]] = category
    return categories


def find_work_start(client, issue_key, categories):
    if not ISSUE_KEY_PATTERN.fullmatch(issue_key):
        return None
    payload = client.get_json(
        ISSUE_PATH.format(key=issue_key), {"expand": "changelog", "fields": "status"}
    )
    histories = _as_dict(_as_dict(payload).get("changelog")).get("histories")
    transitions = []
    for entry in histories if isinstance(histories, list) else []:
        created = parse_timestamp(entry.get("created")) if isinstance(entry, dict) else None
        if created is None:
            continue
        items = entry.get("items")
        for item in items if isinstance(items, list) else []:
            if isinstance(item, dict) and item.get("field") == "status":
                transitions.append((created, item.get("toString")))
    for created, status_name in sorted(transitions, key=lambda transition: transition[0]):
        if categories.get(status_name) == "indeterminate":
            return created
    return None


def update_history(history, issues, client, categories, now, out_of_time=None):
    merged = dict(history)
    for issue in issues:
        previous = _as_dict(merged.get(issue.key))
        started = _valid_started(previous.get("started"))
        if needs_changelog(previous, issue) and (out_of_time is None or not out_of_time()):
            started = _read_work_start(client, issue, categories) or started
        merged[issue.key] = _history_record(issue, started)
    return merged


def _valid_started(raw):
    if not isinstance(raw, str):
        return None
    try:
        parsed = datetime.fromisoformat(raw)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.isoformat()


def needs_changelog(previous, issue):
    previous = _as_dict(previous)
    return issue.status_category == "done" and (
        previous.get("updated") != issue.updated.isoformat()
        or _valid_started(previous.get("started")) is None
    )


def _read_work_start(client, issue, categories):
    try:
        found = find_work_start(client, issue.key, categories)
    except JiraError:
        return None
    return (found or issue.created).isoformat()


def _history_record(issue, started):
    cycle_hours = None
    if issue.resolved is not None and started:
        seconds = (issue.resolved - datetime.fromisoformat(started)).total_seconds()
        cycle_hours = round(max(0.0, seconds / SECONDS_PER_HOUR), CYCLE_HOURS_DECIMALS)
    return {
        "key": issue.key,
        "assignee": issue.assignee_name,
        "assignee_id": issue.assignee_id,
        "summary": issue.summary,
        "status": issue.status,
        "status_category": issue.status_category,
        "created": issue.created.isoformat(),
        "updated": issue.updated.isoformat(),
        "started": started,
        "resolved": issue.resolved.isoformat() if issue.resolved else None,
        "cycle_hours": cycle_hours,
    }


@dataclass
class TeamResolution:
    members: dict
    warnings: list


def resolve_team(entries, issues, client, cache):
    in_scope = {}
    for issue in issues:
        if issue.assignee_id and issue.assignee_name:
            in_scope.setdefault(issue.assignee_id, issue.assignee_name)
    updated_cache = dict(cache)
    if not entries:
        return TeamResolution(dict(in_scope), []), updated_cache
    members = {}
    warnings = []
    for entry in entries:
        found = _lookup_entry(entry, in_scope, client, updated_cache)
        if found is None:
            warnings.append(f'não encontrei "{entry}" no Jira')
        else:
            members[found[0]] = found[1]
    return TeamResolution(members, warnings), updated_cache


def _lookup_entry(entry, in_scope, client, cache):
    if "@" in entry:
        found = _lookup_email(entry, client, cache)
        if found is not None:
            return found
    if entry in in_scope:
        return entry, in_scope[entry]
    for account_id, name in in_scope.items():
        if name.casefold() == entry.casefold():
            return account_id, name
    return None


def _lookup_email(email, client, cache):
    cached = cache.get(email)
    if isinstance(cached, dict) and cached.get("id"):
        return cached["id"], cached.get("name") or email
    try:
        users = client.get_json(USER_SEARCH_PATH, {"query": email})
    except JiraError:
        return None
    if not isinstance(users, list) or not users or not isinstance(users[0], dict):
        return None
    account_id = users[0].get("accountId")
    if not account_id:
        return None
    name = users[0].get("displayName") or email
    cache[email] = {"id": account_id, "name": name}
    return account_id, name


def snapshot_title(config):
    scope = config.project or f"quadro {config.board}"
    role = "time" if config.mode == "manager" else "meus"
    return f"Jira {config.site.split('.')[0]} {scope} · {role}"


def _line(text, flagged=False):
    line = {"text": text}
    if flagged:
        line["flag"] = UPDATED_FLAG
    return line


def _summary_blocks(lines):
    return [{"type": "lines", "lines": lines}]


def _item(issue, config, now, last_seen, prefix=None):
    summary = flatten_text(issue.summary, SUMMARY_TEXT_LIMIT)
    text = f"{prefix} — {issue.key} {summary}" if prefix else f"{issue.key} {summary}"
    item = {
        "id": issue.key,
        "text": text.strip(),
        "meta": {
            "status": issue.status,
            "prioridade": issue.priority or MISSING_PRIORITY,
            "responsavel": issue.assignee_name or NOBODY_LABEL,
            "criado_ha": format_duration(now - issue.created),
            "parado_ha": format_duration(now - issue.updated),
            "site": config.site,
        },
    }
    if issue.updated > last_seen:
        item["flag"] = UPDATED_FLAG
    return item


def _actions(config):
    return [
        {
            "id": "mark_seen",
            "label": "Marcar como visto",
            "description": f"Marcar tudo como visto em {config.site}",
            "applies_to": "widget",
        },
        {
            "id": "open",
            "label": "Abrir no navegador",
            "description": "Abrir o ticket selecionado no navegador",
            "applies_to": "item",
        },
    ]


def _offline_lines(offline_at):
    if offline_at is None:
        return []
    return [_line(f"dados de {offline_at.astimezone().strftime('%H:%M')} (offline)")]


def _person_label(issue, team):
    if issue.assignee_id is None:
        return NOBODY_LABEL
    return team.members.get(issue.assignee_id, OTHERS_LABEL)


def build_manager_snapshot(issues, config, team, last_seen, now, offline_at=None):
    threshold = timedelta(hours=config.unassigned_hours)
    unassigned = sorted(
        (
            issue for issue in issues
            if issue.assignee_id is None
            and issue.status_category == "new"
            and now - issue.created >= threshold
        ),
        key=lambda issue: (issue.created, issue.key),
    )
    in_progress = sorted(
        (issue for issue in issues if issue.status_category == "indeterminate"),
        key=lambda issue: (_person_label(issue, team) == OTHERS_LABEL, _person_label(issue, team), issue.key),
    )
    updated = sorted(
        (issue for issue in issues if issue.updated > last_seen),
        key=lambda issue: issue.updated,
        reverse=True,
    )
    summary_lines = _offline_lines(offline_at) + [
        _line(f"sem dono: {len(unassigned)}"),
        _line(f"em andamento: {len(in_progress)}"),
        _line(f"atualizados: {len(updated)}", flagged=bool(updated)),
    ]
    summary_lines += [_line(warning) for warning in team.warnings]
    return {
        "summary": {"title": snapshot_title(config), "blocks": _summary_blocks(summary_lines)},
        "detail": [
            {
                "heading": "Sem dono",
                "items": [_item(issue, config, now, last_seen) for issue in unassigned],
            },
            {
                "heading": "Em andamento",
                "items": [
                    _item(issue, config, now, last_seen, prefix=_person_label(issue, team))
                    for issue in in_progress
                ],
            },
            {
                "heading": "Atualizados",
                "items": [_item(issue, config, now, last_seen) for issue in updated],
            },
        ],
        "actions": _actions(config),
    }


def _format_median(values):
    if not values:
        return NO_MEDIAN
    ordered = sorted(values)
    middle = len(ordered) // 2
    median = ordered[middle] if len(ordered) % 2 else (ordered[middle - 1] + ordered[middle]) / 2
    return f"{median:.1f}".rstrip("0").rstrip(".") + "h"


def build_personal_snapshot(issues, history, config, last_seen, now, offline_at=None):
    window = timedelta(days=PERSONAL_WINDOW_DAYS)
    open_issues = sorted(
        (issue for issue in issues if issue.status_category != "done"),
        key=lambda issue: (issue.status_category != "indeterminate", issue.key),
    )
    done_issues = sorted(
        (
            issue for issue in issues
            if issue.status_category == "done" and issue.resolved and now - issue.resolved <= window
        ),
        key=lambda issue: issue.resolved,
        reverse=True,
    )
    in_progress = [issue for issue in open_issues if issue.status_category == "indeterminate"]
    cycles = [
        history[issue.key]["cycle_hours"]
        for issue in done_issues
        if isinstance(history.get(issue.key), dict) and history[issue.key].get("cycle_hours") is not None
    ]
    updated = [issue for issue in open_issues + done_issues if issue.updated > last_seen]
    summary_lines = _offline_lines(offline_at) + [
        _line(f"em andamento: {len(in_progress)}"),
        _line(f"concluídos em {PERSONAL_WINDOW_DAYS} dias: {len(done_issues)}"),
        _line(f"tempo mediano: {_format_median(cycles)}"),
        _line(f"atualizados: {len(updated)}", flagged=bool(updated)),
    ]
    return {
        "summary": {"title": snapshot_title(config), "blocks": _summary_blocks(summary_lines)},
        "detail": [
            {
                "heading": "Meus tickets",
                "items": [_item(issue, config, now, last_seen) for issue in open_issues],
            },
            {
                "heading": f"Concluídos ({PERSONAL_WINDOW_DAYS} dias)",
                "items": [_item(issue, config, now, last_seen) for issue in done_issues],
            },
        ],
        "actions": _actions(config),
    }


class _OfflineClient:
    def get_json(self, path, params=None):
        raise NetworkError("sem conexão")


def open_in_browser(url):
    subprocess.Popen(
        ["xdg-open", url],
        start_new_session=True,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def _window_days(config):
    return MANAGER_WINDOW_DAYS if config.mode == "manager" else PERSONAL_WINDOW_DAYS


def _build_snapshot(config, issues, history, team, last_seen_at, now, offline_at=None):
    if config.mode == "manager":
        return build_manager_snapshot(issues, config, team, last_seen_at, now, offline_at)
    return build_personal_snapshot(issues, history, config, last_seen_at, now, offline_at)


def _accounts_of(state):
    accounts = state.get("accounts")
    return accounts if isinstance(accounts, dict) else {}


def _run_online(config, store, client, now, clock):
    started_at = clock()
    state = store.load_state()
    last_seen_at = last_seen(state, config.mode, now)
    issues = fetch_scope(
        client, config, _window_days(config), now, clock, assignee_only=config.mode == "personal"
    )
    history = store.load_history()
    categories = {}
    if any(needs_changelog(history.get(issue.key) or {}, issue) for issue in issues):
        try:
            categories = fetch_status_categories(client)
        except JiraError:
            categories = {}
    history = update_history(
        history, issues, client, categories, now,
        out_of_time=lambda: clock() - started_at > config.budget_seconds,
    )
    store.save_history(history)
    team = TeamResolution({}, [])
    if config.mode == "manager":
        team, accounts = resolve_team(config.team, issues, client, _accounts_of(state))
        if accounts != _accounts_of(state):
            fresh_state = store.load_state()
            fresh_state["accounts"] = accounts
            store.save_state(fresh_state)
    store.save_snapshot(config.mode, issues, now)
    return _build_snapshot(config, issues, history, team, last_seen_at, now)


def _run_offline(config, store, now):
    cached = store.load_snapshot(config.mode)
    if cached is None:
        return None
    issues, saved_at = cached
    state = store.load_state()
    team = TeamResolution({}, [])
    if config.mode == "manager":
        team, _ = resolve_team(config.team, issues, _OfflineClient(), _accounts_of(state))
        team.warnings = []
    return _build_snapshot(
        config, issues, store.load_history(), team, last_seen(state, config.mode, now), now, saved_at
    )


def _command_snapshot(config, store, http, now, clock, stdout, stderr):
    client = JiraClient(config, http)
    try:
        snapshot = _run_online(config, store, client, now, clock)
    except Exception as error:
        snapshot = _run_offline(config, store, now)
        if snapshot is None:
            reason = str(error) if isinstance(error, JiraError) else f"erro inesperado ({type(error).__name__})"
            stderr.write(f"erro: {reason}\n")
            return 1
    stdout.write(json.dumps(snapshot, ensure_ascii=False) + "\n")
    return 0


def _command_mark_seen(config, store, now, stdout, stderr):
    state = store.load_state()
    state[f"last_seen_{config.mode}"] = now.isoformat()
    if not store.save_state(state):
        stderr.write("erro: não foi possível gravar o estado local\n")
        return 1
    stdout.write("Marcado como visto\n")
    return 0


def _command_open(config, opener, stdin, stdout, stderr):
    try:
        item_id = json.loads(stdin.read() or "{}").get("item_id")
    except (ValueError, AttributeError):
        item_id = None
    if not isinstance(item_id, str) or not ISSUE_KEY_PATTERN.fullmatch(item_id):
        stderr.write("erro: identificador de ticket inválido\n")
        return 1
    try:
        opener(BROWSE_URL.format(site=config.site, key=item_id))
    except OSError:
        stderr.write("erro: não foi possível abrir o navegador (xdg-open)\n")
        return 1
    stdout.write(f"Abrindo {item_id}\n")
    return 0


def main(
    arguments,
    environ=None,
    http=urllib_http,
    now=None,
    clock=time.monotonic,
    opener=open_in_browser,
    stdin=None,
    stdout=None,
    stderr=None,
):
    environ = os.environ if environ is None else environ
    stdin = sys.stdin if stdin is None else stdin
    stdout = sys.stdout if stdout is None else stdout
    stderr = sys.stderr if stderr is None else stderr
    commands = (["snapshot"], ["action", "mark_seen"], ["action", "open"])
    if arguments not in commands:
        stderr.write("uso: jira.py snapshot | action mark_seen | action open\n")
        return 2
    try:
        config = load_config(environ)
    except ConfigError as error:
        stderr.write(f"erro: {error}\n")
        return 1
    store = StateStore(config.state_dir / state_key(config))
    moment = now or datetime.now(timezone.utc)
    if arguments == ["snapshot"]:
        return _command_snapshot(config, store, http, moment, clock, stdout, stderr)
    if arguments == ["action", "mark_seen"]:
        return _command_mark_seen(config, store, moment, stdout, stderr)
    return _command_open(config, opener, stdin, stdout, stderr)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
