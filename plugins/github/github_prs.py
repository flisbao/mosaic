#!/usr/bin/env python3
"""Plugin do mosaic: pull requests do GitHub que esperam por você (somente leitura)."""

from __future__ import annotations

import hashlib
import http.client
import json
import os
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable, Mapping
from dataclasses import dataclass, replace
from datetime import datetime, timedelta, timezone
from pathlib import Path

DEFAULT_API_URL = "https://api.github.com"
GRAPHQL_PATH = "/graphql"
DEFAULT_TITLE = "GitHub"
DEFAULT_STATE_DIRECTORY = "~/.local/state/mosaic/github"
REQUEST_TIMEOUT_SECONDS = 20
MAXIMUM_RESPONSE_BYTES = 8 * 1024 * 1024
MAXIMUM_ERROR_BODY_BYTES = 8 * 1024
LOCAL_HOSTS = ("127.0.0.1", "localhost")
SERVICE_NAME = "GitHub"
HTTP_REDIRECT_FLOOR = 300
HTTP_UNAUTHORIZED = 401
HTTP_FORBIDDEN = 403
HTTP_TOO_MANY_REQUESTS = 429
RATE_LIMIT_MARKER = b"rate limit"
RATE_LIMITED_ERROR_TYPE = "RATE_LIMITED"
OWNERS_SEPARATOR = ","
API_URL_VARIABLE = "GITHUB_API_URL"
TOKEN_PREFIX = "GITHUB"
STATE_DIRECTORY_VARIABLE = "GITHUB_STATE_DIR"
TIMESTAMP_FORMATS = ("%Y-%m-%dT%H:%M:%S.%f%z", "%Y-%m-%dT%H:%M:%S%z")
UTC_OFFSET_SUFFIX = "+0000"
UTC_ZULU_SUFFIX = "Z"
REVIEW_SECTION = "Aguardando minha revisão"
MINE_SECTION = "Meus PRs"
UNREVIEWED = "unreviewed"
REVIEWED = "reviewed"
APPROVED = "approved"
UNKNOWN_AUTHOR = "desconhecido"
STATE_NEW = "novo"
STATE_PUSHED = "push novo"
STATE_CONFLICT = "conflito"
STATE_CHANGES = "mudança pedida"
STATE_APPROVED = "aprovado"
STATE_WAITING = "aguardando"
REVIEW_SEARCH = "is:pr is:open draft:false archived:false review-requested:@me"
REVIEWED_SEARCH = "is:pr is:open draft:false archived:false reviewed-by:@me -author:@me"
MINE_SEARCH = "is:pr is:open draft:false archived:false author:@me"
OWNER_QUALIFIER = "user:{owner}"
REVIEW_ALIAS = "review"
REVIEWED_ALIAS = "reviewed"
MINE_ALIAS = "mine"
REVIEW_STATE_APPROVED = "APPROVED"
REVIEW_STATE_CHANGES_REQUESTED = "CHANGES_REQUESTED"
DECISION_CHANGES_REQUESTED = "CHANGES_REQUESTED"
DECISION_APPROVED = "APPROVED"
MERGEABLE_CONFLICTING = "CONFLICTING"
TITLE_TEXT_LIMIT = 80
MAXIMUM_BLOCK_ROWS = 200
PROTOCOL_VERSION = 1
ITEM_SEPARATOR = " · "
EMPTY_TEXT = "nada pendente"
MINI_TEXT = "para revisar"
REVIEW_LABEL = "revisar"
MINE_LABEL = "meus PRs"
ALERT_ID_PREFIX = "review-"
MAXIMUM_ALERT_ID_LENGTH = 100
ALERT_TITLE_PREFIX = "Revisão pedida: "
ALERT_LEVEL = "info"
UPDATED_FLAG = "updated"
TONE_NORMAL = "normal"
TONE_MUTED = "muted"
TONE_WARN = "warn"
TONE_OK = "ok"
TONE_CURRENT = "current"
TONE_DANGER = "danger"
TONE_BY_STATE = {
    STATE_NEW: TONE_CURRENT,
    STATE_PUSHED: TONE_WARN,
    STATE_CONFLICT: TONE_DANGER,
    STATE_CHANGES: TONE_WARN,
    STATE_APPROVED: TONE_OK,
    STATE_WAITING: TONE_MUTED,
}
OFFLINE_NOTICE_FORMAT = "dados de %H:%M (offline)"
SECONDS_PER_MINUTE = 60
MINUTES_PER_HOUR = 60
HOURS_PER_DAY = 24
HOURS_SHOWN_BEFORE_DAYS = 48
ELLIPSIS = "…"
STATE_FILE_SUFFIX = ".json"
TEMPORARY_FILE_SUFFIX = ".tmp"
STATE_DIRECTORY_MODE = 0o700
ARGUMENT_ERROR_EXIT_CODE = 2
FAILURE_EXIT_CODE = 1
USAGE_MESSAGE = "uso: github_prs.py snapshot"
SNAPSHOT_COMMAND = ["snapshot"]
STATE_KEY_SEPARATOR = "\n"
TOKEN_VARIABLE_KEY = f"{TOKEN_PREFIX}_TOKEN_ENV"
TOKEN_FILE_KEY = f"{TOKEN_PREFIX}_TOKEN_FILE"


