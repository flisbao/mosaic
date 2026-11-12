import re
import tomllib
import unittest
from pathlib import Path

PLUGINS_DIRECTORY = Path(__file__).resolve().parents[1]
NOT_PLUGIN_DIRECTORIES = ("tests", "__pycache__")
MANIFEST_FILE_NAME = "setup.toml"
README_VARIABLE_PATTERN = re.compile(r"^\| `([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+)`", re.MULTILINE)
README_FLAG_PATTERN = re.compile(r"^\| `(--[a-z-]+)`", re.MULTILINE)
TOKEN_SUFFIXES = ("_TOKEN_ENV", "_TOKEN_FILE")


def plugin_names():
    return sorted(
        path.name
        for path in PLUGINS_DIRECTORY.iterdir()
        if path.is_dir() and path.name not in NOT_PLUGIN_DIRECTORIES
    )


def load_manifest(name):
    with open(PLUGINS_DIRECTORY / name / MANIFEST_FILE_NAME, "rb") as handle:
        return tomllib.load(handle)


def plugin_code(name):
    return "\n".join(
        path.read_text(encoding="utf-8") for path in sorted((PLUGINS_DIRECTORY / name).glob("*.py"))
    )


def readme(name):
    return (PLUGINS_DIRECTORY / name / "README.md").read_text(encoding="utf-8")


def manifest_variables(manifest):
    names = []
    for step in manifest["steps"]:
        if step["kind"] == "variable":
            names.append(step["name"])
        elif step["kind"] == "secret":
            names.extend([step["environment_key"], step["file_key"]])
    return names


def manifest_flags(manifest):
    return [step["flag"] for step in manifest["steps"] if step["kind"] == "argument"]


class SetupManifestTests(unittest.TestCase):
    def test_every_plugin_declares_stable_individual_versions(self):
        version_pattern = r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$"
        for name in plugin_names():
            with self.subTest(plugin=name):
                manifest = load_manifest(name)
                for field in ("version", "min_mosaic_version"):
                    self.assertIsInstance(manifest.get(field), str)
                    self.assertRegex(manifest[field], version_pattern)

    def test_every_plugin_has_a_manifest(self):
        for name in plugin_names():
            with self.subTest(plugin=name):
                self.assertTrue((PLUGINS_DIRECTORY / name / MANIFEST_FILE_NAME).is_file())

    def test_every_documented_variable_is_covered_by_the_manifest(self):
        for name in plugin_names():
            with self.subTest(plugin=name):
                manifest = load_manifest(name)
                covered = set(manifest_variables(manifest)) | set(manifest.get("testing_only", []))
                documented = set(README_VARIABLE_PATTERN.findall(readme(name)))
                self.assertEqual(sorted(documented - covered), [], "variáveis do README fora do setup.toml")

    def test_every_documented_option_is_covered_by_the_manifest(self):
        for name in plugin_names():
            with self.subTest(plugin=name):
                documented = set(README_FLAG_PATTERN.findall(readme(name)))
                self.assertEqual(sorted(documented - set(manifest_flags(load_manifest(name)))), [])

    def test_every_manifest_variable_exists_in_the_plugin_code(self):
        for name in plugin_names():
            with self.subTest(plugin=name):
                code = plugin_code(name)
                for key in manifest_variables(load_manifest(name)):
                    prefix = next((key[: -len(suffix)] for suffix in TOKEN_SUFFIXES if key.endswith(suffix)), None)
                    built_from_prefix = prefix is not None and f'"{prefix}"' in code
                    self.assertTrue(key in code or built_from_prefix, f"{key} não existe no código de {name}")

    def test_every_manifest_option_exists_in_the_plugin_code(self):
        for name in plugin_names():
            with self.subTest(plugin=name):
                code = plugin_code(name)
                for flag in manifest_flags(load_manifest(name)):
                    self.assertIn(f'"{flag}"', code)

    def test_choices_and_defaults_are_known_to_the_plugin(self):
        for name in plugin_names():
            with self.subTest(plugin=name):
                code = plugin_code(name)
                text = readme(name)
                for step in load_manifest(name)["steps"]:
                    for choice in step.get("choices", []):
                        self.assertIn(f'"{choice["value"]}"', code, f"{step.get('name') or step.get('flag')}")
                    default = step.get("default")
                    if default is not None:
                        self.assertIn(default, text, f"padrão {default!r} não está no README")

    def test_presets_only_use_options_the_plugin_asks(self):
        for name in plugin_names():
            with self.subTest(plugin=name):
                manifest = load_manifest(name)
                flags = set(manifest_flags(manifest))
                for preset in manifest.get("presets", []):
                    self.assertLessEqual(set(preset["arguments"]), flags)

    def test_every_visible_text_of_a_bilingual_manifest_has_its_english_field(self):
        translated = []
        for name in plugin_names():
            manifest = load_manifest(name)
            text = str(manifest)
            if not any(f"'{key}'" in text for key in ("prompt_en", "description_en", "text_en")):
                continue
            translated.append(name)
            with self.subTest(plugin=name):
                for step in manifest["steps"]:
                    self.assertIn("prompt_en", step, step.get("name") or step.get("flag"))
                    for choice in step.get("choices", []):
                        self.assertIn("description_en", choice, choice["value"])
                for note in manifest.get("notes", []):
                    self.assertIn("text_en", note)
                for preset in manifest.get("presets", []):
                    self.assertIn("description_en", preset, preset["name"])
                    for field in preset.get("inputs", []):
                        self.assertIn("prompt_en", field, field["name"])
        self.assertGreaterEqual(len(translated), 1)


if __name__ == "__main__":
    unittest.main()
