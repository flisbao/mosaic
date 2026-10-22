import io
import json
import stat
import sys
import tempfile
import threading
import time
import unittest
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "github"))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import github_prs  # noqa: E402
import pull_request_fixtures as fixtures  # noqa: E402

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

    def test_token_with_internal_whitespace_is_rejected_without_leaking(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "token"
            path.write_text("abc\ndef\n", encoding="utf-8")
            environment = base_environment(
                GITHUB_TOKEN_ENV=None, MEU_TOKEN=None, GITHUB_TOKEN_FILE=str(path)
            )
            with self.assertRaises(github_prs.ConfigError) as caught:
                github_prs.load_config(environment)
        self.assertNotIn("abc", str(caught.exception))
        self.assertIn("GITHUB_TOKEN_ENV", str(caught.exception))

    def test_token_with_space_in_variable_is_rejected_without_leaking(self):
        with self.assertRaises(github_prs.ConfigError) as caught:
            github_prs.load_config(base_environment(MEU_TOKEN="a b"))
        self.assertNotIn("a b", str(caught.exception))

    def test_token_outside_latin1_is_rejected_without_leaking(self):
        with self.assertRaises(github_prs.ConfigError) as caught:
            github_prs.load_config(base_environment(MEU_TOKEN="€abc"))
        self.assertNotIn("€abc", str(caught.exception))

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
            (400, github_prs.RequestRejectedError),
            (404, github_prs.RequestRejectedError),
            (410, github_prs.RequestRejectedError),
            (500, github_prs.UnexpectedResponseError),
        ]
        for status, error in cases:
            with self.assertRaises(error, msg=status):
                make_client(FakeHttp([(status, b"{}")])).query("q", {})

    def test_forbidden_with_rate_limit_body_is_rate_limit(self):
        body = b'{"message": "You have exceeded a secondary rate limit"}'
        with self.assertRaises(github_prs.RateLimitError):
            make_client(FakeHttp([(403, body)])).query("q", {})

    def test_graphql_errors_without_data_are_rejected(self):
        http = FakeHttp([(200, {"errors": [{"message": SECRET}], "data": None})])
        with self.assertRaises(github_prs.RequestRejectedError) as caught:
            make_client(http).query("q", {})
        self.assertEqual(str(caught.exception), "o GitHub recusou a consulta; o plugin pode estar desatualizado")

    def test_graphql_without_errors_and_without_data_is_unexpected(self):
        with self.assertRaises(github_prs.UnexpectedResponseError):
            make_client(FakeHttp([(200, {})])).query("q", {})

    def test_graphql_errors_with_data_keep_the_data(self):
        payload = {"errors": [{"message": "parcial"}], "data": {"viewer": {"login": "eu"}}}
        self.assertEqual(make_client(FakeHttp([(200, payload)])).query("q", {}), {"viewer": {"login": "eu"}})

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


class DecodeResponseTests(unittest.TestCase):
    def test_permanent_rejections_have_a_fixed_message(self):
        for status in (400, 404, 410):
            body = json.dumps({"message": SECRET}).encode()
            with self.assertRaises(github_prs.RequestRejectedError, msg=status) as caught:
                github_prs.decode_response(status, body, "GitHub")
            self.assertEqual(
                str(caught.exception),
                f"o GitHub recusou a requisição (HTTP {status}); confira a configuração do widget",
            )

    def test_transient_statuses_are_not_permanent_rejections(self):
        for status in (500, 502, 503):
            with self.assertRaises(github_prs.UnexpectedResponseError, msg=status):
                github_prs.decode_response(status, b"{}", "GitHub")


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


class ParseTimestampTests(unittest.TestCase):
    def test_accepts_zulu_suffix_and_offsets(self):
        self.assertEqual(github_prs.parse_timestamp("2026-10-20T10:00:00Z").hour, 10)
        self.assertEqual(github_prs.parse_timestamp("2026-10-20T10:00:00.500+00:00").microsecond, 500000)

    def test_invalid_text_is_none(self):
        self.assertIsNone(github_prs.parse_timestamp("ontem"))
        self.assertIsNone(github_prs.parse_timestamp(None))


