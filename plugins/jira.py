#!/usr/bin/env python3
"""Plugin do mosaic: lê um Jira Cloud (somente leitura); cada widget define o próprio JQL."""

from __future__ import annotations

import argparse
import base64
import http.client
import json
import re
import time
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

DEFAULT_BUDGET_SECONDS = 40
DEFAULT_STATE_DIRECTORY = "~/.local/state/mosaic/jira"
SITE_PATTERN = re.compile(r"^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$")
MAXIMUM_SITE_LENGTH = 253
LOCAL_HOSTS = ("127.0.0.1", "localhost")
HTTP_REDIRECT_FLOOR = 300
REQUEST_TIMEOUT_SECONDS = 20
MAXIMUM_RESPONSE_BYTES = 8 * 1024 * 1024
MAXIMUM_ERROR_BODY_BYTES = 8 * 1024
HTTP_BAD_REQUEST = 400
HTTP_UNAUTHORIZED = 401
HTTP_FORBIDDEN = 403
HTTP_TOO_MANY_REQUESTS = 429
TIMESTAMP_FORMATS = ("%Y-%m-%dT%H:%M:%S.%f%z", "%Y-%m-%dT%H:%M:%S%z")
SECONDS_PER_MINUTE = 60
MINUTES_PER_HOUR = 60
HOURS_PER_DAY = 24
HOURS_SHOWN_BEFORE_DAYS = 48
ELLIPSIS = "…"
GROUP_BY_CHOICES = ("none", "assignee", "status", "priority")
VIEW_CHOICES = ("lines", "table", "stat")
COLUMN_CHOICES = ("key", "summary", "status", "assignee", "priority", "updated")
DEFAULT_COLUMNS = ("key", "summary", "status", "assignee")
DEFAULT_TITLE = "Jira"
DEFAULT_MAX_ISSUES = 100
MAXIMUM_ISSUES = 500
COLUMNS_SEPARATOR = ","
ARGUMENT_ERROR_EXIT_CODE = 2
SEARCH_PATH = "/rest/api/3/search/jql"
SEARCH_FIELDS = "summary,status,assignee,priority,updated,created"
MAXIMUM_PAGE_SIZE = 100
MAXIMUM_QUERY_MESSAGE_LENGTH = 200
STATUS_CATEGORIES = ("new", "indeterminate", "done")
UNKNOWN_CATEGORY = "unknown"
PROTOCOL_VERSION = 1
SUMMARY_TEXT_LIMIT = 80
MAXIMUM_BLOCK_ROWS = 200
MAXIMUM_STAT_ITEMS = 8
OTHERS_LABEL = "Outros"
NOBODY_LABEL = "ninguem"
MISSING_PRIORITY = "-"
EMPTY_LABEL = "nenhum ticket"
MORE_RESULTS_SUFFIX = "+"
DETAIL_SEPARATOR = " — "
OFFLINE_NOTICE_FORMAT = "dados de %H:%M (offline)"
TONE_BY_CATEGORY = {"new": "muted", "indeterminate": "current", "done": "ok"}
DEFAULT_TONE = "normal"
COLUMN_TITLES = {
    "key": "Chave",
    "summary": "Resumo",
    "status": "Status",
    "assignee": "Responsável",
    "priority": "Prioridade",
    "updated": "Atualizado",
}
REMOVED_VARIABLES = (
    "JIRA_MODE",
    "JIRA_PROJECT",
    "JIRA_BOARD",
    "JIRA_TEAM",
    "JIRA_UNASSIGNED_HOURS",
)


class ConfigError(Exception):
    pass


@dataclass(frozen=True)
class JiraConfig:
    site: str
    email: str
    token: str
    state_dir: Path
    budget_seconds: int
    base_url: str


@dataclass(frozen=True)
class WidgetOptions:
    jql: str
    title: str
    group_by: str
    view: str
    columns: tuple[str, ...]
    max_issues: int


