import io
import json
import re
import sys
import tempfile
import types
import unittest
import urllib.parse
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "azure-pipelines"))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import azure_pipelines as pipelines  # noqa: E402
import pipeline_fixtures as fixtures  # noqa: E402

SECRET = "TOKEN-SECRETO-123"
NOW = fixtures.FIXTURE_NOW
UTC = timezone.utc


def base_environment(**overrides):
    environment = {"AZDO_ORGANIZATION": fixtures.ORGANIZATION, "AZDO_PROJECTS": "Alpha,Beta"}
    environment.update(overrides)
    return {key: value for key, value in environment.items() if value is not None}


class StaticTokenProvider:
    def __init__(self, token=SECRET):
        self.token = token

    def access_token(self):
        return self.token


def fake_azure_cli(token=SECRET):
    def run(command, **options):
        return types.SimpleNamespace(returncode=0, stdout=json.dumps({"accessToken": token}), stderr="")

    return run


class FakeHttp:
    def __init__(self, responses=None, error=None):
        self.responses = list(responses or [])
        self.error = error
        self.calls = []

    def __call__(self, url, method, headers, body, timeout):
        self.calls.append({"url": url, "method": method, "headers": headers, "body": body})
        if self.error is not None:
            raise self.error
        status, payload = self.responses.pop(0)
        if not isinstance(payload, bytes):
            payload = json.dumps(payload).encode("utf-8")
        return status, payload


class FixtureHttp:
    """Responde como a API de builds, a partir de pipeline_fixtures."""

    def __init__(self, missing_timelines=()):
        self.missing_timelines = set(missing_timelines)
        self.calls = []

    def __call__(self, url, method, headers, body, timeout):
        self.calls.append(url)
        path = urllib.parse.unquote(urllib.parse.urlparse(url).path)
        if path.endswith("/_apis/projects"):
            return self._ok(fixtures.raw_projects())
        builds = re.fullmatch(r"/org/([^/]+)/_apis/build/builds", path)
        if builds:
            raw = fixtures.builds_of(builds.group(1))
            return self._ok({"count": len(raw), "value": raw})
        timeline = re.fullmatch(r"/org/[^/]+/_apis/build/builds/(\d+)/timeline", path)
        if timeline and int(timeline.group(1)) not in self.missing_timelines:
            return self._ok(fixtures.raw_timeline(int(timeline.group(1))))
        return 404, b"{}"

    @staticmethod
    def _ok(payload):
        return 200, json.dumps(payload).encode("utf-8")

    def timeline_calls(self):
        return [call for call in self.calls if "/timeline" in call]


def make_config(**overrides):
    return pipelines.load_config(base_environment(**overrides))


def make_client(http, config=None, token_provider=None):
    return pipelines.AzureClient(config or make_config(), http, token_provider or StaticTokenProvider())


def build_run(build_id, **overrides):
    spec = next(spec for spec in fixtures.BUILD_SPECS if spec[0] == build_id)
    run = pipelines.parse_build_run(fixtures.raw_build(*spec), spec[1])
    return pipelines.replace(run, **overrides) if overrides else run


def all_runs():
    return [build_run(spec[0]) for spec in fixtures.BUILD_SPECS if spec[6] != "canceled"]


def step(name, seconds):
    return pipelines.StepDuration(name, seconds)


def statistic(**overrides):
    values = dict(
        project="Alpha",
        pipeline_id=1,
        pipeline_name="Build API",
        run_count=3,
        average_seconds=1200.0,
        maximum_seconds=1500.0,
        average_queue_seconds=60.0,
        maximum_queue_seconds=90.0,
        failure_rate=0.0,
        url="https://dev.azure.com/org/Alpha/_build/results?buildId=103",
    )
    values.update(overrides)
    return pipelines.PipelineStatistic(**values)


