import io
import os
import json
import shutil
import subprocess
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

PLUGINS_DIRECTORY = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PLUGINS_DIRECTORY))

import example  # noqa: E402
import gallery  # noqa: E402

LANGUAGE_VARIABLE = "MOSAIC_LANGUAGE"
PORTUGUESE = "pt-BR"
ENGLISH = "en"
NODE_AVAILABLE = shutil.which("node") is not None
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


def run_script(interpreter, script, arguments, language):
    environment = {"PATH": os.environ.get("PATH", ""), "LANG": "pt_BR.UTF-8"}
    if language is not None:
        environment[LANGUAGE_VARIABLE] = language
    return subprocess.run(
        [interpreter, str(PLUGINS_DIRECTORY / script), *arguments],
        env=environment,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        check=False,
    )


def snapshot_of(interpreter, script, language):
    result = run_script(interpreter, script, ["snapshot"], language)
    assert result.returncode == 0, result.stderr
    return json.loads(result.stdout)


def without_alert_ids(snapshot):
    for alert in snapshot.get("alerts", []):
        alert.pop("id")
    return snapshot


def summary_texts(snapshot):
    return [line["text"] for line in snapshot["summary"]["blocks"][0]["lines"]]


class MessageTableTests(unittest.TestCase):
    def test_every_table_has_the_same_keys_in_both_languages(self):
        for module in (example, gallery):
            portuguese = module.MESSAGES[PORTUGUESE]
            english = module.MESSAGES[ENGLISH]
            self.assertEqual(portuguese.keys(), english.keys(), module.__name__)
            for key in portuguese:
                with self.subTest(module=module.__name__, key=key):
                    self.assertTrue(portuguese[key])
                    self.assertTrue(english[key])

    def test_messages_differ_between_languages_except_for_shared_words(self):
        shared_words = {("gallery", "state_ok")}
        for module in (example, gallery):
            for key, portuguese in module.MESSAGES[PORTUGUESE].items():
                if (module.__name__, key) in shared_words:
                    continue
                with self.subTest(module=module.__name__, key=key):
                    self.assertNotEqual(portuguese, module.MESSAGES[ENGLISH][key])

    def test_full_agenda_has_the_same_length_in_both_languages(self):
        self.assertEqual(
            len(gallery.MESSAGES[PORTUGUESE]["full_agenda"]),
            len(gallery.MESSAGES[ENGLISH]["full_agenda"]),
        )

    def test_language_resolution_defaults_to_english_and_ignores_locale(self):
        for module in (example, gallery):
            self.assertEqual(module.resolve_language({}), ENGLISH)
            self.assertEqual(module.resolve_language({LANGUAGE_VARIABLE: ""}), ENGLISH)
            self.assertEqual(module.resolve_language({LANGUAGE_VARIABLE: "fr"}), ENGLISH)
            self.assertEqual(
                module.resolve_language({"LANG": "pt_BR.UTF-8", "LC_ALL": "pt_BR.UTF-8"}),
                ENGLISH,
            )
            for value in ("pt-BR", "PT", " pt_br "):
                self.assertEqual(module.resolve_language({LANGUAGE_VARIABLE: value}), PORTUGUESE)


class ExampleScriptTests(unittest.TestCase):
    PYTHON_TEXTS = {
        PORTUGUESE: ("Exemplo Python", ["olá do plugin em Python", "troque este script pelo seu"]),
        ENGLISH: (
            "Python example",
            ["hello from the Python plugin", "replace this script with yours"],
        ),
    }
    NODE_TEXTS = {
        PORTUGUESE: ("Exemplo Node", ["olá do plugin em Node", "troque este script pelo seu"]),
        ENGLISH: (
            "Node example",
            ["hello from the Node plugin", "replace this script with yours"],
        ),
    }

    def check(self, interpreter, script, expectations):
        for language, (title, texts) in expectations.items():
            with self.subTest(script=script, language=language):
                snapshot = snapshot_of(interpreter, script, language)
                self.assertEqual(snapshot["summary"]["title"], title)
                self.assertEqual(summary_texts(snapshot), texts)

    def test_python_example_in_both_languages(self):
        self.check(sys.executable, "example.py", self.PYTHON_TEXTS)

    def test_python_example_defaults_to_english_without_the_variable(self):
        snapshot = snapshot_of(sys.executable, "example.py", None)
        self.assertEqual(snapshot["summary"]["title"], "Python example")

    def test_python_example_error_message_per_language(self):
        portuguese = run_script(sys.executable, "example.py", ["nada"], PORTUGUESE)
        english = run_script(sys.executable, "example.py", ["nada"], None)
        self.assertEqual(portuguese.returncode, 2)
        self.assertEqual(portuguese.stderr, "este exemplo só implementa 'snapshot'\n")
        self.assertEqual(english.returncode, 2)
        self.assertEqual(english.stderr, "this example only implements 'snapshot'\n")

    @unittest.skipUnless(NODE_AVAILABLE, "node não está instalado")
    def test_node_example_in_both_languages_and_default(self):
        self.check("node", "example.js", self.NODE_TEXTS)
        self.assertEqual(
            snapshot_of("node", "example.js", None)["summary"]["title"], "Node example"
        )
        english = run_script("node", "example.js", ["nada"], None)
        self.assertEqual(english.returncode, 2)
        self.assertEqual(english.stderr, "this example only implements 'snapshot'\n")

    def test_main_accepts_an_injected_environment(self):
        output, errors = io.StringIO(), io.StringIO()
        with redirect_stdout(output), redirect_stderr(errors):
            code = example.main(["snapshot"], {LANGUAGE_VARIABLE: ENGLISH})
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(output.getvalue())["summary"]["title"], "Python example")


