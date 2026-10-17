import base64
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

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import jira  # noqa: E402

SECRET = "TOKEN-SECRETO-123"


def base_environment(**overrides):
    environment = {
        "JIRA_SITE": "empresa.atlassian.net",
        "JIRA_EMAIL": "eu@empresa.com",
        "JIRA_TOKEN_ENV": "MEU_TOKEN",
        "MEU_TOKEN": SECRET,
        "JIRA_MODE": "manager",
        "JIRA_PROJECT": "VHPJ",
    }
    environment.update(overrides)
    return {key: value for key, value in environment.items() if value is not None}


class ConfigTests(unittest.TestCase):
    def test_loads_a_valid_manager_config(self):
        config = jira.load_config(
            base_environment(JIRA_BOARD="42", JIRA_TEAM="ana@x.com, Bruno ,,carla@x.com")
        )
        self.assertEqual(config.site, "empresa.atlassian.net")
        self.assertEqual(config.email, "eu@empresa.com")
        self.assertEqual(config.token, SECRET)
        self.assertEqual(config.mode, "manager")
        self.assertEqual(config.board, 42)
        self.assertEqual(config.project, "VHPJ")
        self.assertEqual(config.team, ["ana@x.com", "Bruno", "carla@x.com"])

    def test_defaults_are_applied(self):
        config = jira.load_config(base_environment())
        self.assertEqual(config.unassigned_hours, 2)
        self.assertEqual(config.budget_seconds, 40)
        self.assertIsNone(config.board)
        self.assertEqual(config.team, [])
        self.assertTrue(str(config.state_dir).endswith(".local/state/panel-tui/jira"))

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

    def test_invalid_mode_is_rejected(self):
        for mode in (None, "", "gerente", "Manager"):
            with self.assertRaises(jira.ConfigError, msg=str(mode)):
                jira.load_config(base_environment(JIRA_MODE=mode))

    def test_board_or_project_is_required(self):
        with self.assertRaises(jira.ConfigError):
            jira.load_config(base_environment(JIRA_PROJECT=None))

    def test_invalid_project_prefix_is_rejected(self):
        for project in ("vhpj", "V HPJ", "VH-PJ", "1ABC", "A;B"):
            with self.assertRaises(jira.ConfigError, msg=project):
                jira.load_config(base_environment(JIRA_PROJECT=project))

    def test_non_numeric_board_is_rejected(self):
        for board in ("abc", "-1", "1.5", "0", " "):
            with self.assertRaises(jira.ConfigError, msg=board):
                jira.load_config(base_environment(JIRA_PROJECT=None, JIRA_BOARD=board))

    def test_hostile_site_values_are_rejected(self):
        hostile = ["https://x.com", "x.com/path", "x .com", "user@x.com", "", "x.com?a=b", "x\n.com"]
        for site in hostile:
            with self.assertRaises(jira.ConfigError, msg=repr(site)):
                jira.load_config(base_environment(JIRA_SITE=site))

    def test_team_ignores_empty_entries_and_control_characters(self):
        config = jira.load_config(base_environment(JIRA_TEAM=",, ,ok\x00bad,fine\n,Ana"))
        self.assertEqual(config.team, ["fine", "Ana"])

    def test_base_url_defaults_to_https_site_and_can_be_overridden(self):
        self.assertEqual(jira.load_config(base_environment()).base_url, "https://empresa.atlassian.net")
        overridden = jira.load_config(base_environment(JIRA_BASE_URL="http://127.0.0.1:9"))
        self.assertEqual(overridden.base_url, "http://127.0.0.1:9")

    def test_numeric_settings_must_be_positive_integers(self):
        for name in ("JIRA_UNASSIGNED_HOURS", "JIRA_BUDGET_SECONDS"):
            for value in ("0", "-3", "x", "1.5"):
                with self.assertRaises(jira.ConfigError, msg=f"{name}={value}"):
                    jira.load_config(base_environment(**{name: value}))


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


class _Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/slow"):
            time.sleep(3)
        body = json.dumps({"path": self.path}).encode()
        self.send_response(200)
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

    def test_urllib_http_times_out(self):
        with self.assertRaises(OSError):
            jira.urllib_http(f"{self.base}/slow", "GET", {}, 1)

    def test_client_over_real_http_with_base_url_override(self):
        client = make_client(jira.urllib_http, JIRA_BASE_URL=self.base)
        self.assertEqual(client.get_json("/rest/api/3/myself"), {"path": "/rest/api/3/myself"})


