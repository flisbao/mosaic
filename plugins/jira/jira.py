#!/usr/bin/env python3
"""Plugin do mosaic: lê um Jira Cloud (somente leitura); cada widget define o próprio JQL."""

from __future__ import annotations

import argparse
import base64
import http.client
import hashlib
import json
import os
import re
import sys
import tempfile
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
FAILURE_EXIT_CODE = 1
SNAPSHOT_COMMAND = ["snapshot"]
STATE_DIRECTORY_MODE = 0o700
CACHE_KEY_SEPARATOR = "\n"
CACHE_FILE_SUFFIX = ".json"
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
MISSING_PRIORITY = "-"
MORE_RESULTS_SUFFIX = "+"
DETAIL_SEPARATOR = " — "
OFFLINE_TIME_FORMAT = "%H:%M"
TONE_BY_CATEGORY = {"new": "muted", "indeterminate": "current", "done": "ok"}
DEFAULT_TONE = "normal"
LANGUAGE_VARIABLE = "MOSAIC_LANGUAGE"
PORTUGUESE_LANGUAGE_PREFIX = "pt"
ENGLISH = "en"
PORTUGUESE = "pt-BR"
REMOVED_VARIABLES = (
    "JIRA_MODE",
    "JIRA_PROJECT",
    "JIRA_BOARD",
    "JIRA_TEAM",
    "JIRA_UNASSIGNED_HOURS",
)


@dataclass(frozen=True)
class Messages:
    usage: str
    error_line: str
    email_invalid: str
    removed_variable: str
    base_url_invalid: str
    site_invalid: str
    positive_integer_required: str
    token_variable_empty: str
    token_file_unreadable: str
    token_file_empty: str
    token_missing: str
    response_too_large: str
    network_failure: str
    invalid_credentials: str
    rate_limited: str
    unexpected_status: str
    response_not_json: str
    invalid_query: str
    time_budget_exhausted: str
    search_without_issues: str
    offline_notice: str
    empty_label: str
    others_label: str
    nobody_label: str
    blank_text: str
    invalid_columns: str
    integer_required: str
    limit_range: str
    meta_status: str
    meta_priority: str
    meta_assignee: str
    meta_site: str
    column_titles: Mapping[str, str]


ENGLISH_MESSAGES = Messages(
    usage="usage: jira.py [options] snapshot",
    error_line="error: {message}",
    email_invalid="JIRA_EMAIL must be a valid email address",
    removed_variable="{name} was removed; use --jql",
    base_url_invalid="JIRA_BASE_URL must be https or a local test address",
    site_invalid="JIRA_SITE must be just the host name (e.g. company.atlassian.net)",
    positive_integer_required="{name} must be a positive integer",
    token_variable_empty="the variable {variable} (JIRA_TOKEN_ENV) is empty or missing",
    token_file_unreadable="could not read the JIRA_TOKEN_FILE file",
    token_file_empty="the JIRA_TOKEN_FILE file is empty",
    token_missing="set JIRA_TOKEN_ENV (name of the variable holding the token) or JIRA_TOKEN_FILE",
    response_too_large="the Jira response exceeds the maximum allowed size",
    network_failure="network failure talking to {site} ({error_type})",
    invalid_credentials="invalid credentials for {site}",
    rate_limited="Jira rate limit reached; will try again later",
    unexpected_status="unexpected response from Jira (HTTP {status})",
    response_not_json="the Jira response is not JSON",
    invalid_query="Invalid JQL",
    time_budget_exhausted="time budget exhausted",
    search_without_issues="the Jira search response has no issue list",
    offline_notice="data from {time} (offline)",
    empty_label="no issues",
    others_label="Others",
    nobody_label="unassigned",
    blank_text="cannot be empty",
    invalid_columns="invalid column: {names} (use {choices})",
    integer_required="must be an integer",
    limit_range="must be between 1 and {maximum}",
    meta_status="status",
    meta_priority="priority",
    meta_assignee="assignee",
    meta_site="site",
    column_titles={
        "key": "Key",
        "summary": "Summary",
        "status": "Status",
        "assignee": "Assignee",
        "priority": "Priority",
        "updated": "Updated",
    },
)

