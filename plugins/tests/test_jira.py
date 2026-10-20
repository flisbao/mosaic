import base64
import io
import json
import sys
import tempfile
import threading
import time
import unittest
import urllib.parse
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "jira"))

import jira  # noqa: E402

SECRET = "TOKEN-SECRETO-123"


def base_environment(**overrides):
    environment = {
        "JIRA_SITE": "empresa.atlassian.net",
        "JIRA_EMAIL": "eu@empresa.com",
        "JIRA_TOKEN_ENV": "MEU_TOKEN",
        "MEU_TOKEN": SECRET,
    }
    environment.update(overrides)
    return {key: value for key, value in environment.items() if value is not None}


class ConfigTests(unittest.TestCase):
    def test_loads_a_valid_config(self):
        config = jira.load_config(base_environment())
        self.assertEqual(config.site, "empresa.atlassian.net")
        self.assertEqual(config.email, "eu@empresa.com")
        self.assertEqual(config.token, SECRET)
        self.assertEqual(config.base_url, "https://empresa.atlassian.net")

    def test_defaults_are_applied(self):
        config = jira.load_config(base_environment())
        self.assertEqual(config.budget_seconds, 40)
        self.assertTrue(str(config.state_dir).endswith(".local/state/mosaic/jira"))

    def test_state_dir_can_be_overridden(self):
        config = jira.load_config(base_environment(JIRA_STATE_DIR="/tmp/estado-jira"))
        self.assertEqual(config.state_dir, Path("/tmp/estado-jira"))

    def test_token_comes_from_the_named_variable(self):
        environment = base_environment(JIRA_TOKEN_ENV="OUTRO", OUTRO="  abc  ")
        self.assertEqual(jira.load_config(environment).token, "abc")

    def test_token_comes_from_a_file(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "token"
            path.write_text(f"{SECRET}\n", encoding="utf-8")
            environment = base_environment(
                JIRA_TOKEN_ENV=None, MEU_TOKEN=None, JIRA_TOKEN_FILE=str(path)
            )
            self.assertEqual(jira.load_config(environment).token, SECRET)

    def test_missing_token_is_an_error_without_leaking_values(self):
        environment = base_environment(MEU_TOKEN=None, OUTRA="valor-secreto-nao-vazar")
        with self.assertRaises(jira.ConfigError) as caught:
            jira.load_config(environment)
        message = str(caught.exception)
        self.assertIn("MEU_TOKEN", message)
        self.assertNotIn("valor-secreto-nao-vazar", message)

    def test_unreadable_token_file_is_an_error(self):
        environment = base_environment(
            JIRA_TOKEN_ENV=None, MEU_TOKEN=None, JIRA_TOKEN_FILE="/nao/existe/token"
        )
        with self.assertRaises(jira.ConfigError):
            jira.load_config(environment)

    def test_missing_site_or_email_is_an_error(self):
        for name in ("JIRA_SITE", "JIRA_EMAIL"):
            with self.assertRaises(jira.ConfigError, msg=name):
                jira.load_config(base_environment(**{name: None}))

    def test_hostile_site_values_are_rejected(self):
        hostile = ["https://x.com", "x.com/path", "x .com", "user@x.com", "", "x.com?a=b", "x\n.com"]
        for site in hostile:
            with self.assertRaises(jira.ConfigError, msg=repr(site)):
                jira.load_config(base_environment(JIRA_SITE=site))

    def test_base_url_only_accepts_https_or_local_http(self):
        overridden = jira.load_config(base_environment(JIRA_BASE_URL="http://127.0.0.1:9"))
        self.assertEqual(overridden.base_url, "http://127.0.0.1:9")
        localhost = jira.load_config(base_environment(JIRA_BASE_URL="http://localhost:9/"))
        self.assertEqual(localhost.base_url, "http://localhost:9")
        secure = jira.load_config(base_environment(JIRA_BASE_URL="https://outro.example.com"))
        self.assertEqual(secure.base_url, "https://outro.example.com")
        for rejected in ("http://example.com", "ftp://x.com", "http://127.0.0.1 :9"):
            with self.assertRaises(jira.ConfigError, msg=rejected):
                jira.load_config(base_environment(JIRA_BASE_URL=rejected))

    def test_budget_must_be_a_positive_integer(self):
        for value in ("0", "-3", "x", "1.5"):
            with self.assertRaises(jira.ConfigError, msg=value):
                jira.load_config(base_environment(JIRA_BUDGET_SECONDS=value))
        self.assertEqual(jira.load_config(base_environment(JIRA_BUDGET_SECONDS="15")).budget_seconds, 15)

    def test_removed_variables_are_rejected_with_a_migration_hint(self):
        for name in jira.REMOVED_VARIABLES:
            with self.assertRaises(jira.ConfigError, msg=name) as caught:
                jira.load_config(base_environment(**{name: "qualquer"}))
            self.assertEqual(str(caught.exception), f"{name} foi removido; use --jql")


class OptionsTests(unittest.TestCase):
    def test_defaults(self):
        options, remainder = jira.parse_options(["--jql", "project = A", "snapshot"])
        self.assertEqual(options.jql, "project = A")
        self.assertEqual(options.title, "Jira")
        self.assertEqual(options.group_by, "none")
        self.assertEqual(options.view, "lines")
        self.assertEqual(options.columns, jira.DEFAULT_COLUMNS)
        self.assertEqual(options.max_issues, 100)
        self.assertEqual(remainder, ["snapshot"])

    def test_all_options_are_parsed(self):
        options, remainder = jira.parse_options(
            [
                "--jql", "assignee = currentUser()", "--title", "Meus",
                "--group-by", "status", "--view", "table",
                "--columns", "key,priority,updated", "--max-issues", "500",
                "action", "x",
            ]
        )
        self.assertEqual(options.title, "Meus")
        self.assertEqual(options.group_by, "status")
        self.assertEqual(options.view, "table")
        self.assertEqual(options.columns, ("key", "priority", "updated"))
        self.assertEqual(options.max_issues, 500)
        self.assertEqual(remainder, ["action", "x"])

    def test_invalid_options_exit_with_code_two(self):
        invalid = [
            ["--jql", "x", "--max-issues", "900", "snapshot"],
            ["--jql", "x", "--max-issues", "0", "snapshot"],
            ["--jql", "x", "--columns", "key,nope", "snapshot"],
            ["--jql", "x", "--group-by", "team", "snapshot"],
            ["--jql", "x", "--view", "grid", "snapshot"],
            ["--jql", "", "snapshot"],
            ["--jql", "   ", "snapshot"],
            ["snapshot"],
        ]
        for arguments in invalid:
            with self.assertRaises(SystemExit, msg=arguments) as caught:
                jira.parse_options(arguments)
            self.assertEqual(caught.exception.code, 2, arguments)


class FakeHttp:
    def __init__(self, responses=None, error=None):
        self.responses = responses or []
        self.error = error
        self.calls = []

    def __call__(self, url, method, headers, timeout):
        self.calls.append({"url": url, "method": method, "headers": headers, "timeout": timeout})
        if self.error is not None:
            raise self.error
        status, body = self.responses.pop(0)
        if not isinstance(body, bytes):
            body = json.dumps(body).encode("utf-8")
        return status, body


def make_client(http, **overrides):
    return jira.JiraClient(jira.load_config(base_environment(**overrides)), http)


class ClientTests(unittest.TestCase):
    def test_sends_basic_auth_and_accept_headers(self):
        http = FakeHttp([(200, {"ok": True})])
        make_client(http).get_json("/rest/api/3/myself")
        headers = http.calls[0]["headers"]
        expected = base64.b64encode(f"eu@empresa.com:{SECRET}".encode()).decode()
        self.assertEqual(headers["Authorization"], f"Basic {expected}")
        self.assertEqual(headers["Accept"], "application/json")
        self.assertEqual(http.calls[0]["method"], "GET")

    def test_builds_url_with_encoded_params(self):
        http = FakeHttp([(200, {})])
        make_client(http).get_json("/rest/api/3/search/jql", {"jql": "project = A AND x != 1", "maxResults": 100})
        url = http.calls[0]["url"]
        self.assertTrue(url.startswith("https://empresa.atlassian.net/rest/api/3/search/jql?"))
        self.assertIn("jql=project+%3D+A+AND+x+%21%3D+1", url)
        self.assertIn("maxResults=100", url)

    def test_401_and_403_become_auth_error(self):
        for status in (401, 403):
            with self.assertRaises(jira.AuthError) as caught:
                make_client(FakeHttp([(status, {})])).get_json("/x")
            self.assertIn("credenciais inválidas para empresa.atlassian.net", str(caught.exception))

    def test_429_becomes_rate_limit_error(self):
        with self.assertRaises(jira.RateLimitError):
            make_client(FakeHttp([(429, {})])).get_json("/x")

    def test_network_exception_becomes_network_error(self):
        for error in (OSError("falhou"), TimeoutError("lento")):
            with self.assertRaises(jira.NetworkError):
                make_client(FakeHttp(error=error)).get_json("/x")

    def test_invalid_json_and_server_errors_become_unexpected_response(self):
        with self.assertRaises(jira.UnexpectedResponseError):
            make_client(FakeHttp([(200, b"<html>")])).get_json("/x")
        with self.assertRaises(jira.UnexpectedResponseError):
            make_client(FakeHttp([(500, {})])).get_json("/x")

    def test_errors_never_contain_the_token(self):
        cases = [
            (jira.AuthError, FakeHttp([(401, {"echo": SECRET})])),
            (jira.RateLimitError, FakeHttp([(429, {"echo": SECRET})])),
            (jira.UnexpectedResponseError, FakeHttp([(500, {"echo": SECRET})])),
            (jira.NetworkError, FakeHttp(error=OSError(f"falha {SECRET}"))),
        ]
        for expected_type, http in cases:
            with self.assertRaises(expected_type) as caught:
                make_client(http).get_json("/x")
            self.assertNotIn(SECRET, str(caught.exception))
            self.assertNotIn(SECRET, repr(caught.exception))


def issue_payload(key, **fields):
    base = {
        "summary": f"Resumo {key}",
        "status": {"name": "Em andamento", "statusCategory": {"key": "indeterminate"}},
        "assignee": {"displayName": "Ana"},
        "priority": {"name": "High"},
        "created": "2026-10-01T10:00:00.000+0000",
        "updated": "2026-10-02T11:30:00.000+0000",
    }
    base.update(fields)
    return {"key": key, "fields": base}


def page(keys, next_token=None):
    body = {"issues": [issue_payload(key) for key in keys]}
    if next_token:
        body["nextPageToken"] = next_token
    return 200, body


class FakeClock:
    def __init__(self, readings):
        self.readings = list(readings)

    def __call__(self):
        return self.readings.pop(0)


class SearchTests(unittest.TestCase):
    def test_jql_is_url_encoded_exactly(self):
        jql = 'summary ~ "a&b=c%" AND nome = "ação"'
        http = FakeHttp([page(["A-1"])])
        jira.search_issues(make_client(http), jql, 10, 40)
        query = urllib.parse.parse_qs(urllib.parse.urlparse(http.calls[0]["url"]).query)
        self.assertEqual(query["jql"], [jql])
        self.assertEqual(query["fields"], ["summary,status,assignee,priority,updated,created"])
        self.assertEqual(query["maxResults"], ["10"])
        self.assertNotIn("nextPageToken", query)

    def test_follows_next_page_token_until_last(self):
        http = FakeHttp([page(["A-1", "A-2"], "t2"), page(["A-3"])])
        result = jira.search_issues(make_client(http), "x", 10, 40)
        self.assertEqual([issue.key for issue in result.issues], ["A-1", "A-2", "A-3"])
        self.assertFalse(result.has_more)
        second = urllib.parse.parse_qs(urllib.parse.urlparse(http.calls[1]["url"]).query)
        self.assertEqual(second["nextPageToken"], ["t2"])
        self.assertEqual(second["maxResults"], ["8"])

    def test_stops_at_max_issues_and_reports_has_more_only_when_token_remains(self):
        with_token = FakeHttp([page(["A-1", "A-2"], "t2")])
        capped = jira.search_issues(make_client(with_token), "x", 2, 40)
        self.assertEqual(len(capped.issues), 2)
        self.assertTrue(capped.has_more)
        self.assertEqual(len(with_token.calls), 1)
        without_token = FakeHttp([page(["A-1", "A-2"])])
        exact = jira.search_issues(make_client(without_token), "x", 2, 40)
        self.assertEqual(len(exact.issues), 2)
        self.assertFalse(exact.has_more)

    def test_caps_page_size_at_one_hundred(self):
        http = FakeHttp([page(["A-1"])])
        jira.search_issues(make_client(http), "x", 500, 40)
        query = urllib.parse.parse_qs(urllib.parse.urlparse(http.calls[0]["url"]).query)
        self.assertEqual(query["maxResults"], ["100"])

    def test_budget_exhausted_raises_network_error(self):
        http = FakeHttp([page(["A-1"], "t2"), page(["A-2"])])
        clock = FakeClock([0, 0, 100])
        with self.assertRaises(jira.NetworkError) as caught:
            jira.search_issues(make_client(http), "x", 10, 40, clock)
        self.assertEqual(str(caught.exception), "orçamento de tempo esgotado")
        self.assertEqual(len(http.calls), 1)

    def test_http_400_becomes_query_error_with_jira_message(self):
        body = {"errorMessages": ["Campo 'foo' não existe."], "errors": {}}
        with self.assertRaises(jira.QueryError) as caught:
            make_client(FakeHttp([(400, body)])).get_json("/x")
        self.assertEqual(str(caught.exception), "JQL inválido: Campo 'foo' não existe.")
        self.assertIsInstance(caught.exception, jira.JiraError)

    def test_http_400_message_is_truncated_and_has_a_fallback(self):
        long_body = {"errorMessages": ["x" * 500]}
        with self.assertRaises(jira.QueryError) as caught:
            make_client(FakeHttp([(400, long_body)])).get_json("/x")
        self.assertEqual(len(str(caught.exception)), len("JQL inválido: ") + 200)
        for unusable in ({}, {"errorMessages": []}, {"errorMessages": [3]}, b"<html>"):
            with self.assertRaises(jira.QueryError) as caught:
                make_client(FakeHttp([(400, unusable)])).get_json("/x")
            self.assertEqual(str(caught.exception), "JQL inválido")

    def test_http_401_still_auth_error(self):
        with self.assertRaises(jira.AuthError):
            jira.search_issues(make_client(FakeHttp([(401, {})])), "x", 10, 40)

    def test_unexpected_search_payload_is_an_error(self):
        with self.assertRaises(jira.UnexpectedResponseError):
            jira.search_issues(make_client(FakeHttp([(200, {"issues": "nope"})])), "x", 10, 40)

    def test_invalid_issues_are_dropped_from_results(self):
        broken = {"key": "A-2", "fields": {"created": "ontem"}}
        http = FakeHttp([(200, {"issues": [issue_payload("A-1"), broken, None]})])
        result = jira.search_issues(make_client(http), "x", 10, 40)
        self.assertEqual([issue.key for issue in result.issues], ["A-1"])


class IssueTests(unittest.TestCase):
    def test_parse_issue_reads_every_field(self):
        issue = jira.parse_issue(issue_payload("A-1"))
        self.assertEqual(issue.key, "A-1")
        self.assertEqual(issue.summary, "Resumo A-1")
        self.assertEqual(issue.status, "Em andamento")
        self.assertEqual(issue.status_category, "indeterminate")
        self.assertEqual(issue.assignee, "Ana")
        self.assertEqual(issue.priority, "High")
        self.assertEqual(issue.updated, datetime(2026, 10, 2, 11, 30, tzinfo=timezone.utc))

    def test_parse_issue_handles_null_assignee_priority_and_unknown_category(self):
        payload = issue_payload(
            "A-1",
            assignee=None,
            priority=None,
            status={"name": "Estranho", "statusCategory": {"key": "outra"}},
        )
        issue = jira.parse_issue(payload)
        self.assertIsNone(issue.assignee)
        self.assertIsNone(issue.priority)
        self.assertEqual(issue.status_category, jira.UNKNOWN_CATEGORY)

    def test_parse_issue_survives_missing_optional_fields(self):
        payload = issue_payload("A-1", summary=None, status=None)
        issue = jira.parse_issue(payload)
        self.assertEqual(issue.summary, "")
        self.assertEqual(issue.status, "")
        self.assertEqual(issue.status_category, jira.UNKNOWN_CATEGORY)

    def test_parse_issue_drops_invalid_created(self):
        for created in ("ontem", None, 5):
            self.assertIsNone(jira.parse_issue(issue_payload("A-1", created=created)))
        self.assertIsNone(jira.parse_issue(issue_payload("A-1", updated="ontem")))

    def test_parse_issue_drops_payloads_without_key_or_fields(self):
        for payload in (None, "x", [], {}, {"key": "A-1"}, {"fields": {}}, {"key": "A-1", "fields": []}):
            self.assertIsNone(jira.parse_issue(payload))

    def test_issue_raw_round_trip(self):
        issue = jira.parse_issue(issue_payload("A-1", assignee=None))
        raw = json.loads(json.dumps(issue.to_raw()))
        self.assertEqual(jira.Issue.from_raw(raw), issue)


class _Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/slow"):
            time.sleep(3)
        if self.path.startswith("/huge"):
            body = b"[" + b"0," * 600 + b"0]"
        elif self.path.startswith("/error"):
            body = b"e" * 20000
        else:
            body = json.dumps({"path": self.path}).encode()
        self.send_response(500 if self.path.startswith("/error") else 200)
        self.send_header("Content-Type", "application/json")
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
        status, body = jira.urllib_http(f"{self.base}/ok", "GET", {"Accept": "application/json"}, 5)
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body), {"path": "/ok"})

    def test_urllib_http_rejects_a_body_above_the_limit(self):
        original = jira.MAXIMUM_RESPONSE_BYTES
        jira.MAXIMUM_RESPONSE_BYTES = 1000
        try:
            with self.assertRaises(jira.UnexpectedResponseError):
                jira.urllib_http(f"{self.base}/huge", "GET", {}, 5)
        finally:
            jira.MAXIMUM_RESPONSE_BYTES = original

    def test_urllib_http_truncates_an_error_body(self):
        status, body = jira.urllib_http(f"{self.base}/error", "GET", {}, 5)
        self.assertEqual(status, 500)
        self.assertEqual(len(body), jira.MAXIMUM_ERROR_BODY_BYTES)

    def test_urllib_http_times_out(self):
        with self.assertRaises(OSError):
            jira.urllib_http(f"{self.base}/slow", "GET", {}, 1)

    def test_client_over_real_http_with_base_url_override(self):
        client = make_client(jira.urllib_http, JIRA_BASE_URL=self.base)
        self.assertEqual(client.get_json("/rest/api/3/myself"), {"path": "/rest/api/3/myself"})


