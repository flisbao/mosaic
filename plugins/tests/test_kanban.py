import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import kanban  # noqa: E402


class KanbanTestCase(unittest.TestCase):
    def setUp(self):
        self._directory = tempfile.TemporaryDirectory()
        self.root = Path(self._directory.name) / "kanban"
        self.root.mkdir()

    def tearDown(self):
        self._directory.cleanup()

    def write_card(self, column, name, content="# Titulo\n"):
        folder = self.root / column
        folder.mkdir(exist_ok=True)
        path = folder / name
        path.write_text(content, encoding="utf-8")
        return path

    def summary_lines(self, snapshot):
        return {line["text"] for line in snapshot["summary"]["blocks"][0]["lines"]}


class SnapshotTests(KanbanTestCase):
    def test_snapshot_counts_cards_per_column(self):
        self.write_card("inbox", "a.md")
        self.write_card("inbox", "b.md")
        self.write_card("done", "c.md")
        lines = self.summary_lines(kanban.build_snapshot(self.root))
        self.assertIn("inbox 2", lines)
        self.assertIn("done 1", lines)
        self.assertIn("backlog 0", lines)

    def test_title_comes_from_first_heading(self):
        self.write_card("inbox", "a.md", "texto solto\n# Corrigir login\n")
        snapshot = kanban.build_snapshot(self.root)
        item = snapshot["detail"][0]["items"][0]
        self.assertEqual(item["text"], "Corrigir login")
        self.assertEqual(item["id"], "inbox/a.md")

    def test_title_falls_back_to_file_name(self):
        self.write_card("inbox", "sem-titulo.md", "so texto\n")
        item = kanban.build_snapshot(self.root)["detail"][0]["items"][0]
        self.assertEqual(item["text"], "sem-titulo")

    def test_front_matter_fields_do_not_break_title(self):
        self.write_card(
            "inbox", "a.md", "---\njira: PAY-1\nprioridade: alta\n---\n# Com metadados\n"
        )
        item = kanban.build_snapshot(self.root)["detail"][0]["items"][0]
        self.assertEqual(item["text"], "Com metadados")

    def test_malformed_front_matter_does_not_break_snapshot(self):
        self.write_card("inbox", "a.md", "---\njira: PAY-1\n# Sem fechamento\n")
        (self.root / "inbox" / "b.md").write_bytes(b"\xff\xfe nao eh utf8 \n")
        items = kanban.build_snapshot(self.root)["detail"][0]["items"]
        self.assertEqual(len(items), 2)

    def test_missing_column_folder_counts_zero(self):
        snapshot = kanban.build_snapshot(self.root)
        self.assertEqual(len(snapshot["detail"]), len(kanban.COLUMNS))
        self.assertIn("in progress 0", self.summary_lines(snapshot))

    def test_missing_root_does_not_break_snapshot(self):
        snapshot = kanban.build_snapshot(self.root / "nao-existe")
        self.assertIn("inbox 0", self.summary_lines(snapshot))

    def test_one_move_action_per_column_scoped_to_item(self):
        actions = kanban.build_snapshot(self.root)["actions"]
        self.assertEqual([a["params"]["to"] for a in actions], kanban.COLUMNS)
        self.assertTrue(all(a["applies_to"] == "item" for a in actions))
        self.assertTrue(all(a["id"] == "move" for a in actions))


class MetaTests(KanbanTestCase):
    def first_item(self):
        return kanban.build_snapshot(self.root)["detail"][0]["items"][0]

    def test_meta_comes_from_front_matter(self):
        self.write_card(
            "inbox", "a.md", "---\nprioridade: alta\njira: PAY-1\n---\n# Titulo\n"
        )
        self.assertEqual(
            self.first_item()["meta"], {"prioridade": "alta", "jira": "PAY-1"}
        )

    def test_meta_ignores_unknown_keys_and_blank_values(self):
        self.write_card(
            "inbox", "a.md", "---\nsegredo: x\nresponsavel:   \nprioridade: baixa\n---\n# T\n"
        )
        self.assertEqual(self.first_item()["meta"], {"prioridade": "baixa"})

    def test_meta_is_absent_without_front_matter(self):
        self.write_card("inbox", "a.md", "# Titulo\n")
        self.assertNotIn("meta", self.first_item())

    def test_malformed_front_matter_yields_no_meta(self):
        self.write_card("inbox", "a.md", "---\nprioridade: alta\n# Sem fechamento\n")
        self.assertNotIn("meta", self.first_item())

    def test_unreadable_bytes_yield_no_meta(self):
        (self.root / "inbox").mkdir()
        (self.root / "inbox" / "a.md").write_bytes(b"---\n\xff\xfe: x\n---\n# T\n")
        self.assertNotIn("meta", self.first_item())