class NormalizeTests(unittest.TestCase):
    def setUp(self):
        self.pulls = {p.item_id: p for p in github_prs.normalize(fixtures.github_response()["data"])}

    def test_review_section_membership(self):
        review = {i for i, p in self.pulls.items() if p.section == github_prs.REVIEW_SECTION}
        self.assertEqual(review, {"acme/api#10", "acme/api#11", "acme/web#12", "acme/lib#14"})

    def test_review_status_and_push(self):
        expected = {
            "acme/api#10": ("unreviewed", False),
            "acme/api#11": ("reviewed", True),
            "acme/web#12": ("unreviewed", False),
            "acme/lib#14": ("reviewed", False),
        }
        for item_id, (status, pushed) in expected.items():
            self.assertEqual((self.pulls[item_id].review_status, self.pulls[item_id].pushed_after_review),
                             (status, pushed), item_id)

    def test_mine_states(self):
        expected = {"acme/api#20": "conflito", "acme/api#21": "mudança pedida",
                    "acme/api#22": "aprovado", "acme/api#23": "aguardando"}
        for item_id, state in expected.items():
            self.assertEqual(self.pulls[item_id].state, state, item_id)
            self.assertEqual(self.pulls[item_id].section, "Meus PRs")

    def test_mine_state_is_derived_from_reviews_without_review_decision(self):
        approved = fixtures.github_review(fixtures.OTHER_LOGIN, "APPROVED", "2026-10-20T10:00:00Z")
        changes = fixtures.github_review("terceira", "CHANGES_REQUESTED", "2026-10-20T11:00:00Z")
        commented = fixtures.github_review(fixtures.OTHER_LOGIN, "COMMENTED", "2026-10-20T10:00:00Z")
        cases = {
            "aprovado": [approved],
            "mudança pedida": [approved, changes],
            "aguardando": [commented],
        }
        for expected, reviews in cases.items():
            self.assertEqual(self.mine_state(reviews=reviews), expected)

    def test_mine_state_ignores_reviews_by_the_author(self):
        own_approval = fixtures.github_review("ana", "APPROVED", "2026-10-20T10:00:00Z")
        own_changes = fixtures.github_review("ana", "CHANGES_REQUESTED", "2026-10-20T10:00:00Z")
        self.assertEqual(self.mine_state(reviews=[own_approval]), "aguardando")
        self.assertEqual(self.mine_state(reviews=[own_changes]), "aguardando")

    def test_mine_state_review_decision_wins_when_it_is_decisive(self):
        changes = fixtures.github_review(fixtures.OTHER_LOGIN, "CHANGES_REQUESTED", "2026-10-20T10:00:00Z")
        approved = fixtures.github_review(fixtures.OTHER_LOGIN, "APPROVED", "2026-10-20T10:00:00Z")
        self.assertEqual(self.mine_state(review_decision="APPROVED", reviews=[changes]), "aprovado")
        self.assertEqual(self.mine_state(review_decision="CHANGES_REQUESTED", reviews=[approved]), "mudança pedida")

    def test_mine_state_conflict_wins_over_reviews(self):
        approved = fixtures.github_review(fixtures.OTHER_LOGIN, "APPROVED", "2026-10-20T10:00:00Z")
        self.assertEqual(self.mine_state(mergeable="CONFLICTING", reviews=[approved]), "conflito")

    def mine_state(self, reviews=(), **options):
        node = fixtures.github_pull(30, "acme/api", "2026-10-20T08:00:00Z", **options)
        node["latestReviews"] = {"nodes": list(reviews)}
        data = {"viewer": {"login": fixtures.VIEWER_LOGIN}, "mine": {"nodes": [node]}}
        return github_prs.normalize(data)[0].state

    def test_pull_request_fields_are_mapped(self):
        pull = self.pulls["acme/api#11"]
        self.assertEqual(pull.repository, "acme/api")
        self.assertEqual(pull.reference, "#11")
        self.assertEqual(pull.author, "ana")
        self.assertEqual(pull.target_branch, "main")
        self.assertEqual(pull.head_commit, "sha11")
        self.assertEqual(pull.url, "https://github.com/acme/api/pull/11")

    def test_pull_request_without_author_uses_placeholder(self):
        self.assertEqual(self.pulls["acme/api#24"].author, "desconhecido")

    def test_pull_request_without_creation_date_is_skipped(self):
        data = fixtures.github_response()["data"]
        data["mine"]["nodes"][0]["createdAt"] = None
        ids = {p.item_id for p in github_prs.normalize(data)}
        self.assertNotIn("acme/api#20", ids)
        self.assertIn("acme/api#21", ids)

    def test_drafts_are_dropped(self):
        self.assertNotIn("acme/api#13", self.pulls)

    def test_reviewed_search_keeps_only_changes_requested_by_me(self):
        self.assertNotIn("acme/lib#15", self.pulls)

    def test_pull_request_already_in_review_section_is_not_repeated(self):
        pulls = github_prs.normalize(fixtures.github_response()["data"])
        matches = [p for p in pulls if p.item_id == "acme/api#11"]
        self.assertEqual(len(matches), 1)
        self.assertEqual(matches[0].section, github_prs.REVIEW_SECTION)

    def test_malformed_nodes_are_skipped(self):
        data = fixtures.github_response()["data"]
        data["review"]["nodes"].append("lixo")
        data["mine"]["nodes"].append(None)
        ids = {p.item_id for p in github_prs.normalize(data)}
        self.assertIn("acme/api#10", ids)

    def test_sections_are_sorted_by_creation_date(self):
        created = [p.created_at for p in github_prs.normalize(fixtures.github_response()["data"])]
        self.assertEqual(created, sorted(created))