class ConfigTests(unittest.TestCase):
    def test_defaults(self):
        config = pipelines.load_config({"AZDO_ORGANIZATION": "org"})
        self.assertEqual(config.base_url, "https://dev.azure.com")
        self.assertEqual(config.organization, "org")
        self.assertEqual(config.projects, ())
        self.assertEqual((config.days, config.top, config.sample), (14, 5, 10))
        self.assertEqual(config.title, "Pipelines lentas")
        self.assertEqual(config.auth.method, "azure-cli")
        self.assertTrue(str(config.state_dir).endswith(".local/state/mosaic/azure-pipelines"))

    def test_requires_organization(self):
        with self.assertRaises(pipelines.ConfigError) as caught:
            pipelines.load_config({})
        self.assertIn("AZDO_ORGANIZATION", str(caught.exception))

    def test_rejects_hostile_organization(self):
        for bad in ("a/b", "../x", "a?b"):
            with self.assertRaises(pipelines.ConfigError, msg=bad):
                make_config(AZDO_ORGANIZATION=bad)

    def test_numeric_variables_are_bounded(self):
        cases = (
            ("AZDO_PIPELINE_DAYS", "0"),
            ("AZDO_PIPELINE_DAYS", "91"),
            ("AZDO_PIPELINE_DAYS", "abc"),
            ("AZDO_PIPELINE_TOP", "11"),
            ("AZDO_PIPELINE_TOP", "0"),
            ("AZDO_PIPELINE_SAMPLE", "21"),
            ("AZDO_PIPELINE_SAMPLE", "-1"),
        )
        for variable, value in cases:
            with self.subTest(variable=variable, value=value):
                with self.assertRaises(pipelines.ConfigError) as caught:
                    make_config(**{variable: value})
                self.assertIn(variable, str(caught.exception))
        config = make_config(AZDO_PIPELINE_DAYS="90", AZDO_PIPELINE_TOP="10", AZDO_PIPELINE_SAMPLE="1")
        self.assertEqual((config.days, config.top, config.sample), (90, 10, 1))

    def test_projects_are_split_and_trimmed(self):
        config = make_config(AZDO_PROJECTS=" Alpha, ,Beta ")
        self.assertEqual(config.projects, ("Alpha", "Beta"))

    def test_pat_variables_are_rejected(self):
        with self.assertRaises(pipelines.ConfigError) as caught:
            make_config(AZDO_TOKEN_ENV="MEU_TOKEN")
        self.assertIn("AZDO_AUTH_METHOD", str(caught.exception))

    def test_base_url_accepts_only_https_or_local_http(self):
        for bad in ("http://dev.azure.com", "ftp://x"):
            with self.assertRaises(pipelines.ConfigError, msg=bad):
                make_config(AZDO_BASE_URL=bad)


class ClientTests(unittest.TestCase):
    def test_requests_use_bearer_token(self):
        http = FakeHttp([(200, {})])
        make_client(http, token_provider=StaticTokenProvider("tok")).get_json("/_apis/projects", {})
        call = http.calls[0]
        self.assertEqual(call["headers"]["Authorization"], "Bearer tok")
        self.assertEqual(call["method"], "GET")
        self.assertEqual(call["url"], "https://dev.azure.com/org/_apis/projects")

    def test_identity_not_in_organization_message(self):
        body = b'{"message": "TF401444: Please sign-in at least once"}'
        with self.assertRaises(pipelines.AuthError) as caught:
            make_client(FakeHttp([(401, body)])).get_json("/x", {})
        self.assertIn("não foi adicionada à organização", str(caught.exception))

    def test_error_messages_never_contain_the_token(self):
        for status in (401, 403, 429, 500):
            body = json.dumps({"message": f"bad {SECRET}"}).encode()
            with self.assertRaises(pipelines.ApiError, msg=status) as caught:
                make_client(FakeHttp([(status, body)])).get_json("/x", {})
            self.assertNotIn(SECRET, str(caught.exception))

    def test_sign_in_page_and_redirect_are_auth_errors(self):
        for status, body in ((203, b"<html>entrar</html>"), (302, b"")):
            with self.assertRaises(pipelines.AuthError, msg=status):
                make_client(FakeHttp([(status, body)])).get_json("/x", {})

    def test_status_errors_map_to_exceptions(self):
        cases = [
            (429, pipelines.RateLimitError),
            (400, pipelines.RequestRejectedError),
            (404, pipelines.RequestRejectedError),
            (500, pipelines.UnexpectedResponseError),
        ]
        for status, error in cases:
            with self.assertRaises(error, msg=status):
                make_client(FakeHttp([(status, b"{}")])).get_json("/x", {})

    def test_network_error_never_contains_the_token(self):
        with self.assertRaises(pipelines.NetworkError) as caught:
            make_client(FakeHttp(error=OSError(f"falha {SECRET}"))).get_json("/x", {})
        self.assertNotIn(SECRET, str(caught.exception))

    def test_missing_organization_names_the_variable(self):
        with self.assertRaises(pipelines.RequestRejectedError) as caught:
            make_client(FakeHttp([(404, b"{}")])).get_json("/_apis/projects", {})
        self.assertIn("AZDO_ORGANIZATION", str(caught.exception))


    def test_missing_project_names_the_variables(self):
        with self.assertRaises(pipelines.RequestRejectedError) as caught:
            pipelines.fetch_runs(make_client(FakeHttp([(404, b"{}")])), "Alpha", NOW)
        self.assertIn("AZDO_PROJECTS", str(caught.exception))