def raw_issue(key="VHPJ-1", **fields):
    base = {
        "summary": "Corrigir login",
        "status": {"name": "To Do", "statusCategory": {"key": "new"}},
        "assignee": {"accountId": "acc-1", "displayName": "Ana"},
        "creator": {"accountId": "acc-9", "displayName": "Carla"},
        "priority": {"name": "High"},
        "created": "2026-10-17T08:00:00.000+0000",
        "updated": "2026-10-17T09:30:00.000+0000",
        "resolutiondate": None,
    }
    base.update(fields)
    return {"key": key, "fields": base}


class IssueTests(unittest.TestCase):
    def test_parses_a_complete_issue(self):
        issue = jira.parse_issue(raw_issue())
        self.assertEqual(issue.key, "VHPJ-1")
        self.assertEqual(issue.summary, "Corrigir login")
        self.assertEqual(issue.status, "To Do")
        self.assertEqual(issue.status_category, "new")
        self.assertEqual((issue.assignee_id, issue.assignee_name), ("acc-1", "Ana"))
        self.assertEqual(issue.creator_name, "Carla")
        self.assertEqual(issue.priority, "High")
        self.assertEqual(issue.created, datetime(2026, 10, 17, 8, 0, tzinfo=timezone.utc))
        self.assertEqual(issue.updated, datetime(2026, 10, 17, 9, 30, tzinfo=timezone.utc))
        self.assertIsNone(issue.resolved)

    def test_null_assignee_creator_priority_are_none(self):
        issue = jira.parse_issue(raw_issue(assignee=None, creator=None, priority=None))
        self.assertIsNone(issue.assignee_id)
        self.assertIsNone(issue.assignee_name)
        self.assertIsNone(issue.creator_name)
        self.assertIsNone(issue.priority)

    def test_unknown_status_category_is_unknown(self):
        issue = jira.parse_issue(raw_issue(status={"name": "X", "statusCategory": {"key": "weird"}}))
        self.assertEqual(issue.status_category, "unknown")
        issue = jira.parse_issue(raw_issue(status=None))
        self.assertEqual(issue.status_category, "unknown")

    def test_missing_key_returns_none(self):
        self.assertIsNone(jira.parse_issue({"fields": {}}))
        self.assertIsNone(jira.parse_issue({"key": 5, "fields": {}}))
        self.assertIsNone(jira.parse_issue("lixo"))
        self.assertIsNone(jira.parse_issue({"key": "A-1"}))

    def test_unreadable_dates_return_none(self):
        self.assertIsNone(jira.parse_issue(raw_issue(created="ontem")))
        self.assertIsNone(jira.parse_issue(raw_issue(updated=None)))

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

    def test_raw_round_trip_preserves_the_issue(self):
        issue = jira.parse_issue(raw_issue(resolutiondate="2026-10-17T12:00:00.000+0000"))
        self.assertEqual(jira.Issue.from_raw(issue.to_raw()), issue)
        self.assertEqual(json.loads(json.dumps(issue.to_raw())), issue.to_raw())


NOW = datetime(2026, 10, 17, 12, 0, tzinfo=timezone.utc)


def many_issues(start, count):
    return [raw_issue(key=f"VHPJ-{number}") for number in range(start, start + count)]


class ScriptedHttp:
    def __init__(self, responses):
        self.responses = list(responses)
        self.calls = []

    def __call__(self, url, method, headers, timeout):
        parsed = urllib.parse.urlparse(url)
        self.calls.append((parsed.path, dict(urllib.parse.parse_qsl(parsed.query))))
        status, body = self.responses.pop(0) if self.responses else (200, {})
        return status, json.dumps(body).encode("utf-8")


def fetch(http, assignee_only=False, clock=None, window_days=14, **overrides):
    config = jira.load_config(base_environment(**overrides))
    client = jira.JiraClient(config, http)
    return jira.fetch_scope(
        client, config, window_days, NOW, clock or (lambda: 0.0), assignee_only=assignee_only
    )


