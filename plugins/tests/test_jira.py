import base64
import io
import json
import os
import stat
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
        self.assertTrue(str(config.state_dir).endswith(".local/state/mosaic/jira"))

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


class StateStoreTests(unittest.TestCase):
    def setUp(self):
        self._directory = tempfile.TemporaryDirectory()
        self.root = Path(self._directory.name) / "estado" / "chave"
        self.store = jira.StateStore(self.root)

    def tearDown(self):
        self._directory.cleanup()

    def mode_of(self, path):
        return stat.S_IMODE(os.stat(path).st_mode)

    def test_round_trips_state_history_and_snapshot(self):
        self.assertTrue(self.store.save_state({"last_seen_manager": "2026-10-17T00:00:00+00:00"}))
        self.assertTrue(self.store.save_history({"VHPJ-1": {"assignee": "Ana"}}))
        issue = jira.parse_issue(raw_issue())
        self.assertTrue(self.store.save_snapshot("manager", [issue], NOW))
        self.assertEqual(self.store.load_state()["last_seen_manager"], "2026-10-17T00:00:00+00:00")
        self.assertEqual(self.store.load_history(), {"VHPJ-1": {"assignee": "Ana"}})
        self.assertEqual(self.store.load_snapshot("manager"), ([issue], NOW))

    def test_files_are_private(self):
        self.store.save_state({"a": 1})
        self.store.save_history({})
        self.store.save_snapshot("personal", [], NOW)
        self.assertEqual(self.mode_of(self.root), 0o700)
        for name in ("state.json", "history.json", "snapshot-personal.json"):
            self.assertEqual(self.mode_of(self.root / name), 0o600, name)

    def test_missing_empty_and_corrupt_files_return_defaults(self):
        self.assertEqual(self.store.load_state(), {})
        self.assertEqual(self.store.load_history(), {})
        self.assertIsNone(self.store.load_snapshot("manager"))
        self.root.mkdir(parents=True)
        for content in ("", "{nao json", "[1,2]", "null", '"texto"'):
            for name in ("state.json", "history.json", "snapshot-manager.json"):
                (self.root / name).write_text(content, encoding="utf-8")
            self.assertEqual(self.store.load_state(), {}, content)
            self.assertEqual(self.store.load_history(), {}, content)
            self.assertIsNone(self.store.load_snapshot("manager"), content)

    def test_snapshot_with_bad_issue_data_returns_none(self):
        self.root.mkdir(parents=True)
        (self.root / "snapshot-manager.json").write_text(
            json.dumps({"saved_at": NOW.isoformat(), "issues": [{"key": "X"}]}), encoding="utf-8"
        )
        self.assertIsNone(self.store.load_snapshot("manager"))

    @unittest.skipIf(os.geteuid() == 0, "root ignora permissões")
    def test_unreadable_file_returns_default(self):
        self.store.save_state({"a": 1})
        os.chmod(self.root / "state.json", 0o000)
        self.assertEqual(self.store.load_state(), {})

    def test_save_into_unwritable_directory_returns_false(self):
        blocker = Path(self._directory.name) / "arquivo"
        blocker.write_text("x", encoding="utf-8")
        store = jira.StateStore(blocker / "dentro")
        self.assertFalse(store.save_state({"a": 1}))
        self.assertFalse(store.save_history({}))
        self.assertFalse(store.save_snapshot("manager", [], NOW))

    def test_write_is_atomic_no_temp_files_left(self):
        for number in range(5):
            self.store.save_history({"n": number})
        self.assertEqual(sorted(path.name for path in self.root.iterdir()), ["history.json"])

    def test_last_seen_defaults_to_24_hours_ago(self):
        self.assertEqual(jira.last_seen({}, "manager", NOW), NOW - timedelta(hours=24))
        self.assertEqual(
            jira.last_seen({"last_seen_manager": "lixo"}, "manager", NOW), NOW - timedelta(hours=24)
        )

    def test_last_seen_is_per_mode(self):
        state = {"last_seen_manager": "2026-10-17T10:00:00+00:00"}
        self.assertEqual(jira.last_seen(state, "manager", NOW), datetime(2026, 10, 17, 10, tzinfo=timezone.utc))
        self.assertEqual(jira.last_seen(state, "personal", NOW), NOW - timedelta(hours=24))

    def test_state_key_is_filesystem_safe(self):
        config = jira.load_config(base_environment(JIRA_BOARD="7"))
        key = jira.state_key(config)
        self.assertEqual(key, "empresa.atlassian.net-VHPJ-board7")
        board_only = jira.load_config(base_environment(JIRA_PROJECT=None, JIRA_BOARD="7"))
        self.assertEqual(jira.state_key(board_only), "empresa.atlassian.net-board7")
        self.assertNotIn("/", key)

    def test_state_never_contains_the_token(self):
        self.store.save_state({"accounts": {"a@x.com": "acc-1"}})
        self.store.save_history({"VHPJ-1": {"assignee": "Ana"}})
        self.store.save_snapshot("manager", [jira.parse_issue(raw_issue())], NOW)
        for path in self.root.iterdir():
            self.assertNotIn(SECRET, path.read_text(encoding="utf-8"))


class RoutedHttp:
    def __init__(self, routes):
        self.routes = routes
        self.calls = []
        self.urls = []

    def __call__(self, url, method, headers, timeout):
        path = urllib.parse.urlparse(url).path
        self.calls.append(path)
        self.urls.append(url)
        outcome = self.routes.get(path)
        if outcome is None:
            return 404, b"{}"
        if isinstance(outcome, Exception):
            raise outcome
        status, body = outcome
        return status, json.dumps(body).encode("utf-8")


CATEGORIES = {"To Do": "new", "In Progress": "indeterminate", "Done": "done"}


def history_entry(created, to_status):
    return {"created": created, "items": [{"field": "status", "toString": to_status}]}