class ConfigError(Exception):
    pass


@dataclass(frozen=True)
class GitHubConfig:
    api_url: str
    token: str
    owners: tuple[str, ...]
    title: str
    state_dir: Path


def load_config(environ: Mapping[str, str]) -> GitHubConfig:
    return GitHubConfig(
        api_url=read_base_url(environ, API_URL_VARIABLE, DEFAULT_API_URL),
        token=read_token(environ, TOKEN_PREFIX),
        owners=_read_owners(environ),
        title=environ.get("GITHUB_TITLE", "").strip() or DEFAULT_TITLE,
        state_dir=read_state_dir(environ, STATE_DIRECTORY_VARIABLE, DEFAULT_STATE_DIRECTORY),
    )


def _read_owners(environ: Mapping[str, str]) -> tuple[str, ...]:
    names = (name.strip() for name in environ.get("GITHUB_OWNERS", "").split(OWNERS_SEPARATOR))
    return tuple(name for name in names if name)


def read_base_url(environ: Mapping[str, str], variable: str, default: str) -> str:
    override = environ.get(variable, "").strip()
    if not override:
        return default
    parsed = urllib.parse.urlparse(override)
    is_secure = parsed.scheme == "https" and bool(parsed.hostname)
    is_local = parsed.scheme == "http" and parsed.hostname in LOCAL_HOSTS
    if not (is_secure or is_local) or any(character.isspace() for character in override):
        raise ConfigError(f"{variable} precisa ser https ou um endereço local de teste")
    return override.rstrip("/")


def read_state_dir(environ: Mapping[str, str], variable: str, default: str) -> Path:
    raw = environ.get(variable, "").strip() or default
    return Path(raw).expanduser()


def read_token(environ: Mapping[str, str], prefix: str) -> str:
    return _ensure_token_is_header_safe(_read_token_source(environ, prefix), prefix)


def _ensure_token_is_header_safe(value: str, prefix: str) -> str:
    if any(character.isspace() or not character.isprintable() for character in value) or not _is_latin1(value):
        raise ConfigError(
            f"o token de {prefix}_TOKEN_ENV/{prefix}_TOKEN_FILE contém caracteres inválidos"
        )
    return value


def _is_latin1(value: str) -> bool:
    try:
        value.encode("latin-1")
    except UnicodeEncodeError:
        return False
    return True


def _read_token_source(environ: Mapping[str, str], prefix: str) -> str:
    variable_key = f"{prefix}_TOKEN_ENV"
    file_key = f"{prefix}_TOKEN_FILE"
    variable = environ.get(variable_key, "").strip()
    if variable:
        return _read_token_variable(environ, variable, variable_key)
    path = environ.get(file_key, "").strip()
    if path:
        return _read_token_file(path, file_key)
    raise ConfigError(f"defina {variable_key} (nome da variável com o token) ou {file_key}")


def _read_token_variable(environ: Mapping[str, str], variable: str, variable_key: str) -> str:
    value = environ.get(variable, "").strip()
    if not value:
        raise ConfigError(f"a variável {variable} ({variable_key}) está vazia ou ausente")
    return value


