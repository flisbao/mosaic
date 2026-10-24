#!/usr/bin/env python3
"""Plugin do mosaic: as pipelines mais lentas do Azure DevOps e os passos que mais demoram (somente leitura)."""

from __future__ import annotations

import hashlib
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
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field, replace
from datetime import datetime, timedelta, timezone
from http.client import HTTPException
from pathlib import Path
from typing import Protocol

DEFAULT_BASE_URL = "https://dev.azure.com"
DEFAULT_TITLE = "Pipelines lentas"
DEFAULT_STATE_DIRECTORY = "~/.local/state/mosaic/azure-pipelines"
SERVICE_NAME = "Azure DevOps"
API_VERSION = "7.1"
BASE_URL_VARIABLE = "AZDO_BASE_URL"
ORGANIZATION_VARIABLE = "AZDO_ORGANIZATION"
PROJECTS_VARIABLE = "AZDO_PROJECTS"
TITLE_VARIABLE = "AZDO_TITLE"
STATE_DIRECTORY_VARIABLE = "AZDO_STATE_DIR"
DAYS_VARIABLE = "AZDO_PIPELINE_DAYS"
TOP_VARIABLE = "AZDO_PIPELINE_TOP"
SAMPLE_VARIABLE = "AZDO_PIPELINE_SAMPLE"
ORGANIZATION_PATTERN = re.compile(r"[A-Za-z0-9][A-Za-z0-9-]*")
FRACTION_BEYOND_MICROSECONDS = re.compile(r"(\.\d{6})\d+")
LIST_SEPARATOR = ","
API_VERSION_PARAMETER = "api-version"
TOP_PARAMETER = "$top"
STATE_FILTER_PARAMETER = "stateFilter"
WELL_FORMED_PROJECTS = "wellFormed"
MIN_TIME_PARAMETER = "minTime"
STATUS_FILTER_PARAMETER = "statusFilter"
COMPLETED_STATUS = "completed"
QUERY_ORDER_PARAMETER = "queryOrder"
FINISH_TIME_DESCENDING = "finishTimeDescending"
MINIMUM_TIME_FORMAT = "%Y-%m-%dT%H:%M:%SZ"
HTTP_SIGN_IN_PAGE = 203
HTTP_REDIRECT_FLOOR = 300
HTTP_CLIENT_ERROR_FLOOR = 400
HTTP_BAD_REQUEST = 400
HTTP_UNAUTHORIZED = 401
HTTP_FORBIDDEN = 403
HTTP_NOT_FOUND = 404
HTTP_GONE = 410
HTTP_TOO_MANY_REQUESTS = 429
REJECTED_STATUSES = (HTTP_BAD_REQUEST, HTTP_NOT_FOUND, HTTP_GONE)
RATE_LIMIT_MARKER = b"rate limit"
REQUEST_TIMEOUT_SECONDS = 20
MAXIMUM_RESPONSE_BYTES = 8 * 1024 * 1024
MAXIMUM_ERROR_BODY_BYTES = 8 * 1024
LOCAL_HOSTS = ("127.0.0.1", "localhost")
IDENTITY_NOT_IN_ORGANIZATION_MARKER = b"TF401444"
IDENTITY_NOT_IN_ORGANIZATION_MESSAGE = (
    "a identidade não foi adicionada à organização no Azure DevOps (Organization settings > Users)"
)
ORGANIZATION_NOT_FOUND_MESSAGE = "organização não encontrada no Azure DevOps; confira AZDO_ORGANIZATION"
PROJECT_NOT_FOUND_MESSAGE = (
    "projeto ou organização não encontrado no Azure DevOps; confira AZDO_ORGANIZATION e AZDO_PROJECTS"
)
TIMESTAMP_FORMATS = ("%Y-%m-%dT%H:%M:%S.%f%z", "%Y-%m-%dT%H:%M:%S%z")
UTC_OFFSET_SUFFIX = "+0000"
UTC_ZULU_SUFFIX = "Z"
TONE_NORMAL = "normal"
TONE_MUTED = "muted"
TONE_WARN = "warn"
TONE_DANGER = "danger"
ATTENTION_FLAG = "attention"
OFFLINE_NOTICE_FORMAT = "dados de %H:%M (offline)"
OFFLINE_NOTICE_FORMAT_WITH_DATE = "dados de %d/%m %H:%M (offline)"
ELLIPSIS = "…"
STATE_FILE_SUFFIX = ".json"
TEMPORARY_FILE_SUFFIX = ".tmp"
STATE_DIRECTORY_MODE = 0o700
ARGUMENT_ERROR_EXIT_CODE = 2
FAILURE_EXIT_CODE = 1
USAGE_MESSAGE = "uso: azure_pipelines.py snapshot|login"
SNAPSHOT_COMMAND = ["snapshot"]
LOGIN_COMMAND = ["login"]
LOGIN_INSTRUCTION = "python3 plugins/azure-pipelines/azure_pipelines.py login"
STATE_KEY_SEPARATOR = "\n"
PROTOCOL_VERSION = 1
MAXIMUM_PROJECTS = 50
MAXIMUM_BUILDS_PER_PROJECT = 1000
MAXIMUM_TIMELINE_REQUESTS_PER_RUN = 30
MAXIMUM_CACHED_TIMELINES = 300
STEPS_PER_PIPELINE = 10
PROJECTS_PATH = "/_apis/projects"
BUILDS_PATH_SUFFIX = "/_apis/build/builds"
BUILDS_PATH_FORMAT = "/{project}" + BUILDS_PATH_SUFFIX
TIMELINE_PATH_FORMAT = "/{project}" + BUILDS_PATH_SUFFIX + "/{build_id}/timeline"
VALUE_KEY = "value"
RECORDS_KEY = "records"
TASK_RECORD_TYPE = "Task"
JOB_RECORD_TYPE = "Job"
CANCELED_RESULT = "canceled"
FAILED_RESULT = "failed"
QUEUE_STEP_NAME = "espera na fila"
STEP_PATH_SEPARATOR = " › "
TIMELINES_STATE_KEY = "timelines"
SECONDS_PER_MINUTE = 60
SECONDS_PER_HOUR = 3600
PERCENT = 100
ROW_TITLE_LIMIT = 80
MAXIMUM_BLOCK_ROWS = 200
MINI_VALUE_LIMIT = 8
MINI_OVERFLOW_TEXT = "999h+"
MINI_TEXT = "pipeline mais lenta"
EMPTY_TEXT_FORMAT = "nenhuma execução concluída nos últimos {window}"
STEPS_UNAVAILABLE_TEXT = "passos indisponíveis nesta leitura"
PIPELINE_HEADING_SEPARATOR = "/"
ITEM_ID_SEPARATOR = ":"


class ConfigError(Exception):
    pass


@dataclass(frozen=True)
class BoundedSetting:
    variable: str
    default: int
    minimum: int
    maximum: int


DAYS_SETTING = BoundedSetting(DAYS_VARIABLE, 14, 1, 90)
TOP_SETTING = BoundedSetting(TOP_VARIABLE, 5, 1, 10)
SAMPLE_SETTING = BoundedSetting(SAMPLE_VARIABLE, 10, 1, 20)


@dataclass(frozen=True)
class PipelinesConfig:
    base_url: str
    organization: str
    projects: tuple[str, ...]
    days: int
    top: int
    sample: int
    title: str
    state_dir: Path
    auth: EntraConfig


