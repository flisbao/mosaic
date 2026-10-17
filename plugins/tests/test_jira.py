import sys
import tempfile
import unittest
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


if __name__ == "__main__":
    unittest.main()