class SearchQueryTests(unittest.TestCase):
    def test_queries_without_owners(self):
        self.assertEqual(github_prs.search_queries(())["review"],
                         "is:pr is:open draft:false archived:false review-requested:@me")

    def test_owners_become_user_qualifiers(self):
        self.assertTrue(github_prs.search_queries(("acme", "eu"))["mine"].endswith(" user:acme user:eu"))

    def test_reviewed_query_excludes_my_own_pull_requests(self):
        self.assertEqual(github_prs.search_queries(())["reviewed"],
                         "is:pr is:open draft:false archived:false reviewed-by:@me -author:@me")


class FetchTests(unittest.TestCase):
    def test_fetch_sends_one_query_with_the_three_searches(self):
        http = FakeHttp([(200, fixtures.github_response())])
        pulls = github_prs.fetch_pull_requests(make_client(http))
        self.assertEqual(len(http.calls), 1)
        variables = json.loads(http.calls[0]["body"])["variables"]
        self.assertEqual(set(variables), {"review", "reviewed", "mine"})
        self.assertTrue(pulls)


def pull(item_id, section, review_status="", pushed=None, head="h1", state="", created="2026-10-20T10:00:00Z", title="T"):
    return github_prs.PullRequest(
        item_id=item_id,
        repository="acme/api",
        reference="#1",
        title=title,
        author="ana",
        created_at=github_prs.parse_timestamp(created),
        url="https://x/1",
        target_branch="main",
        head_commit=head,
        section=section,
        review_status=review_status,
        pushed_after_review=pushed,
        state=state,
    )


REVIEW = github_prs.REVIEW_SECTION
MINE = github_prs.MINE_SECTION
NOW = datetime(2026, 10, 22, 12, 0, tzinfo=timezone.utc)


class ReviewMemoryTests(unittest.TestCase):
    def test_unreviewed_without_memory_is_new(self):
        shown, _ = github_prs.apply_review_memory([pull("a", REVIEW, "unreviewed")], {})
        self.assertEqual(shown[0].state, "novo")

    def test_reviewed_then_head_changes_is_pushed(self):
        _, memory = github_prs.apply_review_memory([pull("a", REVIEW, "reviewed", head="h1")], {})
        shown, _ = github_prs.apply_review_memory([pull("a", REVIEW, "reviewed", head="h2")], memory)
        self.assertEqual(shown[0].state, "push novo")

    def test_reviewed_same_head_has_no_tag(self):
        _, memory = github_prs.apply_review_memory([pull("a", REVIEW, "reviewed", head="h1")], {})
        shown, _ = github_prs.apply_review_memory([pull("a", REVIEW, "reviewed", head="h1")], memory)
        self.assertEqual(shown[0].state, "")

    def test_vote_reset_by_push_is_pushed_not_new(self):
        _, memory = github_prs.apply_review_memory([pull("a", REVIEW, "approved", head="h1")], {})
        shown, _ = github_prs.apply_review_memory([pull("a", REVIEW, "unreviewed", head="h2")], memory)
        self.assertEqual(shown[0].state, "push novo")

    def test_approved_is_hidden_but_remembered(self):
        shown, memory = github_prs.apply_review_memory([pull("a", REVIEW, "approved")], {})
        self.assertEqual(shown, [])
        self.assertEqual(memory["a"]["reviewed_head"], "h1")

    def test_known_push_flag_wins_over_memory(self):
        shown, _ = github_prs.apply_review_memory([pull("a", REVIEW, "reviewed", pushed=True)], {})
        self.assertEqual(shown[0].state, "push novo")

    def test_mine_passes_through_and_memory_drops_vanished(self):
        shown, memory = github_prs.apply_review_memory([pull("m", MINE, state="aprovado")], {"old": {}})
        self.assertEqual(shown[0].state, "aprovado")
        self.assertEqual(memory, {})

    def test_non_dict_memory_entry_is_ignored(self):
        shown, _ = github_prs.apply_review_memory([pull("a", REVIEW, "unreviewed")], {"a": "lixo"})
        self.assertEqual(shown[0].state, "novo")


