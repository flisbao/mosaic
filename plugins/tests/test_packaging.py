import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
STAGE_SCRIPT = REPOSITORY_ROOT / "scripts" / "stage-package.sh"
SKILL_PREFIX = "mosaic-plugin-"
AUTHORING_SKILL = "mosaic-widget-authoring"
USAGE_EXIT_CODE = 2
ALLOWED_PLUGIN_SUFFIXES = (".py", ".js")
ALLOWED_PLUGIN_FILE_NAMES = ("README.md", "SKILL.md", "setup.toml")
SKIPPED_PLUGIN_FOLDERS = ("tests", "__pycache__")
PACKAGED_PLUGINS = "usr/share/mosaic/plugins"
PACKAGED_SKILLS = "usr/share/mosaic/skills"
WORKFLOWS = REPOSITORY_ROOT / ".github" / "workflows"


def stage(package, destination, repository=None, umask=None):
    environment = dict(os.environ)
    if repository is not None:
        environment["MOSAIC_REPO"] = str(repository)
    return subprocess.run(
        ["bash", str(STAGE_SCRIPT), package, str(destination)],
        capture_output=True,
        text=True,
        env=environment,
        preexec_fn=None if umask is None else lambda: os.umask(umask),
    )


def listing(directory):
    return sorted(path.name for path in Path(directory).iterdir())


def relative_files(root):
    return {
        str(path.relative_to(root))
        for path in Path(root).rglob("*")
        if path.is_file() or path.is_symlink()
    }


