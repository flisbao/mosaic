import json
import sys
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "github"))

import github_prs  # noqa: E402

SECRET = "TOKEN-SECRETO-123"


def base_environment(**overrides):
    environment = {
        "GITHUB_TOKEN_ENV": "MEU_TOKEN",
        "MEU_TOKEN": SECRET,
    }
    environment.update(overrides)
    return {key: value for key, value in environment.items() if value is not None}


class FakeHttp:
    def __init__(self, responses=None, error=None):
        self.responses = responses or []
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


def make_client(http, **overrides):
    return github_prs.GitHubClient(github_prs.load_config(base_environment(**overrides)), http)


class ConfigTests(unittest.TestCase):
    def test_defaults(self):
        config = github_prs.load_config(base_environment())
        self.assertEqual(config.api_url, "https://api.github.com")
        self.assertEqual(config.token, SECRET)
        self.assertEqual(config.owners, ())
        self.assertEqual(config.title, "GitHub")
        self.assertTrue(str(config.state_dir).endswith(".local/state/mosaic/github"))

    def test_owners_are_split_and_trimmed(self):
        config = github_prs.load_config(base_environment(GITHUB_OWNERS=" acme, ,eu "))
        self.assertEqual(config.owners, ("acme", "eu"))

    def test_title_and_state_dir_can_be_overridden(self):
        config = github_prs.load_config(
            base_environment(GITHUB_TITLE="Meus PRs", GITHUB_STATE_DIR="/tmp/estado-github")
        )
        self.assertEqual(config.title, "Meus PRs")
        self.assertEqual(config.state_dir, Path("/tmp/estado-github"))

    def test_token_comes_from_a_file(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "token"
            path.write_text(f"{SECRET}\n", encoding="utf-8")
            environment = base_environment(
                GITHUB_TOKEN_ENV=None, MEU_TOKEN=None, GITHUB_TOKEN_FILE=str(path)
            )
            self.assertEqual(github_prs.load_config(environment).token, SECRET)

    def test_missing_token_is_an_error(self):
        with self.assertRaises(github_prs.ConfigError) as caught:
            github_prs.load_config(base_environment(GITHUB_TOKEN_ENV=None, MEU_TOKEN=None))
        self.assertIn("GITHUB_TOKEN_ENV", str(caught.exception))

    def test_base_url_accepts_only_https_or_local_http(self):
        for bad in ("http://api.github.com", "ftp://x", "https://", "https://a b"):
            with self.assertRaises(github_prs.ConfigError, msg=bad):
                github_prs.load_config(base_environment(GITHUB_API_URL=bad))
        local = github_prs.load_config(base_environment(GITHUB_API_URL="http://127.0.0.1:8080/"))
        self.assertEqual(local.api_url, "http://127.0.0.1:8080")


class ClientTests(unittest.TestCase):
    def test_posts_graphql_with_bearer_token(self):
        http = FakeHttp([(200, {"data": {"viewer": {"login": "eu"}}})])
        data = make_client(http).query("query { viewer { login } }", {})
        self.assertEqual(data, {"viewer": {"login": "eu"}})
        call = http.calls[0]
        self.assertEqual(call["url"], "https://api.github.com/graphql")
        self.assertEqual(call["method"], "POST")
        self.assertEqual(call["timeout"], github_prs.REQUEST_TIMEOUT_SECONDS)
        self.assertEqual(call["headers"]["Authorization"], f"Bearer {SECRET}")
        self.assertEqual(call["headers"]["Content-Type"], "application/json")
        self.assertEqual(call["headers"]["Accept"], "application/json")
        self.assertEqual(json.loads(call["body"]), {"query": "query { viewer { login } }", "variables": {}})

    def test_status_errors_map_to_exceptions(self):
        cases = [
            (401, github_prs.AuthError),
            (403, github_prs.AuthError),
            (429, github_prs.RateLimitError),
            (500, github_prs.UnexpectedResponseError),
        ]
        for status, error in cases:
            with self.assertRaises(error, msg=status):
                make_client(FakeHttp([(status, b"{}")])).query("q", {})

    def test_forbidden_with_rate_limit_body_is_rate_limit(self):
        body = b'{"message": "You have exceeded a secondary rate limit"}'
        with self.assertRaises(github_prs.RateLimitError):
            make_client(FakeHttp([(403, body)])).query("q", {})

    def test_graphql_errors_without_data_are_unexpected(self):
        http = FakeHttp([(200, {"errors": [{"message": "x"}], "data": None})])
        with self.assertRaises(github_prs.UnexpectedResponseError):
            make_client(http).query("q", {})

    def test_graphql_rate_limited_error_type_is_rate_limit(self):
        http = FakeHttp([(200, {"errors": [{"type": "RATE_LIMITED", "message": "x"}]})])
        with self.assertRaises(github_prs.RateLimitError):
            make_client(http).query("q", {})

    def test_network_failure_is_network_error(self):
        for error in (OSError("boom"), TimeoutError("lento")):
            with self.assertRaises(github_prs.NetworkError):
                make_client(FakeHttp(error=error)).query("q", {})

    def test_error_messages_never_contain_the_token(self):
        for status in (401, 403, 429, 500):
            body = json.dumps({"message": f"bad {SECRET}"}).encode()
            with self.assertRaises(github_prs.ApiError) as caught:
                make_client(FakeHttp([(status, body)])).query("q", {})
            self.assertNotIn(SECRET, str(caught.exception))
            self.assertNotIn(SECRET, repr(caught.exception))

    def test_graphql_error_messages_never_contain_the_token(self):
        http = FakeHttp([(200, {"errors": [{"message": SECRET}], "data": None})])
        with self.assertRaises(github_prs.ApiError) as caught:
            make_client(http).query("q", {})
        self.assertNotIn(SECRET, str(caught.exception))

    def test_network_error_never_contains_the_token(self):
        with self.assertRaises(github_prs.NetworkError) as caught:
            make_client(FakeHttp(error=OSError(f"falha {SECRET}"))).query("q", {})
        self.assertNotIn(SECRET, str(caught.exception))


class _Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/slow"):
            time.sleep(3)
        if self.path.startswith("/redirect"):
            self._send(302, b"", [("Location", "/ok")])
        elif self.path.startswith("/huge"):
            self._send(200, b"[" + b"0," * 600 + b"0]")
        elif self.path.startswith("/error"):
            self._send(500, b"e" * 20000)
        else:
            self._send(200, json.dumps({"path": self.path}).encode())

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        self._send(200, self.rfile.read(length))

    def _send(self, status, body, headers=()):
        self.send_response(status)
        for name, value in headers:
            self.send_header(name, value)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *arguments):
        pass


