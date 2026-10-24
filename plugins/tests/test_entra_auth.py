import importlib.util
import io
import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
import urllib.parse
from pathlib import Path

PLUGINS_DIRECTORY = Path(__file__).resolve().parent.parent
ENTRA_MODULES = {
    "azure_devops": "azure-devops/azure_devops.py",
    "azure_pipelines": "azure-pipelines/azure_pipelines.py",
}
BLOCK_START = "# --- entra-auth: início ---"
BLOCK_END = "# --- entra-auth: fim ---"
NAMES_THE_BLOCK_EXPECTS_FROM_ITS_MODULE = (
    "ConfigError",
    "ApiError",
    "AuthError",
    "RateLimitError",
    "NetworkError",
    "UnexpectedResponseError",
    "HTTP_TOO_MANY_REQUESTS",
    "STATE_FILE_SUFFIX",
    "TEMPORARY_FILE_SUFFIX",
    "LOGIN_INSTRUCTION",
    "read_base_url",
    "read_state_dir",
)
SECRET = "SEGREDO-123"
ACCESS_TOKEN = "token-de-acesso"
TENANT = "11111111-2222-3333-4444-555555555555"
CLIENT_ID = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
AZURE_DEVOPS_RESOURCE = "499b84ac-1321-427f-aa17-267ca6975798"