def load_config(environ: Mapping[str, str]) -> JiraConfig:
    _reject_removed_variables(environ)
    site = _read_site(environ)
    email = environ.get("JIRA_EMAIL", "").strip()
    if "@" not in email or not email.isprintable():
        raise ConfigError("JIRA_EMAIL precisa ser um e-mail válido")
    return JiraConfig(
        site=site,
        email=email,
        token=_read_token(environ),
        state_dir=Path(environ.get("JIRA_STATE_DIR", DEFAULT_STATE_DIRECTORY)).expanduser(),
        budget_seconds=_read_positive(environ, "JIRA_BUDGET_SECONDS", DEFAULT_BUDGET_SECONDS),
        base_url=_read_base_url(environ, site),
    )


def _reject_removed_variables(environ):
    for name in REMOVED_VARIABLES:
        if name in environ:
            raise ConfigError(f"{name} foi removido; use --jql")


def parse_options(arguments: list[str]) -> tuple[WidgetOptions, list[str]]:
    parser = _build_options_parser()
    namespace, remainder = parser.parse_known_args(arguments)
    options = WidgetOptions(
        jql=namespace.jql,
        title=namespace.title,
        group_by=namespace.group_by,
        view=namespace.view,
        columns=namespace.columns,
        max_issues=namespace.max_issues,
    )
    return options, remainder


def _build_options_parser():
    parser = argparse.ArgumentParser(prog="jira.py", allow_abbrev=False)
    parser.add_argument("--jql", required=True, type=_non_blank_text)
    parser.add_argument("--title", default=DEFAULT_TITLE)
    parser.add_argument("--group-by", default="none", choices=GROUP_BY_CHOICES)
    parser.add_argument("--view", default="lines", choices=VIEW_CHOICES)
    parser.add_argument("--columns", default=DEFAULT_COLUMNS, type=_column_list)
    parser.add_argument("--max-issues", default=DEFAULT_MAX_ISSUES, type=_issue_limit)
    return parser


def _non_blank_text(raw):
    if not raw.strip():
        raise argparse.ArgumentTypeError("não pode ser vazio")
    return raw


def _column_list(raw):
    columns = tuple(name.strip() for name in raw.split(COLUMNS_SEPARATOR))
    unknown = [name for name in columns if name not in COLUMN_CHOICES]
    if unknown:
        raise argparse.ArgumentTypeError(
            f"coluna inválida: {', '.join(unknown)} (use {', '.join(COLUMN_CHOICES)})"
        )
    return columns


def _issue_limit(raw):
    try:
        limit = int(raw)
    except ValueError:
        raise argparse.ArgumentTypeError("precisa ser um número inteiro") from None
    if not 1 <= limit <= MAXIMUM_ISSUES:
        raise argparse.ArgumentTypeError(f"precisa estar entre 1 e {MAXIMUM_ISSUES}")
    return limit


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


def _read_positive(environ, name, default):
    raw = environ.get(name)
    if raw is None or raw == "":
        return default
    if not raw.isascii() or not raw.isdigit() or int(raw) == 0:
        raise ConfigError(f"{name} precisa ser um número inteiro positivo")
    return int(raw)


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


class QueryError(JiraError):
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
        if status == HTTP_BAD_REQUEST:
            raise QueryError(_describe_query_failure(body))
        if status == HTTP_TOO_MANY_REQUESTS:
            raise RateLimitError("limite de requisições do Jira; tentando de novo mais tarde")
        if status >= HTTP_REDIRECT_FLOOR:
            raise UnexpectedResponseError(f"resposta inesperada do Jira (HTTP {status})")
        try:
            return json.loads(body)
        except ValueError:
            raise UnexpectedResponseError("a resposta do Jira não é JSON") from None


def _describe_query_failure(body):
    prefix = "JQL inválido"
    try:
        messages = json.loads(body).get("errorMessages")
    except (ValueError, AttributeError):
        return prefix
    if not isinstance(messages, list) or not messages or not isinstance(messages[0], str):
        return prefix
    first_message = messages[0].strip()[:MAXIMUM_QUERY_MESSAGE_LENGTH]
    return f"{prefix}: {first_message}" if first_message else prefix

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