class SnapshotTests(unittest.TestCase):
    def test_sections_order_and_item_text(self):
        pulls = [
            pull("b", REVIEW, state="novo", created="2026-10-21T12:00:00Z"),
            pull("a", REVIEW, state="push novo", created="2026-10-19T12:00:00Z"),
            pull("m", MINE, state="aprovado"),
        ]
        snapshot = github_prs.build_snapshot(pulls, "GitHub", NOW)
        self.assertEqual([s["heading"] for s in snapshot["detail"]], ["Aguardando minha revisão", "Meus PRs"])
        review_items = snapshot["detail"][0]["items"]
        self.assertEqual([i["id"] for i in review_items], ["a", "b"])
        self.assertEqual(review_items[0]["text"], "acme/api #1 T · ana · 3d [push novo]")
        self.assertEqual(review_items[0]["flag"], "updated")
        self.assertNotIn("flag", review_items[1])
        self.assertEqual(review_items[0]["meta"], {"url": "https://x/1", "target_branch": "main",
                                                   "state": "push novo", "author": "ana"})

    def test_home_stat_rows_mini_and_alerts(self):
        snapshot = github_prs.build_snapshot([pull("a", REVIEW, state="novo"), pull("m", MINE, state="conflito")], "GitHub", NOW)
        stat, rows = snapshot["summary"]["blocks"]
        self.assertEqual(stat["items"], [{"value": "1", "label": "revisar"}, {"value": "1", "label": "meus PRs"}])
        self.assertEqual(rows["rows"][0]["tags"], [{"text": "novo", "tone": "current"}])
        self.assertEqual(snapshot["mini"], {"value": "1", "text": "para revisar", "tone": "warn"})
        self.assertEqual(snapshot["alerts"][0]["id"], "review-a")
        self.assertEqual(snapshot["protocol"], 1)

    def test_empty_shows_nothing_pending(self):
        snapshot = github_prs.build_snapshot([], "GitHub", NOW)
        self.assertEqual(snapshot["summary"]["blocks"][1],
                         {"type": "lines", "lines": [{"text": "nada pendente", "tone": "muted"}]})
        self.assertEqual(snapshot["mini"]["tone"], "normal")

    def test_home_rows_are_capped_at_protocol_limit(self):
        pulls = [pull(f"p{n}", REVIEW, state="novo") for n in range(250)]
        rows = github_prs.build_snapshot(pulls, "GitHub", NOW)["summary"]["blocks"][1]["rows"]
        self.assertEqual(len(rows), 200)

    def test_long_titles_are_flattened_and_cut(self):
        pulls = [pull("a", REVIEW, state="novo", title="linha um\nlinha dois " + "x" * 200)]
        item_text = github_prs.build_snapshot(pulls, "GitHub", NOW)["detail"][0]["items"][0]["text"]
        self.assertNotIn("\n", item_text)
        self.assertIn("…", item_text)
        self.assertLessEqual(len(item_text.split(" · ")[0]), len("acme/api #1 ") + 80)

    def test_offline_notice_is_first_block_with_warn_tone(self):
        snapshot = github_prs.build_snapshot([pull("a", REVIEW, state="novo")], "GitHub", NOW)
        saved_at = NOW
        notice = github_prs.with_offline_notice(snapshot, saved_at, NOW)["summary"]["blocks"][0]
        expected = saved_at.astimezone().strftime("dados de %H:%M (offline)")
        self.assertEqual(notice, {"type": "lines", "lines": [{"text": expected, "tone": "warn"}]})
        self.assertEqual(snapshot["summary"]["blocks"][0]["type"], "stat")


    def test_reviewed_items_have_no_alert(self):
        pulls = [pull("a", REVIEW, state="novo"), pull("b", REVIEW, state="")]
        alerts = github_prs.build_snapshot(pulls, "GitHub", NOW)["alerts"]
        self.assertEqual([alert["id"] for alert in alerts], ["review-a"])