def done_issue(key="VHPJ-5", updated="2026-10-17T12:00:00.000+0000"):
    return jira.parse_issue(
        raw_issue(
            key=key,
            status={"name": "Done", "statusCategory": {"key": "done"}},
            updated=updated,
            resolutiondate="2026-10-17T12:00:00.000+0000",
        )
    )


def client_for(http):
    return jira.JiraClient(jira.load_config(base_environment()), http)


class HistoryTests(unittest.TestCase):
    changelog = {
        "changelog": {
            "histories": [
                history_entry("2026-10-17T11:00:00.000+0000", "Done"),
                history_entry("2026-10-17T09:00:00.000+0000", "To Do"),
                history_entry("2026-10-17T10:00:00.000+0000", "In Progress"),
            ]
        }
    }

    def test_status_categories_map_names_to_categories(self):
        http = RoutedHttp(
            {"/rest/api/3/status": (200, [{"name": "To Do", "statusCategory": {"key": "new"}}, {"name": "X"}, "lixo"])}
        )
        self.assertEqual(jira.fetch_status_categories(client_for(http)), {"To Do": "new"})
        self.assertEqual(jira.fetch_status_categories(client_for(RoutedHttp({"/rest/api/3/status": (200, {})}))), {})

    def test_work_start_rejects_keys_that_could_alter_the_url(self):
        http = RoutedHttp({})
        for key in ("VHPJ-1\n", "../x", "VHPJ-1/changelog", "vhpj-1", ""):
            self.assertIsNone(jira.find_work_start(client_for(http), key, CATEGORIES), repr(key))
        self.assertEqual(http.calls, [])

    def test_new_issue_creates_a_record_without_calling_the_changelog(self):
        http = RoutedHttp({})
        issue = jira.parse_issue(raw_issue())
        history = jira.update_history({}, [issue], client_for(http), CATEGORIES, NOW)
        record = history["VHPJ-1"]
        self.assertEqual(record["assignee"], "Ana")
        self.assertEqual(record["summary"], "Corrigir login")
        self.assertEqual(record["status"], "To Do")
        self.assertIsNone(record["resolved"])
        self.assertEqual(http.calls, [])

    def test_existing_records_are_merged_and_old_ones_kept(self):
        old = {"key": "OLD-1", "assignee": "Zé"}
        history = jira.update_history({"OLD-1": old}, [jira.parse_issue(raw_issue())], client_for(RoutedHttp({})), CATEGORIES, NOW)
        self.assertEqual(history["OLD-1"], old)
        self.assertIn("VHPJ-1", history)

    def test_done_issue_gets_started_from_first_in_progress_transition(self):
        http = RoutedHttp({"/rest/api/3/issue/VHPJ-5": (200, self.changelog)})
        history = jira.update_history({}, [done_issue()], client_for(http), CATEGORIES, NOW)
        record = history["VHPJ-5"]
        self.assertEqual(record["started"], "2026-10-17T10:00:00+00:00")
        self.assertEqual(record["cycle_hours"], 2.0)

    def test_done_issue_without_transition_uses_created(self):
        http = RoutedHttp({"/rest/api/3/issue/VHPJ-5": (200, {"changelog": {"histories": []}})})
        record = jira.update_history({}, [done_issue()], client_for(http), CATEGORIES, NOW)["VHPJ-5"]
        self.assertEqual(record["started"], "2026-10-17T08:00:00+00:00")
        self.assertEqual(record["cycle_hours"], 4.0)

    def test_changelog_is_fetched_only_for_done_issues_that_changed(self):
        http = RoutedHttp({"/rest/api/3/issue/VHPJ-5": (200, self.changelog)})
        client = client_for(http)
        history = jira.update_history({}, [done_issue()], client, CATEGORIES, NOW)
        self.assertEqual(len(http.calls), 1)
        history = jira.update_history(history, [done_issue()], client, CATEGORIES, NOW)
        self.assertEqual(len(http.calls), 1)
        changed = done_issue(updated="2026-10-17T12:30:00.000+0000")
        jira.update_history(history, [changed], client, CATEGORIES, NOW)
        self.assertEqual(len(http.calls), 2)

    def test_cycle_hours_are_never_negative(self):
        early = {"changelog": {"histories": [history_entry("2026-10-17T20:00:00.000+0000", "In Progress")]}}
        http = RoutedHttp({"/rest/api/3/issue/VHPJ-5": (200, early)})
        record = jira.update_history({}, [done_issue()], client_for(http), CATEGORIES, NOW)["VHPJ-5"]
        self.assertEqual(record["cycle_hours"], 0.0)

    def test_changelog_failure_does_not_break_the_update(self):
        http = RoutedHttp({"/rest/api/3/issue/VHPJ-5": jira.NetworkError("fora")})
        history = jira.update_history({}, [done_issue()], client_for(http), CATEGORIES, NOW)
        self.assertIsNone(history["VHPJ-5"]["started"])
        self.assertIsNone(history["VHPJ-5"]["cycle_hours"])
        http.routes["/rest/api/3/issue/VHPJ-5"] = (200, self.changelog)
        history = jira.update_history(history, [done_issue()], client_for(http), CATEGORIES, NOW)
        self.assertEqual(history["VHPJ-5"]["cycle_hours"], 2.0)

    def test_history_never_stores_the_token(self):
        http = RoutedHttp({"/rest/api/3/issue/VHPJ-5": (200, self.changelog)})
        history = jira.update_history({}, [done_issue()], client_for(http), CATEGORIES, NOW)
        self.assertNotIn(SECRET, json.dumps(history))


def assigned_issue(key, account_id, name):
    return jira.parse_issue(raw_issue(key=key, assignee={"accountId": account_id, "displayName": name}))