PORTUGUESE_MESSAGES = Messages(
    usage="uso: jira.py [opções] snapshot",
    error_line="erro: {message}",
    email_invalid="JIRA_EMAIL precisa ser um e-mail válido",
    removed_variable="{name} foi removido; use --jql",
    base_url_invalid="JIRA_BASE_URL precisa ser https ou um endereço local de teste",
    site_invalid="JIRA_SITE precisa ser só o nome do host (ex.: empresa.atlassian.net)",
    positive_integer_required="{name} precisa ser um número inteiro positivo",
    token_variable_empty="a variável {variable} (JIRA_TOKEN_ENV) está vazia ou ausente",
    token_file_unreadable="não foi possível ler o arquivo de JIRA_TOKEN_FILE",
    token_file_empty="o arquivo de JIRA_TOKEN_FILE está vazio",
    token_missing="defina JIRA_TOKEN_ENV (nome da variável com o token) ou JIRA_TOKEN_FILE",
    response_too_large="a resposta do Jira excede o tamanho máximo permitido",
    network_failure="falha de rede ao falar com {site} ({error_type})",
    invalid_credentials="credenciais inválidas para {site}",
    rate_limited="limite de requisições do Jira; tentando de novo mais tarde",
    unexpected_status="resposta inesperada do Jira (HTTP {status})",
    response_not_json="a resposta do Jira não é JSON",
    invalid_query="JQL inválido",
    time_budget_exhausted="orçamento de tempo esgotado",
    search_without_issues="a resposta de busca do Jira não tem a lista de issues",
    offline_notice="dados de {time} (offline)",
    empty_label="nenhum ticket",
    others_label="Outros",
    nobody_label="ninguem",
    blank_text="não pode ser vazio",
    invalid_columns="coluna inválida: {names} (use {choices})",
    integer_required="precisa ser um número inteiro",
    limit_range="precisa estar entre 1 e {maximum}",
    meta_status="status",
    meta_priority="prioridade",
    meta_assignee="responsavel",
    meta_site="site",
    column_titles={
        "key": "Chave",
        "summary": "Resumo",
        "status": "Status",
        "assignee": "Responsável",
        "priority": "Prioridade",
        "updated": "Atualizado",
    },
)

MESSAGES_BY_LANGUAGE = {ENGLISH: ENGLISH_MESSAGES, PORTUGUESE: PORTUGUESE_MESSAGES}


def resolve_language(environ: Mapping[str, str]) -> str:
    requested = environ.get(LANGUAGE_VARIABLE, "").strip().lower()
    return PORTUGUESE if requested.startswith(PORTUGUESE_LANGUAGE_PREFIX) else ENGLISH


def messages_for(language: str) -> Messages:
    return MESSAGES_BY_LANGUAGE[language]


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
    messages: Messages


@dataclass(frozen=True)
class WidgetOptions:
    jql: str
    title: str
    group_by: str
    view: str
    columns: tuple[str, ...]
    max_issues: int


def load_config(environ: Mapping[str, str]) -> JiraConfig:
    messages = messages_for(resolve_language(environ))
    _reject_removed_variables(environ, messages)
    site = _read_site(environ, messages)
    email = environ.get("JIRA_EMAIL", "").strip()
    if "@" not in email or not email.isprintable():
        raise ConfigError(messages.email_invalid)
    return JiraConfig(
        site=site,
        email=email,
        token=_read_token(environ, messages),
        state_dir=Path(environ.get("JIRA_STATE_DIR", DEFAULT_STATE_DIRECTORY)).expanduser(),
        budget_seconds=_read_positive(
            environ, "JIRA_BUDGET_SECONDS", DEFAULT_BUDGET_SECONDS, messages
        ),
        base_url=_read_base_url(environ, site, messages),
        messages=messages,
    )


def _reject_removed_variables(environ, messages):
    for name in REMOVED_VARIABLES:
        if name in environ:
            raise ConfigError(messages.removed_variable.format(name=name))


def parse_options(
    arguments: list[str], messages: Messages = ENGLISH_MESSAGES
) -> tuple[WidgetOptions, list[str]]:
    parser = _build_options_parser(messages)
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


def _build_options_parser(messages):
    parser = argparse.ArgumentParser(prog="jira.py", allow_abbrev=False)
    parser.add_argument("--jql", required=True, type=_non_blank_text(messages))
    parser.add_argument("--title", default=DEFAULT_TITLE)
    parser.add_argument("--group-by", default="none", choices=GROUP_BY_CHOICES)
    parser.add_argument("--view", default="lines", choices=VIEW_CHOICES)
    parser.add_argument("--columns", default=DEFAULT_COLUMNS, type=_column_list(messages))
    parser.add_argument("--max-issues", default=DEFAULT_MAX_ISSUES, type=_issue_limit(messages))
    return parser