class CollectionTests(unittest.TestCase):
    def test_parse_build_run_requires_dates_and_definition(self):
        spec = fixtures.BUILD_SPECS[0]
        raw = fixtures.raw_build(*spec)
        run = pipelines.parse_build_run(raw, "Alpha")
        self.assertEqual((run.build_id, run.pipeline_id, run.pipeline_name), (101, 1, "Build API"))
        self.assertEqual((run.finished_at - run.started_at).total_seconds(), 1200)
        self.assertEqual((run.started_at - run.queued_at).total_seconds(), 60)
        self.assertIn("buildId=101", run.url)
        for broken in (
            {**raw, "id": None},
            {**raw, "definition": None},
            {**raw, "definition": {"name": "x"}},
            {**raw, "startTime": None},
            {**raw, "finishTime": "ontem"},
            {**raw, "queueTime": None},
            "texto",
            None,
        ):
            with self.subTest(broken=broken):
                self.assertIsNone(pipelines.parse_build_run(broken, "Alpha"))

    def test_canceled_runs_are_ignored(self):
        canceled = fixtures.raw_build(*fixtures.BUILD_SPECS[3])
        self.assertIsNone(pipelines.parse_build_run(canceled, "Alpha"))

    def test_fetch_projects_uses_configured_names_without_a_call(self):
        http = FakeHttp()
        names = pipelines.fetch_projects(make_client(http), make_config())
        self.assertEqual(names, ["Alpha", "Beta"])
        self.assertEqual(http.calls, [])

    def test_fetch_projects_lists_organization_projects(self):
        config = make_config(AZDO_PROJECTS=None)
        http = FixtureHttp()
        self.assertEqual(pipelines.fetch_projects(make_client(http, config), config), ["Alpha", "Beta"])
        query = urllib.parse.parse_qs(urllib.parse.urlparse(http.calls[0]).query)
        self.assertEqual(query["api-version"], ["7.1"])
        self.assertEqual(query["stateFilter"], ["wellFormed"])

    def test_fetch_runs_queries_completed_builds_in_the_window(self):
        http = FixtureHttp()
        since = NOW - timedelta(days=14)
        runs = pipelines.fetch_runs(make_client(http), "Alpha", since)
        self.assertEqual(sorted(run.build_id for run in runs), [101, 102, 103, 201, 202])
        query = urllib.parse.parse_qs(urllib.parse.urlparse(http.calls[0]).query)
        self.assertEqual(query["statusFilter"], ["completed"])
        self.assertEqual(query["queryOrder"], ["finishTimeDescending"])
        self.assertEqual(query["$top"], ["1000"])
        self.assertEqual(query["api-version"], ["7.1"])
        self.assertEqual(query["minTime"], ["2026-10-08T12:00:00Z"])

    def test_project_names_are_quoted_in_paths(self):
        http = FakeHttp([(200, {"value": []}), (200, {"records": []})])
        client = make_client(http)
        pipelines.fetch_runs(client, "Meu Projeto", NOW)
        pipelines.fetch_step_durations(client, build_run(101, project="Meu Projeto"))
        self.assertIn("/org/Meu%20Projeto/_apis/build/builds?", http.calls[0]["url"])
        self.assertIn("/org/Meu%20Projeto/_apis/build/builds/101/timeline?", http.calls[1]["url"])

    def test_malformed_builds_are_skipped(self):
        payload = {"value": [fixtures.raw_build(*fixtures.BUILD_SPECS[0]), "lixo", {"id": 1}]}
        runs = pipelines.fetch_runs(make_client(FakeHttp([(200, payload)])), "Alpha", NOW)
        self.assertEqual([run.build_id for run in runs], [101])

    def test_non_list_value_is_unexpected(self):
        with self.assertRaises(pipelines.UnexpectedResponseError):
            pipelines.fetch_runs(make_client(FakeHttp([(200, {"value": "x"})])), "Alpha", NOW)

    def test_parse_timeline_names_steps_with_job(self):
        steps = pipelines.parse_timeline(fixtures.raw_timeline(101))
        self.assertEqual(
            [(item.name, item.seconds) for item in steps],
            [("Compile › dotnet build", 600), ("Compile › Restore", 100), ("Test › Unit tests", 400)],
        )

    def test_records_without_dates_are_skipped(self):
        steps = pipelines.parse_timeline(fixtures.raw_timeline(301))
        self.assertEqual([item.name for item in steps], ["Quality › Run lint"])

    def test_task_without_job_uses_only_its_name(self):
        started = datetime(2026, 10, 20, 8, 0, tzinfo=UTC)
        records = [fixtures.timeline_record("t", None, "Task", "Solta", started, started + timedelta(seconds=7))]
        steps = pipelines.parse_timeline({"records": records})
        self.assertEqual([(item.name, item.seconds) for item in steps], [("Solta", 7)])

    def test_parent_cycle_does_not_loop(self):
        started = datetime(2026, 10, 20, 8, 0, tzinfo=UTC)
        finished = started + timedelta(seconds=5)
        records = [
            fixtures.timeline_record("a", "b", "Phase", "A", started, finished),
            fixtures.timeline_record("b", "a", "Phase", "B", started, finished),
            fixtures.timeline_record("t", "a", "Task", "Presa", started, finished),
        ]
        steps = pipelines.parse_timeline({"records": records})
        self.assertEqual([item.name for item in steps], ["Presa"])

    def test_negative_durations_are_skipped(self):
        started = datetime(2026, 10, 20, 8, 0, tzinfo=UTC)
        records = [fixtures.timeline_record("t", None, "Task", "Volta", started, started - timedelta(seconds=3))]
        self.assertEqual(pipelines.parse_timeline({"records": records}), [])

    def test_timeline_with_wrong_shape_is_empty(self):
        for payload in ({}, {"records": "x"}, {"records": [None, 3]}):
            with self.subTest(payload=payload):
                self.assertEqual(pipelines.parse_timeline(payload), [])