def real_service_plugins():
    plugins = REPOSITORY_ROOT / "plugins"
    return sorted(
        folder.name
        for folder in plugins.iterdir()
        if folder.is_dir()
        and folder.name not in SKIPPED_PLUGIN_FOLDERS
        and (folder / "SKILL.md").is_file()
    )


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.destination = Path(self.temporary.name) / "root"

    def test_version_file_is_semver(self):
        text = (REPOSITORY_ROOT / "plugins" / "VERSION").read_text(encoding="utf-8")
        self.assertRegex(text, r"^\d+\.\d+\.\d+\n?$")

    def test_staged_permissions_do_not_depend_on_the_umask(self):
        for package in ("core", "plugins"):
            destination = self.destination / package
            result = stage(package, destination, umask=0o002)
            self.assertEqual(result.returncode, 0, result.stderr)
            for path in [destination, *destination.rglob("*")]:
                if path.is_symlink():
                    continue
                mode = path.stat().st_mode & 0o777
                self.assertEqual(mode & 0o022, 0, f"{path}: {oct(mode)}")

    def test_stage_core_has_docs_and_only_the_authoring_skill(self):
        result = stage("core", self.destination)
        self.assertEqual(result.returncode, 0, result.stderr)
        documents = self.destination / "usr/share/doc/mosaic"
        self.assertEqual(
            listing(documents), ["PROTOCOL.md", "README.md", "copyright", "protocol.schema.json"]
        )
        self.assertEqual(listing(self.destination / PACKAGED_SKILLS), [AUTHORING_SKILL])
        self.assertTrue((self.destination / PACKAGED_SKILLS / AUTHORING_SKILL / "SKILL.md").is_file())
        self.assertFalse((self.destination / PACKAGED_PLUGINS).exists())

    def test_stage_plugins_has_every_service_plugin_with_its_skill_and_setup_manifest(self):
        result = stage("plugins", self.destination)
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = real_service_plugins()
        self.assertGreaterEqual(len(expected), 6)
        self.assertEqual(listing(self.destination / PACKAGED_PLUGINS), expected)
        for name in expected:
            folder = self.destination / PACKAGED_PLUGINS / name
            with self.subTest(plugin=name):
                self.assertTrue((folder / "SKILL.md").is_file())
                self.assertTrue((folder / "setup.toml").is_file())
        self.assertFalse((self.destination / PACKAGED_SKILLS).exists())

    def test_plugins_package_has_only_runtime_files(self):
        stage("plugins", self.destination)
        for relative in relative_files(self.destination / PACKAGED_PLUGINS):
            path = Path(relative)
            with self.subTest(file=relative):
                self.assertEqual(len(path.parts), 2, "só arquivos direto na pasta do plugin")
                self.assertTrue(
                    path.name in ALLOWED_PLUGIN_FILE_NAMES or path.suffix in ALLOWED_PLUGIN_SUFFIXES
                )
                self.assertFalse(path.name.startswith(("fake_", "test_")))
                self.assertNotIn(".pyc", path.name)

    def test_plugins_package_carries_version_and_license(self):
        stage("plugins", self.destination)
        documents = self.destination / "usr/share/doc/mosaic-plugins"
        self.assertEqual(listing(documents), ["VERSION", "copyright"])
        version = (documents / "VERSION").read_text(encoding="utf-8")
        self.assertEqual(version, (REPOSITORY_ROOT / "plugins" / "VERSION").read_text(encoding="utf-8"))

    def test_core_and_plugins_do_not_overlap(self):
        core = Path(self.temporary.name) / "core"
        plugins = Path(self.temporary.name) / "plugins"
        stage("core", core)
        stage("plugins", plugins)
        self.assertEqual(relative_files(core) & relative_files(plugins), set())

    def test_stage_rejects_an_unknown_package_name(self):
        result = stage("outro", self.destination)
        self.assertEqual(result.returncode, USAGE_EXIT_CODE)
        self.assertIn("uso:", result.stderr)

    def test_stage_requires_both_arguments(self):
        result = subprocess.run(["bash", str(STAGE_SCRIPT)], capture_output=True, text=True)
        self.assertEqual(result.returncode, USAGE_EXIT_CODE)

    def test_a_plugin_folder_without_a_skill_is_not_packaged(self):
        repository = self.fake_repository()
        (repository / "plugins" / "semskill").mkdir()
        (repository / "plugins" / "semskill" / "x.py").write_text("print()", encoding="utf-8")
        result = stage("plugins", self.destination, repository)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(listing(self.destination / PACKAGED_PLUGINS), ["alpha"])

    def test_symlinks_and_other_files_are_not_copied(self):
        repository = self.fake_repository()
        outside = Path(self.temporary.name) / "segredo.py"
        outside.write_text("segredo", encoding="utf-8")
        (repository / "plugins" / "alpha" / "linked.py").symlink_to(outside)
        (repository / "plugins" / "alpha" / "notes.txt").write_text("x", encoding="utf-8")
        (repository / "plugins" / "alpha" / "tests").mkdir()
        (repository / "plugins" / "alpha" / "tests" / "test_a.py").write_text("x", encoding="utf-8")
        stage("plugins", self.destination, repository)
        self.assertEqual(
            listing(self.destination / PACKAGED_PLUGINS / "alpha"),
            ["README.md", "SKILL.md", "alpha.py", "setup.toml"],
        )

    def test_plugins_release_workflow_is_triggered_only_by_plugins_tags(self):
        text = (WORKFLOWS / "release-plugins.yml").read_text(encoding="utf-8")
        self.assertIn('- "plugins-v*"', text)
        self.assertNotIn('- "v*"', text)
        self.assertIn('"plugins-v${version}"', text)
        for step in ("stage-package.sh plugins", "build-plugins-deb.sh", "mosaic plugins install", "unittest discover"):
            self.assertIn(step, text)

    def test_plugins_release_never_takes_the_latest_label(self):
        text = (WORKFLOWS / "release-plugins.yml").read_text(encoding="utf-8")
        self.assertIn("--latest=false", text)

    def test_docs_do_not_say_service_plugins_ship_in_the_binary(self):
        calendar = (REPOSITORY_ROOT / "plugins/calendar/README.md").read_text(encoding="utf-8")
        self.assertNotIn("o plugin já está em", calendar)
        self.assertIn("mosaic plugins install calendar", calendar)
        portuguese = (REPOSITORY_ROOT / "README.pt-BR.md").read_text(encoding="utf-8")
        first_run = next(line for line in portuguese.splitlines() if "primeira execução" in line)
        self.assertIn("(kanban, example, galeria)", first_run)
        self.assertNotIn("kanban, Jira", first_run)
        self.assertIn("mosaic plugins install", portuguese)

    def test_core_release_workflow_is_not_triggered_by_plugins_tags(self):
        text = (WORKFLOWS / "release.yml").read_text(encoding="utf-8")
        self.assertIn('- "v*"', text)
        self.assertNotIn("plugins-v", text)

    def fake_repository(self):
        repository = Path(self.temporary.name) / "repo"
        (repository / "plugins" / "alpha").mkdir(parents=True)
        (repository / "plugins" / "alpha" / "alpha.py").write_text("print()", encoding="utf-8")
        (repository / "plugins" / "alpha" / "README.md").write_text("# a", encoding="utf-8")
        (repository / "plugins" / "alpha" / "SKILL.md").write_text("---\nname: x\n---\n", encoding="utf-8")
        (repository / "plugins" / "alpha" / "setup.toml").write_text("plugin = \"alpha\"\n", encoding="utf-8")
        (repository / "plugins" / "tests").mkdir()
        (repository / "plugins" / "VERSION").write_text("1.2.3\n", encoding="utf-8")
        authoring = repository / ".claude" / "skills" / AUTHORING_SKILL
        authoring.mkdir(parents=True)
        (authoring / "SKILL.md").write_text("---\nname: y\n---\n", encoding="utf-8")
        for name in ("README.md", "PROTOCOL.md", "LICENSE"):
            (repository / name).write_text(name, encoding="utf-8")
        (repository / "schema").mkdir()
        (repository / "schema" / "protocol.schema.json").write_text("{}", encoding="utf-8")
        return repository


if __name__ == "__main__":
    unittest.main()