def load_config(environ: Mapping[str, str]) -> PipelinesConfig:
    return PipelinesConfig(
        base_url=read_base_url(environ, BASE_URL_VARIABLE, DEFAULT_BASE_URL),
        organization=_read_organization(environ),
        projects=_read_projects(environ),
        days=_read_bounded_integer(environ, DAYS_SETTING),
        top=_read_bounded_integer(environ, TOP_SETTING),
        sample=_read_bounded_integer(environ, SAMPLE_SETTING),
        title=environ.get(TITLE_VARIABLE, "").strip() or DEFAULT_TITLE,
        state_dir=read_state_dir(environ, STATE_DIRECTORY_VARIABLE, DEFAULT_STATE_DIRECTORY),
        auth=load_entra_config(environ),
    )


def _read_bounded_integer(environ: Mapping[str, str], setting: BoundedSetting) -> int:
    raw = environ.get(setting.variable, "").strip()
    if not raw:
        return setting.default
    try:
        value = int(raw)
    except ValueError:
        value = None
    if value is None or not setting.minimum <= value <= setting.maximum:
        raise ConfigError(
            f"{setting.variable} precisa ser um número inteiro de {setting.minimum} a {setting.maximum}"
        )
    return value


def _read_organization(environ: Mapping[str, str]) -> str:
    organization = environ.get(ORGANIZATION_VARIABLE, "").strip()
    if not organization:
        raise ConfigError(f"defina {ORGANIZATION_VARIABLE} com o nome da organização do Azure DevOps")
    if ORGANIZATION_PATTERN.fullmatch(organization) is None:
        raise ConfigError(f"{ORGANIZATION_VARIABLE} precisa ser só o nome da organização")
    return organization


def _read_projects(environ: Mapping[str, str]) -> tuple[str, ...]:
    names = (name.strip() for name in environ.get(PROJECTS_VARIABLE, "").split(LIST_SEPARATOR))
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


class RequestRejectedError(ApiError):
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


# --- entra-auth: início ---
AZURE_DEVOPS_RESOURCE_ID = "499b84ac-1321-427f-aa17-267ca6975798"
AZURE_DEVOPS_SCOPE = f"{AZURE_DEVOPS_RESOURCE_ID}/.default"
OFFLINE_ACCESS_SCOPE = "offline_access"
DEFAULT_LOGIN_URL = "https://login.microsoftonline.com"
DEFAULT_IMDS_URL = "http://169.254.169.254"
IMDS_TOKEN_PATH = "/metadata/identity/oauth2/token"
IMDS_API_VERSION = "2018-02-01"
IMDS_METADATA_HEADER = "Metadata"
DEFAULT_TENANT = "organizations"
UNSPECIFIC_TENANTS = (DEFAULT_TENANT, "common")
AUTH_METHOD_VARIABLE = "AZDO_AUTH_METHOD"
TENANT_VARIABLE = "AZDO_TENANT_ID"
CLIENT_ID_VARIABLE = "AZDO_CLIENT_ID"
CLIENT_SECRET_VARIABLE_KEY = "AZDO_CLIENT_SECRET_ENV"
CLIENT_SECRET_FILE_KEY = "AZDO_CLIENT_SECRET_FILE"
CREDENTIALS_DIRECTORY_VARIABLE = "AZDO_CREDENTIALS_DIR"
DEFAULT_CREDENTIALS_DIRECTORY = "~/.local/state/mosaic/azure-devops-credentials"
LOGIN_URL_VARIABLE = "AZDO_LOGIN_URL"
IMDS_URL_VARIABLE = "AZDO_IMDS_URL"
LEGACY_TOKEN_VARIABLES = ("AZDO_TOKEN_ENV", "AZDO_TOKEN_FILE")
AZURE_CLI_EXECUTABLE = "az"
AZURE_CLI_TIMEOUT_SECONDS = 15
IMDS_TIMEOUT_SECONDS = 3
TOKEN_REQUEST_TIMEOUT_SECONDS = 10
DEVICE_CODE_GRANT = "urn:ietf:params:oauth:grant-type:device_code"
AUTH_METHOD_AZURE_CLI = "azure-cli"
AUTH_METHOD_DEVICE_CODE = "device-code"
AUTH_METHOD_SERVICE_PRINCIPAL = "service-principal"
AUTH_METHOD_MANAGED_IDENTITY = "managed-identity"
AUTH_METHODS = (
    AUTH_METHOD_AZURE_CLI,
    AUTH_METHOD_DEVICE_CODE,
    AUTH_METHOD_SERVICE_PRINCIPAL,
    AUTH_METHOD_MANAGED_IDENTITY,
)
HTTP_OK = 200
HTTP_SERVER_ERROR_FLOOR = 500
ENTRA_CLIENT_ERROR_FLOOR = 400
IDENTIFIER_PATTERN = re.compile(r"[A-Za-z0-9.-]+")
DEFAULT_POLL_INTERVAL_SECONDS = 5
SLOW_DOWN_INCREMENT_SECONDS = 5
MAXIMUM_POLL_INTERVAL_SECONDS = 60
DEFAULT_DEVICE_CODE_LIFETIME_SECONDS = 900
MAXIMUM_DEVICE_CODE_LIFETIME_SECONDS = 1800
CREDENTIAL_FILE_PREFIX = "entra-"
CREDENTIAL_DIRECTORY_MODE = 0o700
CREDENTIAL_FORBIDDEN_BITS = 0o077
CREDENTIAL_KEY_SEPARATOR = "\n"
BEARER_PREFIX = "Bearer "
FORM_CONTENT_TYPE = "application/x-www-form-urlencoded"
JSON_CONTENT_TYPE = "application/json"
ERROR_KEY = "error"
ACCESS_TOKEN_KEY = "access_token"
REFRESH_TOKEN_KEY = "refresh_token"
REFRESH_TOKEN_GRANT = "refresh_token"
PENDING_ERROR_CODE = "authorization_pending"
SLOW_DOWN_ERROR_CODE = "slow_down"
INVALID_GRANT_ERROR_CODE = "invalid_grant"
LOGIN_REQUIRED_MESSAGE = f"sem login do Entra; rode: {LOGIN_INSTRUCTION}"
LOGIN_DONE_MESSAGE = "login concluído"
ENTRA_REQUEST_REFUSED_MESSAGE = "o Entra recusou o pedido de token"
DIRECTORY_NOT_PRIVATE_MESSAGE = f"a pasta de {CREDENTIALS_DIRECTORY_VARIABLE} precisa ser privada (chmod 700)"
FILE_NOT_PRIVATE_MESSAGE = "o arquivo de credencial do Entra precisa ser privado (chmod 600)"
CREDENTIAL_WRITE_FAILED_MESSAGE = "não foi possível gravar a credencial do login do Entra"
CREDENTIAL_DELETE_FAILED_MESSAGE = "não foi possível remover a credencial do login do Entra"
LOGIN_ONLY_FOR_DEVICE_CODE_MESSAGE = f"login só é usado com {AUTH_METHOD_VARIABLE}={AUTH_METHOD_DEVICE_CODE}"
LEGACY_TOKEN_MESSAGE = (
    "PAT não é mais suportado no Azure DevOps; defina "
    f"{AUTH_METHOD_VARIABLE} ({', '.join(AUTH_METHODS[:-1])} ou {AUTH_METHODS[-1]}), "
    "veja plugins/azure-devops/README.md"
)
ENTRA_ERROR_MESSAGES = {
    "invalid_client": "o Entra recusou o client id ou o segredo do app",
    "unauthorized_client": "o app não tem permissão para esse fluxo no Entra",
    "invalid_grant": "o login do Entra venceu ou foi revogado",
    "invalid_scope": "o app não tem o escopo do Azure DevOps no Entra",
    "interaction_required": "o Entra exige consentimento ou login interativo",
    "consent_required": "o Entra exige consentimento do administrador para o app",
    "expired_token": "o código do dispositivo venceu antes da autorização",
    "authorization_declined": "a autorização do dispositivo foi recusada",
    "bad_verification_code": "o código do dispositivo é inválido",
}