class UrllibHttpTests(unittest.TestCase):
    def setUp(self):
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), _Handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.server.server_address[1]}"

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()

    def test_urllib_http_reads_a_real_response(self):
        status, body = github_prs.urllib_http(f"{self.base}/ok", "GET", {}, None, 5)
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body), {"path": "/ok"})

    def test_urllib_http_sends_the_request_body(self):
        status, body = github_prs.urllib_http(
            f"{self.base}/echo", "POST", {"Content-Type": "application/json"}, b'{"a": 1}', 5
        )
        self.assertEqual(status, 200)
        self.assertEqual(body, b'{"a": 1}')

    def test_urllib_http_refuses_redirects(self):
        status, _ = github_prs.urllib_http(f"{self.base}/redirect", "GET", {}, None, 5)
        self.assertEqual(status, 302)

    def test_urllib_http_rejects_a_body_above_the_limit(self):
        original = github_prs.MAXIMUM_RESPONSE_BYTES
        github_prs.MAXIMUM_RESPONSE_BYTES = 1000
        try:
            with self.assertRaises(github_prs.UnexpectedResponseError):
                github_prs.urllib_http(f"{self.base}/huge", "GET", {}, None, 5)
        finally:
            github_prs.MAXIMUM_RESPONSE_BYTES = original

    def test_urllib_http_truncates_an_error_body(self):
        status, body = github_prs.urllib_http(f"{self.base}/error", "GET", {}, None, 5)
        self.assertEqual(status, 500)
        self.assertEqual(len(body), github_prs.MAXIMUM_ERROR_BODY_BYTES)

    def test_urllib_http_times_out(self):
        with self.assertRaises(OSError):
            github_prs.urllib_http(f"{self.base}/slow", "GET", {}, None, 1)


if __name__ == "__main__":
    unittest.main()
