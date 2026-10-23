import re
import unittest
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
PLUGINS_DIRECTORY = REPOSITORY_ROOT / "plugins"
SKILLS_DIRECTORY = REPOSITORY_ROOT / ".claude" / "skills"
PLUGIN_SKILL_PREFIX = "mosaic-plugin-"
AUTHORING_SKILL = "mosaic-widget-authoring"
NOT_PLUGIN_DIRECTORIES = ("tests", "__pycache__")
DESCRIPTION_PREFIX = "Use quando"
MAXIMUM_FRONT_MATTER_CHARACTERS = 1024
FRONT_MATTER_FENCE = "---"
REQUIRED_HEADINGS = (
    "Visão geral",
    "Configurar com a pessoa",
    "Bloco do mosaic.toml",
    "Verificar",
    "Mexendo no código",
    "Armadilhas",
)
IDENTIFIER_PATTERN = re.compile(r"`([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+)`")
PLUGIN_PATH_PATTERN = re.compile(r"`(plugins/[\w./-]+\.(?:py|md))`")
HEADING_PATTERN = re.compile(r"^##\s+(.+?)\s*$", re.MULTILINE)


def plugin_names():
    return sorted(
        path.name
        for path in PLUGINS_DIRECTORY.iterdir()
        if path.is_dir() and path.name not in NOT_PLUGIN_DIRECTORIES
    )


def plugin_files(name):
    folder = PLUGINS_DIRECTORY / name
    return [path for path in folder.rglob("*") if path.suffix in (".py", ".md")]


def known_text(name):
    sources = plugin_files(name) + sorted((PLUGINS_DIRECTORY / "tests").glob("*.py"))
    return "\n".join(path.read_text(encoding="utf-8") for path in sources if path.exists())


def read_skill(directory):
    text = (directory / "SKILL.md").read_text(encoding="utf-8")
    lines = text.split("\n")
    assert lines[0] == FRONT_MATTER_FENCE, f"{directory.name}: SKILL.md sem front matter"
    closing = lines.index(FRONT_MATTER_FENCE, 1)
    front_matter = "\n".join(lines[1:closing])
    fields = {}
    for line in lines[1:closing]:
        key, _, value = line.partition(":")
        fields[key.strip()] = value.strip()
    return fields, front_matter, "\n".join(lines[closing + 1 :])


def all_skill_directories():
    return sorted(path for path in SKILLS_DIRECTORY.iterdir() if path.is_dir())


class SkillFormatTests(unittest.TestCase):
    def test_every_skill_has_valid_front_matter(self):
        for directory in all_skill_directories():
            with self.subTest(skill=directory.name):
                fields, front_matter, _ = read_skill(directory)
                self.assertEqual(fields.get("name"), directory.name)
                self.assertRegex(directory.name, r"^[a-z0-9-]+$")
                self.assertTrue(
                    fields.get("description", "").startswith(DESCRIPTION_PREFIX),
                    "a descrição precisa começar com 'Use quando'",
                )
                self.assertLessEqual(len(front_matter), MAXIMUM_FRONT_MATTER_CHARACTERS)

    def test_authoring_skill_ships_the_plugin_skill_template(self):
        template = SKILLS_DIRECTORY / AUTHORING_SKILL / "plugin-skill-template.md"
        self.assertTrue(template.is_file())
        _, _, body = read_skill(SKILLS_DIRECTORY / AUTHORING_SKILL)
        self.assertIn("(plugin-skill-template.md)", body)


class PluginSkillTests(unittest.TestCase):
    def test_every_plugin_has_a_skill(self):
        for name in plugin_names():
            with self.subTest(plugin=name):
                skill = SKILLS_DIRECTORY / f"{PLUGIN_SKILL_PREFIX}{name}" / "SKILL.md"
                self.assertTrue(
                    skill.is_file(),
                    f"plugin '{name}' sem skill: crie .claude/skills/{PLUGIN_SKILL_PREFIX}{name}/SKILL.md "
                    f"pelo template em .claude/skills/{AUTHORING_SKILL}/plugin-skill-template.md",
                )

    def test_every_plugin_skill_belongs_to_a_plugin(self):
        names = set(plugin_names())
        for directory in all_skill_directories():
            if not directory.name.startswith(PLUGIN_SKILL_PREFIX):
                continue
            with self.subTest(skill=directory.name):
                self.assertIn(directory.name[len(PLUGIN_SKILL_PREFIX) :], names)

    def test_plugin_skills_have_the_required_sections(self):
        for name in plugin_names():
            directory = SKILLS_DIRECTORY / f"{PLUGIN_SKILL_PREFIX}{name}"
            if not directory.is_dir():
                continue
            with self.subTest(plugin=name):
                _, _, body = read_skill(directory)
                headings = [heading.replace("`", "") for heading in HEADING_PATTERN.findall(body)]
                self.assertEqual([h for h in headings if h in REQUIRED_HEADINGS], list(REQUIRED_HEADINGS))

    def test_identifiers_in_a_plugin_skill_exist_in_the_plugin(self):
        for name in plugin_names():
            directory = SKILLS_DIRECTORY / f"{PLUGIN_SKILL_PREFIX}{name}"
            if not directory.is_dir():
                continue
            with self.subTest(plugin=name):
                _, _, body = read_skill(directory)
                text = known_text(name)
                unknown = sorted({found for found in IDENTIFIER_PATTERN.findall(body) if found not in text})
                self.assertEqual(unknown, [], "identificadores que o plugin não conhece")

    def test_files_cited_in_skills_exist(self):
        for directory in all_skill_directories():
            with self.subTest(skill=directory.name):
                _, _, body = read_skill(directory)
                missing = sorted(
                    cited for cited in set(PLUGIN_PATH_PATTERN.findall(body)) if not (REPOSITORY_ROOT / cited).exists()
                )
                self.assertEqual(missing, [])


if __name__ == "__main__":
    unittest.main()