class TokenProvider(Protocol):
    def access_token(self) -> str: ...


class EntraAuthError(AuthError):
    def __init__(self, message: str, code: str):
        super().__init__(message)
        self.code = code


class AuthorizationPendingError(UnexpectedResponseError):
    def __init__(self, slow_down: bool):
        super().__init__("a autorização do dispositivo ainda está pendente")
        self.slow_down = slow_down


@dataclass(frozen=True)
class EntraConfig:
    method: str
    tenant: str
    client_id: str
    client_secret: str = field(repr=False)
    login_url: str
    imds_url: str
    credentials_dir: Path


def load_entra_config(environ: Mapping[str, str]) -> EntraConfig:
    _reject_legacy_token(environ)
    method = environ.get(AUTH_METHOD_VARIABLE, "").strip() or AUTH_METHOD_AZURE_CLI
    if method not in AUTH_METHODS:
        raise ConfigError(f"{AUTH_METHOD_VARIABLE} desconhecido; use {', '.join(AUTH_METHODS)}")
    config = EntraConfig(
        method=method,
        tenant=_read_identifier(environ, TENANT_VARIABLE, DEFAULT_TENANT),
        client_id=_read_identifier(environ, CLIENT_ID_VARIABLE, ""),
        client_secret="",
        login_url=read_base_url(environ, LOGIN_URL_VARIABLE, DEFAULT_LOGIN_URL),
        imds_url=read_base_url(environ, IMDS_URL_VARIABLE, DEFAULT_IMDS_URL),
        credentials_dir=read_state_dir(
            environ, CREDENTIALS_DIRECTORY_VARIABLE, DEFAULT_CREDENTIALS_DIRECTORY
        ),
    )
    return _complete_for_method(config, environ)


def _reject_legacy_token(environ: Mapping[str, str]) -> None:
    if any(environ.get(variable, "").strip() for variable in LEGACY_TOKEN_VARIABLES):
        raise ConfigError(LEGACY_TOKEN_MESSAGE)


def _read_identifier(environ: Mapping[str, str], variable: str, default: str) -> str:
    value = environ.get(variable, "").strip() or default
    if value and IDENTIFIER_PATTERN.fullmatch(value) is None:
        raise ConfigError(f"{variable} contém caracteres inválidos")
    return value


def _complete_for_method(config: EntraConfig, environ: Mapping[str, str]) -> EntraConfig:
    needs_client = config.method in (AUTH_METHOD_DEVICE_CODE, AUTH_METHOD_SERVICE_PRINCIPAL)
    if needs_client and not config.client_id:
        raise ConfigError(
            f"defina {CLIENT_ID_VARIABLE} com o Application (client) ID do app registrado no Entra"
        )
    if config.method != AUTH_METHOD_SERVICE_PRINCIPAL:
        return config
    if config.tenant in UNSPECIFIC_TENANTS:
        raise ConfigError(
            f"defina {TENANT_VARIABLE} com o Directory (tenant) ID; "
            f"'{DEFAULT_TENANT}' e 'common' não servem ao service principal"
        )
    secret = read_secret(environ, CLIENT_SECRET_VARIABLE_KEY, CLIENT_SECRET_FILE_KEY)
    return replace(config, client_secret=secret)


def read_secret(environ: Mapping[str, str], variable_key: str, file_key: str) -> str:
    value = _read_secret_source(environ, variable_key, file_key)
    if _has_unsafe_characters(value):
        raise ConfigError(f"o segredo de {variable_key}/{file_key} contém caracteres inválidos")
    return value


def _read_secret_source(environ: Mapping[str, str], variable_key: str, file_key: str) -> str:
    variable = environ.get(variable_key, "").strip()
    if variable:
        value = environ.get(variable, "").strip()
        if not value:
            raise ConfigError(f"a variável {variable} ({variable_key}) está vazia ou ausente")
        return value
    path = environ.get(file_key, "").strip()
    if path:
        return _read_secret_file(path, file_key)
    raise ConfigError(f"defina {variable_key} (nome da variável com o segredo) ou {file_key}")


def _read_secret_file(path: str, file_key: str) -> str:
    try:
        value = Path(path).expanduser().read_text(encoding="utf-8").strip()
    except (OSError, UnicodeDecodeError):
        raise ConfigError(f"não foi possível ler o arquivo de {file_key}") from None
    if not value:
        raise ConfigError(f"o arquivo de {file_key} está vazio")
    return value


def _has_unsafe_characters(value: str) -> bool:
    if any(character.isspace() or not character.isprintable() for character in value):
        return True
    try:
        value.encode("latin-1")
    except UnicodeEncodeError:
        return True
    return False


def bearer_authorization(token: str) -> str:
    if _has_unsafe_characters(token):
        raise ConfigError("o token de acesso recebido contém caracteres inválidos")
    return BEARER_PREFIX + token


def decode_entra_response(status: int, body: bytes) -> dict:
    if status == HTTP_TOO_MANY_REQUESTS:
        raise RateLimitError("limite de requisições do Entra; tento de novo na próxima leitura")
    if status >= HTTP_SERVER_ERROR_FLOOR:
        raise NetworkError(f"o Entra está indisponível (HTTP {status})")
    payload = _parse_json_object(body)
    if status == HTTP_OK:
        if payload is None:
            raise UnexpectedResponseError("a resposta do Entra não é um objeto JSON")
        return payload
    _raise_for_entra_error(status, payload or {})


def _parse_json_object(body: bytes) -> dict | None:
    try:
        payload = json.loads(body)
    except ValueError:
        return None
    return payload if isinstance(payload, dict) else None


def _raise_for_entra_error(status: int, payload: dict) -> None:
    code = payload.get(ERROR_KEY)
    if code in (PENDING_ERROR_CODE, SLOW_DOWN_ERROR_CODE):
        raise AuthorizationPendingError(slow_down=code == SLOW_DOWN_ERROR_CODE)
    if isinstance(code, str) and code in ENTRA_ERROR_MESSAGES:
        raise EntraAuthError(ENTRA_ERROR_MESSAGES[code], code)
    if ENTRA_CLIENT_ERROR_FLOOR <= status < HTTP_SERVER_ERROR_FLOOR:
        raise EntraAuthError(ENTRA_REQUEST_REFUSED_MESSAGE, code="")
    raise UnexpectedResponseError(f"resposta inesperada do Entra (HTTP {status})")


def _send_to_entra(http, url: str, method: str, headers: dict, body: bytes | None, timeout: int) -> dict:
    try:
        status, payload = http(url, method, headers, body, timeout)
    except (OSError, HTTPException):
        raise NetworkError("falha de rede ao falar com o Entra") from None
    return decode_entra_response(status, payload)