@dataclass(frozen=True)
class Issue:
    key: str
    summary: str
    status: str
    status_category: str
    assignee: str | None
    priority: str | None
    created: datetime
    updated: datetime

    def to_raw(self):
        return {
            "key": self.key,
            "summary": self.summary,
            "status": self.status,
            "status_category": self.status_category,
            "assignee": self.assignee,
            "priority": self.priority,
            "created": self.created.isoformat(),
            "updated": self.updated.isoformat(),
        }

    @classmethod
    def from_raw(cls, raw):
        return cls(
            key=raw["key"],
            summary=raw["summary"],
            status=raw["status"],
            status_category=raw["status_category"],
            assignee=raw["assignee"],
            priority=raw["priority"],
            created=parse_timestamp(raw["created"]),
            updated=parse_timestamp(raw["updated"]),
        )


@dataclass(frozen=True)
class SearchResult:
    issues: list[Issue]
    has_more: bool


def parse_issue(payload):
    if not isinstance(payload, dict) or not isinstance(payload.get("key"), str):
        return None
    fields = payload.get("fields")
    if not isinstance(fields, dict):
        return None
    created = parse_timestamp(fields.get("created"))
    updated = parse_timestamp(fields.get("updated"))
    if created is None or updated is None:
        return None
    status = fields.get("status")
    status = status if isinstance(status, dict) else {}
    return Issue(
        key=payload["key"],
        summary=_text_or_empty(fields.get("summary")),
        status=_text_or_empty(status.get("name")),
        status_category=_read_status_category(status),
        assignee=_read_name(fields.get("assignee"), "displayName"),
        priority=_read_name(fields.get("priority"), "name"),
        created=created,
        updated=updated,
    )


def _text_or_empty(value):
    return value if isinstance(value, str) else ""


def _read_name(entity, field_name):
    if not isinstance(entity, dict):
        return None
    name = entity.get(field_name)
    return name if isinstance(name, str) and name else None


def _read_status_category(status):
    category = status.get("statusCategory")
    key = category.get("key") if isinstance(category, dict) else None
    return key if key in STATUS_CATEGORIES else UNKNOWN_CATEGORY


def search_issues(
    client: JiraClient,
    jql: str,
    max_issues: int,
    budget_seconds: int,
    clock: Callable[[], float] = time.monotonic,
) -> SearchResult:
    started_at = clock()
    issues: list[Issue] = []
    page_token = None
    while True:
        if clock() - started_at > budget_seconds:
            raise NetworkError("orçamento de tempo esgotado")
        remaining = max_issues - len(issues)
        payload = client.get_json(SEARCH_PATH, _search_params(jql, remaining, page_token))
        page_issues = _parse_search_page(payload)
        issues.extend(page_issues[:remaining])
        page_token = payload.get("nextPageToken") or None
        if len(issues) >= max_issues:
            return SearchResult(issues, has_more=bool(page_token) or len(page_issues) > remaining)
        if page_token is None:
            return SearchResult(issues, has_more=False)


def _search_params(jql, remaining, page_token):
    params = {
        "jql": jql,
        "fields": SEARCH_FIELDS,
        "maxResults": min(MAXIMUM_PAGE_SIZE, remaining),
    }
    if page_token:
        params["nextPageToken"] = page_token
    return params


def _parse_search_page(payload):
    raw_issues = payload.get("issues") if isinstance(payload, dict) else None
    if not isinstance(raw_issues, list):
        raise UnexpectedResponseError("a resposta de busca do Jira não tem a lista de issues")
    parsed = (parse_issue(raw_issue) for raw_issue in raw_issues)
    return [issue for issue in parsed if issue is not None]