def _read_token_file(path: str, file_key: str) -> str:
    try:
        value = Path(path).expanduser().read_text(encoding="utf-8").strip()
    except (OSError, UnicodeDecodeError):
        raise ConfigError(f"não foi possível ler o arquivo de {file_key}") from None
    if not value:
        raise ConfigError(f"o arquivo de {file_key} está vazio")
    return value


class ApiError(Exception):
    pass


class AuthError(ApiError):
    pass


class RateLimitError(ApiError):
    pass


class NetworkError(ApiError):
    pass


class UnexpectedResponseError(ApiError):
    pass


class _RefuseRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, file_pointer, code, message, headers, new_url):
        return None


_OPENER = urllib.request.build_opener(_RefuseRedirects)


def urllib_http(url: str, method: str, headers: dict, body: bytes | None, timeout: int) -> tuple[int, bytes]:
    request = urllib.request.Request(url, data=body, method=method, headers=headers)
    try:
        with _OPENER.open(request, timeout=timeout) as response:
            return response.status, _read_limited(response, MAXIMUM_RESPONSE_BYTES)
    except urllib.error.HTTPError as error:
        with error:
            return error.code, error.read(MAXIMUM_ERROR_BODY_BYTES)


def _read_limited(stream, limit: int) -> bytes:
    body = stream.read(limit + 1)
    if len(body) > limit:
        raise UnexpectedResponseError(f"a resposta do {SERVICE_NAME} excede o tamanho máximo permitido")
    return body


def decode_response(status: int, body: bytes, service: str) -> object:
    if status == HTTP_UNAUTHORIZED:
        raise AuthError(_auth_message(service))
    if status == HTTP_FORBIDDEN:
        if RATE_LIMIT_MARKER in body.lower():
            raise RateLimitError(_rate_limit_message(service))
        raise AuthError(_auth_message(service))
    if status == HTTP_TOO_MANY_REQUESTS:
        raise RateLimitError(_rate_limit_message(service))
    if status >= HTTP_REDIRECT_FLOOR:
        raise UnexpectedResponseError(f"resposta inesperada do {service} (HTTP {status})")
    try:
        return json.loads(body)
    except ValueError:
        raise UnexpectedResponseError(f"a resposta do {service} não é JSON") from None


def _auth_message(service: str) -> str:
    return f"token inválido ou sem permissão para {service}"


def _rate_limit_message(service: str) -> str:
    return f"limite de requisições do {service}; tento de novo na próxima leitura"


class GitHubClient:
    def __init__(self, config: GitHubConfig, http):
        self._config = config
        self._http = http
        self._headers = {
            "Authorization": f"Bearer {config.token}",
            "Content-Type": "application/json",
            "Accept": "application/json",
        }

    @property
    def owners(self) -> tuple[str, ...]:
        return self._config.owners

    def query(self, document: str, variables: dict) -> dict:
        payload = self._post_graphql(document, variables)
        _raise_on_graphql_errors(payload)
        data = payload.get("data") if isinstance(payload, dict) else None
        if not isinstance(data, dict):
            raise UnexpectedResponseError(f"o {SERVICE_NAME} recusou a consulta")
        return data

    def _post_graphql(self, document: str, variables: dict) -> object:
        body = json.dumps({"query": document, "variables": variables}).encode("utf-8")
        url = self._config.api_url + GRAPHQL_PATH
        try:
            status, response_body = self._http(url, "POST", self._headers, body, REQUEST_TIMEOUT_SECONDS)
        except (OSError, http.client.HTTPException) as error:
            raise NetworkError(
                f"falha de rede ao falar com {SERVICE_NAME} ({type(error).__name__})"
            ) from None
        return decode_response(status, response_body, SERVICE_NAME)


def _raise_on_graphql_errors(payload: object) -> None:
    if not isinstance(payload, dict):
        return
    errors = payload.get("errors")
    if isinstance(errors, list) and any(_is_rate_limited(error) for error in errors):
        raise RateLimitError(_rate_limit_message(SERVICE_NAME))


def _is_rate_limited(error: object) -> bool:
    return isinstance(error, dict) and error.get("type") == RATE_LIMITED_ERROR_TYPE


@dataclass(frozen=True)
class PullRequest:
    item_id: str
    repository: str
    reference: str
    title: str
    author: str
    created_at: datetime
    url: str
    target_branch: str
    head_commit: str
    section: str
    review_status: str = ""
    pushed_after_review: bool | None = None
    state: str = ""