def _post_form(http, url: str, fields: dict) -> dict:
    headers = {"Content-Type": FORM_CONTENT_TYPE, "Accept": JSON_CONTENT_TYPE}
    body = urllib.parse.urlencode(fields).encode("ascii")
    return _send_to_entra(http, url, "POST", headers, body, TOKEN_REQUEST_TIMEOUT_SECONDS)


def _token_url(config: EntraConfig) -> str:
    return f"{config.login_url}/{config.tenant}/oauth2/v2.0/token"


def _device_code_url(config: EntraConfig) -> str:
    return f"{config.login_url}/{config.tenant}/oauth2/v2.0/devicecode"


def _read_access_token(payload: dict) -> str:
    token = payload.get(ACCESS_TOKEN_KEY)
    if not isinstance(token, str) or not token:
        raise UnexpectedResponseError("a resposta do Entra não traz o token de acesso")
    return token


class AzureCliTokenProvider:
    def __init__(self, config: EntraConfig, run_command):
        self._config = config
        self._run_command = run_command

    def access_token(self) -> str:
        completed = self._run_azure_cli()
        if completed.returncode != 0:
            raise AuthError("o az não tem sessão válida; rode az login")
        try:
            token = json.loads(completed.stdout).get("accessToken")
        except (ValueError, TypeError, AttributeError):
            token = None
        if not isinstance(token, str) or not token:
            raise UnexpectedResponseError("resposta inesperada do az")
        return token

    def _run_azure_cli(self):
        try:
            return self._run_command(
                self._command(),
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=AZURE_CLI_TIMEOUT_SECONDS,
                check=False,
            )
        except FileNotFoundError:
            raise ConfigError(
                f"Azure CLI ({AZURE_CLI_EXECUTABLE}) não encontrado; "
                f"instale-o ou escolha outro {AUTH_METHOD_VARIABLE}"
            ) from None
        except subprocess.TimeoutExpired:
            raise NetworkError("o az demorou demais para responder") from None
        except OSError:
            raise ConfigError(f"não foi possível executar o Azure CLI ({AZURE_CLI_EXECUTABLE})") from None

    def _command(self) -> list[str]:
        command = [
            AZURE_CLI_EXECUTABLE,
            "account",
            "get-access-token",
            "--resource",
            AZURE_DEVOPS_RESOURCE_ID,
            "--output",
            "json",
        ]
        if self._config.tenant != DEFAULT_TENANT:
            command += ["--tenant", self._config.tenant]
        return command


class ServicePrincipalTokenProvider:
    def __init__(self, config: EntraConfig, http):
        self._config = config
        self._http = http

    def access_token(self) -> str:
        fields = {
            "grant_type": "client_credentials",
            "client_id": self._config.client_id,
            "client_secret": self._config.client_secret,
            "scope": AZURE_DEVOPS_SCOPE,
        }
        return _read_access_token(_post_form(self._http, _token_url(self._config), fields))


class ManagedIdentityTokenProvider:
    def __init__(self, config: EntraConfig, http):
        self._config = config
        self._http = http

    def access_token(self) -> str:
        headers = {IMDS_METADATA_HEADER: "true", "Accept": JSON_CONTENT_TYPE}
        try:
            payload = _send_to_entra(
                self._http, self._url(), "GET", headers, None, IMDS_TIMEOUT_SECONDS
            )
        except NetworkError:
            raise NetworkError(
                "IMDS indisponível; managed-identity só funciona numa VM do Azure"
            ) from None
        return _read_access_token(payload)

    def _url(self) -> str:
        parameters = {"api-version": IMDS_API_VERSION, "resource": AZURE_DEVOPS_RESOURCE_ID}
        if self._config.client_id:
            parameters["client_id"] = self._config.client_id
        return self._config.imds_url + IMDS_TOKEN_PATH + "?" + urllib.parse.urlencode(parameters)