class OfflineNoticeTests(unittest.TestCase):
    SNAPSHOT = {"summary": {"title": "T", "blocks": [{"type": "stat", "items": []}]}}
    SAVED_AT = datetime(2026, 10, 21, 15, 30).astimezone()

    def notice_text(self, now):
        blocks = github_prs.with_offline_notice(self.SNAPSHOT, self.SAVED_AT, now)["summary"]["blocks"]
        self.assertEqual(blocks[0]["lines"][0]["tone"], "warn")
        self.assertEqual(blocks[1], self.SNAPSHOT["summary"]["blocks"][0])
        return blocks[0]["lines"][0]["text"]

    def test_same_local_day_shows_only_the_time(self):
        now = datetime(2026, 10, 21, 18, 0).astimezone()
        self.assertEqual(self.notice_text(now), "dados de 15:30 (offline)")

    def test_previous_local_day_shows_the_date(self):
        now = datetime(2026, 10, 22, 9, 0).astimezone()
        self.assertEqual(self.notice_text(now), "dados de 21/10 15:30 (offline)")

    def test_days_ago_shows_the_date(self):
        now = datetime(2026, 10, 25, 9, 0).astimezone()
        self.assertEqual(self.notice_text(now), "dados de 21/10 15:30 (offline)")


class StateStoreTests(unittest.TestCase):
    def test_round_trip_and_private_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "estado"
            store = github_prs.StateStore(target, "k")
            document = {"saved_at": NOW.isoformat(), "snapshot": {"protocol": 1}, "reviews": {"a": {"status": "reviewed"}}}
            store.save(document)
            self.assertEqual(store.load(), document)
            self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o700)

    def test_save_failure_is_ignored(self):
        with tempfile.TemporaryDirectory() as directory:
            blocker = Path(directory) / "arquivo"
            blocker.write_text("não é diretório", encoding="utf-8")
            store = github_prs.StateStore(blocker, "k")
            store.save({"saved_at": None, "snapshot": None, "reviews": {}})

    def test_corrupt_state_file_is_treated_as_empty(self):
        with tempfile.TemporaryDirectory() as directory:
            store = github_prs.StateStore(Path(directory) / "s", "k")
            store.save({"saved_at": None, "snapshot": None, "reviews": {}})
            next(Path(directory, "s").glob("*.json")).write_text("{quebrado", encoding="utf-8")
            self.assertEqual(store.load(), {"reviews": {}, "snapshot": None, "saved_at": None})


class MainTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.state_dir = Path(self.temporary.name) / "estado"
        self.stdout = io.StringIO()
        self.stderr = io.StringIO()

    def run_main(self, http, arguments=("snapshot",), **environment_overrides):
        environment = base_environment(GITHUB_STATE_DIR=str(self.state_dir), **environment_overrides)
        return github_prs.main(
            list(arguments),
            environ=environment,
            http=http,
            now=NOW,
            stdout=self.stdout,
            stderr=self.stderr,
        )

    def populate_cache(self):
        self.assertEqual(self.run_main(FakeHttp([(200, fixtures.github_response())])), 0)
        self.stdout = io.StringIO()

    def test_main_prints_snapshot_and_saves_state(self):
        code = self.run_main(FakeHttp([(200, fixtures.github_response())]))
        self.assertEqual(code, 0)
        snapshot = json.loads(self.stdout.getvalue())
        self.assertEqual([s["heading"] for s in snapshot["detail"]], [REVIEW, MINE])
        self.assertEqual(len(list(self.state_dir.glob("*.json"))), 1)

    def test_main_usage_error_exits_two(self):
        self.assertEqual(self.run_main(FakeHttp(), arguments=("outra",)), 2)
        self.assertEqual(self.stderr.getvalue(), "uso: github_prs.py snapshot\n")

    def test_main_config_error_exits_one(self):
        code = self.run_main(FakeHttp(), GITHUB_TOKEN_ENV=None, MEU_TOKEN=None)
        self.assertEqual(code, 1)
        self.assertTrue(self.stderr.getvalue().startswith("erro: "))

    def test_main_auth_error_exits_one_without_cache(self):
        code = self.run_main(FakeHttp([(401, {"message": SECRET})]))
        self.assertEqual(code, 1)
        self.assertEqual(self.stdout.getvalue(), "")
        self.assertTrue(self.stderr.getvalue().startswith("erro: "))
        self.assertNotIn(SECRET, self.stderr.getvalue())

    def test_main_network_error_serves_cache_with_offline_notice(self):
        self.populate_cache()
        code = self.run_main(FakeHttp(error=OSError("sem rede")))
        self.assertEqual(code, 0)
        snapshot = json.loads(self.stdout.getvalue())
        expected = NOW.astimezone().strftime("dados de %H:%M (offline)")
        self.assertEqual(
            snapshot["summary"]["blocks"][0],
            {"type": "lines", "lines": [{"text": expected, "tone": "warn"}]},
        )

    def test_main_permanent_errors_are_not_masked_by_the_cache(self):
        rejected_query = (200, {"errors": [{"message": SECRET}], "data": None})
        for response in (rejected_query, (401, {"message": SECRET})):
            with self.subTest(status=response[0]):
                self.populate_cache()
                self.stderr = io.StringIO()
                self.assertEqual(self.run_main(FakeHttp([response])), 1)
                self.assertEqual(self.stdout.getvalue(), "")
                self.assertTrue(self.stderr.getvalue().startswith("erro: "))
                self.assertNotIn("offline", self.stderr.getvalue())
                self.assertNotIn(SECRET, self.stderr.getvalue())

    def test_main_network_error_without_cache_exits_one(self):
        code = self.run_main(FakeHttp(error=OSError("sem rede")))
        self.assertEqual(code, 1)
        self.assertTrue(self.stderr.getvalue().startswith("erro: "))

    def test_main_output_never_contains_the_token(self):
        self.run_main(FakeHttp([(200, fixtures.github_response())]))
        for path in self.state_dir.iterdir():
            self.assertNotIn(SECRET, path.read_text(encoding="utf-8"))
        self.assertNotIn(SECRET, self.stdout.getvalue())
        self.assertNotIn(SECRET, self.stderr.getvalue())


    def test_widgets_with_different_owners_have_separate_state_files(self):
        self.run_main(FakeHttp([(200, fixtures.github_response())]), GITHUB_OWNERS="acme")
        self.run_main(FakeHttp([(200, fixtures.github_response())]), GITHUB_OWNERS="outra")
        self.assertEqual(len(list(self.state_dir.glob("*.json"))), 2)

    def test_offline_widget_never_shows_another_widgets_snapshot(self):
        self.run_main(FakeHttp([(200, fixtures.github_response())]), GITHUB_OWNERS="acme")
        self.stdout = io.StringIO()
        self.assertEqual(self.run_main(FakeHttp(error=OSError("sem rede")), GITHUB_OWNERS="outra"), 1)
        self.assertEqual(self.stdout.getvalue(), "")
        self.assertTrue(self.stderr.getvalue().startswith("erro: "))

    def test_cached_state_without_valid_summary_is_not_served(self):
        self.populate_cache()
        state_file = next(self.state_dir.glob("*.json"))
        state_file.write_text(
            json.dumps({"reviews": {}, "snapshot": {}, "saved_at": "2026-10-22T12:00:00+00:00"}),
            encoding="utf-8",
        )
        self.stdout = io.StringIO()
        self.stderr = io.StringIO()
        self.assertEqual(self.run_main(FakeHttp(error=OSError("sem rede"))), 1)
        self.assertTrue(self.stderr.getvalue().startswith("erro: "))


if __name__ == "__main__":
    unittest.main()
