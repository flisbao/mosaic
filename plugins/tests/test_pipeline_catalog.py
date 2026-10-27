import io
import json
import sys
import tempfile
import unittest
import urllib.parse
from datetime import timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "azure-pipelines"))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import azure_pipelines as pipelines  # noqa: E402
from test_azure_pipelines import (  # noqa: E402
    NOW,
    FixtureHttp,
    base_environment,
    fake_azure_cli,
    make_client,
)

CATALOG = """\
# catálogo de teste
[build-api]
repository = api
url = https://dev.azure.com/org/Alpha/_build?definitionId=1

[deploy-api]
repository = api
url = https://dev.azure.com/org/Alpha/_build/latest?definitionId=2&branchName=main

[lint-web]
repository = web
url = https://dev.azure.com/org/Beta/_build?definitionId=3
"""


class CatalogTestCase(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)

    def catalog_file(self, text=CATALOG):
        path = self.directory / "pipelines.ini"
        path.write_text(text, encoding="utf-8")
        return str(path)

    def config(self, text=CATALOG, **overrides):
        environment = base_environment(
            AZDO_PROJECTS=None, AZDO_PIPELINES_FILE=self.catalog_file(text), **overrides
        )
        return pipelines.load_config(environment)


class CatalogParsingTests(CatalogTestCase):
    def test_every_section_becomes_a_pipeline_with_project_and_definition_from_the_url(self):
        selected = self.config().catalog
        self.assertEqual(
            [(item.name, item.project, item.repository, item.definition_id) for item in selected],
            [
                ("build-api", "Alpha", "api", 1),
                ("deploy-api", "Alpha", "api", 2),
                ("lint-web", "Beta", "web", 3),
            ],
        )

    def test_percent_encoded_project_names_are_decoded(self):
        text = "[a]\nurl = https://dev.azure.com/org/Meu%20Projeto/_build?definitionId=7\n"
        self.assertEqual(self.config(text).catalog[0].project, "Meu Projeto")

    def test_the_repository_is_optional(self):
        text = "[a]\nurl = https://dev.azure.com/org/Alpha/_build?definitionId=7\n"
        self.assertEqual(self.config(text).catalog[0].repository, "")

    def test_without_the_file_variable_there_is_no_catalog(self):
        config = pipelines.load_config(base_environment())
        self.assertEqual(config.catalog, ())

    def test_percent_signs_in_urls_are_kept_literally(self):
        text = "[a]\nurl = https://dev.azure.com/org/A%2FB/_build?definitionId=7\n"
        self.assertEqual(self.config(text).catalog[0].project, "A/B")

    def assert_rejected(self, text, expected):
        with self.assertRaises(pipelines.ConfigError) as caught:
            self.config(text)
        self.assertIn(expected, str(caught.exception))

    def test_a_url_of_another_organization_is_rejected_naming_the_section(self):
        self.assert_rejected(
            "[a]\nurl = https://dev.azure.com/outra/Alpha/_build?definitionId=1\n", "[a]"
        )

    def test_a_url_of_another_host_is_rejected(self):
        self.assert_rejected("[a]\nurl = https://exemplo.com/org/Alpha/_build?definitionId=1\n", "[a]")

    def test_a_url_without_a_definition_id_is_rejected_with_guidance(self):
        self.assert_rejected(
            "[a]\nurl = https://dev.azure.com/org/Alpha/_build/results?buildId=9\n", "definitionId"
        )

    def test_a_non_numeric_definition_id_is_rejected(self):
        self.assert_rejected("[a]\nurl = https://dev.azure.com/org/Alpha/_build?definitionId=x\n", "definitionId")

    def test_a_section_without_url_is_rejected(self):
        self.assert_rejected("[a]\nrepository = api\n", "url")

    def test_duplicate_sections_are_rejected(self):
        url = "url = https://dev.azure.com/org/Alpha/_build?definitionId=1\n"
        self.assert_rejected(f"[a]\n{url}\n[a]\n{url}", "[a]")

    def test_names_cannot_contain_a_slash_because_it_selects_project_and_repository(self):
        self.assert_rejected("[a/b]\nurl = https://dev.azure.com/org/Alpha/_build?definitionId=1\n", "a/b")

    def test_an_empty_catalog_is_rejected(self):
        self.assert_rejected("# nada\n", "nenhuma pipeline")

    def test_a_missing_file_names_the_path(self):
        environment = base_environment(AZDO_PROJECTS=None, AZDO_PIPELINES_FILE="/nao/existe.ini")
        with self.assertRaises(pipelines.ConfigError) as caught:
            pipelines.load_config(environment)
        self.assertIn("/nao/existe.ini", str(caught.exception))

    def test_a_malformed_file_is_a_config_error_not_a_crash(self):
        self.assert_rejected("isto não é ini\n", "pipelines.ini")

    def test_the_catalog_and_the_project_list_cannot_be_combined(self):
        environment = base_environment(AZDO_PIPELINES_FILE=self.catalog_file())
        with self.assertRaises(pipelines.ConfigError) as caught:
            pipelines.load_config(environment)
        self.assertIn("AZDO_PIPELINES_FILE", str(caught.exception))
        self.assertIn("AZDO_PROJECTS", str(caught.exception))

    def test_watch_without_a_catalog_is_rejected(self):
        with self.assertRaises(pipelines.ConfigError) as caught:
            pipelines.load_config(base_environment(AZDO_PIPELINE_WATCH="x"))
        self.assertIn("AZDO_PIPELINES_FILE", str(caught.exception))