class TeamTests(unittest.TestCase):
    issues = None

    def setUp(self):
        self.issues = [
            assigned_issue("VHPJ-1", "acc-ana", "Ana Souza"),
            assigned_issue("VHPJ-2", "acc-bruno", "Bruno Lima"),
        ]

    def resolve(self, entries, http=None, cache=None):
        client = client_for(http or RoutedHttp({}))
        return jira.resolve_team(entries, self.issues, client, cache if cache is not None else {})

    def test_email_resolves_to_account_id_and_is_cached(self):
        http = RoutedHttp(
            {"/rest/api/3/user/search": (200, [{"accountId": "acc-carla", "displayName": "Carla"}])}
        )
        team, cache = self.resolve(["carla@x.com"], http)
        self.assertEqual(team.members, {"acc-carla": "Carla"})
        self.assertEqual(team.warnings, [])
        self.assertEqual(cache["carla@x.com"], {"id": "acc-carla", "name": "Carla"})

    def test_cached_email_does_not_call_the_api_again(self):
        http = RoutedHttp({})
        cache = {"carla@x.com": {"id": "acc-carla", "name": "Carla"}}
        team, _ = self.resolve(["carla@x.com"], http, cache)
        self.assertEqual(team.members, {"acc-carla": "Carla"})
        self.assertEqual(http.calls, [])

    def test_search_query_uses_the_email(self):
        urls = []

        def http(url, method, headers, timeout):
            urls.append(url)
            return 200, b"[]"

        self.resolve(["carla@x.com"], http)
        self.assertIn("query=carla%40x.com", urls[0])

    def test_account_id_entry_matches_directly(self):
        team, _ = self.resolve(["acc-bruno"])
        self.assertEqual(team.members, {"acc-bruno": "Bruno Lima"})

    def test_display_name_matches_case_insensitively(self):
        team, _ = self.resolve(["ana souza"])
        self.assertEqual(team.members, {"acc-ana": "Ana Souza"})

    def test_hidden_email_falls_back_to_display_name_in_scope(self):
        http = RoutedHttp({"/rest/api/3/user/search": (200, [])})
        team, _ = self.resolve(["Bruno Lima"], http)
        self.assertEqual(team.members, {"acc-bruno": "Bruno Lima"})

    def test_unmatched_entry_becomes_a_warning(self):
        http = RoutedHttp({"/rest/api/3/user/search": (200, [])})
        team, _ = self.resolve(["ninguem@x.com", "Fantasma"], http)
        self.assertEqual(team.members, {})
        self.assertEqual(
            team.warnings,
            ['não encontrei "ninguem@x.com" no Jira', 'não encontrei "Fantasma" no Jira'],
        )

    def test_empty_list_uses_assignees_in_scope(self):
        team, _ = self.resolve([])
        self.assertEqual(team.members, {"acc-ana": "Ana Souza", "acc-bruno": "Bruno Lima"})

    def test_search_failure_becomes_a_warning(self):
        http = RoutedHttp({"/rest/api/3/user/search": jira.NetworkError("fora")})
        team, cache = self.resolve(["carla@x.com"], http)
        self.assertEqual(team.warnings, ['não encontrei "carla@x.com" no Jira'])
        self.assertNotIn("carla@x.com", cache)


def issue_of(key, category="new", assignee=("acc-1", "Ana"), created="2026-10-17T08:00:00.000+0000",
             updated="2026-10-17T09:30:00.000+0000", summary="Corrigir login", resolved=None,
             priority="High"):
    status_names = {"new": "To Do", "indeterminate": "In Progress", "done": "Done"}
    return jira.parse_issue(
        raw_issue(
            key=key,
            summary=summary,
            status={"name": status_names[category], "statusCategory": {"key": category}},
            assignee=None if assignee is None else {"accountId": assignee[0], "displayName": assignee[1]},
            created=created,
            updated=updated,
            resolutiondate=resolved,
            priority=None if priority is None else {"name": priority},
        )
    )


LAST_SEEN = datetime(2026, 10, 17, 9, 0, tzinfo=timezone.utc)


def manager_config(**overrides):
    return jira.load_config(base_environment(**overrides))


def everyone(issues):
    return jira.TeamResolution({i.assignee_id: i.assignee_name for i in issues if i.assignee_id}, [])


def sections_by_name(snapshot):
    return {section["heading"]: section["items"] for section in snapshot["detail"]["sections"]}


def summary_texts(snapshot):
    return [line["text"] for line in snapshot["summary"]["lines"]]