def build_snapshot(result, options, site, now, offline_since=None):
    blocks = [_build_summary_block(result, options, now)]
    if offline_since is not None:
        blocks.insert(0, _offline_notice_block(offline_since))
    snapshot = {
        "protocol": PROTOCOL_VERSION,
        "summary": {"title": options.title, "blocks": blocks},
    }
    if result.issues:
        snapshot["detail"] = _build_detail(result.issues, options, site)
    return snapshot


def _offline_notice_block(offline_since):
    text = offline_since.astimezone().strftime(OFFLINE_NOTICE_FORMAT)
    return {"type": "lines", "lines": [{"text": text, "tone": "warn"}]}


def _build_summary_block(result, options, now):
    if options.view == "stat":
        return _stat_block(result, options)
    if not result.issues:
        return {"type": "lines", "lines": [{"text": EMPTY_LABEL, "tone": "muted"}]}
    visible = result.issues[:MAXIMUM_BLOCK_ROWS]
    if options.view == "table":
        return _table_block(visible, options.columns, now)
    return _lines_block(visible)


def _lines_block(issues):
    return {
        "type": "lines",
        "lines": [
            {
                "text": f"{issue.key} {flatten_text(issue.summary, SUMMARY_TEXT_LIMIT)}",
                "tone": TONE_BY_CATEGORY.get(issue.status_category, DEFAULT_TONE),
            }
            for issue in issues
        ],
    }


def _table_block(issues, columns, now):
    return {
        "type": "table",
        "columns": [{"title": COLUMN_TITLES[column]} for column in columns],
        "rows": [[_cell_text(issue, column, now) for column in columns] for issue in issues],
    }


def _cell_text(issue, column, now):
    if column == "key":
        return issue.key
    if column == "summary":
        return flatten_text(issue.summary, SUMMARY_TEXT_LIMIT)
    if column == "status":
        return issue.status
    if column == "assignee":
        return _group_name(issue, "assignee")
    if column == "priority":
        return _group_name(issue, "priority")
    return format_duration(now - issue.updated)


def _stat_block(result, options):
    suffix = MORE_RESULTS_SUFFIX if result.has_more else ""
    if options.group_by == "none" or not result.issues:
        counts = [(options.title, len(result.issues))]
    else:
        counts = _collapse_small_groups(
            [(name, len(members)) for name, members in _group_issues(result.issues, options.group_by)]
        )
    return {
        "type": "stat",
        "items": [{"value": f"{count}{suffix}", "label": label} for label, count in counts],
    }


def _collapse_small_groups(counts):
    if len(counts) <= MAXIMUM_STAT_ITEMS:
        return counts
    kept = counts[: MAXIMUM_STAT_ITEMS - 1]
    others = sum(count for _, count in counts[MAXIMUM_STAT_ITEMS - 1 :])
    return kept + [(OTHERS_LABEL, others)]


def _group_name(issue, group_by):
    if group_by == "assignee":
        return issue.assignee or NOBODY_LABEL
    if group_by == "priority":
        return issue.priority or MISSING_PRIORITY
    return issue.status


def _group_issues(issues, group_by):
    groups = {}
    for issue in issues:
        groups.setdefault(_group_name(issue, group_by), []).append(issue)
    return sorted(groups.items(), key=lambda group: (-len(group[1]), group[0]))


def _build_detail(issues, options, site):
    if options.group_by == "none":
        groups = [(options.title, issues)]
    else:
        groups = _group_issues(issues, options.group_by)
    return [
        {"heading": name, "items": [_detail_item(issue, site) for issue in members]}
        for name, members in groups
    ]


def _detail_item(issue, site):
    summary = flatten_text(issue.summary, SUMMARY_TEXT_LIMIT)
    return {
        "id": issue.key,
        "text": f"{issue.key}{DETAIL_SEPARATOR}{summary}",
        "meta": {
            "status": issue.status,
            "prioridade": issue.priority or MISSING_PRIORITY,
            "responsavel": issue.assignee or NOBODY_LABEL,
            "site": site,
        },
    }