PULL_REQUESTS_QUERY = """
query PullRequests($review: String!, $reviewed: String!, $mine: String!) {
  viewer { login }
  review: search(type: ISSUE, query: $review, first: 100) { nodes { ...pullRequest } }
  reviewed: search(type: ISSUE, query: $reviewed, first: 100) { nodes { ...pullRequest } }
  mine: search(type: ISSUE, query: $mine, first: 100) { nodes { ...pullRequest } }
}
fragment pullRequest on PullRequest {
  number
  title
  url
  createdAt
  isDraft
  author { login }
  repository { nameWithOwner }
  baseRefName
  headRefOid
  mergeable
  reviewDecision
  commits(last: 1) { nodes { commit { committedDate } } }
  latestReviews(first: 100) { nodes { author { login } state submittedAt } }
}
"""


def parse_timestamp(text: str | None) -> datetime | None:
    if not isinstance(text, str) or not text:
        return None
    candidate = text[:-1] + UTC_OFFSET_SUFFIX if text.endswith(UTC_ZULU_SUFFIX) else text
    for timestamp_format in TIMESTAMP_FORMATS:
        try:
            return datetime.strptime(candidate, timestamp_format).astimezone(timezone.utc)
        except ValueError:
            continue
    return None


def search_queries(owners: tuple[str, ...]) -> dict[str, str]:
    suffix = _owners_suffix(owners)
    return {
        REVIEW_ALIAS: REVIEW_SEARCH + suffix,
        REVIEWED_ALIAS: REVIEWED_SEARCH + suffix,
        MINE_ALIAS: MINE_SEARCH + suffix,
    }


def _owners_suffix(owners: tuple[str, ...]) -> str:
    if not owners:
        return ""
    return " " + " ".join(OWNER_QUALIFIER.format(owner=owner) for owner in owners)


def fetch_pull_requests(client: GitHubClient) -> list[PullRequest]:
    return normalize(client.query(PULL_REQUESTS_QUERY, search_queries(client.owners)))


def normalize(data: dict) -> list[PullRequest]:
    viewer_login = _nested_text(data, "viewer", "login")
    pulls: dict[str, PullRequest] = {}
    for node in _connection_nodes(data.get(REVIEW_ALIAS)):
        _keep_first(pulls, _review_pull(node, viewer_login))
    for node in _connection_nodes(data.get(REVIEWED_ALIAS)):
        _keep_first(pulls, _reviewed_pull(node, viewer_login))
    for node in _connection_nodes(data.get(MINE_ALIAS)):
        _keep_first(pulls, _mine_pull(node))
    return sorted(pulls.values(), key=lambda pull: pull.created_at)


def _keep_first(pulls: dict[str, PullRequest], pull: PullRequest | None) -> None:
    if pull is not None:
        pulls.setdefault(pull.item_id, pull)


def _review_pull(node: dict, viewer_login: str) -> PullRequest | None:
    pull = _base_pull(node)
    if pull is None:
        return None
    review = _my_review(node, viewer_login)
    if review is None:
        return _in_review_section(pull, UNREVIEWED, pushed_after_review=False)
    status = UNREVIEWED if review.get("state") == REVIEW_STATE_APPROVED else REVIEWED
    return _in_review_section(pull, status, _pushed_after_review(node, review))


def _reviewed_pull(node: dict, viewer_login: str) -> PullRequest | None:
    pull = _base_pull(node)
    review = _my_review(node, viewer_login)
    if pull is None or review is None or review.get("state") != REVIEW_STATE_CHANGES_REQUESTED:
        return None
    return _in_review_section(pull, REVIEWED, _pushed_after_review(node, review))


def _mine_pull(node: dict) -> PullRequest | None:
    pull = _base_pull(node)
    if pull is None:
        return None
    return replace(pull, section=MINE_SECTION, state=_mine_state(node))


def _in_review_section(pull: PullRequest, status: str, pushed_after_review: bool) -> PullRequest:
    return replace(
        pull,
        section=REVIEW_SECTION,
        review_status=status,
        pushed_after_review=pushed_after_review,
    )