class ManagerSnapshotTests(unittest.TestCase):
    def build(self, issues, team=None, last_seen=LAST_SEEN, offline_at=None, **overrides):
        return jira.build_manager_snapshot(
            issues, manager_config(**overrides), team or everyone(issues), last_seen, NOW, offline_at
        )

    def test_summary_counts_and_updated_flag(self):
        issues = [
            issue_of("VHPJ-1", assignee=None, updated="2026-10-17T08:00:00.000+0000"),
            issue_of("VHPJ-2", "indeterminate", updated="2026-10-17T10:00:00.000+0000"),
            issue_of("VHPJ-3", "indeterminate", assignee=("acc-2", "Bruno"), updated="2026-10-17T08:30:00.000+0000"),
        ]
        snapshot = self.build(issues)
        self.assertEqual(summary_texts(snapshot), ["sem dono: 1", "em andamento: 2", "atualizados: 1"])
        flagged = [line for line in snapshot["summary"]["lines"] if line.get("flag")]
        self.assertEqual([line["text"] for line in flagged], ["atualizados: 1"])
        self.assertEqual(flagged[0]["flag"], "updated")
        self.assertIn("Jira", snapshot["summary"]["title"])

    def test_no_updates_means_no_flag_on_the_summary(self):
        snapshot = self.build([issue_of("VHPJ-1", "indeterminate", updated="2026-10-17T08:00:00.000+0000")])
        self.assertTrue(all("flag" not in line for line in snapshot["summary"]["lines"]))

    def test_unassigned_requires_new_category_no_assignee_and_older_than_threshold(self):
        issues = [
            issue_of("VHPJ-1", assignee=None),
            issue_of("VHPJ-2", "indeterminate", assignee=None),
            issue_of("VHPJ-3", assignee=("acc-1", "Ana")),
            issue_of("VHPJ-4", "done", assignee=None, resolved="2026-10-17T10:00:00.000+0000"),
        ]
        items = sections_by_name(self.build(issues))["Sem dono"]
        self.assertEqual([item["id"] for item in items], ["VHPJ-1"])

    def test_unassigned_below_threshold_is_excluded(self):
        recent = issue_of("VHPJ-1", assignee=None, created="2026-10-17T11:30:00.000+0000")
        borderline = issue_of("VHPJ-2", assignee=None, created="2026-10-17T10:00:00.000+0000")
        items = sections_by_name(self.build([recent, borderline]))["Sem dono"]
        self.assertEqual([item["id"] for item in items], ["VHPJ-2"])

    def test_unassigned_threshold_is_configurable(self):
        recent = issue_of("VHPJ-1", assignee=None, created="2026-10-17T11:30:00.000+0000")
        items = sections_by_name(self.build([recent], JIRA_UNASSIGNED_HOURS="1"))["Sem dono"]
        self.assertEqual(items, [])

    def test_in_progress_is_sorted_by_person_then_key_with_name_prefix(self):
        issues = [
            issue_of("VHPJ-9", "indeterminate", assignee=("acc-2", "Bruno")),
            issue_of("VHPJ-12", "indeterminate", assignee=("acc-1", "Ana"), summary="Título A"),
            issue_of("VHPJ-3", "indeterminate", assignee=("acc-1", "Ana")),
        ]
        items = sections_by_name(self.build(issues))["Em andamento"]
        self.assertEqual([item["id"] for item in items], ["VHPJ-12", "VHPJ-3", "VHPJ-9"])
        self.assertEqual(items[0]["text"], "Ana — VHPJ-12 Título A")

    def test_people_outside_the_team_go_to_others(self):
        issues = [
            issue_of("VHPJ-1", "indeterminate", assignee=("acc-1", "Ana")),
            issue_of("VHPJ-2", "indeterminate", assignee=("acc-9", "Estranho")),
        ]
        team = jira.TeamResolution({"acc-1": "Ana"}, [])
        items = sections_by_name(self.build(issues, team))["Em andamento"]
        self.assertEqual([item["text"].split(" — ")[0] for item in items], ["Ana", "Outros"])

    def test_updated_section_repeats_tickets_changed_after_last_seen(self):
        issues = [
            issue_of("VHPJ-1", "indeterminate", updated="2026-10-17T10:00:00.000+0000"),
            issue_of("VHPJ-2", "indeterminate", updated="2026-10-17T11:00:00.000+0000"),
            issue_of("VHPJ-3", "indeterminate", updated="2026-10-17T08:00:00.000+0000"),
        ]
        sections = sections_by_name(self.build(issues))
        self.assertEqual([item["id"] for item in sections["Atualizados"]], ["VHPJ-2", "VHPJ-1"])
        in_progress = {item["id"]: item for item in sections["Em andamento"]}
        self.assertEqual(in_progress["VHPJ-1"]["flag"], "updated")
        self.assertNotIn("flag", in_progress["VHPJ-3"])

    def test_items_have_meta_with_all_keys(self):
        item = sections_by_name(self.build([issue_of("VHPJ-1", "indeterminate")]))["Em andamento"][0]
        self.assertEqual(
            item["meta"],
            {
                "status": "In Progress",
                "prioridade": "High",
                "responsavel": "Ana",
                "criado_ha": "4h",
                "parado_ha": "2h",
                "site": "empresa.atlassian.net",
            },
        )

    def test_unassigned_meta_says_ninguem_and_missing_priority_has_a_placeholder(self):
        item = sections_by_name(self.build([issue_of("VHPJ-1", assignee=None, priority=None)]))["Sem dono"][0]
        self.assertEqual(item["meta"]["responsavel"], "ninguem")
        self.assertEqual(item["meta"]["prioridade"], "-")

    def test_long_multiline_summary_is_one_line_and_truncated(self):
        issue = issue_of("VHPJ-1", "indeterminate", summary="linha1\nlinha2 " + "x" * 500)
        text = sections_by_name(self.build([issue]))["Em andamento"][0]["text"]
        self.assertNotIn("\n", text)
        self.assertLessEqual(len(text), len("Ana — VHPJ-1 ") + 80)
        self.assertTrue(text.endswith("…"))

    def test_offline_adds_first_summary_line(self):
        saved_at = datetime(2026, 10, 17, 11, 5, tzinfo=timezone.utc)
        snapshot = self.build([], offline_at=saved_at)
        local_time = saved_at.astimezone().strftime("%H:%M")
        self.assertEqual(summary_texts(snapshot)[0], f"dados de {local_time} (offline)")

    def test_team_warnings_appear_in_summary(self):
        team = jira.TeamResolution({}, ['não encontrei "x" no Jira'])
        self.assertIn('não encontrei "x" no Jira', summary_texts(self.build([], team)))

    def test_actions_are_declared_with_scopes(self):
        actions = {action["id"]: action for action in self.build([])["actions"]}
        self.assertEqual(actions["mark_seen"]["applies_to"], "widget")
        self.assertEqual(actions["open"]["applies_to"], "item")
        self.assertIn("empresa.atlassian.net", actions["mark_seen"]["description"])

    def test_empty_scope_still_returns_valid_sections(self):
        snapshot = self.build([])
        self.assertEqual(
            [section["heading"] for section in snapshot["detail"]["sections"]],
            ["Sem dono", "Em andamento", "Atualizados"],
        )
        self.assertEqual(summary_texts(snapshot), ["sem dono: 0", "em andamento: 0", "atualizados: 0"])


def done_of(key, resolved, cycle=None):
    return issue_of(key, "done", created="2026-10-10T08:00:00.000+0000", updated=resolved, resolved=resolved)