class GalleryScriptTests(unittest.TestCase):
    def test_portuguese_texts_are_unchanged(self):
        snapshot = snapshot_of(sys.executable, "gallery.py", PORTUGUESE)
        blocks = snapshot["summary"]["blocks"]
        self.assertEqual(snapshot["summary"]["title"], "Galeria")
        self.assertEqual(blocks[0]["rows"][1]["text"], "Revisão de código")
        self.assertEqual(blocks[1]["hint"], "dados de exemplo, sem rede")
        self.assertEqual(blocks[2]["columns"][1]["title"], "Em andamento")
        self.assertEqual(blocks[3]["lines"][0]["text"], "tudo em dia")
        self.assertEqual(snapshot["detail"][0]["heading"], "Agenda de hoje")
        self.assertEqual(snapshot["alerts"][0]["title"], "Aviso da galeria")

    def test_english_texts(self):
        snapshot = snapshot_of(sys.executable, "gallery.py", ENGLISH)
        blocks = snapshot["summary"]["blocks"]
        self.assertEqual(snapshot["summary"]["title"], "Gallery")
        self.assertEqual(blocks[0]["rows"][1]["text"], "Code review")
        self.assertEqual(blocks[1]["hint"], "sample data, no network")
        self.assertEqual(blocks[2]["columns"][1]["title"], "In progress")
        self.assertEqual(blocks[3]["lines"][0]["text"], "all caught up")
        self.assertEqual(snapshot["detail"][0]["heading"], "Today's agenda")
        self.assertEqual(snapshot["alerts"][0]["title"], "Gallery alert")

    def test_missing_variable_defaults_to_english(self):
        snapshot = snapshot_of(sys.executable, "gallery.py", None)
        self.assertEqual(snapshot["summary"]["title"], "Gallery")

    def test_error_message_per_language(self):
        portuguese = run_script(sys.executable, "gallery.py", ["nada"], PORTUGUESE)
        english = run_script(sys.executable, "gallery.py", ["nada"], None)
        self.assertEqual(portuguese.stderr, "a galeria só implementa 'snapshot'\n")
        self.assertEqual(english.stderr, "the gallery only implements 'snapshot'\n")
        self.assertEqual((portuguese.returncode, english.returncode), (2, 2))

    def test_data_that_is_not_translated_stays_the_same(self):
        portuguese = snapshot_of(sys.executable, "gallery.py", PORTUGUESE)
        english = snapshot_of(sys.executable, "gallery.py", ENGLISH)
        self.assertEqual(
            portuguese["summary"]["blocks"][2]["rows"][0][:2],
            english["summary"]["blocks"][2]["rows"][0][:2],
        )
        self.assertEqual(portuguese["alerts"][0]["id"][:8], english["alerts"][0]["id"][:8])

    @unittest.skipUnless(NODE_AVAILABLE, "node não está instalado")
    def test_node_gallery_matches_python_gallery_in_every_language(self):
        for language in (PORTUGUESE, ENGLISH, None):
            with self.subTest(language=language):
                python_snapshot = snapshot_of(sys.executable, "gallery.py", language)
                node_snapshot = snapshot_of("node", "gallery.js", language)
                self.assertEqual(
                    without_alert_ids(node_snapshot), without_alert_ids(python_snapshot)
                )


if __name__ == "__main__":
    unittest.main()