class AggregationTests(unittest.TestCase):
    def test_rank_orders_by_average_duration_and_cuts_top(self):
        ranked = pipelines.rank_pipelines(all_runs(), 2)
        self.assertEqual([item.pipeline_name for item in ranked], ["Build API", "Deploy"])
        top = ranked[0]
        self.assertEqual((top.run_count, top.average_seconds, top.maximum_seconds), (3, 1200, 1500))
        self.assertEqual(top.project, "Alpha")

    def test_rank_tie_breaks_by_name(self):
        first = build_run(201)
        tied = pipelines.replace(build_run(301), pipeline_name="Aardvark", finished_at=build_run(301).started_at + timedelta(seconds=480))
        ranked = pipelines.rank_pipelines([first, tied], 5)
        self.assertEqual([item.pipeline_name for item in ranked], ["Aardvark", "Deploy"])

    def test_negative_queue_time_counts_as_zero(self):
        run = build_run(101)
        skewed = pipelines.replace(run, queued_at=run.started_at + timedelta(seconds=30))
        self.assertEqual(pipelines.rank_pipelines([skewed], 1)[0].average_queue_seconds, 0)

    def test_queue_statistics(self):
        top = pipelines.rank_pipelines(all_runs(), 1)[0]
        self.assertEqual((top.average_queue_seconds, top.maximum_queue_seconds), (60, 90))

    def test_failure_rate(self):
        ranked = {item.pipeline_name: item for item in pipelines.rank_pipelines(all_runs(), 5)}
        self.assertAlmostEqual(ranked["Build API"].failure_rate, 1 / 3)
        self.assertEqual(ranked["Deploy"].failure_rate, 0)

    def test_pipelines_with_the_same_id_in_different_projects_stay_apart(self):
        other = pipelines.replace(build_run(301), project="Gamma", pipeline_id=1, pipeline_name="Build API")
        ranked = pipelines.rank_pipelines([build_run(101), other], 5)
        self.assertEqual(sorted(item.project for item in ranked), ["Alpha", "Gamma"])

    def test_summarize_counts_everything_not_only_the_top(self):
        overview = pipelines.summarize(all_runs(), 1)
        self.assertEqual((overview.pipeline_count, overview.run_count), (3, 6))
        self.assertEqual(len(overview.ranked), 1)

    def test_select_sample_takes_most_recent(self):
        runs = [build_run(101), build_run(102), build_run(103)]
        sample = pipelines.select_sample(runs, 2)
        self.assertEqual([run.build_id for run in sample], [103, 102])

    def test_aggregate_steps_average_over_sampled_runs(self):
        durations = {
            1: [step("Compile › build", 600), step("Test › unit", 100)],
            2: [step("Compile › build", 200)],
        }
        result = pipelines.aggregate_steps(statistic(average_queue_seconds=0.0), durations)
        by_name = {item.name: item for item in result.steps}
        self.assertEqual(by_name["Compile › build"].average_seconds, 400)
        self.assertEqual(by_name["Compile › build"].maximum_seconds, 600)
        self.assertEqual(by_name["Test › unit"].average_seconds, 50)
        self.assertEqual(by_name["Test › unit"].maximum_seconds, 100)
        self.assertEqual(result.sampled_runs, 2)

    def test_same_step_name_twice_in_a_run_is_summed(self):
        durations = {1: [step("Job › retry", 30), step("Job › retry", 30)]}
        result = pipelines.aggregate_steps(statistic(average_queue_seconds=0.0), durations)
        self.assertEqual(result.steps[0].average_seconds, 60)

    def test_steps_are_sorted_by_average_and_cut(self):
        durations = {1: [step(f"Job › {number}", number) for number in range(1, 16)]}
        result = pipelines.aggregate_steps(statistic(average_queue_seconds=0.0), durations)
        self.assertEqual(len(result.steps), pipelines.STEPS_PER_PIPELINE)
        self.assertEqual(result.steps[0].name, "Job › 15")

    def test_queue_wait_appears_as_a_step(self):
        durations = {1: [step("Job › build", 1000)]}
        result = pipelines.aggregate_steps(statistic(average_queue_seconds=60.0, maximum_queue_seconds=90.0), durations)
        queue = next(item for item in result.steps if item.name == pipelines.QUEUE_STEP_NAME)
        self.assertEqual((queue.average_seconds, queue.maximum_seconds), (60, 90))
        self.assertEqual(result.steps[-1].name, pipelines.QUEUE_STEP_NAME)

    def test_share_is_relative_to_queue_plus_run(self):
        durations = {1: [step("Job › build", 630)]}
        result = pipelines.aggregate_steps(statistic(average_seconds=1200.0, average_queue_seconds=60.0), durations)
        self.assertAlmostEqual(result.steps[0].share, 630 / 1260)

    def test_share_with_zero_total_is_zero(self):
        durations = {1: [step("Job › build", 10)]}
        result = pipelines.aggregate_steps(statistic(average_seconds=0.0, average_queue_seconds=0.0), durations)
        self.assertEqual(result.steps[0].share, 0)

    def test_aggregate_without_durations_has_no_steps(self):
        result = pipelines.aggregate_steps(statistic(average_queue_seconds=0.0), {})
        self.assertEqual((result.steps, result.sampled_runs), ((), 0))