class PersonalSnapshotTests(unittest.TestCase):
    def build(self, issues, history=None, last_seen=LAST_SEEN, offline_at=None):
        config = manager_config(JIRA_MODE="personal")
        return jira.build_personal_snapshot(issues, history or {}, config, last_seen, NOW, offline_at)

    def test_summary_has_progress_done_and_median(self):
        issues = [
            issue_of("VHPJ-1", "indeterminate"),
            issue_of("VHPJ-2", "indeterminate"),
            issue_of("VHPJ-3", "new"),
            done_of("VHPJ-4", "2026-10-16T10:00:00.000+0000"),
            done_of("VHPJ-5", "2026-10-15T10:00:00.000+0000"),
        ]
        history = {"VHPJ-4": {"cycle_hours": 2.0}, "VHPJ-5": {"cycle_hours": 5.0}}
        texts = summary_texts(self.build(issues, history))
        self.assertEqual(texts[:3], ["em andamento: 2", "concluídos em 7 dias: 2", "tempo mediano: 3.5h"])

    def test_median_uses_history_cycle_hours_of_own_done_tickets_within_window(self):
        issues = [done_of("VHPJ-4", "2026-10-16T10:00:00.000+0000"), done_of("VHPJ-6", "2026-09-01T10:00:00.000+0000")]
        history = {
            "VHPJ-4": {"cycle_hours": 2.0},
            "VHPJ-6": {"cycle_hours": 100.0},
            "OUTRO-1": {"cycle_hours": 50.0},
        }
        self.assertIn("tempo mediano: 2h", summary_texts(self.build(issues, history)))

    def test_median_is_a_dash_without_data(self):
        issues = [done_of("VHPJ-4", "2026-10-16T10:00:00.000+0000")]
        self.assertIn("tempo mediano: —", summary_texts(self.build(issues, {})))
        self.assertIn("tempo mediano: —", summary_texts(self.build([], {})))

    def test_sections_split_open_and_done(self):
        issues = [issue_of("VHPJ-1", "new"), done_of("VHPJ-4", "2026-10-16T10:00:00.000+0000")]
        sections = sections_by_name(self.build(issues))
        self.assertEqual([item["id"] for item in sections["Meus tickets"]], ["VHPJ-1"])
        self.assertEqual([item["id"] for item in sections["Concluídos (7 dias)"]], ["VHPJ-4"])

    def test_in_progress_comes_first(self):
        issues = [issue_of("VHPJ-1", "new"), issue_of("VHPJ-2", "indeterminate"), issue_of("VHPJ-3", "new")]
        items = sections_by_name(self.build(issues))["Meus tickets"]
        self.assertEqual([item["id"] for item in items], ["VHPJ-2", "VHPJ-1", "VHPJ-3"])
        self.assertEqual(items[0]["text"], "VHPJ-2 Corrigir login")

    def test_done_outside_the_window_is_excluded(self):
        issues = [done_of("VHPJ-4", "2026-10-09T10:00:00.000+0000")]
        self.assertEqual(sections_by_name(self.build(issues))["Concluídos (7 dias)"], [])

    def test_updated_flag_uses_last_seen(self):
        issues = [
            issue_of("VHPJ-1", "indeterminate", updated="2026-10-17T10:00:00.000+0000"),
            issue_of("VHPJ-2", "indeterminate", updated="2026-10-17T08:00:00.000+0000"),
        ]
        snapshot = self.build(issues)
        items = {item["id"]: item for item in sections_by_name(snapshot)["Meus tickets"]}
        self.assertEqual(items["VHPJ-1"]["flag"], "updated")
        self.assertNotIn("flag", items["VHPJ-2"])
        flagged = [line for line in snapshot["summary"]["lines"] if line.get("flag")]
        self.assertEqual([line["text"] for line in flagged], ["atualizados: 1"])

    def test_offline_adds_first_summary_line_and_actions_exist(self):
        saved_at = datetime(2026, 10, 17, 11, 5, tzinfo=timezone.utc)
        snapshot = self.build([], offline_at=saved_at)
        self.assertEqual(summary_texts(snapshot)[0], f"dados de {saved_at.astimezone().strftime('%H:%M')} (offline)")
        self.assertEqual([action["id"] for action in snapshot["actions"]], ["mark_seen", "open"])
        self.assertTrue(snapshot["summary"]["title"].endswith("meus"))


STATUS_ROUTE = {
    "/rest/api/3/status": (
        200,
        [
            {"name": "To Do", "statusCategory": {"key": "new"}},
            {"name": "In Progress", "statusCategory": {"key": "indeterminate"}},
            {"name": "Done", "statusCategory": {"key": "done"}},
        ],
    )
}


def raw_issues_for_main():
    return [
        raw_issue(key="VHPJ-1", assignee=None, created="2026-10-17T07:00:00.000+0000"),
        raw_issue(
            key="VHPJ-2",
            status={"name": "In Progress", "statusCategory": {"key": "indeterminate"}},
            updated="2026-10-17T11:00:00.000+0000",
        ),
    ]


