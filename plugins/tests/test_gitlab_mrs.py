import io
import json
import sys
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "gitlab"))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import gitlab_mrs  # noqa: E402
import pull_request_fixtures as fixtures  # noqa: E402

SECRET = "TOKEN-SECRETO-123"
VIEWER = fixtures.VIEWER_LOGIN
REVIEW = gitlab_mrs.REVIEW_SECTION
MINE = gitlab_mrs.MINE_SECTION
NOW = datetime(2026, 10, 22, 12, 0, tzinfo=timezone.utc)


def base_environment(**overrides):
    environment = {
        "GITLAB_TOKEN_ENV": "MEU_TOKEN",
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
    return gitlab_mrs.GitLabClient(gitlab_mrs.load_config(base_environment(**overrides)), http)


def fetch_fixture_merge_requests(**overrides):
    http = FakeHttp([(200, fixtures.gitlab_response())])
    merge_requests = gitlab_mrs.fetch_pull_requests(make_client(http, **overrides))
    return {merge_request.item_id: merge_request for merge_request in merge_requests}


class ConfigTests(unittest.TestCase):
    def test_defaults(self):
        config = gitlab_mrs.load_config(base_environment())
        self.assertEqual(config.base_url, "https://gitlab.com")
        self.assertEqual(config.title, "GitLab")
        self.assertEqual(config.token, SECRET)
        self.assertTrue(str(config.state_dir).endswith(".local/state/mosaic/gitlab"))

    def test_base_url_accepts_only_https_or_local_http(self):
        for bad in ("http://gitlab.com", "ftp://x", "https://a b"):
            with self.assertRaises(gitlab_mrs.ConfigError, msg=bad):
                gitlab_mrs.load_config(base_environment(GITLAB_URL=bad))
        local = gitlab_mrs.load_config(base_environment(GITLAB_URL="http://localhost:8080/"))
        self.assertEqual(local.base_url, "http://localhost:8080")

    def test_token_error_does_not_leak_the_value(self):
        with self.assertRaises(gitlab_mrs.ConfigError) as caught:
            gitlab_mrs.load_config(base_environment(MEU_TOKEN="a b"))
        self.assertNotIn("a b", str(caught.exception))


class ClientTests(unittest.TestCase):
    def test_posts_to_api_graphql_with_bearer(self):
        http = FakeHttp([(200, fixtures.gitlab_response())])
        make_client(http).query("query { currentUser { username } }", {})
        call = http.calls[0]
        self.assertEqual(call["url"], "https://gitlab.com/api/graphql")
        self.assertEqual(call["method"], "POST")
        self.assertEqual(call["headers"]["Authorization"], f"Bearer {SECRET}")
        self.assertEqual(json.loads(call["body"])["variables"], {})

    def test_error_messages_never_contain_the_token(self):
        for status in (401, 403, 429, 500):
            body = json.dumps({"message": f"bad {SECRET}"}).encode()
            with self.assertRaises(gitlab_mrs.ApiError, msg=status) as caught:
                make_client(FakeHttp([(status, body)])).query("query {}", {})
            self.assertNotIn(SECRET, str(caught.exception))

    def test_graphql_errors_never_contain_the_token(self):
        body = {"errors": [{"message": f"bad {SECRET}"}]}
        with self.assertRaises(gitlab_mrs.RequestRejectedError) as caught:
            make_client(FakeHttp([(200, body)])).query("query {}", {})
        self.assertEqual(str(caught.exception), "o GitLab recusou a consulta; o plugin pode estar desatualizado")

    def test_graphql_without_errors_and_without_data_is_unexpected(self):
        with self.assertRaises(gitlab_mrs.UnexpectedResponseError):
            make_client(FakeHttp([(200, {})])).query("query {}", {})

    def test_status_errors_map_to_exceptions(self):
        cases = [
            (401, gitlab_mrs.AuthError),
            (429, gitlab_mrs.RateLimitError),
            (400, gitlab_mrs.RequestRejectedError),
            (404, gitlab_mrs.RequestRejectedError),
            (410, gitlab_mrs.RequestRejectedError),
            (500, gitlab_mrs.UnexpectedResponseError),
        ]
        for status, error in cases:
            with self.assertRaises(error, msg=status):
                make_client(FakeHttp([(status, b"{}")])).query("query {}", {})

    def test_permanent_rejections_have_a_fixed_message(self):
        for status in (400, 404, 410):
            body = json.dumps({"message": SECRET}).encode()
            with self.assertRaises(gitlab_mrs.RequestRejectedError, msg=status) as caught:
                gitlab_mrs.decode_response(status, body, "GitLab")
            self.assertEqual(
                str(caught.exception),
                f"o GitLab recusou a requisição (HTTP {status}); confira a configuração do widget",
            )

    def test_graphql_rate_limited_error_is_rate_limit(self):
        body = {"errors": [{"type": "RATE_LIMITED", "message": "devagar"}]}
        with self.assertRaises(gitlab_mrs.RateLimitError):
            make_client(FakeHttp([(200, body)])).query("query {}", {})

    def test_network_error_never_contains_the_token(self):
        with self.assertRaises(gitlab_mrs.NetworkError) as caught:
            make_client(FakeHttp(error=OSError(f"falha {SECRET}"))).query("query {}", {})
        self.assertNotIn(SECRET, str(caught.exception))


class NormalizeTests(unittest.TestCase):
    def test_review_statuses(self):
        merge_requests = fetch_fixture_merge_requests()
        expected = {
            "grupo/api!10": gitlab_mrs.UNREVIEWED,
            "grupo/api!11": gitlab_mrs.REVIEWED,
            "grupo/api!12": gitlab_mrs.APPROVED,
        }
        for item_id, status in expected.items():
            self.assertEqual(merge_requests[item_id].section, REVIEW, item_id)
            self.assertEqual(merge_requests[item_id].review_status, status, item_id)
            self.assertIsNone(merge_requests[item_id].pushed_after_review, item_id)
        self.assertNotIn("grupo/api!13", merge_requests)

    def test_mine_states(self):
        merge_requests = fetch_fixture_merge_requests()
        expected = {
            "grupo/api!20": gitlab_mrs.STATE_CONFLICT,
            "grupo/api!21": gitlab_mrs.STATE_CHANGES,
            "grupo/api!22": gitlab_mrs.STATE_APPROVED,
            "grupo/api!23": gitlab_mrs.STATE_WAITING,
        }
        for item_id, state in expected.items():
            self.assertEqual(merge_requests[item_id].state, state, item_id)
            self.assertEqual(merge_requests[item_id].section, MINE, item_id)

    def test_mergeable_without_approval_is_waiting(self):
        merge_requests = fetch_fixture_merge_requests()
        self.assertEqual(merge_requests["grupo/api!23"].state, gitlab_mrs.STATE_WAITING)

    def test_merge_request_fields_are_mapped(self):
        merge_request = fetch_fixture_merge_requests()["grupo/api!10"]
        self.assertEqual(merge_request.reference, "!10")
        self.assertEqual(merge_request.repository, "grupo/api")
        self.assertEqual(merge_request.url, "https://gitlab.com/grupo/api/-/merge_requests/10")
        self.assertEqual(merge_request.target_branch, "main")
        self.assertEqual(merge_request.head_commit, "sha10")
        self.assertEqual(merge_request.author, "ana")

    def test_null_current_user_is_auth_error(self):
        http = FakeHttp([(200, {"data": {"currentUser": None}})])
        with self.assertRaises(gitlab_mrs.AuthError) as caught:
            gitlab_mrs.fetch_pull_requests(make_client(http))
        self.assertEqual(str(caught.exception), "token inválido ou sem permissão para GitLab")

    def test_own_merge_request_with_self_review_stays_only_in_my_section(self):
        response = fixtures.gitlab_response()
        own = fixtures.gitlab_merge_request(30, author=VIEWER, reviewers=[(VIEWER, "UNREVIEWED")])
        current_user = response["data"]["currentUser"]
        current_user["reviewRequestedMergeRequests"]["nodes"].append(own)
        current_user["authoredMergeRequests"]["nodes"].append(own)
        merge_requests = gitlab_mrs.normalize(response["data"])
        matches = [merge_request for merge_request in merge_requests if merge_request.item_id == "grupo/api!30"]
        self.assertEqual([merge_request.section for merge_request in matches], [MINE])
        self.assertEqual(len({merge_request.item_id for merge_request in merge_requests}), len(merge_requests))

    def test_merge_request_without_author_uses_placeholder(self):
        response = fixtures.gitlab_response()
        response["data"]["currentUser"]["reviewRequestedMergeRequests"]["nodes"][0]["author"] = None
        merge_requests = gitlab_mrs.normalize(response["data"])
        self.assertEqual(merge_requests[0].author, "desconhecido")


class OfflineNoticeTests(unittest.TestCase):
    SNAPSHOT = {"summary": {"title": "T", "blocks": [{"type": "stat", "items": []}]}}
    SAVED_AT = datetime(2026, 10, 21, 15, 30).astimezone()

    def notice_text(self, now):
        blocks = gitlab_mrs.with_offline_notice(self.SNAPSHOT, self.SAVED_AT, now)["summary"]["blocks"]
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


class MainTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.state_dir = Path(self.temporary.name) / "estado"
        self.stdout = io.StringIO()
        self.stderr = io.StringIO()

    def run_main(self, http, arguments=("snapshot",), **environment_overrides):
        environment = base_environment(GITLAB_STATE_DIR=str(self.state_dir), **environment_overrides)
        return gitlab_mrs.main(
            list(arguments),
            environ=environment,
            http=http,
            now=NOW,
            stdout=self.stdout,
            stderr=self.stderr,
        )

    def test_main_end_to_end(self):
        code = self.run_main(FakeHttp([(200, fixtures.gitlab_response())]))
        self.assertEqual(code, 0)
        snapshot = json.loads(self.stdout.getvalue())
        self.assertEqual(snapshot["summary"]["title"], "GitLab")
        review_texts = [item["text"] for item in snapshot["detail"][0]["items"]]
        self.assertTrue(any("grupo/api !10" in text for text in review_texts))

    def test_main_usage_error_exits_two(self):
        self.assertEqual(self.run_main(FakeHttp(), arguments=("outra",)), 2)
        self.assertEqual(self.stderr.getvalue(), "uso: gitlab_mrs.py snapshot\n")

    def test_main_config_error_exits_one(self):
        code = self.run_main(FakeHttp(), GITLAB_TOKEN_ENV=None)
        self.assertEqual(code, 1)
        self.assertTrue(self.stderr.getvalue().startswith("erro: "))

    def test_main_auth_error_exits_one(self):
        code = self.run_main(FakeHttp([(401, {"message": SECRET})]))
        self.assertEqual(code, 1)
        self.assertEqual(self.stdout.getvalue(), "")
        self.assertTrue(self.stderr.getvalue().startswith("erro: "))
        self.assertNotIn(SECRET, self.stderr.getvalue())

    def test_main_network_error_serves_cache_with_offline_notice(self):
        self.assertEqual(self.run_main(FakeHttp([(200, fixtures.gitlab_response())])), 0)
        self.stdout = io.StringIO()
        self.assertEqual(self.run_main(FakeHttp(error=OSError("sem rede"))), 0)
        snapshot = json.loads(self.stdout.getvalue())
        self.assertIn("(offline)", snapshot["summary"]["blocks"][0]["lines"][0]["text"])

    def test_main_permanent_errors_are_not_masked_by_the_cache(self):
        rejected_query = (200, {"errors": [{"message": SECRET}], "data": None})
        for response in (rejected_query, (401, {"message": SECRET})):
            with self.subTest(status=response[0]):
                self.assertEqual(self.run_main(FakeHttp([(200, fixtures.gitlab_response())])), 0)
                self.stdout = io.StringIO()
                self.stderr = io.StringIO()
                self.assertEqual(self.run_main(FakeHttp([response])), 1)
                self.assertEqual(self.stdout.getvalue(), "")
                self.assertTrue(self.stderr.getvalue().startswith("erro: "))
                self.assertNotIn("offline", self.stderr.getvalue())
                self.assertNotIn(SECRET, self.stderr.getvalue())

    def test_main_output_never_contains_the_token(self):
        self.run_main(FakeHttp([(200, fixtures.gitlab_response())]))
        for path in self.state_dir.iterdir():
            self.assertNotIn(SECRET, path.read_text(encoding="utf-8"))
        self.assertNotIn(SECRET, self.stdout.getvalue())
        self.assertNotIn(SECRET, self.stderr.getvalue())

    def test_state_key_uses_token_source_not_value(self):
        environment = base_environment()
        config = gitlab_mrs.load_config(environment)
        key = gitlab_mrs.state_key(config, environment)
        self.assertNotIn(SECRET, key)
        self.assertIn("env:MEU_TOKEN", key)


if __name__ == "__main__":
    unittest.main()
