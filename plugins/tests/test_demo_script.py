import unittest
from pathlib import Path

SCRIPTS_DIRECTORY = Path(__file__).resolve().parents[2] / "scripts"
REMOVED_PAT_VARIABLES = ("AZDO_TOKEN_ENV", "AZDO_TOKEN_FILE")
AZURE_DEVOPS_PLUGIN = "plugins/azure-devops/azure_devops.py"


class DemoScriptTests(unittest.TestCase):
    def test_no_script_uses_the_removed_azure_pat_variables(self):
        for script in sorted(SCRIPTS_DIRECTORY.glob("*.sh")):
            text = script.read_text(encoding="utf-8")
            for variable in REMOVED_PAT_VARIABLES:
                with self.subTest(script=script.name, variable=variable):
                    self.assertNotIn(variable, text)

    def test_demo_signs_in_to_azure_devops_through_the_fake_entra_server(self):
        text = (SCRIPTS_DIRECTORY / "demo.sh").read_text(encoding="utf-8")
        self.assertIn("fake_entra_server.py", text)
        self.assertIn('AZDO_AUTH_METHOD = "service-principal"', text)
        self.assertIn("AZDO_LOGIN_URL", text)


if __name__ == "__main__":
    unittest.main()