class MainTests(unittest.TestCase):
    def setUp(self):
        self._directory = tempfile.TemporaryDirectory()
        self.state_dir = Path(self._directory.name) / "estado"

    def tearDown(self):
        self._directory.cleanup()

    def run_main(self, arguments, routes=None, stdin_text="", opener=None, clock=None, **overrides):
        environment = base_environment(JIRA_STATE_DIR=str(self.state_dir), **overrides)
        http = RoutedHttp({**STATUS_ROUTE, **(routes or {})})
        stdout, stderr = io.StringIO(), io.StringIO()
        code = jira.main(
            arguments,
            environ=environment,
            http=http,
            now=NOW,
            clock=clock or (lambda: 0.0),
            opener=opener or (lambda url: None),
            stdin=io.StringIO(stdin_text),
            stdout=stdout,
            stderr=stderr,
        )
        self.http = http
        return code, stdout.getvalue(), stderr.getvalue()

    def search_ok(self, issues=None):
        return {"/rest/api/3/search/jql": (200, {"issues": issues if issues is not None else raw_issues_for_main()})}

    def state_files(self):
        return [path for path in self.state_dir.rglob("*") if path.is_file()]

    def test_snapshot_manager_end_to_end_with_fake_jira(self):
        code, out, err = self.run_main(["snapshot"], self.search_ok())
        self.assertEqual(code, 0, err)
        snapshot = json.loads(out)
        self.assertIn("sem dono: 1", summary_texts(snapshot))
        self.assertIn("em andamento: 1", summary_texts(snapshot))
        sections = sections_by_name(snapshot)
        self.assertEqual([item["id"] for item in sections["Sem dono"]], ["VHPJ-1"])
        self.assertEqual(sections["Em andamento"][0]["id"], "VHPJ-2")

    def test_snapshot_personal_end_to_end(self):
        done = raw_issue(
            key="VHPJ-3",
            status={"name": "Done", "statusCategory": {"key": "done"}},
            created="2026-10-16T08:00:00.000+0000",
            updated="2026-10-16T14:00:00.000+0000",
            resolutiondate="2026-10-16T14:00:00.000+0000",
        )
        routes = self.search_ok(raw_issues_for_main() + [done])
        routes["/rest/api/3/issue/VHPJ-3"] = (
            200,
            {"changelog": {"histories": [history_entry("2026-10-16T10:00:00.000+0000", "In Progress")]}},
        )
        code, out, err = self.run_main(["snapshot"], routes, JIRA_MODE="personal")
        self.assertEqual(code, 0, err)
        texts = summary_texts(json.loads(out))
        self.assertIn("concluídos em 7 dias: 1", texts)
        self.assertIn("tempo mediano: 4h", texts)
        self.assertIn("assignee+%3D+currentUser%28%29", self.http.urls[0])

    def test_snapshot_updates_history_and_offline_cache(self):
        self.run_main(["snapshot"], self.search_ok())
        names = sorted(path.name for path in self.state_files())
        self.assertEqual(names, ["history.json", "snapshot-manager.json"])
        history = json.loads(next(p for p in self.state_files() if p.name == "history.json").read_text())
        self.assertEqual(sorted(history), ["VHPJ-1", "VHPJ-2"])

    def test_auth_error_without_cache_exits_one_with_message(self):
        code, out, err = self.run_main(["snapshot"], {"/rest/api/3/search/jql": (401, {})})
        self.assertEqual(code, 1)
        self.assertEqual(out, "")
        self.assertIn("credenciais inválidas", err)

    def test_network_error_with_cache_prints_offline_snapshot(self):
        self.run_main(["snapshot"], self.search_ok())
        code, out, err = self.run_main(["snapshot"], {"/rest/api/3/search/jql": jira.NetworkError("fora")})
        self.assertEqual(code, 0, err)
        snapshot = json.loads(out)
        self.assertIn("(offline)", summary_texts(snapshot)[0])
        self.assertEqual(sections_by_name(snapshot)["Sem dono"][0]["id"], "VHPJ-1")

    def test_budget_exhaustion_prints_partial_data(self):
        page = {"issues": raw_issues_for_main(), "nextPageToken": "t2"}
        ticks = iter([0.0, 0.0, 0.0] + [1000.0 * n for n in range(1, 40)])
        code, out, err = self.run_main(
            ["snapshot"], {"/rest/api/3/search/jql": (200, page)}, clock=lambda: next(ticks)
        )
        self.assertEqual(code, 0, err)
        self.assertIn("em andamento: 1", summary_texts(json.loads(out)))
        search_calls = [path for path in self.http.calls if path.endswith("/search/jql")]
        self.assertEqual(len(search_calls), 1)

    def test_mark_seen_advances_only_the_current_mode(self):
        code, out, err = self.run_main(["action", "mark_seen"], JIRA_MODE="personal")
        self.assertEqual(code, 0, err)
        state = json.loads(next(p for p in self.state_files() if p.name == "state.json").read_text())
        self.assertEqual(state["last_seen_personal"], NOW.isoformat())
        self.assertNotIn("last_seen_manager", state)

    def test_mark_seen_makes_the_flag_disappear_on_the_next_snapshot(self):
        routes = self.search_ok()
        _, before, _ = self.run_main(["snapshot"], routes)
        self.assertIn("atualizados: 2", summary_texts(json.loads(before)))
        self.run_main(["action", "mark_seen"])
        _, after, _ = self.run_main(["snapshot"], routes)
        self.assertIn("atualizados: 0", summary_texts(json.loads(after)))

    def test_open_validates_key_and_calls_opener_with_browse_url(self):
        opened = []
        code, out, err = self.run_main(
            ["action", "open"], stdin_text='{"item_id": "VHPJ-12"}', opener=opened.append
        )
        self.assertEqual(code, 0, err)
        self.assertEqual(opened, ["https://empresa.atlassian.net/browse/VHPJ-12"])

    def test_open_rejects_invalid_keys(self):
        for item_id in ["../x", "A-1; rm -rf", "vhpj-1", "", "VHPJ-", "VHPJ-1 ", "VHPJ-1\n", "-1"]:
            opened = []
            code, _, err = self.run_main(
                ["action", "open"], stdin_text=json.dumps({"item_id": item_id}), opener=opened.append
            )
            self.assertEqual(code, 1, repr(item_id))
            self.assertEqual(opened, [], repr(item_id))
            self.assertTrue(err.strip())
        for stdin_text in ["", "{", "[1]", '{"outro": 1}']:
            opened = []
            code, _, _ = self.run_main(["action", "open"], stdin_text=stdin_text, opener=opened.append)
            self.assertEqual(code, 1, repr(stdin_text))
            self.assertEqual(opened, [])

    def test_open_failure_returns_error(self):
        def failing(url):
            raise OSError("sem navegador")

        code, _, err = self.run_main(["action", "open"], stdin_text='{"item_id": "VHPJ-1"}', opener=failing)
        self.assertEqual(code, 1)
        self.assertIn("não foi possível abrir", err)

    def test_config_error_exits_one_and_never_prints_the_token(self):
        for overrides in ({"JIRA_SITE": "https://x.com"}, {"JIRA_MODE": "gerente"}, {"JIRA_PROJECT": None}):
            code, out, err = self.run_main(["snapshot"], **overrides)
            self.assertEqual(code, 1)
            self.assertNotIn(SECRET, out + err)

    def test_output_and_stderr_never_contain_the_token(self):
        scenarios = [
            (["snapshot"], self.search_ok()),
            (["snapshot"], {"/rest/api/3/search/jql": (401, {"echo": SECRET})}),
            (["snapshot"], {"/rest/api/3/search/jql": jira.NetworkError(SECRET)}),
            (["action", "mark_seen"], None),
        ]
        for arguments, routes in scenarios:
            _, out, err = self.run_main(arguments, routes)
            self.assertNotIn(SECRET, out + err)
        for path in self.state_files():
            self.assertNotIn(SECRET, path.read_text(encoding="utf-8"))

    def test_unknown_command_exits_two(self):
        for arguments in ([], ["foo"], ["action"], ["action", "delete"], ["snapshot", "x"]):
            code, _, err = self.run_main(arguments)
            self.assertEqual(code, 2, arguments)
            self.assertTrue(err.strip())

    def test_output_is_valid_protocol_json(self):
        for mode in ("manager", "personal"):
            _, out, _ = self.run_main(["snapshot"], self.search_ok(), JIRA_MODE=mode)
            snapshot = json.loads(out)
            self.assertIsInstance(snapshot["summary"]["title"], str)
            for line in snapshot["summary"]["lines"]:
                self.assertIsInstance(line["text"], str)
                self.assertIn(line.get("flag"), (None, "updated", "warning"))
            for section in snapshot["detail"]["sections"]:
                self.assertIsInstance(section["heading"], str)
                for item in section["items"]:
                    self.assertIsInstance(item["text"], str)
                    self.assertTrue(all(isinstance(v, str) for v in item["meta"].values()))
            for action in snapshot["actions"]:
                self.assertIn(action["applies_to"], ("widget", "item"))
                self.assertTrue(action["id"] and action["label"])