def load_module(name):
    path = PLUGINS_DIRECTORY / ENTRA_MODULES[name]
    spec = importlib.util.spec_from_file_location(f"entra_under_test_{name}", path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class FakeHttp:
    def __init__(self, responses=None, error=None):
        self.responses = list(responses or [])
        self.error = error
        self.calls = []

    def __call__(self, url, method, headers, body, timeout):
        self.calls.append(
            {"url": url, "method": method, "headers": headers, "body": body, "timeout": timeout}
        )
        if self.error is not None:
            raise self.error
        status, payload = self.responses.pop(0)
        if not isinstance(payload, bytes):
            payload = json.dumps(payload).encode("utf-8")
        return status, payload


class FakeCompleted:
    def __init__(self, returncode=0, stdout="", stderr=""):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


class FakeRun:
    def __init__(self, result=None, error=None):
        self.result = result
        self.error = error
        self.calls = []

    def __call__(self, command, **options):
        self.calls.append({"command": command, "options": options})
        if self.error is not None:
            raise self.error
        return self.result


def service_principal_environment(**overrides):
    environment = {
        "AZDO_AUTH_METHOD": "service-principal",
        "AZDO_TENANT_ID": TENANT,
        "AZDO_CLIENT_ID": CLIENT_ID,
        "AZDO_CLIENT_SECRET_ENV": "SP_SECRET",
        "SP_SECRET": SECRET,
    }
    environment.update(overrides)
    return {key: value for key, value in environment.items() if value is not None}


class EntraAuthTests:
    module = None

    def test_default_method_is_azure_cli(self):
        config = self.module.load_entra_config({})
        self.assertEqual(config.method, "azure-cli")
        self.assertEqual(config.tenant, "organizations")

    def test_legacy_pat_variables_are_rejected_with_guidance(self):
        for variable in ("AZDO_TOKEN_ENV", "AZDO_TOKEN_FILE"):
            with self.subTest(variable=variable):
                with self.assertRaises(self.module.ConfigError) as caught:
                    self.module.load_entra_config({variable: SECRET})
                message = str(caught.exception)
                self.assertIn("PAT não é mais suportado", message)
                self.assertIn("AZDO_AUTH_METHOD", message)
                self.assertNotIn(SECRET, message)

    def test_unknown_method_lists_the_four_methods(self):
        with self.assertRaises(self.module.ConfigError) as caught:
            self.module.load_entra_config({"AZDO_AUTH_METHOD": "pat"})
        for method in ("azure-cli", "device-code", "service-principal", "managed-identity"):
            self.assertIn(method, str(caught.exception))

    def test_service_principal_requires_tenant_client_and_secret(self):
        cases = (
            ("AZDO_CLIENT_ID", "AZDO_CLIENT_ID"),
            ("AZDO_TENANT_ID", "AZDO_TENANT_ID"),
            ("AZDO_CLIENT_SECRET_ENV", "AZDO_CLIENT_SECRET_ENV"),
        )
        for removed, expected_mention in cases:
            with self.subTest(removed=removed):
                environment = service_principal_environment(**{removed: None})
                with self.assertRaises(self.module.ConfigError) as caught:
                    self.module.load_entra_config(environment)
                self.assertIn(expected_mention, str(caught.exception))

    def test_service_principal_rejects_organizations_tenant(self):
        for tenant in ("organizations", "common"):
            with self.subTest(tenant=tenant):
                with self.assertRaises(self.module.ConfigError):
                    self.module.load_entra_config(service_principal_environment(AZDO_TENANT_ID=tenant))

    def test_device_code_requires_client_id(self):
        with self.assertRaises(self.module.ConfigError) as caught:
            self.module.load_entra_config({"AZDO_AUTH_METHOD": "device-code"})
        self.assertIn("AZDO_CLIENT_ID", str(caught.exception))

    def test_tenant_and_client_id_accept_only_safe_characters(self):
        for variable in ("AZDO_TENANT_ID", "AZDO_CLIENT_ID"):
            with self.subTest(variable=variable):
                with self.assertRaises(self.module.ConfigError):
                    self.module.load_entra_config(
                        service_principal_environment(**{variable: "x/../y"})
                    )

    def test_secret_error_does_not_leak_the_value(self):
        with self.assertRaises(self.module.ConfigError) as caught:
            self.module.load_entra_config(service_principal_environment(SP_SECRET="a b"))
        self.assertNotIn("a b", str(caught.exception))

    def test_secret_can_come_from_a_file(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "segredo"
            path.write_text(SECRET + "\n", encoding="utf-8")
            environment = service_principal_environment(
                AZDO_CLIENT_SECRET_ENV=None, AZDO_CLIENT_SECRET_FILE=str(path)
            )
            config = self.module.load_entra_config(environment)
        self.assertEqual(config.client_secret, SECRET)

    def test_secret_is_not_in_the_config_representation(self):
        config = self.module.load_entra_config(service_principal_environment())
        self.assertNotIn(SECRET, repr(config))

    def test_azure_cli_builds_expected_arguments(self):
        run = FakeRun(FakeCompleted(stdout=json.dumps({"accessToken": ACCESS_TOKEN})))
        config = self.module.load_entra_config({})
        token = self.module.AzureCliTokenProvider(config, run).access_token()
        self.assertEqual(token, ACCESS_TOKEN)
        call = run.calls[0]
        self.assertEqual(
            call["command"],
            ["az", "account", "get-access-token", "--resource", AZURE_DEVOPS_RESOURCE, "--output", "json"],
        )
        self.assertEqual(call["options"]["stdin"], subprocess.DEVNULL)
        self.assertEqual(call["options"]["timeout"], 15)
        self.assertTrue(call["options"]["capture_output"])
        self.assertFalse(call["options"].get("shell", False))

    def test_azure_cli_passes_an_explicit_tenant(self):
        run = FakeRun(FakeCompleted(stdout=json.dumps({"accessToken": ACCESS_TOKEN})))
        config = self.module.load_entra_config({"AZDO_TENANT_ID": TENANT})
        self.module.AzureCliTokenProvider(config, run).access_token()
        self.assertEqual(run.calls[0]["command"][-2:], ["--tenant", TENANT])

    def test_azure_cli_missing_is_config_error(self):
        config = self.module.load_entra_config({})
        provider = self.module.AzureCliTokenProvider(config, FakeRun(error=FileNotFoundError()))
        with self.assertRaises(self.module.ConfigError) as caught:
            provider.access_token()
        self.assertIn("az", str(caught.exception))

    def test_azure_cli_failure_is_auth_error(self):
        config = self.module.load_entra_config({})
        provider = self.module.AzureCliTokenProvider(config, FakeRun(FakeCompleted(returncode=1)))
        with self.assertRaises(self.module.AuthError) as caught:
            provider.access_token()
        self.assertIn("az login", str(caught.exception))

    def test_azure_cli_stderr_is_never_echoed(self):
        config = self.module.load_entra_config({})
        run = FakeRun(FakeCompleted(returncode=1, stderr=f"falhou {SECRET}", stdout=SECRET))
        with self.assertRaises(self.module.AuthError) as caught:
            self.module.AzureCliTokenProvider(config, run).access_token()
        self.assertNotIn(SECRET, str(caught.exception))

    def test_azure_cli_garbage_output_is_unexpected_and_never_echoed(self):
        config = self.module.load_entra_config({})
        run = FakeRun(FakeCompleted(stdout=f"não é json {SECRET}"))
        with self.assertRaises(self.module.UnexpectedResponseError) as caught:
            self.module.AzureCliTokenProvider(config, run).access_token()
        self.assertNotIn(SECRET, str(caught.exception))

    def test_azure_cli_timeout_is_network_error(self):
        config = self.module.load_entra_config({})
        run = FakeRun(error=subprocess.TimeoutExpired(cmd="az", timeout=15))
        with self.assertRaises(self.module.NetworkError):
            self.module.AzureCliTokenProvider(config, run).access_token()

    def test_service_principal_posts_client_credentials(self):
        http = FakeHttp([(200, {"access_token": ACCESS_TOKEN, "expires_in": 3600})])
        config = self.module.load_entra_config(service_principal_environment())
        token = self.module.ServicePrincipalTokenProvider(config, http).access_token()
        self.assertEqual(token, ACCESS_TOKEN)
        call = http.calls[0]
        self.assertEqual(call["method"], "POST")
        self.assertEqual(call["url"], f"https://login.microsoftonline.com/{TENANT}/oauth2/v2.0/token")
        self.assertEqual(call["headers"]["Content-Type"], "application/x-www-form-urlencoded")
        fields = urllib.parse.parse_qs(call["body"].decode("ascii"))
        self.assertEqual(
            fields,
            {
                "grant_type": ["client_credentials"],
                "client_id": [CLIENT_ID],
                "client_secret": [SECRET],
                "scope": [f"{AZURE_DEVOPS_RESOURCE}/.default"],
            },
        )

    def test_managed_identity_sends_metadata_header_and_resource(self):
        http = FakeHttp([(200, {"access_token": ACCESS_TOKEN})])
        config = self.module.load_entra_config({"AZDO_AUTH_METHOD": "managed-identity"})
        token = self.module.ManagedIdentityTokenProvider(config, http).access_token()
        self.assertEqual(token, ACCESS_TOKEN)
        call = http.calls[0]
        self.assertEqual(call["method"], "GET")
        self.assertEqual(call["headers"]["Metadata"], "true")
        parsed = urllib.parse.urlparse(call["url"])
        self.assertEqual(f"{parsed.scheme}://{parsed.netloc}", "http://169.254.169.254")
        self.assertEqual(parsed.path, "/metadata/identity/oauth2/token")
        query = urllib.parse.parse_qs(parsed.query)
        self.assertEqual(query["resource"], [AZURE_DEVOPS_RESOURCE])
        self.assertIn("api-version", query)
        self.assertNotIn("client_id", query)

    def test_managed_identity_selects_a_user_assigned_identity(self):
        http = FakeHttp([(200, {"access_token": ACCESS_TOKEN})])
        config = self.module.load_entra_config(
            {"AZDO_AUTH_METHOD": "managed-identity", "AZDO_CLIENT_ID": CLIENT_ID}
        )
        self.module.ManagedIdentityTokenProvider(config, http).access_token()
        query = urllib.parse.parse_qs(urllib.parse.urlparse(http.calls[0]["url"]).query)
        self.assertEqual(query["client_id"], [CLIENT_ID])

    def test_managed_identity_unreachable_is_network_error(self):
        config = self.module.load_entra_config({"AZDO_AUTH_METHOD": "managed-identity"})
        provider = self.module.ManagedIdentityTokenProvider(config, FakeHttp(error=OSError("sem rota")))
        with self.assertRaises(self.module.NetworkError) as caught:
            provider.access_token()
        self.assertIn("IMDS", str(caught.exception))

    def test_entra_error_body_is_never_echoed(self):
        body = {"error": "invalid_client", "error_description": f"AADSTS7000215 {SECRET}"}
        http = FakeHttp([(401, body)])
        config = self.module.load_entra_config(service_principal_environment())
        with self.assertRaises(self.module.AuthError) as caught:
            self.module.ServicePrincipalTokenProvider(config, http).access_token()
        self.assertNotIn(SECRET, str(caught.exception))
        self.assertNotIn("AADSTS", str(caught.exception))

    def test_entra_errors_map_to_exceptions(self):
        module = self.module
        cases = (
            (400, {"error": "invalid_client"}, module.AuthError),
            (401, {"error": "unauthorized_client"}, module.AuthError),
            (400, {"error": "invalid_grant"}, module.AuthError),
            (429, {}, module.RateLimitError),
            (503, {}, module.NetworkError),
            (400, {"error": "algo_novo"}, module.AuthError),
            (404, b"", module.AuthError),
            (302, b"", module.UnexpectedResponseError),
        )
        for status, payload, expected in cases:
            with self.subTest(status=status, payload=payload):
                body = payload if isinstance(payload, bytes) else json.dumps(payload).encode("utf-8")
                with self.assertRaises(expected):
                    module.decode_entra_response(status, body)

    def test_unmapped_entra_client_error_is_permanent_and_never_echoed(self):
        body = {"error": "invalid_request", "error_description": f"AADSTS900144 {SECRET}"}
        with self.assertRaises(self.module.AuthError) as caught:
            self.module.decode_entra_response(400, json.dumps(body).encode("utf-8"))
        self.assertNotIn(SECRET, str(caught.exception))
        self.assertNotIn("invalid_request", str(caught.exception))

    def test_entra_success_with_garbage_body_is_unexpected(self):
        with self.assertRaises(self.module.UnexpectedResponseError):
            self.module.decode_entra_response(200, b"<html>")
        with self.assertRaises(self.module.UnexpectedResponseError):
            self.module.decode_entra_response(200, b"[]")

    def test_response_without_access_token_is_unexpected(self):
        http = FakeHttp([(200, {"expires_in": 3600})])
        config = self.module.load_entra_config(service_principal_environment())
        with self.assertRaises(self.module.UnexpectedResponseError):
            self.module.ServicePrincipalTokenProvider(config, http).access_token()

    def test_bearer_header_rejects_non_latin1_token(self):
        self.assertEqual(self.module.bearer_authorization("tok"), "Bearer tok")
        for token in ("tóken€", "com espaço", "quebra\nlinha"):
            with self.subTest(token=token):
                with self.assertRaises(self.module.ConfigError) as caught:
                    self.module.bearer_authorization(token)
                self.assertNotIn(token, str(caught.exception))

    def test_login_and_imds_urls_accept_only_https_or_local_http(self):
        for variable in ("AZDO_LOGIN_URL", "AZDO_IMDS_URL"):
            with self.subTest(variable=variable):
                with self.assertRaises(self.module.ConfigError):
                    self.module.load_entra_config({variable: "http://evil.example.com"})
                local = self.module.load_entra_config({variable: "http://127.0.0.1:9000/"})
                value = local.login_url if variable == "AZDO_LOGIN_URL" else local.imds_url
                self.assertEqual(value, "http://127.0.0.1:9000")

    def test_token_provider_for_selects_by_method(self):
        module = self.module
        expected = {
            "azure-cli": module.AzureCliTokenProvider,
            "service-principal": module.ServicePrincipalTokenProvider,
            "managed-identity": module.ManagedIdentityTokenProvider,
        }
        environments = {
            "azure-cli": {},
            "service-principal": service_principal_environment(),
            "managed-identity": {"AZDO_AUTH_METHOD": "managed-identity"},
        }
        for method, provider_class in expected.items():
            with self.subTest(method=method):
                config = module.load_entra_config(environments[method])
                provider = module.token_provider_for(config, FakeHttp(), FakeRun())
                self.assertIsInstance(provider, provider_class)


REFRESH_TOKEN = "REFRESH-SECRETO-456"
DEVICE_CODE = "DEVICE-CODE-789"
LOGIN_MESSAGE = "Abra https://microsoft.com/devicelogin e digite o código ABC123"


def device_code_grant(**overrides):
    grant = {
        "device_code": DEVICE_CODE,
        "user_code": "ABC123",
        "message": LOGIN_MESSAGE,
        "interval": 5,
        "expires_in": 900,
    }
    grant.update(overrides)
    return grant


class DeviceCodeTests:
    module = None

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.credentials_dir = Path(temporary.name) / "credenciais"
        self.config = self.module.load_entra_config(
            {
                "AZDO_AUTH_METHOD": "device-code",
                "AZDO_CLIENT_ID": CLIENT_ID,
                "AZDO_CREDENTIALS_DIR": str(self.credentials_dir),
            }
        )
        self.stderr = io.StringIO()
        self.sleeps = []

    def login(self, responses):
        http = FakeHttp(responses)
        self.module.run_device_code_login(self.config, http, self.sleeps.append, self.stderr)
        return http

    def store(self):
        return self.module.CredentialStore(self.config)

    def authorized(self):
        return (200, {"access_token": ACCESS_TOKEN, "refresh_token": REFRESH_TOKEN})

    def pending(self, code="authorization_pending"):
        return (400, {"error": code})

    def test_login_prints_only_the_message_to_stderr(self):
        self.login([(200, device_code_grant()), self.authorized()])
        self.assertEqual(self.stderr.getvalue(), LOGIN_MESSAGE + "\nlogin concluído\n")
        for secret in (DEVICE_CODE, REFRESH_TOKEN, ACCESS_TOKEN):
            self.assertNotIn(secret, self.stderr.getvalue())

    def test_login_requests_device_code_with_offline_access(self):
        http = self.login([(200, device_code_grant()), self.authorized()])
        call = http.calls[0]
        self.assertEqual(call["url"], "https://login.microsoftonline.com/organizations/oauth2/v2.0/devicecode")
        fields = urllib.parse.parse_qs(call["body"].decode("ascii"))
        self.assertEqual(fields["client_id"], [CLIENT_ID])
        self.assertEqual(fields["scope"], [f"{AZURE_DEVOPS_RESOURCE}/.default offline_access"])

    def test_login_polls_until_authorized(self):
        http = self.login(
            [(200, device_code_grant()), self.pending(), self.pending(), self.authorized()]
        )
        self.assertEqual(self.sleeps, [5, 5])
        poll = urllib.parse.parse_qs(http.calls[1]["body"].decode("ascii"))
        self.assertEqual(poll["grant_type"], ["urn:ietf:params:oauth:grant-type:device_code"])
        self.assertEqual(poll["device_code"], [DEVICE_CODE])
        self.assertEqual(poll["client_id"], [CLIENT_ID])

    def test_login_slow_down_increases_interval(self):
        self.login(
            [
                (200, device_code_grant()),
                self.pending("slow_down"),
                self.pending(),
                self.authorized(),
            ]
        )
        self.assertEqual(self.sleeps, [10, 10])

    def test_login_expired_is_auth_error(self):
        responses = [(200, device_code_grant(expires_in=10))] + [self.pending()] * 5
        with self.assertRaises(self.module.AuthError):
            self.login(responses)
        self.assertIsNone(self.store().load_refresh_token())

    def test_login_expired_token_error_is_auth_error(self):
        with self.assertRaises(self.module.AuthError):
            self.login([(200, device_code_grant()), self.pending("expired_token")])

    def test_login_declined_is_auth_error(self):
        with self.assertRaises(self.module.AuthError):
            self.login([(200, device_code_grant()), self.pending("authorization_declined")])

    def test_login_without_refresh_token_is_unexpected(self):
        with self.assertRaises(self.module.UnexpectedResponseError):
            self.login([(200, device_code_grant()), (200, {"access_token": ACCESS_TOKEN})])

    def test_login_saves_refresh_token_with_private_mode(self):
        self.login([(200, device_code_grant()), self.authorized()])
        self.assertEqual(stat.S_IMODE(self.credentials_dir.stat().st_mode), 0o700)
        files = list(self.credentials_dir.iterdir())
        self.assertEqual(len(files), 1)
        self.assertEqual(stat.S_IMODE(files[0].stat().st_mode), 0o600)
        self.assertEqual(self.store().load_refresh_token(), REFRESH_TOKEN)

    def test_credentials_directory_must_be_private(self):
        self.credentials_dir.mkdir(mode=0o755)
        os.chmod(self.credentials_dir, 0o755)
        with self.assertRaises(self.module.ConfigError):
            self.store().save_refresh_token(REFRESH_TOKEN)
        self.assertEqual(list(self.credentials_dir.iterdir()), [])

    def test_login_with_other_method_is_config_error(self):
        config = self.module.load_entra_config({})
        with self.assertRaises(self.module.ConfigError) as caught:
            self.module.run_device_code_login(config, FakeHttp(), self.sleeps.append, self.stderr)
        self.assertIn("device-code", str(caught.exception))

    def test_snapshot_without_login_asks_to_login(self):
        provider = self.module.DeviceCodeTokenProvider(self.config, FakeHttp(), self.store())
        with self.assertRaises(self.module.AuthError) as caught:
            provider.access_token()
        self.assertIn("login", str(caught.exception))

    def test_refresh_returns_access_token_and_rotates_refresh_token(self):
        self.store().save_refresh_token("antigo")
        http = FakeHttp([(200, {"access_token": ACCESS_TOKEN, "refresh_token": "novo"})])
        provider = self.module.DeviceCodeTokenProvider(self.config, http, self.store())
        self.assertEqual(provider.access_token(), ACCESS_TOKEN)
        fields = urllib.parse.parse_qs(http.calls[0]["body"].decode("ascii"))
        self.assertEqual(fields["grant_type"], ["refresh_token"])
        self.assertEqual(fields["refresh_token"], ["antigo"])
        self.assertEqual(fields["client_id"], [CLIENT_ID])
        self.assertEqual(self.store().load_refresh_token(), "novo")

    def test_refresh_without_rotation_keeps_the_stored_token(self):
        self.store().save_refresh_token("antigo")
        http = FakeHttp([(200, {"access_token": ACCESS_TOKEN})])
        self.module.DeviceCodeTokenProvider(self.config, http, self.store()).access_token()
        self.assertEqual(self.store().load_refresh_token(), "antigo")

    def test_invalid_grant_removes_credential_and_asks_to_login(self):
        self.store().save_refresh_token("antigo")
        http = FakeHttp([(400, {"error": "invalid_grant", "error_description": SECRET})])
        provider = self.module.DeviceCodeTokenProvider(self.config, http, self.store())
        with self.assertRaises(self.module.AuthError) as caught:
            provider.access_token()
        self.assertIn("login", str(caught.exception))
        self.assertNotIn(SECRET, str(caught.exception))
        self.assertIsNone(self.store().load_refresh_token())

    def test_other_entra_errors_keep_the_credential(self):
        self.store().save_refresh_token("antigo")
        http = FakeHttp([(401, {"error": "invalid_client"})])
        provider = self.module.DeviceCodeTokenProvider(self.config, http, self.store())
        with self.assertRaises(self.module.AuthError):
            provider.access_token()
        self.assertEqual(self.store().load_refresh_token(), "antigo")

    def test_credential_file_is_keyed_by_login_tenant_and_client(self):
        other = self.module.load_entra_config(
            {
                "AZDO_AUTH_METHOD": "device-code",
                "AZDO_CLIENT_ID": "outro-cliente",
                "AZDO_CREDENTIALS_DIR": str(self.credentials_dir),
            }
        )
        self.store().save_refresh_token("antigo")
        self.assertIsNone(self.module.CredentialStore(other).load_refresh_token())

    def test_login_checks_the_credentials_directory_before_requesting_a_code(self):
        self.credentials_dir.mkdir(mode=0o755)
        os.chmod(self.credentials_dir, 0o755)
        http = FakeHttp()
        with self.assertRaises(self.module.ConfigError):
            self.module.run_device_code_login(self.config, http, self.sleeps.append, self.stderr)
        self.assertEqual(http.calls, [])

    def test_refresh_refuses_a_credentials_directory_that_is_not_private(self):
        self.store().save_refresh_token("antigo")
        os.chmod(self.credentials_dir, 0o755)
        http = FakeHttp()
        provider = self.module.DeviceCodeTokenProvider(self.config, http, self.store())
        with self.assertRaises(self.module.ConfigError):
            provider.access_token()
        self.assertEqual(http.calls, [])

    def test_refresh_refuses_a_credential_file_that_is_not_private(self):
        self.store().save_refresh_token("antigo")
        (credential,) = self.credentials_dir.iterdir()
        os.chmod(credential, 0o644)
        http = FakeHttp()
        provider = self.module.DeviceCodeTokenProvider(self.config, http, self.store())
        with self.assertRaises(self.module.ConfigError):
            provider.access_token()
        self.assertEqual(http.calls, [])

    def test_rotation_failure_is_not_swallowed(self):
        module = self.module

        class UnwritableStore:
            def load_refresh_token(self):
                return "antigo"

            def save_refresh_token(self, token):
                raise module.ConfigError("não foi possível gravar a credencial")

        http = FakeHttp([(200, {"access_token": ACCESS_TOKEN, "refresh_token": "novo"})])
        with self.assertRaises(module.ConfigError):
            module.DeviceCodeTokenProvider(self.config, http, UnwritableStore()).access_token()

    @unittest.skipIf(os.geteuid() == 0, "root ignora as permissões da pasta")
    def test_credential_delete_failure_is_a_config_error(self):
        self.store().save_refresh_token("antigo")
        os.chmod(self.credentials_dir, 0o500)
        self.addCleanup(os.chmod, self.credentials_dir, 0o700)
        with self.assertRaises(self.module.ConfigError):
            self.store().delete()

    def test_token_provider_for_device_code(self):
        provider = self.module.token_provider_for(self.config, FakeHttp(), FakeRun())
        self.assertIsInstance(provider, self.module.DeviceCodeTokenProvider)


def entra_block_of(name):
    text = (PLUGINS_DIRECTORY / ENTRA_MODULES[name]).read_text(encoding="utf-8")
    start = text.index(BLOCK_START)
    return text[start : text.index(BLOCK_END, start) + len(BLOCK_END)]


class SharedBlockTests(unittest.TestCase):
    def test_entra_auth_blocks_are_identical(self):
        blocks = {name: entra_block_of(name) for name in ENTRA_MODULES}
        reference_name, *others = blocks
        for name in others:
            with self.subTest(module=name):
                self.assertEqual(blocks[name], blocks[reference_name])

    def test_every_module_defines_the_names_the_block_expects(self):
        for name in ENTRA_MODULES:
            module = load_module(name)
            for expected in NAMES_THE_BLOCK_EXPECTS_FROM_ITS_MODULE:
                with self.subTest(module=name, name=expected):
                    self.assertTrue(hasattr(module, expected))

    def test_login_instruction_points_to_the_module_own_script(self):
        for name, relative_path in ENTRA_MODULES.items():
            with self.subTest(module=name):
                self.assertIn(Path(relative_path).name, load_module(name).LOGIN_INSTRUCTION)


for _name in ENTRA_MODULES:
    _module = load_module(_name)
    for _mixin in (EntraAuthTests, DeviceCodeTests):
        _test_name = f"{_mixin.__name__}_{_name}"
        globals()[_test_name] = type(_test_name, (_mixin, unittest.TestCase), {"module": _module})
del _name, _module, _mixin, _test_name


if __name__ == "__main__":
    unittest.main()