def _non_blank_text(messages):
    def convert(raw):
        if not raw.strip():
            raise argparse.ArgumentTypeError(messages.blank_text)
        return raw

    return convert


def _column_list(messages):
    def convert(raw):
        columns = tuple(name.strip() for name in raw.split(COLUMNS_SEPARATOR))
        unknown = [name for name in columns if name not in COLUMN_CHOICES]
        if unknown:
            raise argparse.ArgumentTypeError(
                messages.invalid_columns.format(
                    names=", ".join(unknown), choices=", ".join(COLUMN_CHOICES)
                )
            )
        return columns

    return convert


def _issue_limit(messages):
    def convert(raw):
        try:
            limit = int(raw)
        except ValueError:
            raise argparse.ArgumentTypeError(messages.integer_required) from None
        if not 1 <= limit <= MAXIMUM_ISSUES:
            raise argparse.ArgumentTypeError(messages.limit_range.format(maximum=MAXIMUM_ISSUES))
        return limit

    return convert


def _read_base_url(environ, site, messages):
    override = environ.get("JIRA_BASE_URL", "").strip()
    if not override:
        return f"https://{site}"
    parsed = urllib.parse.urlparse(override)
    is_secure = parsed.scheme == "https" and bool(parsed.hostname)
    is_local = parsed.scheme == "http" and parsed.hostname in LOCAL_HOSTS
    if not (is_secure or is_local) or any(character.isspace() for character in override):
        raise ConfigError(messages.base_url_invalid)
    return override.rstrip("/")


def _read_site(environ, messages):
    site = environ.get("JIRA_SITE", "")
    if not site or len(site) > MAXIMUM_SITE_LENGTH or not SITE_PATTERN.fullmatch(site):
        raise ConfigError(messages.site_invalid)
    return site


def _read_positive(environ, name, default, messages):
    raw = environ.get(name)
    if raw is None or raw == "":
        return default
    if not raw.isascii() or not raw.isdigit() or int(raw) == 0:
        raise ConfigError(messages.positive_integer_required.format(name=name))
    return int(raw)


def _read_token(environ, messages):
    variable = environ.get("JIRA_TOKEN_ENV", "").strip()
    if variable:
        value = environ.get(variable, "").strip()
        if not value:
            raise ConfigError(messages.token_variable_empty.format(variable=variable))
        return value
    path = environ.get("JIRA_TOKEN_FILE", "").strip()
    if path:
        try:
            value = Path(path).expanduser().read_text(encoding="utf-8").strip()
        except OSError:
            raise ConfigError(messages.token_file_unreadable) from None
        if not value:
            raise ConfigError(messages.token_file_empty)
        return value
    raise ConfigError(messages.token_missing)


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


class ResponseTooLargeError(UnexpectedResponseError):
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
        raise ResponseTooLargeError
    return body


class JiraClient:
    def __init__(self, config, http):
        self._config = config
        self._http = http
        self._messages = config.messages
        credentials = f"{config.email}:{config.token}".encode("utf-8")
        self._headers = {
            "Authorization": "Basic " + base64.b64encode(credentials).decode("ascii"),
            "Accept": "application/json",
        }

    @property
    def messages(self):
        return self._messages

    def get_json(self, path, params=None):
        url = self._config.base_url + path
        if params:
            url += "?" + urllib.parse.urlencode(params)
        try:
            status, body = self._http(url, "GET", self._headers, REQUEST_TIMEOUT_SECONDS)
        except ResponseTooLargeError:
            raise UnexpectedResponseError(self._messages.response_too_large) from None
        except (OSError, http.client.HTTPException) as error:
            raise NetworkError(
                self._messages.network_failure.format(
                    site=self._config.site, error_type=type(error).__name__
                )
            ) from None
        return self._decode(status, body)

    def _decode(self, status, body):
        messages = self._messages
        if status in (HTTP_UNAUTHORIZED, HTTP_FORBIDDEN):
            raise AuthError(messages.invalid_credentials.format(site=self._config.site))
        if status == HTTP_BAD_REQUEST:
            raise QueryError(_describe_query_failure(body, messages))
        if status == HTTP_TOO_MANY_REQUESTS:
            raise RateLimitError(messages.rate_limited)
        if status >= HTTP_REDIRECT_FLOOR:
            raise UnexpectedResponseError(messages.unexpected_status.format(status=status))
        try:
            return json.loads(body)
        except ValueError:
            raise UnexpectedResponseError(messages.response_not_json) from None