class HelperTests(unittest.TestCase):
    def test_timestamp_formats(self):
        expected = datetime(2026, 10, 17, 13, 0, tzinfo=timezone.utc)
        for text in (
            "2026-10-17T10:00:00.000-0300",
            "2026-10-17T10:00:00-03:00",
            "2026-10-17T13:00:00Z",
            "2026-10-17T13:00:00.000+0000",
            "2026-10-17T13:00:00+0000",
        ):
            self.assertEqual(jira.parse_timestamp(text), expected, text)
        self.assertEqual(
            jira.parse_timestamp("2026-10-17T13:00:00.5+0000").microsecond, 500000
        )
        for text in ("", "x", None, 5):
            self.assertIsNone(jira.parse_timestamp(text))

    def test_future_dates_give_zero_duration(self):
        self.assertEqual(jira.format_duration(timedelta(hours=-5)), "0m")
        self.assertEqual(jira.format_duration(timedelta(0)), "0m")

    def test_format_duration_boundaries(self):
        self.assertEqual(jira.format_duration(timedelta(minutes=59)), "59m")
        self.assertEqual(jira.format_duration(timedelta(minutes=60)), "1h")
        self.assertEqual(jira.format_duration(timedelta(hours=47, minutes=59)), "47h")
        self.assertEqual(jira.format_duration(timedelta(hours=48)), "2d")
        self.assertEqual(jira.format_duration(timedelta(days=10)), "10d")

    def test_flatten_text_collapses_and_truncates(self):
        self.assertEqual(jira.flatten_text("a\n\n b\t c  ", 80), "a b c")
        flattened = jira.flatten_text("x" * 500, 10)
        self.assertEqual(flattened, "x" * 9 + "…")
        self.assertEqual(jira.flatten_text("abc", 0), "")