class ReviewFixTests(unittest.TestCase):
    def setUp(self):
        self._directory = tempfile.TemporaryDirectory()
        self.state_dir = Path(self._directory.name) / "estado"

    def tearDown(self):
        self._directory.cleanup()

    def run_main(self, arguments, routes=None, stdin_text="", clock=None, opener=None, http=None, **overrides):
        environment = base_environment(JIRA_STATE_DIR=str(self.state_dir), **overrides)
        stdout, stderr = io.StringIO(), io.StringIO()
        code = jira.main(
            arguments,
            environ=environment,
            http=http or RoutedHttp({**STATUS_ROUTE, **(routes or {})}),
            now=NOW,
            clock=clock or (lambda: 0.0),
            opener=opener or (lambda url: None),
            stdin=io.StringIO(stdin_text),
            stdout=stdout,
            stderr=stderr,
        )
        return code, stdout.getvalue(), stderr.getvalue()

    def store(self):
        return jira.StateStore(self.state_dir / "empresa.atlassian.net-VHPJ")

    def search(self, issues):
        return {"/rest/api/3/search/jql": (200, {"issues": issues})}

    # 1. trailing newline accepted by "$"
    def test_site_and_project_with_trailing_newline_are_rejected(self):
        with self.assertRaises(jira.ConfigError):
            jira.load_config(base_environment(JIRA_SITE="x.net\n"))
        with self.assertRaises(jira.ConfigError):
            jira.load_config(base_environment(JIRA_PROJECT="VHPJ\n"))
        opened = []
        code, _, _ = self.run_main(
            ["action", "open"], stdin_text='{"item_id": "AB-1"}', opener=opened.append, JIRA_SITE="x.net\n"
        )
        self.assertEqual((code, opened), (1, []))

    # 2. corrupted history / state
    def test_corrupt_history_and_state_do_not_crash_the_snapshot(self):
        store = self.store()
        store.save_history({"VHPJ-1": "texto", "VHPJ-2": {"started": "lixo", "updated": 5}, "VHPJ-3": {"started": "2026-10-17T08:00:00"}})
        store.save_state({"accounts": "nao-eh-dict", "last_seen_manager": 12})
        done = raw_issue(
            key="VHPJ-3",
            status={"name": "Done", "statusCategory": {"key": "done"}},
            resolutiondate="2026-10-17T11:00:00.000+0000",
        )
        issues = [raw_issue(key="VHPJ-1"), raw_issue(key="VHPJ-2"), done]
        code, out, err = self.run_main(["snapshot"], self.search(issues), JIRA_TEAM="a@x.com")
        self.assertEqual(code, 0, err)
        self.assertNotIn("Traceback", err)
        json.loads(out)

    def test_update_history_tolerates_bad_records(self):
        done = done_issue()
        history = {"VHPJ-5": {"started": "naive-ou-lixo", "updated": None}, "OUTRO": 7}
        result = jira.update_history(history, [done], client_for(RoutedHttp({})), CATEGORIES, NOW)
        self.assertIn("VHPJ-5", result)

    def test_unexpected_payload_types_do_not_crash_parsing(self):
        weird = raw_issue(assignee="Ana", creator=[1], priority="High", status="x")
        issue = jira.parse_issue(weird)
        self.assertIsNotNone(issue)
        self.assertIsNone(issue.assignee_id)
        self.assertIsNone(jira.find_work_start(client_for(RoutedHttp({"/rest/api/3/issue/VHPJ-1": (200, [1])})), "VHPJ-1", CATEGORIES))

    def test_unexpected_exception_falls_back_to_the_offline_cache_or_a_clean_error(self):
        self.run_main(["snapshot"], self.search([raw_issue()]))

        def exploding(url, method, headers, timeout):
            raise RuntimeError("bug inesperado " + SECRET)

        code, out, err = self.run_main(["snapshot"], http=exploding)
        self.assertEqual(code, 0, err)
        self.assertIn("(offline)", summary_texts(json.loads(out))[0])
        self.assertNotIn(SECRET, out + err)
        fresh = tempfile.TemporaryDirectory()
        self.state_dir = Path(fresh.name) / "novo"
        code, out, err = self.run_main(["snapshot"], http=exploding)
        self.assertEqual(code, 1)
        self.assertNotIn("Traceback", err)
        self.assertNotIn(SECRET, out + err)
        fresh.cleanup()

    def test_naive_dates_in_the_offline_cache_are_treated_as_utc(self):
        raw = jira.parse_issue(raw_issue()).to_raw()
        raw["created"] = "2026-10-17T08:00:00"
        raw["updated"] = "2026-10-17T09:00:00"
        issue = jira.Issue.from_raw(raw)
        self.assertEqual(issue.created.tzinfo, timezone.utc)
        self.assertEqual(issue.updated.tzinfo, timezone.utc)

    # 3. race between snapshot and mark_seen
    def test_mark_seen_during_a_snapshot_is_not_lost(self):
        store = self.store()
        marker = NOW.isoformat()

        def http(url, method, headers, timeout):
            path = urllib.parse.urlparse(url).path
            if path == "/rest/api/3/search/jql":
                return 200, json.dumps({"issues": [raw_issue()]}).encode()
            if path == "/rest/api/3/user/search":
                state = store.load_state()
                state["last_seen_manager"] = marker
                store.save_state(state)
                return 200, json.dumps([{"accountId": "acc-x", "displayName": "X"}]).encode()
            return 200, b"[]"

        code, _, err = self.run_main(["snapshot"], http=http, JIRA_TEAM="x@x.com")
        self.assertEqual(code, 0, err)
        state = store.load_state()
        self.assertEqual(state["last_seen_manager"], marker)
        self.assertEqual(state["accounts"]["x@x.com"]["id"], "acc-x")

    # 4. changelog fetches respect the time budget
    def test_changelog_fetches_stop_when_time_runs_out(self):
        issues = [done_issue(key=f"VHPJ-{n}") for n in range(10, 15)]
        http = RoutedHttp({f"/rest/api/3/issue/VHPJ-{n}": (200, HistoryTests.changelog) for n in range(10, 15)})
        remaining = iter([False, False, True, True, True, True])
        history = jira.update_history(
            {}, issues, client_for(http), CATEGORIES, NOW, out_of_time=lambda: next(remaining)
        )
        self.assertEqual(len(http.calls), 2)
        self.assertEqual(len(history), 5)

    def test_main_passes_a_shared_budget_to_the_history_update(self):
        done = [
            raw_issue(
                key=f"VHPJ-{n}",
                status={"name": "Done", "statusCategory": {"key": "done"}},
                resolutiondate="2026-10-17T11:00:00.000+0000",
            )
            for n in range(10, 16)
        ]
        routes = self.search(done)
        for n in range(10, 16):
            routes[f"/rest/api/3/issue/VHPJ-{n}"] = (200, HistoryTests.changelog)
        ticks = {"value": 0.0}

        def clock():
            ticks["value"] += 20.0
            return ticks["value"]

        http = RoutedHttp({**STATUS_ROUTE, **routes})
        code, _, err = self.run_main(["snapshot"], clock=clock, http=http)
        self.assertEqual(code, 0, err)
        changelog_calls = [path for path in http.calls if "/issue/" in path]
        self.assertLess(len(changelog_calls), 6)

    # 5. redirects must not forward credentials; base url restricted
    def test_redirects_are_not_followed_and_credentials_are_not_forwarded(self):
        received = []

        class Target(BaseHTTPRequestHandler):
            def do_GET(self):
                received.append(self.headers.get("Authorization"))
                self.send_response(200)
                self.send_header("Content-Length", "2")
                self.end_headers()
                self.wfile.write(b"{}")

            def log_message(self, *arguments):
                pass

        target = ThreadingHTTPServer(("127.0.0.1", 0), Target)
        threading.Thread(target=target.serve_forever, daemon=True).start()
        destination = f"http://127.0.0.1:{target.server_address[1]}/alvo"

        class Redirector(BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(302)
                self.send_header("Location", destination)
                self.send_header("Content-Length", "0")
                self.end_headers()

            def log_message(self, *arguments):
                pass

        origin = ThreadingHTTPServer(("127.0.0.1", 0), Redirector)
        threading.Thread(target=origin.serve_forever, daemon=True).start()
        try:
            base = f"http://127.0.0.1:{origin.server_address[1]}"
            status, _ = jira.urllib_http(f"{base}/x", "GET", {"Authorization": "Basic abc"}, 5)
            self.assertEqual(status, 302)
            client = make_client(jira.urllib_http, JIRA_BASE_URL=base)
            with self.assertRaises(jira.UnexpectedResponseError):
                client.get_json("/x")
            self.assertEqual(received, [])
        finally:
            for server in (origin, target):
                server.shutdown()
                server.server_close()

    def test_base_url_must_be_https_or_localhost(self):
        for url in ("http://evil.example.com", "ftp://x", "file:///etc/passwd", "evil.com", "http://127.0.0.1.evil.com"):
            with self.assertRaises(jira.ConfigError, msg=url):
                jira.load_config(base_environment(JIRA_BASE_URL=url))
        for url in ("https://empresa.atlassian.net", "http://127.0.0.1:8080", "http://localhost:9"):
            jira.load_config(base_environment(JIRA_BASE_URL=url))


if __name__ == "__main__":
    unittest.main()