def _describe_query_failure(body, messages):
    prefix = messages.invalid_query
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
            raise NetworkError(client.messages.time_budget_exhausted)
        remaining = max_issues - len(issues)
        payload = client.get_json(SEARCH_PATH, _search_params(jql, remaining, page_token))
        page_issues = _parse_search_page(payload, client.messages)
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


def _parse_search_page(payload, messages):
    raw_issues = payload.get("issues") if isinstance(payload, dict) else None
    if not isinstance(raw_issues, list):
        raise UnexpectedResponseError(messages.search_without_issues)
    parsed = (parse_issue(raw_issue) for raw_issue in raw_issues)
    return [issue for issue in parsed if issue is not None]


def build_snapshot(result, options, site, now, offline_since=None, messages=ENGLISH_MESSAGES):
    blocks = [_build_summary_block(result, options, now, messages)]
    if offline_since is not None:
        blocks.insert(0, _offline_notice_block(offline_since, messages))
    snapshot = {
        "protocol": PROTOCOL_VERSION,
        "summary": {"title": options.title, "blocks": blocks},
    }
    if result.issues:
        snapshot["detail"] = _build_detail(result.issues, options, site, messages)
    return snapshot


def _offline_notice_block(offline_since, messages):
    time_of_day = offline_since.astimezone().strftime(OFFLINE_TIME_FORMAT)
    text = messages.offline_notice.format(time=time_of_day)
    return {"type": "lines", "lines": [{"text": text, "tone": "warn"}]}


def _build_summary_block(result, options, now, messages):
    if options.view == "stat":
        return _stat_block(result, options, messages)
    if not result.issues:
        return {"type": "lines", "lines": [{"text": messages.empty_label, "tone": "muted"}]}
    visible = result.issues[:MAXIMUM_BLOCK_ROWS]
    if options.view == "table":
        return _table_block(visible, options.columns, now, messages)
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


def _table_block(issues, columns, now, messages):
    return {
        "type": "table",
        "columns": [{"title": messages.column_titles[column]} for column in columns],
        "rows": [[_cell_text(issue, column, now, messages) for column in columns] for issue in issues],
    }


def _cell_text(issue, column, now, messages):
    if column == "key":
        return issue.key
    if column == "summary":
        return flatten_text(issue.summary, SUMMARY_TEXT_LIMIT)
    if column == "status":
        return issue.status
    if column == "assignee":
        return _group_name(issue, "assignee", messages)
    if column == "priority":
        return _group_name(issue, "priority", messages)
    return format_duration(now - issue.updated)


def _stat_block(result, options, messages):
    suffix = MORE_RESULTS_SUFFIX if result.has_more else ""
    if options.group_by == "none" or not result.issues:
        counts = [(options.title, len(result.issues))]
    else:
        counts = _collapse_small_groups(
            [
                (name, len(members))
                for name, members in _group_issues(result.issues, options.group_by, messages)
            ],
            messages,
        )
    return {
        "type": "stat",
        "items": [{"value": f"{count}{suffix}", "label": label} for label, count in counts],
    }


def _collapse_small_groups(counts, messages):
    if len(counts) <= MAXIMUM_STAT_ITEMS:
        return counts
    kept = counts[: MAXIMUM_STAT_ITEMS - 1]
    others = sum(count for _, count in counts[MAXIMUM_STAT_ITEMS - 1 :])
    return kept + [(messages.others_label, others)]


def _group_name(issue, group_by, messages):
    if group_by == "assignee":
        return issue.assignee or messages.nobody_label
    if group_by == "priority":
        return issue.priority or MISSING_PRIORITY
    return issue.status


def _group_issues(issues, group_by, messages):
    groups = {}
    for issue in issues:
        groups.setdefault(_group_name(issue, group_by, messages), []).append(issue)
    return sorted(groups.items(), key=lambda group: (-len(group[1]), group[0]))


def _build_detail(issues, options, site, messages):
    if options.group_by == "none":
        groups = [(options.title, issues)]
    else:
        groups = _group_issues(issues, options.group_by, messages)
    return [
        {"heading": name, "items": [_detail_item(issue, site, messages) for issue in members]}
        for name, members in groups
    ]


