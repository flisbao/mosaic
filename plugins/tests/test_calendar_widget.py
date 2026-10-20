import io
import json
import subprocess
import sys
import tempfile
import traceback
import unittest
import urllib.error
from datetime import date, datetime, timedelta, timezone, tzinfo
from pathlib import Path
from unittest import mock
from urllib.parse import parse_qs, quote, urlsplit
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


def parse_events(*lines, zone=ZONE):
    text = "\r\n".join(["BEGIN:VCALENDAR", "BEGIN:VEVENT", *lines, "END:VEVENT", "END:VCALENDAR"])
    events, _ = calendar_widget.parse_ics(text, zone)
    return events


def parse_event(*lines, zone=ZONE):
    return parse_events(*lines, zone=zone)[0]


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


class IcsParserTests(unittest.TestCase):
    def test_unfolds_lines_and_unescapes_text(self):
        text = (
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:1\r\nDTSTART:20261020T130000Z\r\n"
            "SUMMARY:Reuni\r\n ão\\, sala 2\\nandar 3\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        )
        events, _ = calendar_widget.parse_ics(text, ZONE)
        self.assertEqual(events[0].summary, "Reunião, sala 2\nandar 3")

    def test_escaped_semicolon_and_backslash_are_unescaped(self):
        event_ = parse_event("DTSTART:20261020T090000", "LOCATION:Sala A\\; porta\\\\fundos")
        self.assertEqual(event_.location, "Sala A; porta\\fundos")

    def test_uid_and_location_are_read(self):
        event_ = parse_event("UID:abc-123", "DTSTART:20261020T090000", "LOCATION:Sala 2")
        self.assertEqual((event_.uid, event_.location), ("abc-123", "Sala 2"))

    def test_utc_tzid_floating_and_date_values(self):
        utc = parse_event("DTSTART:20261020T130000Z")
        self.assertEqual(utc.start, datetime(2026, 10, 20, 13, tzinfo=timezone.utc))
        self.assertIs(utc.start.tzinfo, timezone.utc)
        named = parse_event('DTSTART;TZID="America/New_York":20261020T090000')
        self.assertEqual(named.start, datetime(2026, 10, 20, 9, tzinfo=ZoneInfo("America/New_York")))
        self.assertEqual(named.start.tzinfo.key, "America/New_York")
        floating = parse_event("DTSTART:20261020T090000")
        self.assertEqual(floating.start, at(20, 9))
        self.assertEqual(floating.start.tzinfo.key, ZONE.key)
        whole_day = parse_event("DTSTART;VALUE=DATE:20261020")
        self.assertEqual(whole_day.start, date(2026, 10, 20))

    def test_unknown_tzid_falls_back_to_local_zone(self):
        event_ = parse_event("DTSTART;TZID=W. Europe Standard Time:20261020T090000")
        self.assertEqual(event_.start, at(20, 9))
        self.assertEqual(event_.start.tzinfo.key, ZONE.key)

    def test_dtend_is_read_and_duration_is_none(self):
        event_ = parse_event("DTSTART:20261020T090000", "DTEND:20261020T100000")
        self.assertEqual(event_.end, at(20, 10))
        self.assertIsNone(event_.duration)

    def test_duration_instead_of_dtend(self):
        timed = parse_event("DTSTART:20261020T090000", "DURATION:PT1H30M")
        self.assertIsNone(timed.end)
        self.assertEqual(timed.duration, timedelta(hours=1, minutes=30))
        whole_day = parse_event("DTSTART;VALUE=DATE:20261020", "DURATION:P1D")
        self.assertEqual(whole_day.duration, timedelta(days=1))

    def test_exdate_lists_rdate_recurrence_id_and_status(self):
        event_ = parse_event(
            "DTSTART:20261020T090000",
            'EXDATE;TZID="America/Sao_Paulo":20261021T090000,20261028T090000',
            "RDATE:20261030T120000Z",
            'RECURRENCE-ID;TZID="America/Sao_Paulo":20261021T090000',
            "STATUS:CANCELLED",
            "RRULE:FREQ=WEEKLY;BYDAY=TU",
        )
        self.assertEqual(event_.exdates, [at(21, 9), at(28, 9)])
        self.assertEqual(event_.rdates, [datetime(2026, 10, 30, 12, tzinfo=timezone.utc)])
        self.assertEqual(event_.recurrence_id, at(21, 9))
        self.assertEqual(event_.status, "CANCELLED")
        self.assertEqual(event_.rrule, "FREQ=WEEKLY;BYDAY=TU")

    def test_defaults_when_properties_are_absent(self):
        event_ = parse_event("DTSTART:20261020T090000")
        self.assertEqual((event_.uid, event_.summary, event_.location), ("", "", ""))
        self.assertIsNone(event_.end)
        self.assertIsNone(event_.duration)
        self.assertIsNone(event_.rrule)
        self.assertIsNone(event_.recurrence_id)
        self.assertEqual((event_.exdates, event_.rdates), ([], []))
        self.assertEqual(event_.status, "CONFIRMED")

    def test_calendar_name(self):
        _, name = calendar_widget.parse_ics("BEGIN:VCALENDAR\r\nX-WR-CALNAME:Pessoal\r\nEND:VCALENDAR\r\n", ZONE)
        self.assertEqual(name, "Pessoal")
        _, missing = calendar_widget.parse_ics("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n", ZONE)
        self.assertEqual(missing, "")

    def test_empty_text_has_no_events_and_no_name(self):
        self.assertEqual(calendar_widget.parse_ics("", ZONE), ([], ""))

    def test_events_inside_other_components_are_ignored(self):
        text = "\r\n".join([
            "BEGIN:VCALENDAR",
            "BEGIN:VTIMEZONE",
            "TZID:America/Sao_Paulo",
            "BEGIN:STANDARD",
            "DTSTART:19700101T000000",
            "END:STANDARD",
            "END:VTIMEZONE",
            "BEGIN:VEVENT",
            "UID:a",
            "DTSTART:20261020T090000",
            "SUMMARY:Real",
            "BEGIN:VALARM",
            "ACTION:DISPLAY",
            "DTSTART:20261019T000000",
            "SUMMARY:Alarme",
            "END:VALARM",
            "END:VEVENT",
            "END:VCALENDAR",
        ])
        events, _ = calendar_widget.parse_ics(text, ZONE)
        self.assertEqual([item.summary for item in events], ["Real"])

    def test_event_without_valid_dtstart_is_skipped(self):
        text = "\r\n".join([
            "BEGIN:VCALENDAR",
            "BEGIN:VEVENT",
            "SUMMARY:Sem início",
            "END:VEVENT",
            "BEGIN:VEVENT",
            "DTSTART:20261399T090000",
            "SUMMARY:Data inválida",
            "END:VEVENT",
            "BEGIN:VEVENT",
            "DTSTART:20261020T090000",
            "SUMMARY:Com início",
            "END:VEVENT",
            "END:VCALENDAR",
        ])
        events, _ = calendar_widget.parse_ics(text, ZONE)
        self.assertEqual([item.summary for item in events], ["Com início"])


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


NEW_YORK = ZoneInfo("America/New_York")


def parse_calendar(*blocks):
    lines = ["BEGIN:VCALENDAR"]
    for block in blocks:
        lines += ["BEGIN:VEVENT", *block, "END:VEVENT"]
    lines.append("END:VCALENDAR")
    events, _ = calendar_widget.parse_ics("\r\n".join(lines), ZONE)
    return events


def sao_paulo_series(rule, *extra, uid="serie", summary="Série", start="20261014T090000", end="20261014T093000"):
    return [
        f"UID:{uid}",
        f"SUMMARY:{summary}",
        f"DTSTART;TZID=America/Sao_Paulo:{start}",
        f"DTEND;TZID=America/Sao_Paulo:{end}",
        f"RRULE:{rule}",
        *extra,
    ]


def occurrences(lines, window_start=NOW, window_end=WINDOW_END, zone=ZONE):
    return calendar_widget.expand_occurrences(parse_event(*lines), window_start, window_end, zone)


def in_window(blocks, window_start=NOW, window_end=WINDOW_END, calendar="Pessoal"):
    return calendar_widget.ics_events_in_window(parse_calendar(*blocks), calendar, window_start, window_end, ZONE)


class RecurrenceTests(unittest.TestCase):
    def test_daily_with_interval_and_count(self):
        found = occurrences(sao_paulo_series("FREQ=DAILY;INTERVAL=2;COUNT=5", end="20261014T103000"))
        self.assertEqual(found, [at(20, 9), at(22, 9)])

    def test_weekly_byday_with_until(self):
        lines = sao_paulo_series(
            "FREQ=WEEKLY;BYDAY=MO,WE,FR;UNTIL=20261023T235959Z", start="20261019T080000", end="20261019T083000"
        )
        found = occurrences(lines, window_end=datetime(2026, 11, 2, tzinfo=ZONE))
        self.assertEqual(found, [at(21, 8), at(23, 8)])

    def test_until_in_utc_is_compared_in_the_start_time_zone(self):
        lines = sao_paulo_series("FREQ=DAILY;UNTIL=20261022T115959Z", start="20261021T090000", end="20261021T100000")
        self.assertEqual(occurrences(lines), [at(21, 9)])

    def test_monthly_positional_byday(self):
        lines = sao_paulo_series("FREQ=MONTHLY;BYDAY=-1FR", start="20260102T100000", end="20260102T110000")
        self.assertEqual(occurrences(lines), [])
        end_of_month = datetime(2026, 11, 1, tzinfo=ZONE)
        self.assertEqual(occurrences(lines, at(27, 0), end_of_month), [at(30, 10)])

    def test_monthly_bymonthday_skips_missing_days_and_counts_from_the_end(self):
        thirty_first = sao_paulo_series("FREQ=MONTHLY;BYMONTHDAY=31", start="20260131T100000", end="20260131T110000")
        september = (datetime(2026, 9, 25, tzinfo=ZONE), datetime(2026, 10, 5, tzinfo=ZONE))
        self.assertEqual(occurrences(thirty_first, *september), [])
        last_day = sao_paulo_series("FREQ=MONTHLY;BYMONTHDAY=-1", start="20260131T100000", end="20260131T110000")
        self.assertEqual(occurrences(last_day, at(25, 0), datetime(2026, 11, 2, tzinfo=ZONE)), [at(31, 10)])

    def test_yearly_bymonth_and_bymonthday(self):
        lines = sao_paulo_series("FREQ=YEARLY;BYMONTH=10,11;BYMONTHDAY=22", start="20240222T090000", end="20240222T100000")
        self.assertEqual(occurrences(lines), [at(22, 9)])
        november = datetime(2026, 11, 22, 8, tzinfo=ZONE), datetime(2026, 11, 23, tzinfo=ZONE)
        self.assertEqual(occurrences(lines, *november), [datetime(2026, 11, 22, 9, tzinfo=ZONE)])

    def test_yearly_bymonthday_without_bymonth_expands_every_month(self):
        lines = sao_paulo_series("FREQ=YEARLY;BYMONTHDAY=-1", start="20240110T100000", end="20240110T110000")
        window = at(25, 0), datetime(2026, 11, 2, tzinfo=ZONE)
        self.assertEqual(occurrences(lines, *window), [at(31, 10)])

    def test_wkst_changes_biweekly_weeks(self):
        window = datetime(1997, 8, 1, tzinfo=NEW_YORK), datetime(1997, 9, 1, tzinfo=NEW_YORK)
        expected = {"MO": [5, 10, 19, 24], "SU": [5, 17, 19, 31]}
        for week_start, days in expected.items():
            lines = [
                "DTSTART;TZID=America/New_York:19970805T090000",
                f"RRULE:FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST={week_start}",
            ]
            found = occurrences(lines, *window, zone=NEW_YORK)
            self.assertEqual(found, [datetime(1997, 8, day, 9, tzinfo=NEW_YORK) for day in days])

    def test_weekly_keeps_wall_clock_across_dst(self):
        lines = ["DTSTART;TZID=America/New_York:20270308T090000", "RRULE:FREQ=WEEKLY"]
        window = datetime(2027, 3, 15, tzinfo=NEW_YORK), datetime(2027, 3, 22, tzinfo=NEW_YORK)
        found = occurrences(lines, *window, zone=NEW_YORK)
        self.assertEqual(found, [datetime(2027, 3, 15, 9, tzinfo=NEW_YORK)])
        self.assertEqual(found[0].astimezone(timezone.utc).hour, 13)

    def test_long_running_daily_from_years_ago(self):
        lines = sao_paulo_series("FREQ=DAILY", start="20100101T090000", end="20100101T093000")
        found = occurrences(lines)
        self.assertEqual(found, [at(day, 9) for day in range(21, 26)])

    def test_unsupported_part_raises(self):
        rules = [
            "FREQ=DAILY;BYSETPOS=1",
            "FREQ=DAILY;BYHOUR=9",
            "FREQ=YEARLY;BYWEEKNO=20",
            "FREQ=YEARLY;BYYEARDAY=100",
            "FREQ=HOURLY",
            "FREQ=WEEKLY;BYDAY=1MO",
            "INTERVAL=2",
        ]
        for rule in rules:
            with self.subTest(rule=rule), self.assertRaises(calendar_widget.UnsupportedRule):
                occurrences(sao_paulo_series(rule))

    def test_exdate_removes_and_rdate_adds(self):
        lines = sao_paulo_series(
            "FREQ=DAILY;COUNT=5",
            "EXDATE;TZID=America/Sao_Paulo:20261022T090000",
            "RDATE;TZID=America/Sao_Paulo:20261024T090000",
            start="20261019T090000",
            end="20261019T093000",
        )
        self.assertEqual(occurrences(lines), [at(21, 9), at(23, 9), at(24, 9)])

    def test_recurrence_id_override_moves_and_cancelled_override_removes(self):
        master = sao_paulo_series("FREQ=DAILY;COUNT=4", uid="a", summary="Original", start="20261021T090000", end="20261021T100000")
        moved = [
            "UID:a",
            "SUMMARY:Movido",
            "RECURRENCE-ID;TZID=America/Sao_Paulo:20261022T090000",
            "DTSTART;TZID=America/Sao_Paulo:20261022T150000",
            "DTEND;TZID=America/Sao_Paulo:20261022T160000",
        ]
        removed = [
            "UID:a",
            "SUMMARY:Removido",
            "STATUS:cancelled",
            "RECURRENCE-ID;TZID=America/Sao_Paulo:20261023T090000",
            "DTSTART;TZID=America/Sao_Paulo:20261023T090000",
        ]
        result = in_window([master, moved, removed])
        found = [(item.title, item.start) for item in sorted(result.events, key=lambda item: item.start)]
        self.assertEqual(found, [("Original", at(21, 9)), ("Movido", at(22, 15)), ("Original", at(24, 9))])

    def test_unsupported_rule_becomes_warning_and_others_survive(self):
        yoga = sao_paulo_series("FREQ=DAILY;BYSETPOS=1", uid="ioga", summary="Ioga")
        simple = ["UID:s", "SUMMARY:Simples", "DTSTART;TZID=America/Sao_Paulo:20261021T090000", "DTEND;TZID=America/Sao_Paulo:20261021T100000"]
        result = in_window([yoga, simple])
        self.assertEqual(result.warnings, ["repetição não suportada em “Ioga”"])
        self.assertEqual([item.title for item in result.events], ["Simples"])

    def test_event_ids_are_uid_and_start(self):
        timed = ["UID:x", "SUMMARY:Um", "DTSTART:20261021T120000Z", "DTEND:20261021T130000Z"]
        all_day = ["UID:y", "SUMMARY:Dois", "DTSTART;VALUE=DATE:20261021"]
        result = in_window([timed, all_day])
        ids = {item.title: item.id for item in result.events}
        self.assertEqual(ids, {"Um": "x@2026-10-21T12:00:00+00:00", "Dois": "y@2026-10-21"})

    def test_long_event_ids_are_shortened_deterministically(self):
        long_uid = "u" * 300

        def ids_for(start):
            block = [f"UID:{long_uid}", "SUMMARY:Longo", f"DTSTART:{start}", "DTEND:20261022T130000Z"]
            return [item.id for item in in_window([block]).events]

        first = ids_for("20261021T120000Z")
        self.assertEqual(len(first[0]), 200)
        self.assertEqual(first, ids_for("20261021T120000Z"))
        self.assertNotEqual(first, ids_for("20261021T120100Z"))

    def test_all_day_event_becomes_local_midnight_range(self):
        block = ["UID:d", "SUMMARY:Feriado", "LOCATION:Casa", "DTSTART;VALUE=DATE:20261021", "DTEND;VALUE=DATE:20261023"]
        (item,) = in_window([block], calendar="Pessoal").events
        self.assertEqual((item.start, item.end), (at(21, 0), at(23, 0)))
        self.assertEqual((item.all_day, item.location, item.calendar, item.cancelled), (True, "Casa", "Pessoal", False))

    def test_cancelled_status_is_case_insensitive(self):
        block = ["UID:c", "SUMMARY:Cancelado", "STATUS:Cancelled", "DTSTART:20261021T120000Z", "DTEND:20261021T130000Z"]
        (item,) = in_window([block]).events
        self.assertTrue(item.cancelled)


def ics_document(*lines, calendar_name=None):
    header = [f"X-WR-CALNAME:{calendar_name}"] if calendar_name is not None else []
    return "\r\n".join(["BEGIN:VCALENDAR", *header, "VERSION:2.0", "BEGIN:VEVENT", *lines, "END:VEVENT", "END:VCALENDAR"])


def meeting(uid, summary, hour):
    return [f"UID:{uid}", f"SUMMARY:{summary}", f"DTSTART:20261020T{hour:02d}0000", f"DTEND:20261020T{hour + 1:02d}0000"]


def ics_config(*urls):
    return calendar_widget.Config(source="ics", title="Agenda", ics_urls=list(urls), goa_account=None, calendars=[])


def fake_fetch(responses):
    def fetch(url):
        value = responses[url]
        if isinstance(value, Exception):
            raise value
        return value

    return fetch


def run_ics_source(config, fetch=None):
    if fetch is None:
        return calendar_widget.ics_source(config, NOW, WINDOW_END, ZONE)
    return calendar_widget.ics_source(config, NOW, WINDOW_END, ZONE, fetch=fetch)


def http_error(code):
    error = urllib.error.HTTPError("https://segredo.example.com/a.ics", code, "erro", None, io.BytesIO(b""))
    error.close()
    return error


class IcsSourceTests(unittest.TestCase):
    def test_default_sources_point_to_ics_source(self):
        self.assertIs(calendar_widget.DEFAULT_SOURCES[calendar_widget.SOURCE_ICS], calendar_widget.ics_source)

    def test_merges_several_links_in_link_order(self):
        responses = {
            "https://a.example/a.ics": ics_document(*meeting("a", "Primeira", 14), calendar_name="Trabalho"),
            "https://b.example/b.ics": ics_document(*meeting("b", "Segunda", 15), calendar_name="Família"),
        }
        result = run_ics_source(ics_config(*responses), fake_fetch(responses))
        self.assertEqual([item.title for item in result.events], ["Primeira", "Segunda"])
        self.assertEqual([item.calendar for item in result.events], ["Trabalho", "Família"])
        self.assertEqual(result.warnings, [])

    def test_link_without_calendar_name_uses_its_number(self):
        responses = {
            "https://a.example/a.ics": ics_document(*meeting("a", "Sem nome", 14)),
            "https://b.example/b.ics": ics_document(*meeting("b", "Sem nome", 15), calendar_name=""),
        }
        result = run_ics_source(ics_config(*responses), fake_fetch(responses))
        self.assertEqual([item.calendar for item in result.events], ["ICS 1", "ICS 2"])

    def test_partial_failure_warns_without_leaking_url(self):
        responses = {
            "https://a.example/a.ics": ics_document(*meeting("a", "Ok", 14)),
            "https://segredo.example.com/b.ics": urllib.error.URLError("segredo.example.com"),
        }
        result = run_ics_source(ics_config(*responses), fake_fetch(responses))
        self.assertEqual([item.title for item in result.events], ["Ok"])
        self.assertEqual(result.warnings, ["agenda ICS 2 indisponível"])
        rendered = repr(result)
        self.assertNotIn("segredo", rendered)
        self.assertNotIn("example.com", rendered)

    def test_all_links_failing_raises_source_error_without_leaking(self):
        responses = {
            "https://segredo.example.com/a.ics": urllib.error.URLError("segredo.example.com"),
            "https://segredo.example.com/b.ics": http_error(500),
        }
        with self.assertRaises(calendar_widget.SourceError) as context:
            run_ics_source(ics_config(*responses), fake_fetch(responses))
        message = str(context.exception)
        self.assertEqual(message, "nenhuma agenda ICS respondeu; confira o link (seção ICS do README)")
        rendered = "".join(traceback.format_exception(context.exception))
        self.assertNotIn("segredo", rendered)
        self.assertIsNone(context.exception.__cause__)

    def test_http_error_and_oversized_body_count_as_failure(self):
        oversized = io.BytesIO(b"x" * (calendar_widget.MAXIMUM_ICS_BYTES + 1))
        valid = io.BytesIO(ics_document(*meeting("a", "Ok", 14)).encode("utf-8"))
        with mock.patch.object(calendar_widget.urllib.request, "urlopen", side_effect=[http_error(404), oversized]):
            with self.assertRaises(calendar_widget.SourceError):
                run_ics_source(ics_config("https://a.example/a.ics", "https://b.example/b.ics"))
        with mock.patch.object(calendar_widget.urllib.request, "urlopen", side_effect=[oversized, valid]):
            result = run_ics_source(ics_config("https://a.example/a.ics", "https://b.example/b.ics"))
        self.assertEqual([item.title for item in result.events], ["Ok"])
        self.assertEqual(result.warnings, ["agenda ICS 1 indisponível"])

    def test_invalid_ics_counts_as_failure(self):
        responses = {
            "https://a.example/a.ics": "<html>não é um calendário</html>",
            "https://b.example/b.ics": ics_document(*meeting("b", "Ok", 15)),
        }
        result = run_ics_source(ics_config(*responses), fake_fetch(responses))
        self.assertEqual([item.title for item in result.events], ["Ok"])
        self.assertEqual(result.warnings, ["agenda ICS 1 indisponível"])

    def test_invalid_ics_on_every_link_raises_source_error(self):
        responses = {"https://a.example/a.ics": "lixo"}
        with self.assertRaises(calendar_widget.SourceError):
            run_ics_source(ics_config(*responses), fake_fetch(responses))


class FetchUrlTests(unittest.TestCase):
    def test_decodes_utf8_and_replaces_invalid_bytes(self):
        body = "Reunião".encode("utf-8") + b"\xff"
        with mock.patch.object(calendar_widget.urllib.request, "urlopen", return_value=io.BytesIO(body)) as urlopen:
            text = calendar_widget.fetch_url("https://a.example/a.ics")
        self.assertEqual(text, "Reunião�")
        self.assertEqual(urlopen.call_args.kwargs["timeout"], calendar_widget.REQUEST_TIMEOUT_SECONDS)

    def test_body_over_the_limit_is_rejected(self):
        body = io.BytesIO(b"x" * (calendar_widget.MAXIMUM_ICS_BYTES + 1))
        with mock.patch.object(calendar_widget.urllib.request, "urlopen", return_value=body):
            with self.assertRaises(calendar_widget.FetchError):
                calendar_widget.fetch_url("https://a.example/a.ics")


GOA_SERVICE = "org.gnome.OnlineAccounts"
GOA_OBJECTS_PATH = "/org/gnome/OnlineAccounts"
GOA_ACCOUNT_INTERFACE = "org.gnome.OnlineAccounts.Account"
GOA_OAUTH2_INTERFACE = "org.gnome.OnlineAccounts.OAuth2Based"
OBJECT_MANAGER_INTERFACE = "org.freedesktop.DBus.ObjectManager"
GOOGLE_API = "https://www.googleapis.com/calendar/v3"
CALENDAR_LIST_URL = f"{GOOGLE_API}/users/me/calendarList"
ACCOUNT_ONE = "/org/gnome/OnlineAccounts/Accounts/account_1"
ACCOUNT_TWO = "/org/gnome/OnlineAccounts/Accounts/account_2"
ACCOUNT_THREE = "/org/gnome/OnlineAccounts/Accounts/account_3"
PERSONAL_ID = "pessoal@grupo.calendar.google.com"
WORK_ID = "trabalho@grupo.calendar.google.com"
HIDDEN_ID = "oculto@grupo.calendar.google.com"
TOKEN = "TOKEN-GOA"
MANAGED_ARGUMENTS = [GOA_SERVICE, GOA_OBJECTS_PATH, OBJECT_MANAGER_INTERFACE, "GetManagedObjects"]
MANAGED_KEY = tuple(MANAGED_ARGUMENTS[1:])
NO_GOOGLE_ACCOUNT_MESSAGE = "nenhuma conta Google com agenda no GNOME Online Accounts; veja a seção GOA do README"
GOOGLE_UNAVAILABLE_MESSAGE = "não foi possível consultar a API do Google Agenda"
ALL_ACCOUNTS_FAILED_MESSAGE = "nenhuma conta Google respondeu; veja a seção GOA do README"


def goa_account_objects(path, email, provider="google", calendar_disabled=False, attention_needed=False):
    properties = {
        "ProviderType": {"type": "s", "data": provider},
        "PresentationIdentity": {"type": "s", "data": email},
        "CalendarDisabled": {"type": "b", "data": calendar_disabled},
        "AttentionNeeded": {"type": "b", "data": attention_needed},
    }
    return {path: {GOA_ACCOUNT_INTERFACE: properties, GOA_OAUTH2_INTERFACE: {}}}


GOOGLE_ACCOUNT = goa_account_objects(ACCOUNT_ONE, "eu@gmail.com")


def managed_objects(*accounts):
    objects = {}
    for account in accounts:
        objects.update(account)
    return {"type": "a{oa{sa{sv}}}", "data": [objects]}


def token_reply(token):
    return {"type": "si", "data": [token, 3600]}


def token_arguments(path):
    return [GOA_SERVICE, path, GOA_OAUTH2_INTERFACE, "GetAccessToken"]


def token_key(path):
    return tuple(token_arguments(path)[1:])


def ensure_arguments(path):
    return [GOA_SERVICE, path, GOA_ACCOUNT_INTERFACE, "EnsureCredentials"]


def ensure_key(path):
    return tuple(ensure_arguments(path)[1:])


def events_path(calendar_id):
    return f"{GOOGLE_API}/calendars/{quote(calendar_id, safe='')}/events"


def calendar_item(calendar_id, summary, selected=True, summary_override=None):
    item = {"id": calendar_id, "summary": summary, "selected": selected}
    if summary_override is not None:
        item["summaryOverride"] = summary_override
    return item


def calendar_list(*items):
    return {"items": list(items)}


def google_event(event_id, summary, start, end, **fields):
    return {"id": event_id, "summary": summary, "start": start, "end": end, **fields}


def date_time(text):
    return {"dateTime": text}


def meeting_event(event_id="e1", summary="Reunião", **fields):
    return google_event(
        event_id,
        summary,
        date_time("2026-10-20T14:00:00-03:00"),
        date_time("2026-10-20T15:00:00-03:00"),
        **fields,
    )


def query_of(url):
    return parse_qs(urlsplit(url).query)


def titles(result):
    return [item.title for item in result.events]


def goa_config(goa_account=None, calendars=()):
    return calendar_widget.Config(
        source="goa", title="Agenda", ics_urls=[], goa_account=goa_account, calendars=list(calendars)
    )


def run_goa_source(config, busctl, http_get):
    return calendar_widget.goa_source(config, NOW, WINDOW_END, ZONE, busctl=busctl, http_get=http_get)


def single_calendar_fakes(events):
    busctl = FakeBusctl({MANAGED_KEY: managed_objects(GOOGLE_ACCOUNT), token_key(ACCOUNT_ONE): token_reply(TOKEN)})
    google = FakeGoogle(
        {
            (CALENDAR_LIST_URL, TOKEN): calendar_list(calendar_item(PERSONAL_ID, "Pessoal")),
            (events_path(PERSONAL_ID), TOKEN): {"items": events},
        }
    )
    return busctl, google


class FakeBusctl:
    def __init__(self, replies):
        self.replies = replies
        self.calls = []

    def __call__(self, arguments):
        self.calls.append(list(arguments))
        reply = self.replies[tuple(arguments[1:])]
        if isinstance(reply, list):
            reply = reply.pop(0)
        if isinstance(reply, Exception):
            raise reply
        return reply


class FakeGoogle:
    def __init__(self, responses):
        self.responses = responses
        self.calls = []

    def __call__(self, url, token):
        self.calls.append((url, token))
        reply = self.responses[(url.split("?")[0], token)]
        if isinstance(reply, list):
            reply = reply.pop(0)
        if isinstance(reply, Exception):
            raise reply
        return reply


class GoaSourceTests(unittest.TestCase):
    def test_default_sources_point_to_goa_source(self):
        self.assertIs(calendar_widget.DEFAULT_SOURCES[calendar_widget.SOURCE_GOA], calendar_widget.goa_source)

    def test_reads_selected_calendars_and_events(self):
        busctl = FakeBusctl({MANAGED_KEY: managed_objects(GOOGLE_ACCOUNT), token_key(ACCOUNT_ONE): token_reply(TOKEN)})
        google = FakeGoogle(
            {
                (CALENDAR_LIST_URL, TOKEN): calendar_list(
                    calendar_item(PERSONAL_ID, "Pessoal"),
                    calendar_item(WORK_ID, "Trabalho", summary_override="Work"),
                    calendar_item(HIDDEN_ID, "Oculto", selected=False),
                ),
                (events_path(PERSONAL_ID), TOKEN): {"items": [meeting_event("e1", location="Sala 1")]},
                (events_path(WORK_ID), TOKEN): {"items": [meeting_event("e2", "Revisão")]},
            }
        )
        result = run_goa_source(goa_config(), busctl, google)
        self.assertEqual(busctl.calls, [MANAGED_ARGUMENTS, token_arguments(ACCOUNT_ONE)])
        self.assertEqual(google.calls[0], (CALENDAR_LIST_URL, TOKEN))
        self.assertEqual(
            [(item.id, item.title, item.calendar, item.location) for item in result.events],
            [("e1", "Reunião", "Pessoal", "Sala 1"), ("e2", "Revisão", "Work", "")],
        )
        self.assertEqual(result.warnings, [])

    def test_events_query_uses_window_and_quoted_calendar_id(self):
        busctl, google = single_calendar_fakes([meeting_event()])
        run_goa_source(goa_config(), busctl, google)
        events_url = google.calls[1][0]
        self.assertIn(f"/calendars/{quote(PERSONAL_ID, safe='')}/events?", events_url)
        self.assertEqual(
            query_of(events_url),
            {
                "timeMin": [NOW.isoformat()],
                "timeMax": [WINDOW_END.isoformat()],
                "singleEvents": ["true"],
                "orderBy": ["startTime"],
                "maxResults": [str(calendar_widget.GOOGLE_PAGE_SIZE)],
            },
        )

    def test_pagination_follows_next_page_token(self):
        busctl = FakeBusctl({MANAGED_KEY: managed_objects(GOOGLE_ACCOUNT), token_key(ACCOUNT_ONE): token_reply(TOKEN)})
        google = FakeGoogle(
            {
                (CALENDAR_LIST_URL, TOKEN): [
                    {"items": [calendar_item(PERSONAL_ID, "Pessoal")], "nextPageToken": "lista-2"},
                    {"items": [calendar_item(WORK_ID, "Trabalho")]},
                ],
                (events_path(PERSONAL_ID), TOKEN): [
                    {"items": [meeting_event("e1", "Primeira")], "nextPageToken": "eventos-2"},
                    {"items": [meeting_event("e2", "Segunda")]},
                ],
                (events_path(WORK_ID), TOKEN): {"items": []},
            }
        )
        result = run_goa_source(goa_config(), busctl, google)
        self.assertEqual(query_of(google.calls[1][0])["pageToken"], ["lista-2"])
        self.assertEqual(query_of(google.calls[3][0])["pageToken"], ["eventos-2"])
        self.assertEqual(titles(result), ["Primeira", "Segunda"])

    def test_pagination_stops_at_maximum_pages(self):
        busctl = FakeBusctl({MANAGED_KEY: managed_objects(GOOGLE_ACCOUNT), token_key(ACCOUNT_ONE): token_reply(TOKEN)})
        event_pages = []

        def endless_google(url, token):
            if url == CALENDAR_LIST_URL:
                return calendar_list(calendar_item(PERSONAL_ID, "Pessoal"))
            event_pages.append(url)
            return {"items": [], "nextPageToken": f"pagina-{len(event_pages)}"}

        run_goa_source(goa_config(), busctl, endless_google)
        self.assertEqual(len(event_pages), calendar_widget.MAXIMUM_PAGES)

    def test_maps_all_day_cancelled_and_declined(self):
        busctl, google = single_calendar_fakes(
            [
                google_event("dia", "Férias", {"date": "2026-10-19"}, {"date": "2026-10-22"}),
                meeting_event("cancelado", "Cancelada", status="cancelled"),
                meeting_event(
                    "recusado",
                    "Recusada",
                    attendees=[
                        {"email": "outra@x.com", "responseStatus": "accepted"},
                        {"self": True, "responseStatus": "declined"},
                    ],
                ),
            ]
        )
        events = {item.id: item for item in run_goa_source(goa_config(), busctl, google).events}
        self.assertEqual(events["dia"].start, datetime(2026, 10, 19, tzinfo=ZONE))
        self.assertEqual(events["dia"].end, datetime(2026, 10, 22, tzinfo=ZONE))
        self.assertTrue(events["dia"].all_day)
        self.assertTrue(events["cancelado"].cancelled)
        self.assertFalse(events["cancelado"].declined)
        self.assertTrue(events["recusado"].declined)
        self.assertFalse(events["recusado"].cancelled)

    def test_filters_by_account_and_calendar_names(self):
        busctl = FakeBusctl(
            {
                MANAGED_KEY: managed_objects(GOOGLE_ACCOUNT, goa_account_objects(ACCOUNT_TWO, "trabalho@empresa.com")),
                token_key(ACCOUNT_ONE): token_reply(TOKEN),
            }
        )
        google = FakeGoogle(
            {
                (CALENDAR_LIST_URL, TOKEN): calendar_list(
                    calendar_item(PERSONAL_ID, "Pessoal"),
                    calendar_item(WORK_ID, "Trabalho", summary_override="Work"),
                ),
                (events_path(PERSONAL_ID), TOKEN): {"items": [meeting_event("p", "Pessoal")]},
                (events_path(WORK_ID), TOKEN): {"items": [meeting_event("w", "Do trabalho")]},
            }
        )
        result = run_goa_source(goa_config(goa_account="eu@gmail.com", calendars=["Work"]), busctl, google)
        self.assertEqual(titles(result), ["Do trabalho"])
        self.assertEqual(busctl.calls[1:], [token_arguments(ACCOUNT_ONE)])
        self.assertEqual([url.split("?")[0] for url, _ in google.calls[1:]], [events_path(WORK_ID)])

    def test_ignores_non_google_and_calendar_disabled(self):
        busctl = FakeBusctl(
            {
                MANAGED_KEY: managed_objects(
                    GOOGLE_ACCOUNT,
                    goa_account_objects(ACCOUNT_TWO, "eu@nextcloud.example", provider="owncloud"),
                    goa_account_objects(ACCOUNT_THREE, "desligada@gmail.com", calendar_disabled=True),
                ),
                token_key(ACCOUNT_ONE): token_reply(TOKEN),
            }
        )
        google = FakeGoogle(
            {
                (CALENDAR_LIST_URL, TOKEN): calendar_list(calendar_item(PERSONAL_ID, "Pessoal")),
                (events_path(PERSONAL_ID), TOKEN): {"items": [meeting_event()]},
            }
        )
        result = run_goa_source(goa_config(), busctl, google)
        self.assertEqual(busctl.calls[1:], [token_arguments(ACCOUNT_ONE)])
        self.assertEqual(titles(result), ["Reunião"])

    def test_no_google_account_is_a_clear_error(self):
        other_provider = managed_objects(goa_account_objects(ACCOUNT_TWO, "eu@nextcloud.example", provider="owncloud"))
        for managed, goa_account in ((other_provider, None), (managed_objects(GOOGLE_ACCOUNT), "outra@gmail.com")):
            with self.subTest(goa_account=goa_account):
                busctl = FakeBusctl({MANAGED_KEY: managed})
                with self.assertRaises(calendar_widget.SourceError) as context:
                    run_goa_source(goa_config(goa_account=goa_account), busctl, FakeGoogle({}))
                self.assertEqual(str(context.exception), NO_GOOGLE_ACCOUNT_MESSAGE)

    def test_attention_needed_warns_and_skips_the_account(self):
        busctl = FakeBusctl(
            {
                MANAGED_KEY: managed_objects(
                    goa_account_objects(ACCOUNT_ONE, "eu@gmail.com", attention_needed=True),
                    goa_account_objects(ACCOUNT_TWO, "trabalho@empresa.com"),
                ),
                token_key(ACCOUNT_TWO): token_reply("TOKEN-2"),
            }
        )
        google = FakeGoogle(
            {
                (CALENDAR_LIST_URL, "TOKEN-2"): calendar_list(calendar_item(PERSONAL_ID, "Pessoal")),
                (events_path(PERSONAL_ID), "TOKEN-2"): {"items": [meeting_event()]},
            }
        )
        result = run_goa_source(goa_config(), busctl, google)
        self.assertEqual(
            result.warnings,
            ["conta eu@gmail.com precisa entrar de novo em Configurações → Contas Online"],
        )
        self.assertEqual(titles(result), ["Reunião"])
        self.assertNotIn(token_arguments(ACCOUNT_ONE), busctl.calls)

    def test_unauthorized_calls_ensure_credentials_and_retries_once(self):
        busctl = FakeBusctl(
            {
                MANAGED_KEY: managed_objects(GOOGLE_ACCOUNT),
                token_key(ACCOUNT_ONE): [token_reply("TOKEN-1"), token_reply("TOKEN-2")],
                ensure_key(ACCOUNT_ONE): {"type": "i", "data": 0},
            }
        )
        google = FakeGoogle(
            {
                (CALENDAR_LIST_URL, "TOKEN-1"): calendar_widget.HttpUnauthorized(),
                (CALENDAR_LIST_URL, "TOKEN-2"): calendar_list(calendar_item(PERSONAL_ID, "Pessoal")),
                (events_path(PERSONAL_ID), "TOKEN-2"): {"items": [meeting_event()]},
            }
        )
        result = run_goa_source(goa_config(), busctl, google)
        self.assertEqual(
            busctl.calls,
            [
                MANAGED_ARGUMENTS,
                token_arguments(ACCOUNT_ONE),
                ensure_arguments(ACCOUNT_ONE),
                token_arguments(ACCOUNT_ONE),
            ],
        )
        self.assertEqual([token for _, token in google.calls], ["TOKEN-1", "TOKEN-2", "TOKEN-2"])
        self.assertEqual(titles(result), ["Reunião"])
        self.assertEqual(result.warnings, [])

    def test_second_unauthorized_fails_only_that_account(self):
        busctl = FakeBusctl(
            {
                MANAGED_KEY: managed_objects(GOOGLE_ACCOUNT, goa_account_objects(ACCOUNT_TWO, "trabalho@empresa.com")),
                token_key(ACCOUNT_ONE): [token_reply("TOKEN-1"), token_reply("TOKEN-2")],
                ensure_key(ACCOUNT_ONE): {"type": "i", "data": 0},
                token_key(ACCOUNT_TWO): token_reply("TOKEN-3"),
            }
        )
        google = FakeGoogle(
            {
                (CALENDAR_LIST_URL, "TOKEN-1"): calendar_widget.HttpUnauthorized(),
                (CALENDAR_LIST_URL, "TOKEN-2"): calendar_widget.HttpUnauthorized(),
                (CALENDAR_LIST_URL, "TOKEN-3"): calendar_list(calendar_item(PERSONAL_ID, "Pessoal")),
                (events_path(PERSONAL_ID), "TOKEN-3"): {"items": [meeting_event()]},
            }
        )
        result = run_goa_source(goa_config(), busctl, google)
        self.assertEqual(result.warnings, ["conta eu@gmail.com indisponível"])
        self.assertEqual(titles(result), ["Reunião"])

    def test_one_account_failing_keeps_the_other(self):
        busctl = FakeBusctl(
            {
                MANAGED_KEY: managed_objects(GOOGLE_ACCOUNT, goa_account_objects(ACCOUNT_TWO, "trabalho@empresa.com")),
                token_key(ACCOUNT_ONE): calendar_widget.SourceError("falha"),
                token_key(ACCOUNT_TWO): token_reply("TOKEN-3"),
            }
        )
        google = FakeGoogle(
            {
                (CALENDAR_LIST_URL, "TOKEN-3"): calendar_list(calendar_item(PERSONAL_ID, "Pessoal")),
                (events_path(PERSONAL_ID), "TOKEN-3"): {"items": [meeting_event()]},
            }
        )
        result = run_goa_source(goa_config(), busctl, google)
        self.assertEqual(result.warnings, ["conta eu@gmail.com indisponível"])
        self.assertEqual(titles(result), ["Reunião"])
        self.assertNotIn("TOKEN-3", repr(result.warnings))

    def test_all_accounts_failing_raises_source_error(self):
        busctl = FakeBusctl({MANAGED_KEY: managed_objects(GOOGLE_ACCOUNT), token_key(ACCOUNT_ONE): token_reply(TOKEN)})
        google = FakeGoogle({(CALENDAR_LIST_URL, TOKEN): calendar_widget.SourceError("falha")})
        with self.assertRaises(calendar_widget.SourceError) as context:
            run_goa_source(goa_config(), busctl, google)
        self.assertEqual(str(context.exception), ALL_ACCOUNTS_FAILED_MESSAGE)

    def test_busctl_failure_on_managed_objects_propagates(self):
        busctl = FakeBusctl({MANAGED_KEY: calendar_widget.SourceError("sem barramento")})
        with self.assertRaises(calendar_widget.SourceError) as context:
            run_goa_source(goa_config(), busctl, FakeGoogle({}))
        self.assertEqual(str(context.exception), "sem barramento")


class RunBusctlTests(unittest.TestCase):
    ARGUMENTS = ["org.gnome.OnlineAccounts", "/org/gnome/OnlineAccounts/Accounts/account_1", "i", "m"]

    def test_calls_busctl_on_the_user_bus_and_parses_json(self):
        completed = subprocess.CompletedProcess(args=[], returncode=0, stdout=b'{"type":"s","data":"x"}')
        with mock.patch.object(calendar_widget.subprocess, "run", return_value=completed) as run:
            reply = calendar_widget.run_busctl(self.ARGUMENTS)
        self.assertEqual(reply, {"type": "s", "data": "x"})
        self.assertEqual(run.call_args.args[0], ["busctl", "--user", "--json=short", "call", *self.ARGUMENTS])
        self.assertEqual(run.call_args.kwargs["timeout"], calendar_widget.BUSCTL_TIMEOUT_SECONDS)
        self.assertTrue(run.call_args.kwargs["capture_output"])
        self.assertIs(run.call_args.kwargs["stdin"], subprocess.DEVNULL)

    def test_bus_failures_are_source_errors_hinting_inherit_env(self):
        failures = [
            FileNotFoundError("busctl"),
            subprocess.CalledProcessError(1, "busctl", stderr=b"TOKEN-SECRETO"),
            subprocess.TimeoutExpired("busctl", calendar_widget.BUSCTL_TIMEOUT_SECONDS),
        ]
        for failure in failures:
            with self.subTest(failure=type(failure).__name__):
                with mock.patch.object(calendar_widget.subprocess, "run", side_effect=failure):
                    with self.assertRaises(calendar_widget.SourceError) as context:
                        calendar_widget.run_busctl(self.ARGUMENTS)
                message = str(context.exception)
                self.assertIn("DBUS_SESSION_BUS_ADDRESS", message)
                self.assertIn("inherit_env", message)
                self.assertNotIn("TOKEN-SECRETO", message)

    def test_invalid_json_is_a_source_error(self):
        completed = subprocess.CompletedProcess(args=[], returncode=0, stdout=b"lixo")
        with mock.patch.object(calendar_widget.subprocess, "run", return_value=completed):
            with self.assertRaises(calendar_widget.SourceError):
                calendar_widget.run_busctl(self.ARGUMENTS)


class GoogleGetTests(unittest.TestCase):
    URL = f"{GOOGLE_API}/users/me/calendarList"

    def test_sends_bearer_token_and_returns_json(self):
        body = io.BytesIO(json.dumps({"items": []}).encode("utf-8"))
        with mock.patch.object(calendar_widget.urllib.request, "urlopen", return_value=body) as urlopen:
            payload = calendar_widget.google_get(self.URL, TOKEN)
        self.assertEqual(urlopen.call_args.args[0].get_header("Authorization"), f"Bearer {TOKEN}")
        self.assertEqual(urlopen.call_args.kwargs["timeout"], calendar_widget.REQUEST_TIMEOUT_SECONDS)
        self.assertEqual(payload, {"items": []})

    def test_unauthorized_raises_http_unauthorized(self):
        with mock.patch.object(calendar_widget.urllib.request, "urlopen", side_effect=http_error(401)):
            with self.assertRaises(calendar_widget.HttpUnauthorized):
                calendar_widget.google_get(self.URL, TOKEN)

    def test_failures_are_source_errors_without_secrets(self):
        failures = [http_error(500), urllib.error.URLError("TOKEN-SECRETO"), TimeoutError("TOKEN-SECRETO")]
        for failure in failures:
            with self.subTest(failure=type(failure).__name__):
                with mock.patch.object(calendar_widget.urllib.request, "urlopen", side_effect=failure):
                    with self.assertRaises(calendar_widget.SourceError) as context:
                        calendar_widget.google_get(self.URL, "TOKEN-SECRETO")
                self.assertEqual(str(context.exception), GOOGLE_UNAVAILABLE_MESSAGE)
                rendered = "".join(traceback.format_exception(context.exception))
                self.assertNotIn("TOKEN-SECRETO", rendered)
                self.assertIsNone(context.exception.__cause__)

    def test_oversized_or_invalid_body_is_a_source_error(self):
        oversized = io.BytesIO(b"x" * (calendar_widget.MAXIMUM_GOOGLE_RESPONSE_BYTES + 1))
        for body in (oversized, io.BytesIO(b"nao e json")):
            with self.subTest(size=body.getbuffer().nbytes):
                with mock.patch.object(calendar_widget.urllib.request, "urlopen", return_value=body):
                    with self.assertRaises(calendar_widget.SourceError):
                        calendar_widget.google_get(self.URL, TOKEN)


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
            environ={"CALENDAR_SOURCE": "gcalcli", "CALENDAR_GOA_ACCOUNT": "conta"},
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