def _mine_state(node: dict) -> str:
    if node.get("mergeable") == MERGEABLE_CONFLICTING:
        return STATE_CONFLICT
    decision = node.get("reviewDecision")
    if decision == DECISION_CHANGES_REQUESTED:
        return STATE_CHANGES
    if decision == DECISION_APPROVED:
        return STATE_APPROVED
    return STATE_WAITING


def _base_pull(node: dict) -> PullRequest | None:
    number = node.get("number")
    created_at = parse_timestamp(node.get("createdAt"))
    if not isinstance(number, int) or created_at is None or node.get("isDraft") is True:
        return None
    repository = _nested_text(node, "repository", "nameWithOwner")
    return PullRequest(
        item_id=f"{repository}#{number}",
        repository=repository,
        reference=f"#{number}",
        title=node.get("title") or "",
        author=_nested_text(node, "author", "login") or UNKNOWN_AUTHOR,
        created_at=created_at,
        url=node.get("url") or "",
        target_branch=node.get("baseRefName") or "",
        head_commit=node.get("headRefOid") or "",
        section="",
    )


def _my_review(node: dict, viewer_login: str) -> dict | None:
    for review in _connection_nodes(node.get("latestReviews")):
        if _nested_text(review, "author", "login") == viewer_login:
            return review
    return None


def _pushed_after_review(node: dict, review: dict) -> bool:
    committed_at = parse_timestamp(_last_commit_date(node))
    submitted_at = parse_timestamp(review.get("submittedAt"))
    if committed_at is None or submitted_at is None:
        return False
    return committed_at > submitted_at


def _last_commit_date(node: dict) -> str | None:
    commits = _connection_nodes(node.get("commits"))
    if not commits:
        return None
    commit = commits[0].get("commit")
    return commit.get("committedDate") if isinstance(commit, dict) else None


def _connection_nodes(connection: object) -> list[dict]:
    if not isinstance(connection, dict):
        return []
    nodes = connection.get("nodes")
    if not isinstance(nodes, list):
        return []
    return [node for node in nodes if isinstance(node, dict)]


def _nested_text(node: dict, key: str, field: str) -> str:
    inner = node.get(key)
    value = inner.get(field) if isinstance(inner, dict) else None
    return value or ""


def apply_review_memory(
    pull_requests: list[PullRequest], memory: dict
) -> tuple[list[PullRequest], dict]:
    displayed, remembered = [], {}
    for pull in pull_requests:
        if pull.section != REVIEW_SECTION:
            displayed.append(pull)
            continue
        previous = _previous_entry(memory, pull.item_id)
        reviewed_head = _reviewed_head_after(pull, previous)
        remembered[pull.item_id] = {
            "head": pull.head_commit,
            "reviewed_head": reviewed_head,
            "status": pull.review_status,
        }
        if pull.review_status != APPROVED:
            displayed.append(replace(pull, state=_review_state(pull, reviewed_head)))
    return displayed, remembered


def _previous_entry(memory: dict, item_id: str) -> dict:
    if not isinstance(memory, dict):
        return {}
    entry = memory.get(item_id)
    return entry if isinstance(entry, dict) else {}


def _reviewed_head_after(pull: PullRequest, previous: dict) -> str | None:
    reviewed_head = previous.get("reviewed_head")
    if pull.review_status in (REVIEWED, APPROVED):
        if reviewed_head is None or previous.get("status") != pull.review_status:
            return pull.head_commit
    return reviewed_head


def _review_state(pull: PullRequest, reviewed_head: str | None) -> str:
    if pull.pushed_after_review is not None:
        if pull.pushed_after_review:
            return STATE_PUSHED
        return STATE_NEW if pull.review_status == UNREVIEWED else ""
    pushed = reviewed_head is not None and reviewed_head != pull.head_commit
    if pushed:
        return STATE_PUSHED
    return STATE_NEW if pull.review_status == UNREVIEWED else ""