NOW = datetime(2026, 10, 20, 12, 0, tzinfo=timezone.utc)
SITE = "empresa.atlassian.net"


def make_issue(index=1, **overrides):
    values = {
        "key": f"ABC-{index}",
        "summary": f"Resumo {index}",
        "status": "Em andamento",
        "status_category": "indeterminate",
        "assignee": "Ana",
        "priority": "Alta",
        "created": NOW - timedelta(days=3),
        "updated": NOW - timedelta(hours=5),
    }
    values.update(overrides)
    return jira.Issue(**values)


def make_options(**overrides):
    values = {
        "jql": "x",
        "title": "Meus tickets",
        "group_by": "none",
        "view": "lines",
        "columns": jira.DEFAULT_COLUMNS,
        "max_issues": 100,
    }
    values.update(overrides)
    return jira.WidgetOptions(**values)


def snapshot_of(issues, has_more=False, offline_since=None, **option_overrides):
    return jira.build_snapshot(
        jira.SearchResult(issues, has_more),
        make_options(**option_overrides),
        SITE,
        NOW,
        offline_since,
    )


class PresentationTests(unittest.TestCase):
    def test_lines_view_one_line_per_issue_with_tone_by_category(self):
        issues = [
            make_issue(1, status_category="new"),
            make_issue(2, status_category="indeterminate"),
            make_issue(3, status_category="done"),
            make_issue(4, status_category="unknown"),
        ]
        snapshot = snapshot_of(issues)
        self.assertEqual(snapshot["protocol"], 1)
        self.assertEqual(snapshot["summary"]["title"], "Meus tickets")
        block = snapshot["summary"]["blocks"][0]
        self.assertEqual(block["type"], "lines")
        self.assertEqual(
            block["lines"],
            [
                {"text": "ABC-1 Resumo 1", "tone": "muted"},
                {"text": "ABC-2 Resumo 2", "tone": "current"},
                {"text": "ABC-3 Resumo 3", "tone": "ok"},
                {"text": "ABC-4 Resumo 4", "tone": "normal"},
            ],
        )
        self.assertNotIn("actions", snapshot)

    def test_table_view_uses_requested_columns_and_headers(self):
        snapshot = snapshot_of(
            [make_issue(1, assignee=None)],
            view="table",
            columns=("key", "status", "assignee", "priority", "updated"),
        )
        block = snapshot["summary"]["blocks"][0]
        self.assertEqual(block["type"], "table")
        self.assertEqual(
            [column["title"] for column in block["columns"]],
            ["Chave", "Status", "Responsável", "Prioridade", "Atualizado"],
        )
        self.assertEqual(
            block["rows"], [["ABC-1", "Em andamento", "ninguem", "Alta", "5h"]]
        )

    def test_stat_view_counts_and_marks_has_more_with_plus(self):
        issues = [make_issue(1), make_issue(2)]
        exact = snapshot_of(issues, view="stat")["summary"]["blocks"][0]
        self.assertEqual(exact["type"], "stat")
        self.assertEqual(exact["items"], [{"value": "2", "label": "Meus tickets"}])
        partial = snapshot_of(issues, has_more=True, view="stat")
        self.assertEqual(partial["summary"]["blocks"][0]["items"][0]["value"], "2+")

    def test_group_by_assignee_orders_by_count_then_name_and_labels_nobody(self):
        issues = [
            make_issue(1, assignee="Zoe"),
            make_issue(2, assignee=None),
            make_issue(3, assignee="Ana"),
            make_issue(4, assignee="Zoe"),
        ]
        snapshot = snapshot_of(issues, view="stat", group_by="assignee")
        items = snapshot["summary"]["blocks"][0]["items"]
        self.assertEqual(
            [(item["label"], item["value"]) for item in items],
            [("Zoe", "2"), ("Ana", "1"), ("ninguem", "1")],
        )
        self.assertEqual(
            [section["heading"] for section in snapshot["detail"]],
            ["Zoe", "Ana", "ninguem"],
        )

    def test_group_by_priority_labels_missing_priority(self):
        snapshot = snapshot_of(
            [make_issue(1, priority=None)], view="stat", group_by="priority"
        )
        label = snapshot["summary"]["blocks"][0]["items"][0]["label"]
        self.assertEqual(label, "-")

    def test_stat_with_more_than_eight_groups_collapses_into_others(self):
        issues = []
        for group in range(10):
            for copy in range(10 - group):
                issues.append(
                    make_issue(len(issues), status=f"S{group}", assignee="x")
                )
        snapshot = snapshot_of(issues, view="stat", group_by="status")
        items = snapshot["summary"]["blocks"][0]["items"]
        self.assertEqual(len(items), 8)
        self.assertEqual(items[-1], {"value": "6", "label": "Outros"})
        self.assertEqual(len(snapshot["detail"]), 10)

    def test_detail_items_carry_key_and_meta(self):
        snapshot = snapshot_of([make_issue(1, assignee=None)])
        self.assertEqual(len(snapshot["detail"]), 1)
        section = snapshot["detail"][0]
        self.assertEqual(section["heading"], "Meus tickets")
        self.assertEqual(
            section["items"],
            [
                {
                    "id": "ABC-1",
                    "text": "ABC-1 — Resumo 1",
                    "meta": {
                        "status": "Em andamento",
                        "prioridade": "Alta",
                        "responsavel": "ninguem",
                        "site": SITE,
                    },
                }
            ],
        )

    def test_empty_result_is_a_valid_widget_in_every_view(self):
        for view in jira.VIEW_CHOICES:
            snapshot = snapshot_of([], view=view)
            self.assertNotIn("detail", snapshot)
            block = snapshot["summary"]["blocks"][0]
            if view == "stat":
                self.assertEqual(
                    block["items"], [{"value": "0", "label": "Meus tickets"}]
                )
            else:
                self.assertEqual(block["type"], "lines")
                self.assertEqual(
                    block["lines"], [{"text": "nenhum ticket", "tone": "muted"}]
                )
        grouped = snapshot_of([], view="stat", group_by="status")
        self.assertEqual(grouped["summary"]["blocks"][0]["items"][0]["value"], "0")

    def test_rows_are_capped_at_200_and_output_stays_under_1_mib(self):
        issues = [make_issue(index, summary="longo " * 100) for index in range(500)]
        for view in ("lines", "table"):
            snapshot = snapshot_of(issues, view=view, max_issues=500)
            self.assertLess(len(json.dumps(snapshot).encode()), 1_048_576)
            block = snapshot["summary"]["blocks"][0]
            rows = block["lines"] if view == "lines" else block["rows"]
            self.assertEqual(len(rows), 200)

    def test_offline_notice_is_first_block_with_warn_tone(self):
        offline_since = datetime(2026, 10, 20, 9, 30, tzinfo=timezone.utc)
        snapshot = snapshot_of([make_issue(1)], offline_since=offline_since)
        blocks = snapshot["summary"]["blocks"]
        expected = offline_since.astimezone().strftime("dados de %H:%M (offline)")
        self.assertEqual(
            blocks[0], {"type": "lines", "lines": [{"text": expected, "tone": "warn"}]}
        )
        self.assertEqual(blocks[1]["type"], "lines")

    def test_summary_text_is_flattened_and_truncated(self):
        title = "linha um\nlinha dois " + "x" * 200
        snapshot = snapshot_of([make_issue(1, summary=title)])
        text = snapshot["summary"]["blocks"][0]["lines"][0]["text"]
        self.assertNotIn("\n", text)
        self.assertTrue(text.startswith("ABC-1 linha um linha dois"))
        self.assertLessEqual(len(text), len("ABC-1 ") + 80)
        self.assertTrue(text.endswith("…"))


class CacheTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name) / "estado"
        self.cache = jira.OfflineCache(self.directory)
        self.result = jira.SearchResult([make_issue(1), make_issue(2, assignee=None)], True)

    def test_cache_round_trip_and_permissions(self):
        self.cache.save(SITE, "x", 100, self.result, NOW)
        loaded = self.cache.load(SITE, "x", 100)
        self.assertEqual(loaded, (self.result, NOW))
        self.assertEqual(self.directory.stat().st_mode & 0o777, 0o700)
        path = self.cache.path_for(SITE, "x", 100)
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assertRegex(path.name, r"^[0-9a-f]{64}\.json$")

    def test_cache_key_ignores_presentation_options(self):
        same = self.cache.path_for(SITE, "x", 100)
        self.assertEqual(same, self.cache.path_for(SITE, "x", 100))
        self.assertNotEqual(same, self.cache.path_for(SITE, "y", 100))
        self.assertNotEqual(same, self.cache.path_for("outro.atlassian.net", "x", 100))
        self.assertNotEqual(same, self.cache.path_for(SITE, "x", 50))

    def test_missing_cache_is_none(self):
        self.assertIsNone(self.cache.load(SITE, "x", 100))

    def test_corrupt_cache_is_ignored(self):
        self.cache.save(SITE, "x", 100, self.result, NOW)
        path = self.cache.path_for(SITE, "x", 100)
        for content in ("{nao e json", "[]", '{"issues": [{"key": "A"}]}', '{"issues": 3}'):
            path.write_text(content, encoding="utf-8")
            self.assertIsNone(self.cache.load(SITE, "x", 100), content)

    def test_unwritable_state_dir_does_not_fail_the_snapshot(self):
        blocker = Path(self.temporary.name) / "arquivo"
        blocker.write_text("x", encoding="utf-8")
        cache = jira.OfflineCache(blocker)
        cache.save(SITE, "x", 100, self.result, NOW)
        self.assertIsNone(cache.load(SITE, "x", 100))


class MainTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.state_dir = Path(self.temporary.name) / "estado"
        self.stdout = io.StringIO()
        self.stderr = io.StringIO()

    def run_main(self, http, arguments=("--jql", "x", "snapshot"), **environment_overrides):
        environment = base_environment(JIRA_STATE_DIR=str(self.state_dir), **environment_overrides)
        return jira.main(
            list(arguments),
            environ=environment,
            http=http,
            now=NOW,
            stdout=self.stdout,
            stderr=self.stderr,
        )

    def populate_cache(self):
        self.assertEqual(self.run_main(FakeHttp([page(["A-1"])])), 0)
        self.stdout.truncate(0)
        self.stdout.seek(0)

    def test_main_success_prints_valid_json_and_saves_cache(self):
        code = self.run_main(FakeHttp([page(["A-1", "A-2"])]))
        self.assertEqual(code, 0)
        snapshot = json.loads(self.stdout.getvalue())
        self.assertEqual(snapshot["protocol"], 1)
        self.assertEqual(len(snapshot["detail"][0]["items"]), 2)
        self.assertEqual(len(list(self.state_dir.glob("*.json"))), 1)

    def test_main_network_error_serves_cache_with_offline_notice(self):
        self.populate_cache()
        code = self.run_main(FakeHttp(error=OSError("sem rede")))
        self.assertEqual(code, 0)
        snapshot = json.loads(self.stdout.getvalue())
        first = snapshot["summary"]["blocks"][0]
        self.assertEqual(first["lines"][0]["tone"], "warn")
        self.assertIn("(offline)", first["lines"][0]["text"])

    def test_main_rate_limit_serves_cache(self):
        self.populate_cache()
        self.assertEqual(self.run_main(FakeHttp([(429, {})])), 0)
        self.assertIn("(offline)", self.stdout.getvalue())

    def test_main_auth_error_has_no_fallback_and_hides_token(self):
        self.populate_cache()
        self.assertEqual(self.run_main(FakeHttp([(401, {})])), 1)
        self.assertEqual(self.stdout.getvalue(), "")
        self.assertEqual(self.stderr.getvalue(), f"erro: credenciais inválidas para {SITE}\n")
        self.assertNotIn(SECRET, self.stderr.getvalue())

    def test_main_query_error_prints_jira_message_without_fallback(self):
        self.populate_cache()
        body = {"errorMessages": ["Field 'foo' does not exist"]}
        self.assertEqual(self.run_main(FakeHttp([(400, body)])), 1)
        self.assertEqual(self.stdout.getvalue(), "")
        self.assertIn("erro: JQL inválido: ", self.stderr.getvalue())
        self.assertIn("foo", self.stderr.getvalue())

    def test_main_without_cache_exits_1_with_message(self):
        self.assertEqual(self.run_main(FakeHttp([(429, {})])), 1)
        self.assertEqual(self.stdout.getvalue(), "")
        self.assertEqual(
            self.stderr.getvalue(),
            "erro: limite de requisições do Jira; tentando de novo mais tarde\n",
        )

    def test_main_action_exits_2(self):
        for arguments in (["--jql", "x", "action", "abc"], ["--jql", "x", "--nope", "snapshot"], ["--jql", "x"]):
            self.stderr.truncate(0)
            self.stderr.seek(0)
            self.assertEqual(self.run_main(FakeHttp(), arguments), 2, arguments)
            self.assertEqual(self.stderr.getvalue(), "uso: jira.py [opções] snapshot\n")
        self.assertEqual(self.stdout.getvalue(), "")

    def test_main_invalid_options_exit_2(self):
        self.assertEqual(self.run_main(FakeHttp(), ["snapshot"]), 2)

    def test_removed_variable_exits_1_with_hint(self):
        self.assertEqual(self.run_main(FakeHttp(), JIRA_MODE="x"), 1)
        self.assertEqual(self.stderr.getvalue(), "erro: JIRA_MODE foi removido; use --jql\n")

    def test_unwritable_state_dir_still_prints_snapshot(self):
        blocker = Path(self.temporary.name) / "arquivo"
        blocker.write_text("x", encoding="utf-8")
        self.state_dir = blocker
        self.assertEqual(self.run_main(FakeHttp([page(["A-1"])])), 0)
        self.assertEqual(json.loads(self.stdout.getvalue())["protocol"], 1)

    def test_token_never_in_stdout_stderr_or_cache_files(self):
        self.populate_cache()
        self.run_main(FakeHttp([(401, {})]))
        self.run_main(FakeHttp(error=OSError(SECRET)))
        self.run_main(FakeHttp([(500, {})]))
        collected = [self.stdout.getvalue(), self.stderr.getvalue()]
        collected += [path.read_text(encoding="utf-8") for path in self.state_dir.glob("*")]
        for text in collected:
            self.assertNotIn(SECRET, text)


if __name__ == "__main__":
    unittest.main()
