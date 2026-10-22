import base64
import io
import json
import sys
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "azure-devops"))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import azure_devops  # noqa: E402
import pull_request_fixtures as fixtures  # noqa: E402

SECRET = "TOKEN-SECRETO-123"
ORGANIZATION = "org"
VIEWER_ID = fixtures.AZURE_VIEWER_ID
REVIEW = azure_devops.REVIEW_SECTION
MINE = azure_devops.MINE_SECTION
NOW = datetime(2026, 10, 22, 12, 0, tzinfo=timezone.utc)


def base_environment(**overrides):
    environment = {
        "AZDO_ORGANIZATION": ORGANIZATION,
        "AZDO_TOKEN_ENV": "MEU_TOKEN",
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
    return azure_devops.AzureClient(azure_devops.load_config(base_environment(**overrides)), http)


def fetch_with_pages(review_page, mine_page=(), **overrides):
    http = FakeHttp([
        (200, fixtures.azure_connection_data()),
        (200, {"value": list(review_page)}),
        (200, {"value": list(mine_page)}),
    ])
    pulls = azure_devops.fetch_pull_requests(make_client(http, **overrides))
    return pulls, http


def fetch_fixture_pulls(**overrides):
    pulls, _ = fetch_with_pages(fixtures.azure_review_page(), fixtures.azure_mine_page(), **overrides)
    return {pull.item_id: pull for pull in pulls}


class ConfigTests(unittest.TestCase):
    def test_requires_organization(self):
        with self.assertRaises(azure_devops.ConfigError) as caught:
            azure_devops.load_config(base_environment(AZDO_ORGANIZATION=None))
        self.assertIn("AZDO_ORGANIZATION", str(caught.exception))

    def test_rejects_hostile_organization(self):
        for bad in ("a/b", "../x", "a?b"):
            with self.assertRaises(azure_devops.ConfigError, msg=bad) as caught:
                azure_devops.load_config(base_environment(AZDO_ORGANIZATION=bad))
            self.assertEqual(str(caught.exception),
                             "AZDO_ORGANIZATION precisa ser só o nome da organização")

    def test_defaults(self):
        config = azure_devops.load_config(base_environment())
        self.assertEqual(config.base_url, "https://dev.azure.com")
        self.assertEqual(config.organization, ORGANIZATION)
        self.assertEqual(config.projects, ())
        self.assertEqual(config.title, "Azure DevOps")
        self.assertEqual(config.token, SECRET)
        self.assertTrue(str(config.state_dir).endswith(".local/state/mosaic/azure-devops"))

    def test_projects_are_split_and_trimmed(self):
        config = azure_devops.load_config(base_environment(AZDO_PROJECTS=" Proj, ,Outro "))
        self.assertEqual(config.projects, ("Proj", "Outro"))

    def test_base_url_accepts_only_https_or_local_http(self):
        for bad in ("http://dev.azure.com", "ftp://x", "https://a b"):
            with self.assertRaises(azure_devops.ConfigError, msg=bad):
                azure_devops.load_config(base_environment(AZDO_BASE_URL=bad))
        local = azure_devops.load_config(base_environment(AZDO_BASE_URL="http://127.0.0.1:8080/"))
        self.assertEqual(local.base_url, "http://127.0.0.1:8080")

    def test_token_error_does_not_leak_the_value(self):
        with self.assertRaises(azure_devops.ConfigError) as caught:
            azure_devops.load_config(base_environment(MEU_TOKEN="a b"))
        self.assertNotIn("a b", str(caught.exception))


class ClientTests(unittest.TestCase):
    def test_basic_auth_uses_empty_user(self):
        http = FakeHttp([(200, fixtures.azure_connection_data())])
        make_client(http).get_json("/_apis/connectionData", {})
        call = http.calls[0]
        expected = "Basic " + base64.b64encode(f":{SECRET}".encode("utf-8")).decode("ascii")
        self.assertEqual(call["headers"]["Authorization"], expected)
        self.assertEqual(call["headers"]["Accept"], "application/json")
        self.assertEqual(call["method"], "GET")
        self.assertIsNone(call["body"])
        self.assertEqual(call["url"], "https://dev.azure.com/org/_apis/connectionData")

    def test_error_messages_never_contain_the_token(self):
        for status in (401, 403, 429, 500):
            body = json.dumps({"message": f"bad {SECRET}"}).encode()
            with self.assertRaises(azure_devops.ApiError, msg=status) as caught:
                make_client(FakeHttp([(status, body)])).get_json("/_apis/connectionData", {})
            self.assertNotIn(SECRET, str(caught.exception))

    def test_network_error_never_contains_the_token(self):
        with self.assertRaises(azure_devops.NetworkError) as caught:
            make_client(FakeHttp(error=OSError(f"falha {SECRET}"))).get_json("/x", {})
        self.assertNotIn(SECRET, str(caught.exception))

    def test_sign_in_page_and_redirect_are_auth_errors(self):
        for status, body in ((203, b"<html>entrar</html>"), (302, b"")):
            with self.assertRaises(azure_devops.AuthError, msg=status):
                make_client(FakeHttp([(status, body)])).get_json("/_apis/connectionData", {})

    def test_status_errors_map_to_exceptions(self):
        cases = [
            (429, azure_devops.RateLimitError),
            (400, azure_devops.RequestRejectedError),
            (404, azure_devops.RequestRejectedError),
            (410, azure_devops.RequestRejectedError),
            (500, azure_devops.UnexpectedResponseError),
        ]
        for status, error in cases:
            with self.assertRaises(error, msg=status):
                make_client(FakeHttp([(status, b"{}")])).get_json("/x", {})

    def test_permanent_rejections_have_a_fixed_message(self):
        for status in (400, 404, 410):
            body = json.dumps({"message": SECRET}).encode()
            with self.assertRaises(azure_devops.RequestRejectedError, msg=status) as caught:
                azure_devops.decode_response(status, body, "Azure DevOps")
            self.assertEqual(
                str(caught.exception),
                f"o Azure DevOps recusou a requisição (HTTP {status}); confira a configuração do widget",
            )

    def test_missing_organization_names_the_variable(self):
        body = json.dumps({"message": SECRET}).encode()
        with self.assertRaises(azure_devops.RequestRejectedError) as caught:
            make_client(FakeHttp([(404, body)])).get_json("/_apis/connectionData", {})
        self.assertEqual(
            str(caught.exception),
            "organização não encontrada no Azure DevOps; confira AZDO_ORGANIZATION",
        )

    def test_non_object_response_is_unexpected(self):
        with self.assertRaises(azure_devops.UnexpectedResponseError):
            make_client(FakeHttp([(200, [1, 2])])).get_json("/x", {})


class FetchTests(unittest.TestCase):
    def test_fetch_makes_three_calls_with_api_version(self):
        _, http = fetch_with_pages(fixtures.azure_review_page(), fixtures.azure_mine_page())
        self.assertEqual(len(http.calls), 3)
        self.assertIn("searchCriteria.reviewerId=me-1", http.calls[1]["url"])
        self.assertIn("api-version=7.1", http.calls[1]["url"])
        self.assertIn("searchCriteria.creatorId=me-1", http.calls[2]["url"])

    def test_review_section_and_statuses(self):
        pulls = fetch_fixture_pulls()
        review = {i: p for i, p in pulls.items() if p.section == REVIEW}
        self.assertEqual(set(review), {"Proj/api#10", "Proj/api#11", "Proj/api#12", "Outro/api#15"})
        expected = {
            "Proj/api#10": azure_devops.UNREVIEWED,
            "Proj/api#11": azure_devops.REVIEWED,
            "Proj/api#12": azure_devops.APPROVED,
            "Outro/api#15": azure_devops.UNREVIEWED,
        }
        for item_id, status in expected.items():
            self.assertEqual(review[item_id].review_status, status, item_id)
            self.assertIsNone(review[item_id].pushed_after_review, item_id)

    def test_projects_filter(self):
        pulls = fetch_fixture_pulls(AZDO_PROJECTS="Proj")
        self.assertNotIn("Outro/api#15", pulls)
        self.assertIn("Proj/api#10", pulls)

    def test_mine_states(self):
        pulls = fetch_fixture_pulls()
        expected = {
            "Proj/api#20": azure_devops.STATE_CONFLICT,
            "Proj/api#21": azure_devops.STATE_CHANGES,
            "Proj/api#22": azure_devops.STATE_APPROVED,
            "Proj/api#23": azure_devops.STATE_WAITING,
        }
        for item_id, state in expected.items():
            self.assertEqual(pulls[item_id].state, state, item_id)
            self.assertEqual(pulls[item_id].section, MINE, item_id)

    def test_url_and_target_branch(self):
        pull = fetch_fixture_pulls()["Proj/api#10"]
        self.assertEqual(pull.url, "https://dev.azure.com/org/Proj/_git/api/pullrequest/10")
        self.assertEqual(pull.target_branch, "main")
        self.assertEqual(pull.repository, "api")
        self.assertEqual(pull.reference, "#10")
        self.assertEqual(pull.head_commit, "c10")

    def test_drafts_and_own_pull_requests_are_dropped(self):
        pulls = fetch_fixture_pulls()
        self.assertNotIn("Proj/api#13", pulls)
        self.assertNotIn("Proj/api#14", pulls)

    def test_pull_request_without_creation_date_is_skipped(self):
        page = fixtures.azure_review_page()
        page[0]["creationDate"] = None
        pulls, _ = fetch_with_pages(page)
        ids = {pull.item_id for pull in pulls}
        self.assertNotIn("Proj/api#10", ids)
        self.assertIn("Proj/api#11", ids)

    def test_pull_request_without_author_uses_placeholder(self):
        page = fixtures.azure_review_page()
        page[0]["createdBy"] = None
        pulls, _ = fetch_with_pages(page)
        self.assertEqual(pulls[0].author, "desconhecido")

    def test_malformed_entries_are_skipped(self):
        page = fixtures.azure_review_page() + ["lixo", None]
        pulls, _ = fetch_with_pages(page)
        self.assertIn("Proj/api#10", {pull.item_id for pull in pulls})

    def test_seven_digit_fractions_from_azure_are_accepted(self):
        page = [fixtures.azure_pull_request(30, created="2026-10-10T08:00:00.1234567Z")]
        pulls, _ = fetch_with_pages(page)
        self.assertEqual([pull.item_id for pull in pulls], ["Proj/api#30"])

    def test_paginates_until_short_page(self):
        first = [fixtures.azure_pull_request(n) for n in range(100)]
        second = [fixtures.azure_pull_request(n) for n in range(100, 103)]
        http = FakeHttp([
            (200, fixtures.azure_connection_data()),
            (200, {"value": first}),
            (200, {"value": second}),
            (200, {"value": []}),
        ])
        pulls = azure_devops.fetch_pull_requests(make_client(http))
        self.assertEqual(len([pull for pull in pulls if pull.section == REVIEW]), 103)
        self.assertEqual(len(http.calls), 4)
        self.assertIn("$skip=100", http.calls[2]["url"])

    def test_stops_at_section_cap(self):
        first = [fixtures.azure_pull_request(n) for n in range(100)]
        second = [fixtures.azure_pull_request(n) for n in range(100, 200)]
        http = FakeHttp([
            (200, fixtures.azure_connection_data()),
            (200, {"value": first}),
            (200, {"value": second}),
            (200, {"value": []}),
        ])
        pulls = azure_devops.fetch_pull_requests(make_client(http))
        self.assertEqual(len([pull for pull in pulls if pull.section == REVIEW]), 200)
        self.assertEqual(len(http.calls), 4)
        self.assertNotIn("$skip=200", http.calls[-1]["url"])


class OfflineNoticeTests(unittest.TestCase):
    SNAPSHOT = {"summary": {"title": "T", "blocks": [{"type": "stat", "items": []}]}}
    SAVED_AT = datetime(2026, 10, 21, 15, 30).astimezone()

    def notice_text(self, now):
        blocks = azure_devops.with_offline_notice(self.SNAPSHOT, self.SAVED_AT, now)["summary"]["blocks"]
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
        environment = base_environment(AZDO_STATE_DIR=str(self.state_dir), **environment_overrides)
        return azure_devops.main(
            list(arguments),
            environ=environment,
            http=http,
            now=NOW,
            stdout=self.stdout,
            stderr=self.stderr,
        )

    def successful_http(self):
        return FakeHttp([
            (200, fixtures.azure_connection_data()),
            (200, {"value": fixtures.azure_review_page()}),
            (200, {"value": fixtures.azure_mine_page()}),
        ])

    def test_main_end_to_end(self):
        code = self.run_main(self.successful_http())
        self.assertEqual(code, 0)
        snapshot = json.loads(self.stdout.getvalue())
        self.assertEqual(snapshot["summary"]["title"], "Azure DevOps")
        review_ids = {item["id"] for item in snapshot["detail"][0]["items"]}
        self.assertNotIn("Proj/api#12", review_ids)
        self.assertIn("Proj/api#10", review_ids)

    def test_main_usage_error_exits_two(self):
        self.assertEqual(self.run_main(FakeHttp(), arguments=("outra",)), 2)
        self.assertEqual(self.stderr.getvalue(), "uso: azure_devops.py snapshot\n")

    def test_main_config_error_exits_one(self):
        code = self.run_main(FakeHttp(), AZDO_ORGANIZATION=None)
        self.assertEqual(code, 1)
        self.assertTrue(self.stderr.getvalue().startswith("erro: "))

    def test_main_auth_error_exits_one_without_cache(self):
        code = self.run_main(FakeHttp([(401, {"message": SECRET})]))
        self.assertEqual(code, 1)
        self.assertEqual(self.stdout.getvalue(), "")
        self.assertTrue(self.stderr.getvalue().startswith("erro: "))
        self.assertNotIn(SECRET, self.stderr.getvalue())

    def test_main_expired_token_with_cache_exits_one(self):
        for status, body in ((203, b"<html>entrar</html>"), (302, b"")):
            self.stdout = io.StringIO()
            self.stderr = io.StringIO()
            self.assertEqual(self.run_main(self.successful_http()), 0)
            self.stdout = io.StringIO()
            code = self.run_main(FakeHttp([(status, body)]))
            self.assertEqual(code, 1, status)
            self.assertEqual(self.stdout.getvalue(), "", status)
            self.assertTrue(self.stderr.getvalue().startswith("erro: "), status)

    def test_main_permanent_errors_are_not_masked_by_the_cache(self):
        for status in (404, 401):
            with self.subTest(status=status):
                self.stdout = io.StringIO()
                self.stderr = io.StringIO()
                self.assertEqual(self.run_main(self.successful_http()), 0)
                self.stdout = io.StringIO()
                self.assertEqual(self.run_main(FakeHttp([(status, {"message": SECRET})])), 1)
                self.assertEqual(self.stdout.getvalue(), "")
                self.assertTrue(self.stderr.getvalue().startswith("erro: "))
                self.assertNotIn("offline", self.stderr.getvalue())
                self.assertNotIn(SECRET, self.stderr.getvalue())

    def test_main_network_error_serves_cache_with_offline_notice(self):
        self.assertEqual(self.run_main(self.successful_http()), 0)
        self.stdout = io.StringIO()
        self.assertEqual(self.run_main(FakeHttp(error=OSError("sem rede"))), 0)
        snapshot = json.loads(self.stdout.getvalue())
        self.assertIn("(offline)", snapshot["summary"]["blocks"][0]["lines"][0]["text"])

    def test_main_output_never_contains_the_token(self):
        self.run_main(self.successful_http())
        for path in self.state_dir.iterdir():
            self.assertNotIn(SECRET, path.read_text(encoding="utf-8"))
        self.assertNotIn(SECRET, self.stdout.getvalue())
        self.assertNotIn(SECRET, self.stderr.getvalue())

    def test_state_key_uses_token_source_not_value(self):
        environment = base_environment()
        config = azure_devops.load_config(environment)
        key = azure_devops.state_key(config, environment)
        self.assertNotIn(SECRET, key)
        self.assertIn("env:MEU_TOKEN", key)


if __name__ == "__main__":
    unittest.main()