class MoveTests(KanbanTestCase):
    def test_move_renames_file_between_columns(self):
        self.write_card("inbox", "a.md")
        kanban.move_card(self.root, "inbox/a.md", "done")
        self.assertFalse((self.root / "inbox" / "a.md").exists())
        self.assertTrue((self.root / "done" / "a.md").exists())

    def test_move_creates_missing_destination_folder(self):
        self.write_card("inbox", "a.md")
        kanban.move_card(self.root, "inbox/a.md", "backlog")
        self.assertTrue((self.root / "backlog" / "a.md").exists())

    def test_move_refuses_parent_traversal(self):
        outside = self.root.parent / "segredo.md"
        outside.write_text("nao mexer", encoding="utf-8")
        self.write_card("inbox", "a.md")
        invalid_ids = [
            "../segredo.md",
            "inbox/../../segredo.md",
            "inbox/../a.md",
            "/etc/passwd",
            "inbox//a.md",
            "inbox/a.md/..",
            "inbox\\a.md",
            "a.md",
        ]
        for item_id in invalid_ids:
            with self.assertRaises(ValueError, msg=item_id):
                kanban.move_card(self.root, item_id, "done")
        for destination in ["..", "../fora", "/tmp", "done/../.."]:
            with self.assertRaises(ValueError, msg=destination):
                kanban.move_card(self.root, "inbox/a.md", destination)
        self.assertTrue(outside.exists())
        self.assertTrue((self.root / "inbox" / "a.md").exists())

    def test_move_refuses_unknown_destination(self):
        self.write_card("inbox", "a.md")
        with self.assertRaises(ValueError):
            kanban.move_card(self.root, "inbox/a.md", "arquivo-morto")

    def test_move_refuses_non_markdown_and_missing_card(self):
        self.write_card("inbox", "nota.txt")
        with self.assertRaises(ValueError):
            kanban.move_card(self.root, "inbox/nota.txt", "done")
        with self.assertRaises(ValueError):
            kanban.move_card(self.root, "inbox/ausente.md", "done")

    def test_move_refuses_existing_target(self):
        self.write_card("inbox", "a.md", "origem")
        self.write_card("done", "a.md", "destino")
        with self.assertRaises(ValueError):
            kanban.move_card(self.root, "inbox/a.md", "done")
        self.assertEqual((self.root / "done" / "a.md").read_text(), "destino")
        self.assertEqual((self.root / "inbox" / "a.md").read_text(), "origem")

    def test_move_never_changes_card_content(self):
        content = "---\njira: X-1\n---\n# Titulo\ncorpo\n"
        self.write_card("inbox", "a.md", content)
        kanban.move_card(self.root, "inbox/a.md", "done")
        self.assertEqual((self.root / "done" / "a.md").read_text(encoding="utf-8"), content)


class CommandLineTests(KanbanTestCase):
    def run_main(self, arguments, stdin_text=""):
        import io
        import json
        from contextlib import redirect_stdout, redirect_stderr

        os.environ["KANBAN_DIR"] = str(self.root)
        out, err = io.StringIO(), io.StringIO()
        original_stdin = sys.stdin
        sys.stdin = io.StringIO(stdin_text)
        try:
            with redirect_stdout(out), redirect_stderr(err):
                code = kanban.main(arguments)
        finally:
            sys.stdin = original_stdin
        return code, out.getvalue(), err.getvalue(), json

    def test_snapshot_command_prints_valid_json(self):
        code, out, _, json = self.run_main(["snapshot"])
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out)["summary"]["title"], "Kanban")

    def test_move_command_moves_card_and_fails_cleanly(self):
        self.write_card("inbox", "a.md")
        code, out, _, _ = self.run_main(
            ["action", "move"], '{"to":"done","item_id":"inbox/a.md"}'
        )
        self.assertEqual(code, 0)
        self.assertTrue((self.root / "done" / "a.md").exists())
        code, _, err, _ = self.run_main(
            ["action", "move"], '{"to":"done","item_id":"../x.md"}'
        )
        self.assertEqual(code, 1)
        self.assertTrue(err.strip())


if __name__ == "__main__":
    unittest.main()