def _detail_item(issue, site, messages):
    summary = flatten_text(issue.summary, SUMMARY_TEXT_LIMIT)
    return {
        "id": issue.key,
        "text": f"{issue.key}{DETAIL_SEPARATOR}{summary}",
        "meta": {
            messages.meta_status: issue.status,
            messages.meta_priority: issue.priority or MISSING_PRIORITY,
            messages.meta_assignee: issue.assignee or messages.nobody_label,
            messages.meta_site: site,
        },
    }


class OfflineCache:
    def __init__(self, directory: Path):
        self._directory = directory

    def path_for(self, site: str, jql: str, max_issues: int) -> Path:
        key = CACHE_KEY_SEPARATOR.join((site, jql, str(max_issues)))
        digest = hashlib.sha256(key.encode("utf-8")).hexdigest()
        return self._directory / f"{digest}{CACHE_FILE_SUFFIX}"

    def save(self, site, jql, max_issues, result, moment):
        document = {
            "saved_at": moment.isoformat(),
            "has_more": result.has_more,
            "issues": [issue.to_raw() for issue in result.issues],
        }
        try:
            self._directory.mkdir(mode=STATE_DIRECTORY_MODE, parents=True, exist_ok=True)
            self._write_atomically(self.path_for(site, jql, max_issues), json.dumps(document))
        except OSError:
            return

    def _write_atomically(self, destination, content):
        descriptor, temporary_name = tempfile.mkstemp(dir=self._directory, suffix=".tmp")
        try:
            with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
                handle.write(content)
            os.replace(temporary_name, destination)
        except OSError:
            Path(temporary_name).unlink(missing_ok=True)
            raise

    def load(self, site, jql, max_issues):
        try:
            document = json.loads(self.path_for(site, jql, max_issues).read_text(encoding="utf-8"))
            issues = [Issue.from_raw(raw) for raw in document["issues"]]
            saved_at = parse_timestamp(document["saved_at"])
            has_more = document["has_more"]
        except (OSError, ValueError, KeyError, TypeError, AttributeError):
            return None
        if saved_at is None or not isinstance(has_more, bool) or not all(_is_complete(i) for i in issues):
            return None
        return SearchResult(issues, has_more), saved_at


def _is_complete(issue):
    return issue.created is not None and issue.updated is not None


def main(
    arguments: list[str],
    environ=None,
    http=urllib_http,
    now=None,
    clock=time.monotonic,
    stdout=None,
    stderr=None,
) -> int:
    environ = os.environ if environ is None else environ
    stdout = sys.stdout if stdout is None else stdout
    stderr = sys.stderr if stderr is None else stderr
    messages = messages_for(resolve_language(environ))
    try:
        options, remainder = parse_options(arguments, messages)
    except SystemExit:
        return ARGUMENT_ERROR_EXIT_CODE
    if remainder != SNAPSHOT_COMMAND:
        print(messages.usage, file=stderr)
        return ARGUMENT_ERROR_EXIT_CODE
    try:
        config = load_config(environ)
    except ConfigError as error:
        return _fail(stderr, error, messages)
    moment = now if now is not None else datetime.now(timezone.utc)
    return _run_snapshot(config, options, http, moment, clock, stdout, stderr)


def _run_snapshot(config, options, http, moment, clock, stdout, stderr):
    cache = OfflineCache(config.state_dir)
    cache_key = (config.site, options.jql, options.max_issues)
    try:
        result = search_issues(
            JiraClient(config, http), options.jql, options.max_issues, config.budget_seconds, clock
        )
    except (AuthError, QueryError) as error:
        return _fail(stderr, error, config.messages)
    except (RateLimitError, NetworkError, UnexpectedResponseError) as error:
        cached = cache.load(*cache_key)
        if cached is None:
            return _fail(stderr, error, config.messages)
        stale_result, saved_at = cached
        _print_snapshot(stdout, stale_result, options, config, moment, saved_at)
        return 0
    cache.save(*cache_key, result, moment)
    _print_snapshot(stdout, result, options, config, moment, None)
    return 0


def _print_snapshot(stdout, result, options, config, moment, offline_since):
    snapshot = build_snapshot(result, options, config.site, moment, offline_since, config.messages)
    print(json.dumps(snapshot, ensure_ascii=False), file=stdout)


def _fail(stderr, error, messages):
    print(messages.error_line.format(message=error), file=stderr)
    return FAILURE_EXIT_CODE


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
