import io
import json
import sys
import tempfile
import unittest
from datetime import datetime, tzinfo
from pathlib import Path
from unittest import mock
from zoneinfo import ZoneInfo

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "calendar"))

import calendar_widget  # noqa: E402

ZONE = ZoneInfo("America/Sao_Paulo")
NOW = datetime(2026, 10, 20, 10, 0, tzinfo=ZONE)
WINDOW_END = datetime(2026, 10, 26, 0, 0, tzinfo=ZONE)
EMPTY_LINES_BLOCK = {"type": "lines", "lines": [{"text": "nada até domingo", "tone": "muted"}]}


def at(day, hour, minute=0):
    return datetime(2026, 10, day, hour, minute, tzinfo=ZONE)


def event(title, start, end, **fields):
    values = {"id": title, "all_day": False}
    values.update(fields)
    return calendar_widget.Event(title=title, start=start, end=end, **values)


def snapshot(events=(), warnings=()):
    result = calendar_widget.SourceResult(events=list(events), warnings=list(warnings))
    return calendar_widget.build_snapshot(result, NOW, "Agenda")


def summary_rows(snapshot_data):
    return snapshot_data["summary"]["blocks"][0]["rows"]


class ConfigTests(unittest.TestCase):
    def test_source_is_required(self):
        with self.assertRaises(calendar_widget.ConfigError):
            calendar_widget.load_config({})

    def test_source_must_be_a_known_name(self):
        with self.assertRaises(calendar_widget.ConfigError) as context:
            calendar_widget.load_config({"CALENDAR_SOURCE": "outlook"})
        self.assertIn("CALENDAR_SOURCE", str(context.exception))

    def test_ics_urls_come_from_named_variables(self):
        config = calendar_widget.load_config({
            "CALENDAR_SOURCE": "ics",
            "CALENDAR_ICS_URL_ENV": "A, B",
            "A": " https://x/a.ics ",
            "B": "webcal://x/b.ics",
        })
        self.assertEqual(config.ics_urls, ["https://x/a.ics", "https://x/b.ics"])

    def test_ics_urls_come_from_file_one_per_line(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "links.txt"
            path.write_text("  https://x/a.ics  \n\nwebcal://x/b.ics\n", encoding="utf-8")
            config = calendar_widget.load_config({
                "CALENDAR_SOURCE": "ics",
                "CALENDAR_ICS_URL_FILE": str(path),
            })
        self.assertEqual(config.ics_urls, ["https://x/a.ics", "https://x/b.ics"])

    def test_unreadable_url_file_is_an_error_that_names_the_variable(self):
        with self.assertRaises(calendar_widget.ConfigError) as context:
            calendar_widget.load_config({
                "CALENDAR_SOURCE": "ics",
                "CALENDAR_ICS_URL_FILE": "/nao/existe/links.txt",
            })
        self.assertIn("CALENDAR_ICS_URL_FILE", str(context.exception))

    def test_invalid_line_in_url_file_cites_its_line_number(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "links.txt"
            path.write_text("https://x/a.ics\nftp://x/b.ics\n", encoding="utf-8")
            with self.assertRaises(calendar_widget.ConfigError) as context:
                calendar_widget.load_config({
                    "CALENDAR_SOURCE": "ics",
                    "CALENDAR_ICS_URL_FILE": str(path),
                })
        self.assertIn("linha 2", str(context.exception))
        self.assertNotIn("ftp://x/b.ics", str(context.exception))

    def test_missing_named_variable_is_an_error_that_names_it(self):
        with self.assertRaises(calendar_widget.ConfigError) as context:
            calendar_widget.load_config({
                "CALENDAR_SOURCE": "ics",
                "CALENDAR_ICS_URL_ENV": "AUSENTE",
            })
        self.assertIn("AUSENTE", str(context.exception))

    def test_empty_named_variable_is_an_error_that_names_it_without_leaking_the_others(self):
        with self.assertRaises(calendar_widget.ConfigError) as context:
            calendar_widget.load_config({
                "CALENDAR_SOURCE": "ics",
                "CALENDAR_ICS_URL_ENV": "A,B",
                "A": "   ",
                "B": "https://x/segredo-b.ics",
            })
        self.assertIn("'A'", str(context.exception))
        self.assertNotIn("segredo-b", str(context.exception))

    def test_ics_without_any_url_is_an_error(self):
        with self.assertRaises(calendar_widget.ConfigError):
            calendar_widget.load_config({"CALENDAR_SOURCE": "ics"})

    def test_ics_rejects_non_http_scheme_without_echoing_the_link(self):
        with self.assertRaises(calendar_widget.ConfigError) as context:
            calendar_widget.load_config({
                "CALENDAR_SOURCE": "ics",
                "CALENDAR_ICS_URL_ENV": "A",
                "A": "file:///etc/tokens-secretos",
            })
        self.assertIn("'A'", str(context.exception))
        self.assertNotIn("tokens-secretos", str(context.exception))

    def test_defaults(self):
        config = calendar_widget.load_config({"CALENDAR_SOURCE": "goa"})
        self.assertEqual((config.title, config.goa_account, config.calendars), ("Agenda", None, []))

    def test_goa_account_and_calendars_are_read(self):
        config = calendar_widget.load_config({
            "CALENDAR_SOURCE": "gcalcli",
            "CALENDAR_GOA_ACCOUNT": " conta@x.com ",
            "CALENDAR_CALENDARS": "Trabalho, ,Pessoal",
            "CALENDAR_TITLE": "Hoje e semana",
        })
        self.assertEqual(config.goa_account, "conta@x.com")
        self.assertEqual(config.calendars, ["Trabalho", "Pessoal"])
        self.assertEqual(config.title, "Hoje e semana")


class LocalZoneTests(unittest.TestCase):
    def test_valid_tz_variable_wins(self):
        self.assertEqual(calendar_widget.local_zone({"TZ": "America/Sao_Paulo"}).key, "America/Sao_Paulo")

    def test_invalid_tz_falls_back_to_the_localtime_link(self):
        with tempfile.TemporaryDirectory() as directory:
            link = Path(directory) / "localtime"
            link.symlink_to("/usr/share/zoneinfo/America/Sao_Paulo")
            with mock.patch.object(calendar_widget, "LOCALTIME_LINK", str(link)):
                zone = calendar_widget.local_zone({"TZ": "Nowhere/Land"})
        self.assertEqual(zone.key, "America/Sao_Paulo")

    def test_absent_tz_uses_the_localtime_link(self):
        with tempfile.TemporaryDirectory() as directory:
            link = Path(directory) / "localtime"
            link.symlink_to("/usr/share/zoneinfo/America/Sao_Paulo")
            with mock.patch.object(calendar_widget, "LOCALTIME_LINK", str(link)):
                zone = calendar_widget.local_zone({})
        self.assertEqual(zone.key, "America/Sao_Paulo")

    def test_missing_link_and_invalid_tz_does_not_crash(self):
        with mock.patch.object(calendar_widget, "LOCALTIME_LINK", "/nao/existe/localtime"):
            zone = calendar_widget.local_zone({"TZ": "Nowhere/Land"})
        self.assertIsInstance(zone, tzinfo)


class WindowTests(unittest.TestCase):
    def test_window_ends_next_monday_midnight(self):
        self.assertEqual(calendar_widget.week_window(NOW), (NOW, WINDOW_END))

    def test_sunday_night_window_keeps_late_sunday_and_drops_monday_midnight(self):
        sunday = datetime(2026, 10, 25, 22, 0, tzinfo=ZONE)
        _, window_end = calendar_widget.week_window(sunday)
        visible = calendar_widget.visible_events(
            [event("tarde", at(25, 23, 30), at(25, 23, 59)), event("segunda", at(26, 0), at(26, 1))],
            sunday,
            window_end,
        )
        self.assertEqual([item.title for item in visible], ["tarde"])

    def test_event_ending_now_is_hidden_and_starting_now_is_shown(self):
        ended = event("acabou", at(20, 9), NOW)
        starting = event("comeca", NOW, at(20, 11))
        visible = calendar_widget.visible_events([ended, starting], NOW, WINDOW_END)
        self.assertEqual([item.title for item in visible], ["comeca"])

    def test_cancelled_and_declined_are_dropped(self):
        events = [
            event("ok", at(20, 11), at(20, 12)),
            event("cancelado", at(20, 11), at(20, 12), cancelled=True),
            event("recusado", at(20, 11), at(20, 12), declined=True),
        ]
        visible = calendar_widget.visible_events(events, NOW, WINDOW_END)
        self.assertEqual([item.title for item in visible], ["ok"])

    def test_sorted_by_start_then_title(self):
        events = [
            event("b", at(21, 9), at(21, 10)),
            event("c", at(20, 15), at(20, 16)),
            event("a", at(21, 9), at(21, 10)),
        ]
        visible = calendar_widget.visible_events(events, NOW, WINDOW_END)
        self.assertEqual([item.title for item in visible], ["c", "a", "b"])


class SnapshotTests(unittest.TestCase):
    def test_summary_rows_label_today_other_days_and_all_day(self):
        events = [
            event("Hoje", at(20, 14), at(20, 15)),
            event("Quinta", at(22, 9), at(22, 10)),
            event("Feriado hoje", at(20, 0), at(21, 0), all_day=True),
            event("Sábado", at(24, 0), at(25, 0), all_day=True),
        ]
        labels = [(row["time"], row["text"]) for row in summary_rows(snapshot(events))]
        self.assertEqual(
            labels,
            [
                ("dia todo", "Feriado hoje"),
                ("14:00", "Hoje"),
                ("Qui 09:00", "Quinta"),
                ("Sáb dia todo", "Sábado"),
            ],
        )

    def test_ongoing_event_is_current_with_tag_agora(self):
        row = summary_rows(snapshot([event("Revisão", at(20, 9), at(20, 11))]))[0]
        self.assertEqual(row["tone"], "current")
        self.assertEqual(row["tags"], [{"text": "agora", "tone": "ok"}])

    def test_event_starting_now_is_current(self):
        row = summary_rows(snapshot([event("Começa agora", NOW, at(20, 11))]))[0]
        self.assertEqual(row["tags"], [{"text": "agora", "tone": "ok"}])

    def test_next_event_within_15_minutes_gets_warn_tag(self):
        row = summary_rows(snapshot([event("Daily", at(20, 10, 12), at(20, 10, 30))]))[0]
        self.assertEqual(row["tags"], [{"text": "em 12 min", "tone": "warn"}])

    def test_event_exactly_15_minutes_away_gets_warn_tag(self):
        row = summary_rows(snapshot([event("Limite", at(20, 10, 15), at(20, 10, 45))]))[0]
        self.assertEqual(row["tags"], [{"text": "em 15 min", "tone": "warn"}])

    def test_only_the_first_upcoming_event_gets_warn_tag(self):
        rows = summary_rows(snapshot([
            event("Daily", at(20, 10, 12), at(20, 10, 30)),
            event("Outra", at(20, 10, 16), at(20, 10, 45)),
        ]))
        self.assertEqual(rows[0]["tags"], [{"text": "em 12 min", "tone": "warn"}])
        self.assertEqual(rows[1]["tags"], [])

    def test_multi_day_all_day_started_yesterday_shows_today_without_agora(self):
        congress = event("Congresso", at(19, 0), at(22, 0), all_day=True)
        row = summary_rows(snapshot([congress]))[0]
        self.assertEqual((row["time"], row["tone"], row["tags"]), ("dia todo", "normal", []))
        detail = snapshot([congress])["detail"]
        self.assertEqual(detail[0]["heading"], "Hoje")
        self.assertEqual(detail[0]["items"][0]["text"], "dia todo Congresso")

    def test_empty_window_shows_muted_line(self):
        self.assertEqual(snapshot()["summary"]["blocks"], [EMPTY_LINES_BLOCK])

    def test_detail_sections_per_day(self):
        events = [
            event("Reunião", at(20, 14), at(20, 15), location="Sala 2"),
            event("Café", at(20, 16), at(20, 16, 30)),
            event("Standup", at(21, 9), at(21, 9, 15)),
            event("Feriado", at(22, 0), at(23, 0), all_day=True),
        ]
        detail = snapshot(events)["detail"]
        self.assertEqual([section["heading"] for section in detail], ["Hoje", "Amanhã", "Qui 22/10"])
        self.assertEqual(
            detail[0]["items"],
            [
                {"id": "Reunião", "text": "14:00–15:00 Reunião · Sala 2"},
                {"id": "Café", "text": "16:00–16:30 Café"},
            ],
        )
        self.assertEqual(detail[1]["items"], [{"id": "Standup", "text": "09:00–09:15 Standup"}])
        self.assertEqual(detail[2]["items"], [{"id": "Feriado", "text": "dia todo Feriado"}])

    def test_mini_counts_todays_events_and_shows_next_start(self):
        events = [
            event("em andamento", at(20, 9), at(20, 11)),
            event("depois", at(20, 14), at(20, 15)),
            event("amanhã", at(21, 9), at(21, 10)),
        ]
        self.assertEqual(snapshot(events)["mini"], {"value": "2", "text": "próx 14:00"})

    def test_mini_says_today_is_free_without_events(self):
        self.assertEqual(snapshot([event("amanhã", at(21, 9), at(21, 10))])["mini"], {"value": "0", "text": "hoje livre"})

    def test_mini_without_next_timed_event_says_no_next(self):
        events = [event("dia todo", at(20, 0), at(21, 0), all_day=True)]
        self.assertEqual(snapshot(events)["mini"], {"value": "1", "text": "sem próximos"})

    def test_warnings_are_appended_as_warn_lines(self):
        blocks = snapshot(warnings=["ICS 2 falhou"])["summary"]["blocks"]
        self.assertEqual(blocks[0], EMPTY_LINES_BLOCK)
        self.assertEqual(blocks[-1], {"type": "lines", "lines": [{"text": "ICS 2 falhou", "tone": "warn"}]})

    def test_snapshot_has_protocol_and_no_actions_or_alerts(self):
        data = snapshot([event("Reunião", at(20, 14), at(20, 15))])
        self.assertEqual(set(data), {"protocol", "summary", "detail", "mini"})
        self.assertEqual(data["protocol"], 1)
        self.assertEqual(set(data["summary"]), {"title", "blocks"})
        self.assertEqual(data["summary"]["title"], "Agenda")


class MainTests(unittest.TestCase):
    def test_usage_error_returns_2(self):
        stderr = io.StringIO()
        self.assertEqual(calendar_widget.main([], environ={}, stdout=io.StringIO(), stderr=stderr), 2)
        self.assertTrue(stderr.getvalue().startswith("uso:"))
        self.assertEqual(calendar_widget.main(["snapshot", "extra"], environ={}, stderr=io.StringIO()), 2)

    def test_config_error_returns_1_with_message(self):
        stderr = io.StringIO()
        code = calendar_widget.main(["snapshot"], environ={}, stdout=io.StringIO(), stderr=stderr)
        self.assertEqual(code, 1)
        self.assertTrue(stderr.getvalue().startswith("erro: "))

    def test_source_error_returns_1(self):
        def failing_source(config, now, window_end, zone):
            raise calendar_widget.SourceError("x")

        stderr = io.StringIO()
        code = calendar_widget.main(
            ["snapshot"],
            environ={"CALENDAR_SOURCE": "goa"},
            sources={"goa": failing_source},
            now=NOW,
            stdout=io.StringIO(),
            stderr=stderr,
        )
        self.assertEqual(code, 1)
        self.assertEqual(stderr.getvalue(), "erro: x\n")

    def test_placeholder_sources_report_not_implemented(self):
        stderr = io.StringIO()
        code = calendar_widget.main(
            ["snapshot"],
            environ={"CALENDAR_SOURCE": "ics", "CALENDAR_ICS_URL_ENV": "A", "A": "https://x/a.ics"},
            now=NOW,
            stdout=io.StringIO(),
            stderr=stderr,
        )
        self.assertEqual(code, 1)
        self.assertEqual(stderr.getvalue(), "erro: fonte ainda não implementada\n")

    def test_snapshot_prints_json_from_fake_source(self):
        received = []

        def fake_source(config, now, window_end, zone):
            received.append((config.source, now, window_end, zone.key))
            return calendar_widget.SourceResult(
                events=[event("Reunião", at(20, 14), at(20, 15))],
                warnings=[],
            )

        stdout = io.StringIO()
        code = calendar_widget.main(
            ["snapshot"],
            environ={"CALENDAR_SOURCE": "goa", "CALENDAR_TITLE": "Minha agenda", "TZ": "America/Sao_Paulo"},
            sources={"goa": fake_source},
            now=NOW,
            stdout=stdout,
            stderr=io.StringIO(),
        )
        self.assertEqual(code, 0)
        self.assertEqual(received, [("goa", NOW, WINDOW_END, "America/Sao_Paulo")])
        output = stdout.getvalue()
        self.assertIn("Reunião", output)
        data = json.loads(output)
        self.assertEqual(data["summary"]["title"], "Minha agenda")
        self.assertEqual(summary_rows(data)[0]["text"], "Reunião")


if __name__ == "__main__":
    unittest.main()