class SelectionTests(CatalogTestCase):
    def names(self, watch):
        return [item.name for item in self.config(AZDO_PIPELINE_WATCH=watch).catalog]

    def test_by_name(self):
        self.assertEqual(self.names("deploy-api"), ["deploy-api"])

    def test_several_names_keep_the_catalog_order(self):
        self.assertEqual(self.names("lint-web, build-api"), ["build-api", "lint-web"])

    def test_by_project_and_repository(self):
        self.assertEqual(self.names("Alpha/api"), ["build-api", "deploy-api"])

    def test_by_whole_project(self):
        self.assertEqual(self.names("Beta/*"), ["lint-web"])

    def test_project_and_repository_match_ignoring_case(self):
        self.assertEqual(self.names("alpha/API"), ["build-api", "deploy-api"])

    def test_the_same_pipeline_selected_twice_appears_once(self):
        self.assertEqual(self.names("build-api,Alpha/api"), ["build-api", "deploy-api"])

    def test_an_empty_watch_selects_everything(self):
        self.assertEqual(self.names(""), ["build-api", "deploy-api", "lint-web"])

    def test_an_unknown_selector_lists_what_exists(self):
        with self.assertRaises(pipelines.ConfigError) as caught:
            self.config(AZDO_PIPELINE_WATCH="fantasma")
        message = str(caught.exception)
        self.assertIn("fantasma", message)
        self.assertIn("build-api", message)
        self.assertIn("Alpha/api", message)