class FetchScopeTests(unittest.TestCase):
    def test_project_scope_uses_search_jql_and_paginates_with_next_page_token(self):
        http = ScriptedHttp(
            [
                (200, {"issues": many_issues(1, 2), "nextPageToken": "t2"}),
                (200, {"issues": many_issues(3, 1)}),
            ]
        )
        issues = fetch(http)
        self.assertEqual([issue.key for issue in issues], ["VHPJ-1", "VHPJ-2", "VHPJ-3"])
        path, params = http.calls[0]
        self.assertEqual(path, "/rest/api/3/search/jql")
        self.assertIn("project = VHPJ", params["jql"])
        self.assertIn("statusCategory != Done", params["jql"])
        self.assertIn("resolutiondate >= -14d", params["jql"])
        self.assertEqual(params["maxResults"], "100")
        self.assertNotIn("nextPageToken", params)
        self.assertEqual(http.calls[1][1]["nextPageToken"], "t2")

    def test_board_scope_uses_agile_endpoint_and_start_at(self):
        http = ScriptedHttp(
            [(200, {"issues": many_issues(1, 100)}), (200, {"issues": many_issues(101, 3)})]
        )
        issues = fetch(http, JIRA_PROJECT=None, JIRA_BOARD="7")
        self.assertEqual(len(issues), 103)
        self.assertEqual(http.calls[0][0], "/rest/agile/1.0/board/7/issue")
        self.assertEqual(http.calls[0][1]["startAt"], "0")
        self.assertEqual(http.calls[1][1]["startAt"], "100")
        self.assertEqual(len(http.calls), 2)

    def test_board_and_project_apply_the_project_filter_on_the_board(self):
        http = ScriptedHttp([(200, {"issues": []})])
        fetch(http, JIRA_BOARD="7")
        path, params = http.calls[0]
        self.assertEqual(path, "/rest/agile/1.0/board/7/issue")
        self.assertIn("project = VHPJ", params["jql"])

    def test_personal_adds_assignee_current_user(self):
        http = ScriptedHttp([(200, {"issues": []})])
        fetch(http, assignee_only=True, window_days=7)
        jql = http.calls[0][1]["jql"]
        self.assertIn("assignee = currentUser()", jql)
        self.assertIn("resolutiondate >= -7d", jql)

    def test_stops_at_ten_pages(self):
        responses = [
            (200, {"issues": many_issues(page * 10, 10), "nextPageToken": f"t{page}"})
            for page in range(30)
        ]
        http = ScriptedHttp(responses)
        fetch(http)
        self.assertEqual(len(http.calls), jira.MAXIMUM_PAGES)

    def test_stops_at_five_hundred_issues(self):
        responses = [
            (200, {"issues": many_issues(page * 100, 100), "nextPageToken": f"t{page}"})
            for page in range(8)
        ]
        http = ScriptedHttp(responses)
        issues = fetch(http)
        self.assertEqual(len(issues), jira.MAXIMUM_ISSUES)
        self.assertEqual(len(http.calls), 5)

    def test_repeated_next_page_token_stops_the_loop(self):
        http = ScriptedHttp([(200, {"issues": many_issues(1, 2), "nextPageToken": "same"})] * 20)
        fetch(http)
        self.assertLessEqual(len(http.calls), 2)

    def test_server_that_ignores_start_at_stops_the_loop(self):
        page = {"startAt": 0, "issues": many_issues(1, 100)}
        http = ScriptedHttp([(200, page)] * 20)
        issues = fetch(http, JIRA_PROJECT=None, JIRA_BOARD="7")
        self.assertLessEqual(len(http.calls), 2)
        self.assertEqual(len(issues), 100)

    def test_time_budget_exhaustion_returns_partial_results(self):
        ticks = iter([0.0, 0.0, 100.0, 200.0, 300.0])
        http = ScriptedHttp(
            [(200, {"issues": many_issues(page * 10, 10), "nextPageToken": f"t{page}"}) for page in range(5)]
        )
        issues = fetch(http, clock=lambda: next(ticks))
        self.assertEqual(len(http.calls), 1)
        self.assertEqual(len(issues), 10)

    def test_response_without_issues_is_empty(self):
        self.assertEqual(fetch(ScriptedHttp([(200, {})])), [])
        self.assertEqual(fetch(ScriptedHttp([(200, {"issues": None})])), [])

    def test_invalid_issues_are_skipped(self):
        payload = {"issues": [raw_issue(key="VHPJ-1"), {"key": "VHPJ-2"}, "lixo", None]}
        self.assertEqual([i.key for i in fetch(ScriptedHttp([(200, payload)]))], ["VHPJ-1"])

    def test_requests_only_the_needed_fields(self):
        http = ScriptedHttp([(200, {"issues": []})])
        fetch(http)
        self.assertEqual(
            http.calls[0][1]["fields"],
            "summary,status,assignee,creator,created,updated,resolutiondate,priority",
        )

    def test_auth_error_propagates(self):
        with self.assertRaises(jira.AuthError):
            fetch(ScriptedHttp([(401, {})]))


if __name__ == "__main__":
    unittest.main()