class CredentialStore:
    def __init__(self, config: EntraConfig):
        key = CREDENTIAL_KEY_SEPARATOR.join((config.login_url, config.tenant, config.client_id))
        digest = hashlib.sha256(key.encode("utf-8")).hexdigest()
        self._directory = config.credentials_dir
        self._path = self._directory / (CREDENTIAL_FILE_PREFIX + digest + STATE_FILE_SUFFIX)

    def load_refresh_token(self) -> str | None:
        if not self._path.exists():
            return None
        self._require_private(self._directory, DIRECTORY_NOT_PRIVATE_MESSAGE)
        self._require_private(self._path, FILE_NOT_PRIVATE_MESSAGE)
        try:
            document = json.loads(self._path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return None
        token = document.get(REFRESH_TOKEN_KEY) if isinstance(document, dict) else None
        return token if isinstance(token, str) and token else None

    def save_refresh_token(self, token: str) -> None:
        self.ensure_private_directory()
        try:
            self._write_private(json.dumps({REFRESH_TOKEN_KEY: token}))
        except OSError:
            raise ConfigError(CREDENTIAL_WRITE_FAILED_MESSAGE) from None

    def delete(self) -> None:
        try:
            self._path.unlink(missing_ok=True)
        except OSError:
            raise ConfigError(CREDENTIAL_DELETE_FAILED_MESSAGE) from None

    def ensure_private_directory(self) -> None:
        try:
            self._directory.mkdir(mode=CREDENTIAL_DIRECTORY_MODE, parents=True, exist_ok=True)
        except OSError:
            raise ConfigError(CREDENTIAL_WRITE_FAILED_MESSAGE) from None
        self._require_private(self._directory, DIRECTORY_NOT_PRIVATE_MESSAGE)

    def _require_private(self, path: Path, message: str) -> None:
        try:
            is_exposed = path.stat().st_mode & CREDENTIAL_FORBIDDEN_BITS
        except OSError:
            raise ConfigError(message) from None
        if is_exposed:
            raise ConfigError(message)

    def _write_private(self, content: str) -> None:
        descriptor, temporary_name = tempfile.mkstemp(dir=self._directory, suffix=TEMPORARY_FILE_SUFFIX)
        try:
            with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
                handle.write(content)
            os.replace(temporary_name, self._path)
        except OSError:
            Path(temporary_name).unlink(missing_ok=True)
            raise


class DeviceCodeTokenProvider:
    def __init__(self, config: EntraConfig, http, store: CredentialStore):
        self._config = config
        self._http = http
        self._store = store

    def access_token(self) -> str:
        refresh_token = self._store.load_refresh_token()
        if refresh_token is None:
            raise AuthError(LOGIN_REQUIRED_MESSAGE)
        payload = self._refresh(refresh_token)
        self._remember_rotated_token(payload)
        return _read_access_token(payload)

    def _refresh(self, refresh_token: str) -> dict:
        fields = {
            "grant_type": REFRESH_TOKEN_GRANT,
            "refresh_token": refresh_token,
            "client_id": self._config.client_id,
            "scope": f"{AZURE_DEVOPS_SCOPE} {OFFLINE_ACCESS_SCOPE}",
        }
        try:
            return _post_form(self._http, _token_url(self._config), fields)
        except EntraAuthError as error:
            if error.code != INVALID_GRANT_ERROR_CODE:
                raise
            self._store.delete()
            raise AuthError(LOGIN_REQUIRED_MESSAGE) from None

    def _remember_rotated_token(self, payload: dict) -> None:
        rotated = payload.get(REFRESH_TOKEN_KEY)
        if isinstance(rotated, str) and rotated:
            self._store.save_refresh_token(rotated)


def run_device_code_login(config: EntraConfig, http, sleep, stderr) -> None:
    if config.method != AUTH_METHOD_DEVICE_CODE:
        raise ConfigError(LOGIN_ONLY_FOR_DEVICE_CODE_MESSAGE)
    store = CredentialStore(config)
    store.ensure_private_directory()
    grant = _post_form(
        http,
        _device_code_url(config),
        {"client_id": config.client_id, "scope": f"{AZURE_DEVOPS_SCOPE} {OFFLINE_ACCESS_SCOPE}"},
    )
    print(_printable(grant.get("message")), file=stderr)
    payload = _poll_for_authorization(config, http, sleep, grant)
    refresh_token = payload.get(REFRESH_TOKEN_KEY)
    if not isinstance(refresh_token, str) or not refresh_token:
        raise UnexpectedResponseError("o Entra não devolveu o refresh token do login")
    store.save_refresh_token(refresh_token)
    print(LOGIN_DONE_MESSAGE, file=stderr)


def _printable(value: object) -> str:
    text = value if isinstance(value, str) else ""
    return "".join(character for character in text if character.isprintable())


def _poll_for_authorization(config: EntraConfig, http, sleep, grant: dict) -> dict:
    device_code = grant.get("device_code")
    if not isinstance(device_code, str) or not device_code:
        raise UnexpectedResponseError("a resposta do Entra não traz o código do dispositivo")
    interval = _bounded_seconds(grant.get("interval"), DEFAULT_POLL_INTERVAL_SECONDS, MAXIMUM_POLL_INTERVAL_SECONDS)
    lifetime = _bounded_seconds(
        grant.get("expires_in"), DEFAULT_DEVICE_CODE_LIFETIME_SECONDS, MAXIMUM_DEVICE_CODE_LIFETIME_SECONDS
    )
    fields = {"grant_type": DEVICE_CODE_GRANT, "client_id": config.client_id, "device_code": device_code}
    waited = 0
    while True:
        try:
            return _post_form(http, _token_url(config), fields)
        except AuthorizationPendingError as pending:
            if pending.slow_down:
                interval = min(interval + SLOW_DOWN_INCREMENT_SECONDS, MAXIMUM_POLL_INTERVAL_SECONDS)
        waited += interval
        if waited >= lifetime:
            raise EntraAuthError(ENTRA_ERROR_MESSAGES["expired_token"], "expired_token")
        sleep(interval)


def _bounded_seconds(value: object, default: int, maximum: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 1:
        return default
    return min(value, maximum)


AUTH_PROVIDER_FACTORIES = {
    AUTH_METHOD_AZURE_CLI: lambda config, http, run_command: AzureCliTokenProvider(config, run_command),
    AUTH_METHOD_DEVICE_CODE: lambda config, http, run_command: DeviceCodeTokenProvider(
        config, http, CredentialStore(config)
    ),
    AUTH_METHOD_SERVICE_PRINCIPAL: lambda config, http, run_command: ServicePrincipalTokenProvider(config, http),
    AUTH_METHOD_MANAGED_IDENTITY: lambda config, http, run_command: ManagedIdentityTokenProvider(config, http),
}


def token_provider_for(config: EntraConfig, http, run_command) -> TokenProvider:
    return AUTH_PROVIDER_FACTORIES[config.method](config, http, run_command)


# --- entra-auth: fim ---


def decode_response(status: int, body: bytes, service: str) -> object:
    if status == HTTP_UNAUTHORIZED:
        raise AuthError(_auth_message(service))
    if status == HTTP_FORBIDDEN:
        if RATE_LIMIT_MARKER in body.lower():
            raise RateLimitError(_rate_limit_message(service))
        raise AuthError(_auth_message(service))
    if status == HTTP_TOO_MANY_REQUESTS:
        raise RateLimitError(_rate_limit_message(service))
    if status in REJECTED_STATUSES:
        raise RequestRejectedError(_rejected_message(service, status))
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


def _rejected_message(service: str, status: int) -> str:
    return f"o {service} recusou a requisição (HTTP {status}); confira a configuração do widget"


class AzureClient:
    def __init__(self, config: PipelinesConfig, http, token_provider: TokenProvider):
        self._config = config
        self._http = http
        self._token_provider = token_provider
        self._headers: dict | None = None

    def get_json(self, path: str, params: dict) -> dict:
        url = self._config.base_url + "/" + self._config.organization + path
        if params:
            url += "?" + urllib.parse.urlencode(params, safe="$")
        headers = self._request_headers()
        try:
            status, body = self._http(url, "GET", headers, None, REQUEST_TIMEOUT_SECONDS)
        except (OSError, HTTPException) as error:
            raise NetworkError(
                f"falha de rede ao falar com {SERVICE_NAME} ({type(error).__name__})"
            ) from None
        if _signals_invalid_credentials(status):
            raise AuthError(_auth_message(SERVICE_NAME))
        if status == HTTP_UNAUTHORIZED and IDENTITY_NOT_IN_ORGANIZATION_MARKER in body:
            raise AuthError(IDENTITY_NOT_IN_ORGANIZATION_MESSAGE)
        if status == HTTP_NOT_FOUND:
            _raise_if_target_not_found(path)
        payload = decode_response(status, body, SERVICE_NAME)
        if not isinstance(payload, dict):
            raise UnexpectedResponseError(f"resposta inesperada do {SERVICE_NAME}")
        return payload

    def _request_headers(self) -> dict:
        if self._headers is None:
            token = self._token_provider.access_token()
            self._headers = {
                "Authorization": bearer_authorization(token),
                "Accept": "application/json",
            }
        return self._headers


def _raise_if_target_not_found(path: str) -> None:
    if path == PROJECTS_PATH:
        raise RequestRejectedError(ORGANIZATION_NOT_FOUND_MESSAGE)
    if path.endswith(BUILDS_PATH_SUFFIX):
        raise RequestRejectedError(PROJECT_NOT_FOUND_MESSAGE)


@dataclass(frozen=True)
class BuildRun:
    build_id: int
    project: str
    pipeline_id: int
    pipeline_name: str
    queued_at: datetime
    started_at: datetime
    finished_at: datetime
    result: str
    url: str

    @property
    def duration_seconds(self) -> float:
        return (self.finished_at - self.started_at).total_seconds()

    @property
    def queue_seconds(self) -> float:
        return max(0.0, (self.started_at - self.queued_at).total_seconds())


@dataclass(frozen=True)
class StepDuration:
    name: str
    seconds: float


@dataclass(frozen=True)
class StepStatistic:
    name: str
    average_seconds: float
    maximum_seconds: float
    share: float


@dataclass(frozen=True)
class PipelineStatistic:
    project: str
    pipeline_id: int
    pipeline_name: str
    run_count: int
    average_seconds: float
    maximum_seconds: float
    average_queue_seconds: float
    maximum_queue_seconds: float
    failure_rate: float
    url: str
    steps: tuple[StepStatistic, ...] = ()
    sampled_runs: int = 0
    partial: bool = False

    @property
    def heading(self) -> str:
        return flatten_text(f"{self.project}{PIPELINE_HEADING_SEPARATOR}{self.pipeline_name}", ROW_TITLE_LIMIT)


@dataclass(frozen=True)
class Overview:
    pipeline_count: int
    run_count: int
    ranked: tuple[PipelineStatistic, ...]


def parse_build_timestamp(text: object) -> datetime | None:
    if not isinstance(text, str):
        return None
    return parse_timestamp(FRACTION_BEYOND_MICROSECONDS.sub(r"\1", text))


def parse_build_run(raw: object, project: str) -> BuildRun | None:
    if not isinstance(raw, dict) or raw.get("result") == CANCELED_RESULT:
        return None
    build_id = raw.get("id")
    pipeline_id = _nested(raw, "definition", "id")
    pipeline_name = _nested(raw, "definition", "name")
    queued_at, started_at, finished_at = (
        parse_build_timestamp(raw.get(key)) for key in ("queueTime", "startTime", "finishTime")
    )
    identifiers_are_valid = _is_integer(build_id) and _is_integer(pipeline_id)
    name_is_valid = isinstance(pipeline_name, str) and bool(pipeline_name)
    if not (identifiers_are_valid and name_is_valid) or None in (queued_at, started_at, finished_at):
        return None
    if finished_at < started_at:
        return None
    href = _nested(raw, "_links", "web", "href")
    return BuildRun(
        build_id=build_id,
        project=project,
        pipeline_id=pipeline_id,
        pipeline_name=pipeline_name,
        queued_at=queued_at,
        started_at=started_at,
        finished_at=finished_at,
        result=str(raw.get("result") or ""),
        url=href if isinstance(href, str) else "",
    )


def _is_integer(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _nested(node: object, *keys: str) -> object:
    for key in keys:
        if not isinstance(node, dict):
            return None
        node = node.get(key)
    return node


def _value_list(payload: dict) -> list:
    value = payload.get(VALUE_KEY)
    if not isinstance(value, list):
        raise UnexpectedResponseError(f"resposta inesperada do {SERVICE_NAME}")
    return value


def fetch_projects(client: AzureClient, config: PipelinesConfig) -> list[str]:
    if config.projects:
        return list(config.projects)
    payload = client.get_json(
        PROJECTS_PATH,
        {
            TOP_PARAMETER: MAXIMUM_PROJECTS,
            STATE_FILTER_PARAMETER: WELL_FORMED_PROJECTS,
            API_VERSION_PARAMETER: API_VERSION,
        },
    )
    names = (_nested(item, "name") for item in _value_list(payload))
    return [name for name in names if isinstance(name, str) and name]


def fetch_runs(client: AzureClient, project: str, since: datetime) -> list[BuildRun]:
    payload = client.get_json(
        BUILDS_PATH_FORMAT.format(project=_quoted(project)),
        {
            MIN_TIME_PARAMETER: since.astimezone(timezone.utc).strftime(MINIMUM_TIME_FORMAT),
            STATUS_FILTER_PARAMETER: COMPLETED_STATUS,
            QUERY_ORDER_PARAMETER: FINISH_TIME_DESCENDING,
            TOP_PARAMETER: MAXIMUM_BUILDS_PER_PROJECT,
            API_VERSION_PARAMETER: API_VERSION,
        },
    )
    runs = (parse_build_run(raw, project) for raw in _value_list(payload))
    return [run for run in runs if run is not None]


def fetch_step_durations(client: AzureClient, run: BuildRun) -> list[StepDuration]:
    path = TIMELINE_PATH_FORMAT.format(project=_quoted(run.project), build_id=run.build_id)
    return parse_timeline(client.get_json(path, {API_VERSION_PARAMETER: API_VERSION}))


def _quoted(project: str) -> str:
    return urllib.parse.quote(project, safe="")


def parse_timeline(payload: object) -> list[StepDuration]:
    records = payload.get(RECORDS_KEY) if isinstance(payload, dict) else None
    if not isinstance(records, list):
        return []
    by_id = {record["id"]: record for record in records if _has_text_id(record)}
    steps = []
    for record in records:
        step = _step_of(record, by_id)
        if step is not None:
            steps.append(step)
    return steps


def _has_text_id(record: object) -> bool:
    return isinstance(record, dict) and isinstance(record.get("id"), str)


def _step_of(record: object, by_id: dict) -> StepDuration | None:
    if not isinstance(record, dict) or record.get("type") != TASK_RECORD_TYPE:
        return None
    name = record.get("name")
    seconds = _record_seconds(record)
    if not isinstance(name, str) or not name or seconds is None:
        return None
    job = _enclosing_job_name(record, by_id)
    return StepDuration(f"{job}{STEP_PATH_SEPARATOR}{name}" if job else name, seconds)


def _record_seconds(record: dict) -> float | None:
    started = parse_build_timestamp(record.get("startTime"))
    finished = parse_build_timestamp(record.get("finishTime"))
    if started is None or finished is None:
        return None
    seconds = (finished - started).total_seconds()
    return seconds if seconds >= 0 else None


def _enclosing_job_name(record: dict, by_id: dict) -> str | None:
    visited = set()
    parent_id = record.get("parentId")
    while isinstance(parent_id, str) and parent_id not in visited:
        visited.add(parent_id)
        parent = by_id.get(parent_id)
        if parent is None:
            return None
        if parent.get("type") == JOB_RECORD_TYPE:
            name = parent.get("name")
            return name if isinstance(name, str) and name else None
        parent_id = parent.get("parentId")
    return None


def group_runs(runs: list[BuildRun]) -> dict[tuple[str, int], list[BuildRun]]:
    groups: dict[tuple[str, int], list[BuildRun]] = {}
    for run in runs:
        groups.setdefault((run.project, run.pipeline_id), []).append(run)
    return groups


def rank_pipelines(runs: list[BuildRun], top: int) -> list[PipelineStatistic]:
    statistics = [_statistic_of(group) for group in group_runs(runs).values()]
    statistics.sort(key=lambda item: (-item.average_seconds, item.pipeline_name))
    return statistics[:top]


def summarize(runs: list[BuildRun], top: int) -> Overview:
    return Overview(
        pipeline_count=len(group_runs(runs)),
        run_count=len(runs),
        ranked=tuple(rank_pipelines(runs, top)),
    )


def _statistic_of(group: list[BuildRun]) -> PipelineStatistic:
    latest = max(group, key=lambda run: run.finished_at)
    durations = [run.duration_seconds for run in group]
    queues = [run.queue_seconds for run in group]
    return PipelineStatistic(
        project=latest.project,
        pipeline_id=latest.pipeline_id,
        pipeline_name=latest.pipeline_name,
        run_count=len(group),
        average_seconds=sum(durations) / len(group),
        maximum_seconds=max(durations),
        average_queue_seconds=sum(queues) / len(group),
        maximum_queue_seconds=max(queues),
        failure_rate=sum(run.result == FAILED_RESULT for run in group) / len(group),
        url=latest.url,
    )


def select_sample(runs: list[BuildRun], sample: int) -> list[BuildRun]:
    return sorted(runs, key=lambda run: run.finished_at, reverse=True)[:sample]


def aggregate_steps(
    pipeline: PipelineStatistic, durations_by_run: dict[int, list[StepDuration]]
) -> PipelineStatistic:
    sampled_runs = len(durations_by_run)
    if sampled_runs == 0:
        return pipeline
    totals_by_name: dict[str, list[float]] = {}
    for durations in durations_by_run.values():
        for name, total in _sum_by_name(durations).items():
            totals_by_name.setdefault(name, []).append(total)
    total_seconds = pipeline.average_seconds + pipeline.average_queue_seconds
    steps = [
        StepStatistic(name, sum(totals) / sampled_runs, max(totals), _share(sum(totals) / sampled_runs, total_seconds))
        for name, totals in totals_by_name.items()
    ]
    if pipeline.average_queue_seconds > 0:
        steps.append(
            StepStatistic(
                QUEUE_STEP_NAME,
                pipeline.average_queue_seconds,
                pipeline.maximum_queue_seconds,
                _share(pipeline.average_queue_seconds, total_seconds),
            )
        )
    steps.sort(key=lambda step: (-step.average_seconds, step.name))
    return replace(pipeline, steps=tuple(steps[:STEPS_PER_PIPELINE]), sampled_runs=sampled_runs)


def _sum_by_name(durations: list[StepDuration]) -> dict[str, float]:
    totals: dict[str, float] = {}
    for duration in durations:
        totals[duration.name] = totals.get(duration.name, 0.0) + duration.seconds
    return totals


def _share(seconds: float, total_seconds: float) -> float:
    return seconds / total_seconds if total_seconds > 0 else 0.0


class RequestBudget:
    def __init__(self, limit: int):
        self._remaining = limit

    def take(self) -> bool:
        if self._remaining <= 0:
            return False
        self._remaining -= 1
        return True


def collect_overview(
    client: AzureClient, config: PipelinesConfig, now: datetime, timeline_cache: dict
) -> tuple[Overview, dict]:
    since = now - timedelta(days=config.days)
    runs: list[BuildRun] = []
    for project in fetch_projects(client, config):
        runs.extend(fetch_runs(client, project, since))
    overview = summarize(runs, config.top)
    groups = group_runs(runs)
    timelines = _valid_timelines(timeline_cache)
    budget = RequestBudget(MAXIMUM_TIMELINE_REQUESTS_PER_RUN)
    ranked = tuple(
        _with_steps(
            client,
            pipeline,
            select_sample(groups[(pipeline.project, pipeline.pipeline_id)], config.sample),
            timelines,
            budget,
        )
        for pipeline in overview.ranked
    )
    return replace(overview, ranked=ranked), _bounded_timelines(timelines)


def _with_steps(
    client: AzureClient,
    pipeline: PipelineStatistic,
    sample: list[BuildRun],
    timelines: dict,
    budget: RequestBudget,
) -> PipelineStatistic:
    durations_by_run = {}
    for run in sample:
        durations = _durations_of(client, run, timelines, budget)
        if durations is not None:
            durations_by_run[run.build_id] = durations
    aggregated = aggregate_steps(pipeline, durations_by_run)
    return replace(aggregated, partial=len(durations_by_run) < len(sample))


def _durations_of(
    client: AzureClient, run: BuildRun, timelines: dict, budget: RequestBudget
) -> list[StepDuration] | None:
    cached = _durations_from_cache(timelines.get(str(run.build_id)))
    if cached is not None:
        return cached
    if not budget.take():
        return None
    try:
        durations = fetch_step_durations(client, run)
    except (RequestRejectedError, UnexpectedResponseError):
        return None
    timelines[str(run.build_id)] = [[item.name, item.seconds] for item in durations]
    return durations


def _durations_from_cache(entry: object) -> list[StepDuration] | None:
    if not isinstance(entry, list):
        return None
    durations = []
    for pair in entry:
        if not (isinstance(pair, list) and len(pair) == 2):
            return None
        name, seconds = pair
        if not isinstance(name, str) or not _is_non_negative_number(seconds):
            return None
        durations.append(StepDuration(name, float(seconds)))
    return durations


def _is_non_negative_number(value: object) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and value >= 0


def _valid_timelines(raw: object) -> dict:
    if not isinstance(raw, dict):
        return {}
    valid = {}
    for key, entry in raw.items():
        durations = _durations_from_cache(entry)
        if isinstance(key, str) and key.isdigit() and durations is not None:
            valid[key] = [[item.name, item.seconds] for item in durations]
    return valid


def _bounded_timelines(timelines: dict) -> dict:
    newest = sorted(timelines, key=int, reverse=True)[:MAXIMUM_CACHED_TIMELINES]
    return {key: timelines[key] for key in newest}


def _signals_invalid_credentials(status: int) -> bool:
    return status == HTTP_SIGN_IN_PAGE or HTTP_REDIRECT_FLOOR <= status < HTTP_CLIENT_ERROR_FLOOR


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


def format_duration(seconds: float) -> str:
    total = round(seconds)
    hours, remainder = divmod(total, SECONDS_PER_HOUR)
    minutes, remaining_seconds = divmod(remainder, SECONDS_PER_MINUTE)
    if hours:
        return f"{hours}h{minutes:02d}m"
    if minutes:
        return f"{minutes}m{remaining_seconds:02d}s"
    return f"{remaining_seconds}s"


def _percent_text(fraction: float) -> str:
    return f"{round(fraction * PERCENT)}%"


def _window_text(days: int) -> str:
    return f"{days} dia" if days == 1 else f"{days} dias"


def _run_count_text(run_count: int) -> str:
    return f"{run_count} execução" if run_count == 1 else f"{run_count} execuções"


def _bottleneck(pipeline: PipelineStatistic) -> StepStatistic | None:
    return next((step for step in pipeline.steps if step.name != QUEUE_STEP_NAME), None)


def build_snapshot(overview: Overview, config: PipelinesConfig) -> dict:
    snapshot = {
        "protocol": PROTOCOL_VERSION,
        "summary": {"title": config.title, "blocks": _summary_blocks(overview, config.days)},
    }
    if overview.ranked:
        snapshot["detail"] = [_detail_section(pipeline) for pipeline in overview.ranked]
        snapshot["mini"] = _mini(overview.ranked[0])
    return snapshot


def _summary_blocks(overview: Overview, days: int) -> list[dict]:
    if not overview.ranked:
        text = EMPTY_TEXT_FORMAT.format(window=_window_text(days))
        return [{"type": "lines", "lines": [{"text": text, "tone": TONE_MUTED}]}]
    return [_stat_block(overview, days), _rows_block(overview.ranked)]


def _stat_block(overview: Overview, days: int) -> dict:
    return {
        "type": "stat",
        "items": [
            {"value": str(overview.pipeline_count), "label": "pipelines"},
            {"value": str(overview.run_count), "label": "execuções"},
            {"value": _window_text(days), "label": "janela"},
        ],
    }


def _rows_block(ranked: tuple[PipelineStatistic, ...]) -> dict:
    rows = [_row(pipeline, is_slowest=index == 0) for index, pipeline in enumerate(ranked)]
    return {"type": "rows", "rows": rows[:MAXIMUM_BLOCK_ROWS]}


def _row(pipeline: PipelineStatistic, is_slowest: bool) -> dict:
    return {
        "text": pipeline.heading,
        "time": format_duration(pipeline.average_seconds),
        "tags": _row_tags(pipeline),
        "tone": TONE_DANGER if is_slowest else TONE_NORMAL,
    }


def _row_tags(pipeline: PipelineStatistic) -> list[dict]:
    tags = [{"text": _run_count_text(pipeline.run_count), "tone": TONE_MUTED}]
    if pipeline.failure_rate > 0:
        percent = max(1, round(pipeline.failure_rate * PERCENT))
        tags.append({"text": f"{percent}% falhas", "tone": TONE_DANGER})
    return tags


def _detail_section(pipeline: PipelineStatistic) -> dict:
    return {
        "heading": pipeline.heading,
        "blocks": [_pipeline_stat_block(pipeline), _steps_block(pipeline)],
        "items": [_detail_item(pipeline)],
    }


def _pipeline_stat_block(pipeline: PipelineStatistic) -> dict:
    hint = f"amostra de {_run_count_text(pipeline.sampled_runs)}"
    return {
        "type": "stat",
        "items": [
            {"value": format_duration(pipeline.average_seconds), "label": "média"},
            {"value": format_duration(pipeline.maximum_seconds), "label": "máximo"},
            {"value": format_duration(pipeline.average_queue_seconds), "label": "fila média"},
            {"value": _percent_text(pipeline.failure_rate), "label": "falhas"},
        ],
        "hint": f"{hint} (parcial)" if pipeline.partial else hint,
    }


def _steps_block(pipeline: PipelineStatistic) -> dict:
    if not pipeline.steps:
        return {"type": "lines", "lines": [{"text": STEPS_UNAVAILABLE_TEXT, "tone": TONE_MUTED}]}
    bottleneck = _bottleneck(pipeline)
    return {
        "type": "table",
        "columns": [
            {"title": "Passo"},
            {"title": "Média", "align": "right"},
            {"title": "% do total", "align": "right"},
            {"title": "Máximo", "align": "right"},
        ],
        "rows": [_step_row(step, is_bottleneck=step is bottleneck) for step in pipeline.steps],
    }


def _step_row(step: StepStatistic, is_bottleneck: bool) -> list:
    cells = [
        flatten_text(step.name, ROW_TITLE_LIMIT),
        format_duration(step.average_seconds),
        _percent_text(step.share),
        format_duration(step.maximum_seconds),
    ]
    if not is_bottleneck:
        return cells
    return [{"text": cell, "tone": TONE_WARN, "flag": ATTENTION_FLAG} for cell in cells]


def _detail_item(pipeline: PipelineStatistic) -> dict:
    meta = {"media": format_duration(pipeline.average_seconds)}
    bottleneck = _bottleneck(pipeline)
    if bottleneck is not None:
        meta["gargalo"] = bottleneck.name
    return {
        "id": f"{pipeline.project}{ITEM_ID_SEPARATOR}{pipeline.pipeline_id}",
        "text": pipeline.url or pipeline.heading,
        "meta": meta,
    }


def _mini(slowest: PipelineStatistic) -> dict:
    value = format_duration(slowest.average_seconds)
    if len(value) > MINI_VALUE_LIMIT:
        value = MINI_OVERFLOW_TEXT
    return {"value": value, "text": MINI_TEXT, "tone": TONE_WARN}


def flatten_text(text, limit: int) -> str:
    flattened = " ".join(str(text).split())
    if len(flattened) <= limit:
        return flattened
    if limit <= 0:
        return ""
    return flattened[: limit - 1] + ELLIPSIS


def with_offline_notice(snapshot: dict, saved_at: datetime, now: datetime) -> dict:
    summary = snapshot["summary"]
    blocks = [_offline_notice_block(saved_at, now), *summary["blocks"]]
    return {**snapshot, "summary": {**summary, "blocks": blocks}}


def _offline_notice_block(saved_at: datetime, now: datetime) -> dict:
    local_saved_at = saved_at.astimezone()
    is_same_day = local_saved_at.date() == now.astimezone().date()
    notice_format = OFFLINE_NOTICE_FORMAT if is_same_day else OFFLINE_NOTICE_FORMAT_WITH_DATE
    text = local_saved_at.strftime(notice_format)
    return {"type": "lines", "lines": [{"text": text, "tone": TONE_WARN}]}


def _write_atomically(destination: Path, content: str) -> None:
    descriptor, temporary_name = tempfile.mkstemp(dir=destination.parent, suffix=TEMPORARY_FILE_SUFFIX)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            handle.write(content)
        os.replace(temporary_name, destination)
    except OSError:
        Path(temporary_name).unlink(missing_ok=True)
        raise


def _print_json(stdout, document: dict) -> None:
    print(json.dumps(document, ensure_ascii=False), file=stdout)


def _fail(stderr, error: Exception) -> int:
    print(f"erro: {error}", file=stderr)
    return FAILURE_EXIT_CODE


def _run_login(environ: Mapping[str, str], http, sleep, stderr) -> int:
    try:
        run_device_code_login(load_entra_config(environ), http, sleep, stderr)
    except (ApiError, ConfigError) as error:
        return _fail(stderr, error)
    return 0


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
        if not isinstance(document, dict):
            return _empty_state()
        return {
            TIMELINES_STATE_KEY: document.get(TIMELINES_STATE_KEY),
            "snapshot": document.get("snapshot"),
            "saved_at": document.get("saved_at"),
        }

    def save(self, document: dict) -> None:
        try:
            self._directory.mkdir(mode=STATE_DIRECTORY_MODE, parents=True, exist_ok=True)
            _write_atomically(self._path, json.dumps(document))
        except OSError:
            return


def state_key(config: PipelinesConfig) -> str:
    return STATE_KEY_SEPARATOR.join(
        (
            config.base_url,
            config.organization,
            LIST_SEPARATOR.join(config.projects),
            str(config.days),
            str(config.top),
            str(config.sample),
            config.auth.method,
            config.auth.tenant,
            config.auth.client_id,
            config.title,
        )
    )


def _empty_state() -> dict:
    return {TIMELINES_STATE_KEY: None, "snapshot": None, "saved_at": None}


def run_snapshot(
    store: StateStore,
    config: PipelinesConfig,
    collect: Callable[[dict], tuple[Overview, dict]],
    moment: datetime,
    stdout,
    stderr,
) -> int:
    state = store.load()
    try:
        overview, timelines = collect(state[TIMELINES_STATE_KEY])
    except (AuthError, RequestRejectedError, ConfigError) as error:
        return _fail(stderr, error)
    except (RateLimitError, NetworkError, UnexpectedResponseError) as error:
        cached = _cached_snapshot(state)
        if cached is None:
            return _fail(stderr, error)
        snapshot, saved_at = cached
        _print_json(stdout, with_offline_notice(snapshot, saved_at, moment))
        return 0
    snapshot = build_snapshot(overview, config)
    store.save({"saved_at": moment.isoformat(), "snapshot": snapshot, TIMELINES_STATE_KEY: timelines})
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


def main(
    arguments: list[str],
    environ=None,
    http=urllib_http,
    run_command=subprocess.run,
    sleep=time.sleep,
    now=None,
    stdout=None,
    stderr=None,
) -> int:
    environ = os.environ if environ is None else environ
    stdout = sys.stdout if stdout is None else stdout
    stderr = sys.stderr if stderr is None else stderr
    command = list(arguments)
    if command not in (SNAPSHOT_COMMAND, LOGIN_COMMAND):
        print(USAGE_MESSAGE, file=stderr)
        return ARGUMENT_ERROR_EXIT_CODE
    if command == LOGIN_COMMAND:
        return _run_login(environ, http, sleep, stderr)
    try:
        config = load_config(environ)
    except ConfigError as error:
        return _fail(stderr, error)
    store = StateStore(config.state_dir, state_key(config))
    client = AzureClient(config, http, token_provider_for(config.auth, http, run_command))
    moment = now if now is not None else datetime.now(timezone.utc)
    return run_snapshot(
        store,
        config,
        lambda timelines: collect_overview(client, config, moment, timelines),
        moment,
        stdout,
        stderr,
    )


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