def format_age(delta: timedelta) -> str:
    minutes = int(delta.total_seconds() // SECONDS_PER_MINUTE)
    if minutes <= 0:
        return "0m"
    if minutes < MINUTES_PER_HOUR:
        return f"{minutes}m"
    hours = minutes // MINUTES_PER_HOUR
    if hours < HOURS_SHOWN_BEFORE_DAYS:
        return f"{hours}h"
    return f"{hours // HOURS_PER_DAY}d"


def flatten_text(text, limit: int) -> str:
    flattened = " ".join(str(text).split())
    if len(flattened) <= limit:
        return flattened
    if limit <= 0:
        return ""
    return flattened[: limit - 1] + ELLIPSIS


def build_snapshot(pull_requests: list[PullRequest], title: str, now: datetime) -> dict:
    review = _sorted_by_creation(pull for pull in pull_requests if pull.section == REVIEW_SECTION)
    mine = _sorted_by_creation(pull for pull in pull_requests if pull.section == MINE_SECTION)
    return {
        "protocol": PROTOCOL_VERSION,
        "summary": {"title": title, "blocks": _summary_blocks(review, mine, now)},
        "detail": [
            {"heading": REVIEW_SECTION, "items": [_detail_item(pull, now) for pull in review]},
            {"heading": MINE_SECTION, "items": [_detail_item(pull, now) for pull in mine]},
        ],
        "mini": _mini_block(review),
        "alerts": [_alert(pull) for pull in review if pull.state],
    }


def _sorted_by_creation(pulls) -> list[PullRequest]:
    return sorted(pulls, key=lambda pull: pull.created_at)


def _summary_blocks(review: list[PullRequest], mine: list[PullRequest], now: datetime) -> list[dict]:
    blocks = [_stat_block(review, mine)]
    if review:
        blocks.append({"type": "rows", "rows": [_row(pull, now) for pull in review[:MAXIMUM_BLOCK_ROWS]]})
    elif not mine:
        blocks.append({"type": "lines", "lines": [{"text": EMPTY_TEXT, "tone": TONE_MUTED}]})
    return blocks


def _stat_block(review: list[PullRequest], mine: list[PullRequest]) -> dict:
    return {
        "type": "stat",
        "items": [
            {"value": str(len(review)), "label": REVIEW_LABEL},
            {"value": str(len(mine)), "label": MINE_LABEL},
        ],
    }


def _row(pull: PullRequest, now: datetime) -> dict:
    row = {"text": _pull_heading(pull), "time": format_age(now - pull.created_at)}
    if pull.state:
        row["tags"] = [{"text": pull.state, "tone": TONE_BY_STATE[pull.state]}]
    return row


def _pull_heading(pull: PullRequest) -> str:
    return f"{pull.repository} {pull.reference} {flatten_text(pull.title, TITLE_TEXT_LIMIT)}"


def _detail_item(pull: PullRequest, now: datetime) -> dict:
    item = {
        "id": pull.item_id,
        "text": _detail_text(pull, now),
        "meta": {
            "url": pull.url,
            "target_branch": pull.target_branch,
            "state": pull.state,
            "author": pull.author,
        },
    }
    if pull.state == STATE_PUSHED:
        item["flag"] = UPDATED_FLAG
    return item


def _detail_text(pull: PullRequest, now: datetime) -> str:
    text = ITEM_SEPARATOR.join((_pull_heading(pull), pull.author, format_age(now - pull.created_at)))
    if pull.state:
        text += f" [{pull.state}]"
    return text


def _mini_block(review: list[PullRequest]) -> dict:
    return {
        "value": str(len(review)),
        "text": MINI_TEXT,
        "tone": TONE_WARN if review else TONE_NORMAL,
    }


def _alert(pull: PullRequest) -> dict:
    return {
        "id": (ALERT_ID_PREFIX + pull.item_id)[:MAXIMUM_ALERT_ID_LENGTH],
        "title": f"{ALERT_TITLE_PREFIX}{pull.repository} {pull.reference}",
        "body": flatten_text(pull.title, TITLE_TEXT_LIMIT),
        "level": ALERT_LEVEL,
    }


def with_offline_notice(snapshot: dict, saved_at: datetime) -> dict:
    summary = snapshot["summary"]
    blocks = [_offline_notice_block(saved_at), *summary["blocks"]]
    return {**snapshot, "summary": {**summary, "blocks": blocks}}


def _offline_notice_block(saved_at: datetime) -> dict:
    text = saved_at.astimezone().strftime(OFFLINE_NOTICE_FORMAT)
    return {"type": "lines", "lines": [{"text": text, "tone": TONE_WARN}]}


class StateStore:
    def __init__(self, directory: Path, key: str):
        self._directory = directory
        digest = hashlib.sha256(key.encode("utf-8")).hexdigest()
        self._path = directory / (digest + STATE_FILE_SUFFIX)

    def load(self) -> dict:
        try:
            document = json.loads(self._path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return _empty_state()
        if not isinstance(document, dict) or not isinstance(document.get("reviews"), dict):
            return _empty_state()
        return {
            "reviews": document["reviews"],
            "snapshot": document.get("snapshot"),
            "saved_at": document.get("saved_at"),
        }

    def save(self, document: dict) -> None:
        try:
            self._directory.mkdir(mode=STATE_DIRECTORY_MODE, parents=True, exist_ok=True)
            _write_atomically(self._path, json.dumps(document))
        except OSError:
            return


def state_key(config: GitHubConfig, environ: Mapping[str, str]) -> str:
    return STATE_KEY_SEPARATOR.join(
        (config.api_url, OWNERS_SEPARATOR.join(config.owners), _token_source(environ), config.title)
    )


def _token_source(environ: Mapping[str, str]) -> str:
    variable = environ.get(TOKEN_VARIABLE_KEY, "").strip()
    if variable:
        return f"env:{variable}"
    return f"file:{environ.get(TOKEN_FILE_KEY, '').strip()}"


def _empty_state() -> dict:
    return {"reviews": {}, "snapshot": None, "saved_at": None}


def _write_atomically(destination: Path, content: str) -> None:
    descriptor, temporary_name = tempfile.mkstemp(dir=destination.parent, suffix=TEMPORARY_FILE_SUFFIX)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            handle.write(content)
        os.replace(temporary_name, destination)
    except OSError:
        Path(temporary_name).unlink(missing_ok=True)
        raise


def run_snapshot(
    store: StateStore,
    title: str,
    fetch: Callable[[], list[PullRequest]],
    moment: datetime,
    stdout,
    stderr,
) -> int:
    state = store.load()
    try:
        pull_requests = fetch()
    except AuthError as error:
        return _fail(stderr, error)
    except (RateLimitError, NetworkError, UnexpectedResponseError) as error:
        cached = _cached_snapshot(state)
        if cached is None:
            return _fail(stderr, error)
        snapshot, saved_at = cached
        _print_json(stdout, with_offline_notice(snapshot, saved_at))
        return 0
    shown, remembered = apply_review_memory(pull_requests, state["reviews"])
    snapshot = build_snapshot(shown, title, moment)
    store.save({"saved_at": moment.isoformat(), "snapshot": snapshot, "reviews": remembered})
    _print_json(stdout, snapshot)
    return 0


def _cached_snapshot(state: dict) -> tuple[dict, datetime] | None:
    snapshot = state.get("snapshot")
    saved_at = parse_timestamp(state.get("saved_at"))
    if not _has_summary_blocks(snapshot) or saved_at is None:
        return None
    return snapshot, saved_at


def _has_summary_blocks(snapshot: object) -> bool:
    if not isinstance(snapshot, dict):
        return False
    summary = snapshot.get("summary")
    return isinstance(summary, dict) and isinstance(summary.get("blocks"), list)


def _print_json(stdout, document: dict) -> None:
    print(json.dumps(document, ensure_ascii=False), file=stdout)


def _fail(stderr, error: Exception) -> int:
    print(f"erro: {error}", file=stderr)
    return FAILURE_EXIT_CODE


def main(
    arguments: list[str],
    environ=None,
    http=urllib_http,
    now=None,
    stdout=None,
    stderr=None,
) -> int:
    environ = os.environ if environ is None else environ
    stdout = sys.stdout if stdout is None else stdout
    stderr = sys.stderr if stderr is None else stderr
    if list(arguments) != SNAPSHOT_COMMAND:
        print(USAGE_MESSAGE, file=stderr)
        return ARGUMENT_ERROR_EXIT_CODE
    try:
        config = load_config(environ)
    except ConfigError as error:
        return _fail(stderr, error)
    store = StateStore(config.state_dir, state_key(config, environ))
    client = GitHubClient(config, http)
    moment = now if now is not None else datetime.now(timezone.utc)
    return run_snapshot(
        store,
        config.title,
        lambda: fetch_pull_requests(client),
        moment,
        stdout,
        stderr,
    )


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
