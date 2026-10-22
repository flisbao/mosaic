#!/usr/bin/env python3
"""Plugin do mosaic: pull requests do GitHub que esperam por você (somente leitura)."""

from __future__ import annotations

import http.client
import json
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Mapping
from dataclasses import dataclass
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
