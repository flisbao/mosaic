import os
import subprocess
import tempfile
import unittest
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
STAGE_SCRIPT = REPOSITORY_ROOT / "scripts" / "stage-package.sh"
AUTHORING_SKILL = "mosaic-widget-authoring"
USAGE_EXIT_CODE = 2
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


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.destination = Path(self.temporary.name) / "root"


    def test_collective_plugin_package_is_retired(self):
        for relative in ("plugins/VERSION", ".github/workflows/release-plugins.yml", "scripts/build-plugins-deb.sh"):
            with self.subTest(path=relative):
                self.assertFalse((REPOSITORY_ROOT / relative).exists())
        self.assertEqual(stage("plugins", self.destination).returncode, USAGE_EXIT_CODE)

    def test_core_recommends_git_instead_of_the_plugin_package(self):
        text = (REPOSITORY_ROOT / "scripts/build-deb.sh").read_text(encoding="utf-8")
        self.assertIn("Recommends: python3, git", text)
        self.assertNotIn("Suggests: mosaic-plugins", text)

    def test_staged_permissions_do_not_depend_on_the_umask(self):
        for package in ("core",):
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



    def test_stage_rejects_an_unknown_package_name(self):
        result = stage("outro", self.destination)
        self.assertEqual(result.returncode, USAGE_EXIT_CODE)
        self.assertIn("uso:", result.stderr)

    def test_stage_requires_both_arguments(self):
        result = subprocess.run(["bash", str(STAGE_SCRIPT)], capture_output=True, text=True)
        self.assertEqual(result.returncode, USAGE_EXIT_CODE)



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

    def test_ci_workflow_runs_every_check_on_push_and_pull_request(self):
        text = (WORKFLOWS / "ci.yml").read_text(encoding="utf-8")
        for expected in (
            "pull_request:",
            "branches: [main]",
            "cargo fmt --check",
            "cargo clippy --all-targets -- -D warnings",
            "cargo test",
            "python3 -m unittest discover -s plugins/tests",
        ):
            self.assertIn(expected, text)

if __name__ == "__main__":
    unittest.main()