class CollectOverviewTests(unittest.TestCase):
    def collect(self, http=None, cache=None, **environment):
        http = http or FixtureHttp()
        config = make_config(**environment)
        overview, timelines = pipelines.collect_overview(make_client(http, config), config, NOW, cache or {})
        return overview, timelines, http

    def test_collects_ranking_and_steps(self):
        overview, timelines, _ = self.collect()
        self.assertEqual((overview.pipeline_count, overview.run_count), (3, 6))
        self.assertEqual([item.pipeline_name for item in overview.ranked], ["Build API", "Deploy", "Lint"])
        top = overview.ranked[0]
        self.assertEqual(top.sampled_runs, 3)
        self.assertFalse(top.partial)
        self.assertEqual(top.steps[0].name, "Compile › dotnet build")
        self.assertEqual(top.steps[0].average_seconds, 600)
        self.assertEqual(set(timelines), {"101", "102", "103", "201", "202", "301"})

    def test_top_limits_how_many_pipelines_get_steps(self):
        overview, _, http = self.collect(AZDO_PIPELINE_TOP="1")
        self.assertEqual(len(overview.ranked), 1)
        self.assertEqual(len(http.timeline_calls()), 3)

    def test_sample_limits_how_many_timelines_are_read(self):
        overview, _, http = self.collect(AZDO_PIPELINE_SAMPLE="2", AZDO_PIPELINE_TOP="1")
        self.assertEqual(overview.ranked[0].sampled_runs, 2)
        self.assertFalse(overview.ranked[0].partial)
        self.assertEqual(len(http.timeline_calls()), 2)

    def test_cached_timelines_are_not_requested_again(self):
        _, timelines, _ = self.collect()
        _, _, second_http = self.collect(cache=timelines)
        self.assertEqual(second_http.timeline_calls(), [])

    def test_timeline_request_cap_marks_pipeline_partial(self):
        with mock.patch.object(pipelines, "MAXIMUM_TIMELINE_REQUESTS_PER_RUN", 2):
            overview, _, http = self.collect()
        self.assertEqual(len(http.timeline_calls()), 2)
        ranked = {item.pipeline_name: item for item in overview.ranked}
        self.assertTrue(ranked["Build API"].partial)
        self.assertEqual(ranked["Build API"].sampled_runs, 2)
        self.assertTrue(ranked["Deploy"].partial)
        self.assertEqual(ranked["Deploy"].sampled_runs, 0)
        self.assertEqual(ranked["Deploy"].steps[0].name if ranked["Deploy"].steps else "", "")

    def test_timeline_cache_is_bounded_to_the_highest_build_ids(self):
        with mock.patch.object(pipelines, "MAXIMUM_CACHED_TIMELINES", 2):
            _, timelines, _ = self.collect()
        self.assertEqual(set(timelines), {"301", "202"})

    def test_failed_timeline_skips_only_that_build(self):
        overview, timelines, _ = self.collect(http=FixtureHttp(missing_timelines={102}))
        top = overview.ranked[0]
        self.assertEqual(top.sampled_runs, 2)
        self.assertTrue(top.partial)
        self.assertEqual(top.steps[0].average_seconds, 450)
        self.assertNotIn("102", timelines)

    def test_auth_failure_while_reading_a_timeline_propagates(self):
        class ForbiddenTimelines(FixtureHttp):
            def __call__(self, url, method, headers, body, timeout):
                if "/timeline" in url:
                    return 401, b"{}"
                return super().__call__(url, method, headers, body, timeout)

        with self.assertRaises(pipelines.AuthError):
            self.collect(http=ForbiddenTimelines())

    def test_malformed_cache_entries_are_ignored(self):
        cache = {"101": "lixo", "102": [["ok", "x"]], "103": [["Compile › dotnet build", 300]]}
        overview, timelines, http = self.collect(cache=cache)
        self.assertEqual(len(http.timeline_calls()), 5)
        self.assertEqual(timelines["103"], [["Compile › dotnet build", 300]])
        self.assertEqual(overview.ranked[0].sampled_runs, 3)

    def test_empty_window(self):
        class NoBuilds(FixtureHttp):
            def __call__(self, url, method, headers, body, timeout):
                if urllib.parse.urlparse(url).path.endswith("/_apis/build/builds"):
                    self.calls.append(url)
                    return 200, b'{"count": 0, "value": []}'
                return super().__call__(url, method, headers, body, timeout)

        overview, timelines, _ = self.collect(http=NoBuilds())
        self.assertEqual((overview.pipeline_count, overview.run_count, overview.ranked), (0, 0, ()))
        self.assertEqual(timelines, {})