class CatalogCollectionTests(CatalogTestCase):
    def test_projects_come_from_the_catalog_without_a_call(self):
        config = self.config()
        http = FixtureHttp()
        self.assertEqual(pipelines.fetch_projects(make_client(http, config), config), ["Alpha", "Beta"])
        self.assertEqual(http.calls, [])

    def test_runs_are_requested_only_for_the_selected_definitions(self):
        http = FixtureHttp()
        pipelines.fetch_runs(make_client(http), "Alpha", NOW - timedelta(days=14), (1, 2))
        query = urllib.parse.parse_qs(urllib.parse.urlparse(http.calls[0]).query)
        self.assertEqual(query["definitions"], ["1,2"])

    def test_without_definitions_the_parameter_is_not_sent(self):
        http = FixtureHttp()
        pipelines.fetch_runs(make_client(http), "Alpha", NOW - timedelta(days=14))
        query = urllib.parse.parse_qs(urllib.parse.urlparse(http.calls[0]).query)
        self.assertNotIn("definitions", query)

    def test_the_ranking_only_has_the_selected_pipelines(self):
        config = self.config(AZDO_PIPELINE_WATCH="deploy-api")
        overview, _ = pipelines.collect_overview(make_client(FixtureHttp(), config), config, NOW, {})
        self.assertEqual([item.pipeline_name for item in overview.ranked], ["Deploy"])
        self.assertEqual(overview.pipeline_count, 1)

    def test_each_project_is_asked_only_for_its_own_definitions(self):
        config = self.config(AZDO_PIPELINE_WATCH="build-api,lint-web")
        http = FixtureHttp()
        pipelines.collect_overview(make_client(http, config), config, NOW, {})
        build_calls = [url for url in http.calls if url.split("?")[0].endswith("/builds")]
        requested = {
            urllib.parse.unquote(url.split("?")[0]).split("/")[-4]: urllib.parse.parse_qs(
                urllib.parse.urlparse(url).query
            )["definitions"]
            for url in build_calls
        }
        self.assertEqual(requested, {"Alpha": ["1"], "Beta": ["3"]})

    def test_the_state_key_changes_with_the_selection(self):
        everything = pipelines.state_key(self.config())
        only_one = pipelines.state_key(self.config(AZDO_PIPELINE_WATCH="deploy-api"))
        without_catalog = pipelines.state_key(pipelines.load_config(base_environment()))
        self.assertEqual(len({everything, only_one, without_catalog}), 3)


class PipelinesCommandTests(CatalogTestCase):
    def run_command(self, **overrides):
        stdout, stderr = io.StringIO(), io.StringIO()
        environment = base_environment(
            AZDO_PROJECTS=None, AZDO_PIPELINES_FILE=self.catalog_file(), **overrides
        )
        code = pipelines.main(
            ["pipelines"],
            environ=environment,
            http=FixtureHttp(),
            run_command=fake_azure_cli(),
            now=NOW,
            stdout=stdout,
            stderr=stderr,
        )
        return code, stdout.getvalue(), stderr.getvalue()

    def test_it_lists_the_catalog_grouped_by_project_and_repository_without_network(self):
        code, stdout, _ = self.run_command()
        self.assertEqual(code, 0)
        self.assertIn("Alpha/api", stdout)
        self.assertIn("build-api", stdout)
        self.assertIn("definição 3", stdout)
        self.assertIn("Beta/web", stdout)

    def test_it_marks_what_the_watch_variable_selects(self):
        _, stdout, _ = self.run_command(AZDO_PIPELINE_WATCH="deploy-api")
        selected = [line for line in stdout.splitlines() if line.lstrip().startswith("*")]
        self.assertEqual(len(selected), 1)
        self.assertIn("deploy-api", selected[0])

    def test_without_a_catalog_it_explains_how_to_create_one(self):
        stdout, stderr = io.StringIO(), io.StringIO()
        code = pipelines.main(["pipelines"], environ=base_environment(), stdout=stdout, stderr=stderr)
        self.assertEqual(code, 1)
        self.assertIn("AZDO_PIPELINES_FILE", stderr.getvalue())


class SnapshotWithCatalogTests(CatalogTestCase):
    def test_the_snapshot_only_covers_the_selected_pipelines(self):
        stdout, stderr = io.StringIO(), io.StringIO()
        environment = base_environment(
            AZDO_PROJECTS=None,
            AZDO_PIPELINES_FILE=self.catalog_file(),
            AZDO_PIPELINE_WATCH="Alpha/api",
            AZDO_STATE_DIR=str(self.directory / "estado"),
        )
        code = pipelines.main(
            ["snapshot"],
            environ=environment,
            http=FixtureHttp(),
            run_command=fake_azure_cli(),
            now=NOW,
            stdout=stdout,
            stderr=stderr,
        )
        self.assertEqual(code, 0, stderr.getvalue())
        snapshot = json.loads(stdout.getvalue())
        self.assertEqual(len(snapshot["detail"]), 2)


if __name__ == "__main__":
    unittest.main()