class DurationFormatTests(unittest.TestCase):
    def test_format_duration(self):
        cases = ((0, "0s"), (45, "45s"), (59.6, "1m00s"), (185, "3m05s"), (3599, "59m59s"),
                 (3600, "1h00m"), (4320, "1h12m"))
        for seconds, expected in cases:
            with self.subTest(seconds=seconds):
                self.assertEqual(pipelines.format_duration(seconds), expected)


def snapshot_for(overview, **environment):
    return pipelines.build_snapshot(overview, make_config(**environment))


def collected_snapshot(**environment):
    http = FixtureHttp()
    config = make_config(**environment)
    overview, _ = pipelines.collect_overview(make_client(http, config), config, NOW, {})
    return pipelines.build_snapshot(overview, config)


class SnapshotTests(unittest.TestCase):
    def test_snapshot_home_lists_top_pipelines_with_duration_and_bottleneck(self):
        snapshot = collected_snapshot()
        self.assertEqual(snapshot["protocol"], 1)
        self.assertEqual(snapshot["summary"]["title"], "Pipelines lentas")
        stat, rows = snapshot["summary"]["blocks"]
        self.assertEqual(
            [(item["label"], item["value"]) for item in stat["items"]],
            [("pipelines", "3"), ("execuções", "6"), ("janela", "14 dias")],
        )
        first, second, third = rows["rows"]
        self.assertEqual((first["text"], first["time"], first["tone"]), ("Alpha/Build API", "20m00s", "danger"))
        self.assertEqual([tag["text"] for tag in first["tags"]], ["3 execuções", "Compile › dotnet build", "33% falhas"])
        self.assertEqual([tag["text"] for tag in second["tags"]], ["2 execuções", "Release › Publish"])
        self.assertEqual((second["tone"], third["tone"]), ("normal", "normal"))
        self.assertEqual(third["text"], "Beta/Lint")

    def test_bottleneck_ignores_the_queue_wait(self):
        overview = pipelines.Overview(1, 3, (pipelines.aggregate_steps(
            statistic(average_queue_seconds=5000.0, maximum_queue_seconds=6000.0),
            {1: [step("Job › build", 100)]},
        ),))
        row = snapshot_for(overview)["summary"]["blocks"][1]["rows"][0]
        self.assertIn("Job › build", [tag["text"] for tag in row["tags"]])
        self.assertNotIn(pipelines.QUEUE_STEP_NAME, [tag["text"] for tag in row["tags"]])

    def test_snapshot_detail_has_one_section_per_pipeline_with_steps_table(self):
        detail = collected_snapshot()["detail"]
        self.assertEqual([section["heading"] for section in detail], ["Alpha/Build API", "Alpha/Deploy", "Beta/Lint"])
        stat, table = detail[0]["blocks"]
        self.assertEqual(
            [(item["label"], item["value"]) for item in stat["items"]],
            [("média", "20m00s"), ("máximo", "25m00s"), ("fila média", "1m00s"), ("falhas", "33%")],
        )
        self.assertEqual(stat["hint"], "amostra de 3 execuções")
        self.assertEqual([column["title"] for column in table["columns"]], ["Passo", "Média", "% do total", "Máximo"])
        self.assertEqual([column.get("align") for column in table["columns"]], [None, "right", "right", "right"])
        names = [row[0]["text"] if isinstance(row[0], dict) else row[0] for row in table["rows"]]
        self.assertEqual(
            names,
            ["Compile › dotnet build", "Test › Unit tests", "Compile › Restore", pipelines.QUEUE_STEP_NAME],
        )
        average, share, maximum = table["rows"][0][1:]
        self.assertEqual(
            [cell["text"] for cell in (average, share, maximum)], ["10m00s", "48%", "15m00s"]
        )
        item = detail[0]["items"][0]
        self.assertEqual(item["id"], "Alpha:1")
        self.assertIn("buildId=103", item["text"])
        self.assertEqual(item["meta"]["gargalo"], "Compile › dotnet build")

    def test_bottleneck_row_has_attention_flag(self):
        table = collected_snapshot()["detail"][0]["blocks"][1]
        flagged = [row for row in table["rows"] if isinstance(row[0], dict)]
        self.assertEqual(len(flagged), 1)
        self.assertEqual(flagged[0][0], {"text": "Compile › dotnet build", "tone": "warn", "flag": "attention"})
        self.assertTrue(all(isinstance(cell, dict) and cell["flag"] == "attention" for cell in flagged[0]))

    def test_detail_headings_are_stable(self):
        slower = statistic(average_seconds=1300.0, run_count=9)
        faster = statistic(average_seconds=100.0, run_count=2)
        headings = [
            snapshot_for(pipelines.Overview(1, item.run_count, (item,)))["detail"][0]["heading"]
            for item in (slower, faster)
        ]
        self.assertEqual(headings[0], headings[1])

    def test_partial_sample_is_flagged_in_the_hint(self):
        item = pipelines.replace(statistic(), sampled_runs=2, partial=True)
        stat = snapshot_for(pipelines.Overview(1, 3, (item,)))["detail"][0]["blocks"][0]
        self.assertEqual(stat["hint"], "amostra de 2 execuções (parcial)")

    def test_pipeline_without_steps_still_has_a_detail_section(self):
        item = pipelines.replace(statistic(), partial=True)
        detail = snapshot_for(pipelines.Overview(1, 3, (item,)))["detail"]
        self.assertEqual(len(detail), 1)
        self.assertEqual(detail[0]["blocks"][0]["hint"], "amostra de 0 execuções (parcial)")
        self.assertEqual(len(detail[0]["blocks"]), 2)

    def test_empty_window_shows_muted_line(self):
        snapshot = snapshot_for(pipelines.Overview(0, 0, ()))
        self.assertEqual(
            snapshot["summary"]["blocks"],
            [{"type": "lines", "lines": [
                {"text": "nenhuma execução concluída nos últimos 14 dias", "tone": "muted"}
            ]}],
        )
        self.assertNotIn("detail", snapshot)
        self.assertNotIn("mini", snapshot)

    def test_singular_window_text(self):
        stat = snapshot_for(pipelines.Overview(1, 1, (statistic(),)), AZDO_PIPELINE_DAYS="1")["summary"]["blocks"][0]
        self.assertEqual(stat["items"][2]["value"], "1 dia")

    def test_mini_shows_the_slowest_pipeline(self):
        mini = collected_snapshot()["mini"]
        self.assertEqual(mini, {"value": "20m00s", "text": "pipeline mais lenta", "tone": "warn"})

    def test_mini_value_fits_protocol_limit(self):
        huge = statistic(average_seconds=10.0**8)
        mini = snapshot_for(pipelines.Overview(1, 1, (huge,)))["mini"]
        self.assertLessEqual(len(mini["value"]), 8)

    def test_texts_are_cut_at_limit(self):
        item = statistic(pipeline_name="N" * 600)
        snapshot = snapshot_for(pipelines.Overview(1, 1, (item,)))
        row = snapshot["summary"]["blocks"][1]["rows"][0]
        self.assertLessEqual(len(row["text"]), 80)
        self.assertTrue(row["text"].endswith("…"))
        self.assertLessEqual(len(snapshot["detail"][0]["heading"]), 80)

    def test_long_step_names_are_cut_in_the_tag(self):
        item = pipelines.aggregate_steps(statistic(average_queue_seconds=0.0), {1: [step("J › " + "x" * 200, 50)]})
        row = snapshot_for(pipelines.Overview(1, 1, (item,)))["summary"]["blocks"][1]["rows"][0]
        self.assertTrue(all(len(tag["text"]) <= 30 for tag in row["tags"]))

    def test_snapshot_respects_protocol_row_limit(self):
        items = tuple(statistic(pipeline_id=number, pipeline_name=f"P{number}") for number in range(20))
        snapshot = snapshot_for(pipelines.Overview(20, 60, items))
        self.assertLessEqual(len(snapshot["summary"]["blocks"][1]["rows"]), 200)
        self.assertLessEqual(len(snapshot["summary"]["blocks"]), 20)

    def test_offline_notice_is_the_first_block(self):
        snapshot = collected_snapshot()
        marked = pipelines.with_offline_notice(snapshot, NOW - timedelta(hours=1), NOW)
        first = marked["summary"]["blocks"][0]["lines"][0]["text"]
        self.assertTrue(first.startswith("dados de ") and first.endswith("(offline)"))
        self.assertEqual(len(marked["summary"]["blocks"]), len(snapshot["summary"]["blocks"]) + 1)


class MainTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.state_dir = Path(self.temporary.name) / "estado"
        self.stdout = io.StringIO()
        self.stderr = io.StringIO()

    def run_main(self, http, arguments=("snapshot",), **environment_overrides):
        environment = base_environment(AZDO_STATE_DIR=str(self.state_dir), **environment_overrides)
        return pipelines.main(
            list(arguments),
            environ=environment,
            http=http,
            run_command=fake_azure_cli(),
            now=NOW,
            stdout=self.stdout,
            stderr=self.stderr,
        )

    def reset_streams(self):
        self.stdout = io.StringIO()
        self.stderr = io.StringIO()

    def test_main_end_to_end(self):
        self.assertEqual(self.run_main(FixtureHttp()), 0, self.stderr.getvalue())
        snapshot = json.loads(self.stdout.getvalue())
        self.assertEqual(snapshot["summary"]["title"], "Pipelines lentas")
        self.assertEqual(len(snapshot["detail"]), 3)

    def test_main_usage_error_exits_two(self):
        self.assertEqual(self.run_main(FakeHttp(), arguments=("outra",)), 2)
        self.assertEqual(self.stderr.getvalue(), "uso: azure_pipelines.py snapshot|login\n")

    def test_main_config_error_exits_one(self):
        self.assertEqual(self.run_main(FakeHttp(), AZDO_ORGANIZATION=None), 1)
        self.assertTrue(self.stderr.getvalue().startswith("erro: "))

    def test_main_auth_error_exits_one_without_cache(self):
        code = self.run_main(FakeHttp([(401, {"message": SECRET})]))
        self.assertEqual(code, 1)
        self.assertEqual(self.stdout.getvalue(), "")
        self.assertTrue(self.stderr.getvalue().startswith("erro: "))
        self.assertNotIn(SECRET, self.stderr.getvalue())

    def test_main_permanent_errors_are_not_masked_by_the_cache(self):
        for status in (404, 401):
            with self.subTest(status=status):
                self.reset_streams()
                self.assertEqual(self.run_main(FixtureHttp()), 0)
                self.reset_streams()
                self.assertEqual(self.run_main(FakeHttp([(status, {"message": SECRET})])), 1)
                self.assertEqual(self.stdout.getvalue(), "")
                self.assertNotIn("offline", self.stderr.getvalue())
                self.assertNotIn(SECRET, self.stderr.getvalue())

    def test_main_network_error_serves_cache_with_offline_notice(self):
        self.assertEqual(self.run_main(FixtureHttp()), 0)
        self.reset_streams()
        self.assertEqual(self.run_main(FakeHttp(error=OSError("sem rede"))), 0)
        snapshot = json.loads(self.stdout.getvalue())
        self.assertIn("(offline)", snapshot["summary"]["blocks"][0]["lines"][0]["text"])

    def test_main_output_never_contains_the_token(self):
        self.run_main(FixtureHttp())
        for path in self.state_dir.iterdir():
            self.assertNotIn(SECRET, path.read_text(encoding="utf-8"))
        self.assertNotIn(SECRET, self.stdout.getvalue() + self.stderr.getvalue())

    def test_second_run_reads_timelines_from_the_state(self):
        first = FixtureHttp()
        self.assertEqual(self.run_main(first), 0)
        self.reset_streams()
        second = FixtureHttp()
        self.assertEqual(self.run_main(second), 0)
        self.assertEqual(len(first.timeline_calls()), 6)
        self.assertEqual(second.timeline_calls(), [])

    def test_corrupt_state_file_is_treated_as_empty(self):
        self.assertEqual(self.run_main(FixtureHttp()), 0)
        for path in self.state_dir.iterdir():
            path.write_text("{ não é json", encoding="utf-8")
        self.reset_streams()
        self.assertEqual(self.run_main(FixtureHttp()), 0, self.stderr.getvalue())

    def test_state_key_uses_identity_and_settings_not_secret(self):
        environment = base_environment(
            AZDO_AUTH_METHOD="service-principal",
            AZDO_TENANT_ID="tenant-1",
            AZDO_CLIENT_ID="client-1",
            AZDO_CLIENT_SECRET_ENV="SP_SECRET",
            SP_SECRET=SECRET,
        )
        config = pipelines.load_config(environment)
        key = pipelines.state_key(config)
        self.assertNotIn(SECRET, key)
        for part in ("service-principal", "tenant-1", "client-1", "Alpha,Beta", "14"):
            self.assertIn(part, key)
        other = pipelines.load_config({**environment, "AZDO_PIPELINE_TOP": "3"})
        self.assertNotEqual(pipelines.state_key(other), key)

    def test_main_azure_cli_missing_exits_one(self):
        def missing(command, **options):
            raise FileNotFoundError()

        environment = base_environment(AZDO_STATE_DIR=str(self.state_dir))
        code = pipelines.main(
            ["snapshot"], environ=environment, http=FakeHttp(), run_command=missing,
            now=NOW, stdout=self.stdout, stderr=self.stderr,
        )
        self.assertEqual(code, 1)
        self.assertEqual(self.stdout.getvalue(), "")
        self.assertIn("Azure CLI", self.stderr.getvalue())

    def test_main_login_with_other_method_exits_one(self):
        code = pipelines.main(["login"], environ=base_environment(), stdout=self.stdout, stderr=self.stderr)
        self.assertEqual(code, 1)
        self.assertIn("device-code", self.stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
