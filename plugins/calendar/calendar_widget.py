#!/usr/bin/env python3
"""Plugin do mosaic: agenda da semana, de hoje até a próxima segunda-feira, a partir de ICS, GOA ou gcalcli."""

from __future__ import annotations

import hashlib
import http.client
import itertools
import json
import math
import os
import re
import sys
import urllib.parse
import urllib.request
from dataclasses import dataclass, replace
from datetime import date, datetime, time, timedelta, timezone
from pathlib import Path
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

PROTOCOL_VERSION = 1
DEFAULT_TITLE = "Agenda"
SNAPSHOT_COMMAND = "snapshot"
USAGE_MESSAGE = "uso: calendar_widget.py snapshot\n"
SUCCESS_EXIT_CODE = 0
FAILURE_EXIT_CODE = 1
USAGE_EXIT_CODE = 2
NOT_IMPLEMENTED_MESSAGE = "fonte ainda não implementada"

SOURCE_ICS = "ics"
SOURCE_GOA = "goa"
SOURCE_GCALCLI = "gcalcli"
SOURCE_NAMES = (SOURCE_ICS, SOURCE_GOA, SOURCE_GCALCLI)
SOURCE_VARIABLE = "CALENDAR_SOURCE"
ICS_URL_ENV_VARIABLE = "CALENDAR_ICS_URL_ENV"
ICS_URL_FILE_VARIABLE = "CALENDAR_ICS_URL_FILE"
GOA_ACCOUNT_VARIABLE = "CALENDAR_GOA_ACCOUNT"
CALENDARS_VARIABLE = "CALENDAR_CALENDARS"
TITLE_VARIABLE = "CALENDAR_TITLE"
ICS_SCHEMES = ("http", "https", "webcal")
WEBCAL_SCHEME = "webcal"
SECURE_SCHEME = "https"

TIMEZONE_VARIABLE = "TZ"
LOCALTIME_LINK = "/etc/localtime"
ZONEINFO_MARKER = "zoneinfo/"

DAYS_IN_WEEK = 7
SECONDS_PER_MINUTE = 60
UPCOMING_WARNING_MINUTES = 15
WEEKDAY_ABBREVIATIONS = ("Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom")
EMPTY_TEXT = "nada até domingo"
TODAY_HEADING = "Hoje"
TOMORROW_HEADING = "Amanhã"
ALL_DAY_LABEL = "dia todo"
CURRENT_TAG = "agora"
UPCOMING_TAG = "em {minutes} min"
NEXT_EVENT_TEXT = "próx {time}"
FREE_TODAY_TEXT = "hoje livre"
NO_NEXT_EVENT_TEXT = "sem próximos"
TIME_FORMAT = "%H:%M"
DAY_FORMAT = "%d/%m"
TIME_RANGE_SEPARATOR = "–"
LOCATION_SEPARATOR = " · "

ICS_BEGIN = "BEGIN"
ICS_END = "END"
ICS_EVENT_COMPONENT = "VEVENT"
ICS_CALENDAR_NAME = "X-WR-CALNAME"
ICS_UID = "UID"
ICS_SUMMARY = "SUMMARY"
ICS_LOCATION = "LOCATION"
ICS_STATUS = "STATUS"
ICS_DTSTART = "DTSTART"
ICS_DTEND = "DTEND"
ICS_DURATION = "DURATION"
ICS_RRULE = "RRULE"
ICS_EXDATE = "EXDATE"
ICS_RDATE = "RDATE"
ICS_RECURRENCE_ID = "RECURRENCE-ID"
ICS_DEFAULT_STATUS = "CONFIRMED"
ICS_TZID_PARAMETER = "TZID"
ICS_VALUE_PARAMETER = "VALUE"
ICS_DATE_VALUE = "DATE"
ICS_UTC_MARKER = "Z"
ICS_DATE_FORMAT = "%Y%m%d"
ICS_DATETIME_FORMAT = "%Y%m%dT%H%M%S"
ICS_DATE_TIME_SEPARATOR = "T"
ICS_FOLD_CHARACTERS = (" ", "\t")
ICS_CARRIAGE_RETURN = "\r"
ICS_LINE_FEED = "\n"
ICS_QUOTE = '"'
ICS_PROPERTY_SEPARATOR = ":"
ICS_PARAMETER_SEPARATOR = ";"
ICS_PARAMETER_ASSIGNMENT = "="
ICS_VALUE_LIST_SEPARATOR = ","
ICS_PERIOD_SEPARATOR = "/"
ICS_ESCAPE_PATTERN = re.compile(r"\\([\\;,nN])")
ICS_ESCAPES = {"\\": "\\", ";": ";", ",": ",", "n": ICS_LINE_FEED, "N": ICS_LINE_FEED}
ICS_CANCELLED_STATUS = "CANCELLED"
ICS_CALENDAR_BEGIN = "BEGIN:VCALENDAR"
ICS_CALENDAR_NAME_FALLBACK = "ICS {number}"
REQUEST_TIMEOUT_SECONDS = 15
MAXIMUM_ICS_BYTES = 8 * 1024 * 1024
LINK_UNAVAILABLE_WARNING = "agenda ICS {number} indisponível"
ALL_LINKS_FAILED_MESSAGE = "nenhuma agenda ICS respondeu; confira o link (seção ICS do README)"
OVERSIZED_RESPONSE_MESSAGE = "o calendário excede o tamanho máximo permitido"

MAXIMUM_RECURRENCE_ITERATIONS = 100_000
MAXIMUM_ITEM_ID_LENGTH = 200
ITEM_ID_DIGEST_LENGTH = 16
ITEM_ID_DIGEST_SEPARATOR = "#"
ITEM_ID_TEMPLATE = "{uid}@{start}"
UNSUPPORTED_RULE_WARNING = "repetição não suportada em “{title}”"
MONTHS_IN_YEAR = 12
MAXIMUM_MONTH_DAY = 31
RULE_FREQUENCY = "FREQ"
RULE_INTERVAL = "INTERVAL"
RULE_COUNT = "COUNT"
RULE_UNTIL = "UNTIL"
RULE_BY_DAY = "BYDAY"
RULE_BY_MONTH_DAY = "BYMONTHDAY"
RULE_BY_MONTH = "BYMONTH"
RULE_WEEK_START = "WKST"
RULE_SUPPORTED_PARTS = frozenset(
    (RULE_FREQUENCY, RULE_INTERVAL, RULE_COUNT, RULE_UNTIL, RULE_BY_DAY, RULE_BY_MONTH_DAY, RULE_BY_MONTH, RULE_WEEK_START)
)
FREQUENCY_DAILY = "DAILY"
FREQUENCY_WEEKLY = "WEEKLY"
FREQUENCY_MONTHLY = "MONTHLY"
FREQUENCY_YEARLY = "YEARLY"
RULE_FREQUENCIES = (FREQUENCY_DAILY, FREQUENCY_WEEKLY, FREQUENCY_MONTHLY, FREQUENCY_YEARLY)
POSITIONAL_FREQUENCIES = (FREQUENCY_MONTHLY, FREQUENCY_YEARLY)
WEEKDAY_CODES = ("MO", "TU", "WE", "TH", "FR", "SA", "SU")
DEFAULT_WEEK_START = "MO"
DEFAULT_INTERVAL = "1"
BY_DAY_PATTERN = re.compile(r"([+-]?\d{1,2})?(" + "|".join(WEEKDAY_CODES) + ")")
ICS_DURATION_PATTERN = re.compile(
    r"(?P<sign>[+-])?P(?=\d|T\d)"
    r"(?:(?P<weeks>\d+)W)?(?:(?P<days>\d+)D)?"
    r"(?:T(?:(?P<hours>\d+)H)?(?:(?P<minutes>\d+)M)?(?:(?P<seconds>\d+)S)?)?"
)


class ConfigError(Exception):
    pass


class SourceError(Exception):
    pass


class UnsupportedRule(Exception):
    pass


@dataclass(frozen=True)
class Event:
    id: str
    title: str
    start: datetime
    end: datetime
    all_day: bool
    location: str = ""
    calendar: str = ""
    declined: bool = False
    cancelled: bool = False


@dataclass(frozen=True)
class SourceResult:
    events: list[Event]
    warnings: list[str]


@dataclass(frozen=True)
class Config:
    source: str
    title: str
    ics_urls: list[str]
    goa_account: str | None
    calendars: list[str]


def load_config(environ):
    source = _read_source(environ)
    return Config(
        source=source,
        title=_read_optional(environ, TITLE_VARIABLE) or DEFAULT_TITLE,
        ics_urls=_read_ics_urls(environ) if source == SOURCE_ICS else [],
        goa_account=_read_optional(environ, GOA_ACCOUNT_VARIABLE),
        calendars=_read_list(environ, CALENDARS_VARIABLE),
    )


def _read_source(environ):
    source = environ.get(SOURCE_VARIABLE, "").strip()
    if source not in SOURCE_NAMES:
        raise ConfigError(f"{SOURCE_VARIABLE} precisa ser 'ics', 'goa' ou 'gcalcli'")
    return source


def _read_optional(environ, name):
    return environ.get(name, "").strip() or None


def _read_list(environ, name):
    return [item.strip() for item in environ.get(name, "").split(",") if item.strip()]


def _read_ics_urls(environ):
    urls = _read_urls_from_variables(environ) + _read_urls_from_file(environ)
    if not urls:
        raise ConfigError(f"defina {ICS_URL_ENV_VARIABLE} ou {ICS_URL_FILE_VARIABLE} para a fonte ics")
    return urls


def _read_urls_from_variables(environ):
    urls = []
    for name in _read_list(environ, ICS_URL_ENV_VARIABLE):
        value = environ.get(name, "").strip()
        if not value:
            raise ConfigError(f"a variável '{name}' indicada em {ICS_URL_ENV_VARIABLE} está vazia ou ausente")
        urls.append(_normalize_ics_url(value, f"a variável '{name}'"))
    return urls


def _read_urls_from_file(environ):
    raw_path = environ.get(ICS_URL_FILE_VARIABLE, "").strip()
    if not raw_path:
        return []
    try:
        lines = Path(raw_path).expanduser().read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError):
        raise ConfigError(f"não foi possível ler o arquivo de {ICS_URL_FILE_VARIABLE}") from None
    urls = []
    for number, line in enumerate(lines, start=1):
        value = line.strip()
        if value:
            urls.append(_normalize_ics_url(value, f"a linha {number} de {ICS_URL_FILE_VARIABLE}"))
    return urls


def _normalize_ics_url(value, origin):
    parsed = urllib.parse.urlparse(value)
    scheme = parsed.scheme.lower()
    is_valid = scheme in ICS_SCHEMES and bool(parsed.hostname) and not any(char.isspace() for char in value)
    if not is_valid:
        raise ConfigError(f"{origin} não contém um link de calendário válido (http, https ou webcal)")
    if scheme == WEBCAL_SCHEME:
        return urllib.parse.urlunparse(parsed._replace(scheme=SECURE_SCHEME))
    return value


def local_zone(environ):
    for key in (environ.get(TIMEZONE_VARIABLE, ""), _zone_key_from_link()):
        zone = _load_zone(key)
        if zone is not None:
            return zone
    return datetime.now().astimezone().tzinfo


def _load_zone(key):
    if not key:
        return None
    try:
        return ZoneInfo(key)
    except (ZoneInfoNotFoundError, ValueError):
        return None


def _zone_key_from_link():
    try:
        target = os.readlink(LOCALTIME_LINK)
    except OSError:
        return ""
    _, marker, key = target.partition(ZONEINFO_MARKER)
    return key if marker else ""


def week_window(now):
    days_until_monday = DAYS_IN_WEEK - now.weekday()
    monday = now.date() + timedelta(days=days_until_monday)
    return now, datetime.combine(monday, time.min, tzinfo=now.tzinfo)


def visible_events(events, now, window_end):
    visible = [event for event in events if _is_visible(event, now, window_end)]
    return sorted(visible, key=lambda event: (event.start, event.title))


def _is_visible(event, now, window_end):
    return (
        not event.cancelled
        and not event.declined
        and event.end > now
        and event.start < window_end
    )


def build_snapshot(result, now, title):
    _, window_end = week_window(now)
    events = visible_events(result.events, now, window_end)
    return {
        "protocol": PROTOCOL_VERSION,
        "summary": {"title": title, "blocks": _summary_blocks(events, now, result.warnings)},
        "detail": _detail_sections(events, now),
        "mini": _mini(events, now),
    }


def _summary_blocks(events, now, warnings):
    if events:
        blocks = [{"type": "rows", "rows": _summary_rows(events, now)}]
    else:
        blocks = [{"type": "lines", "lines": [{"text": EMPTY_TEXT, "tone": "muted"}]}]
    if warnings:
        blocks.append({"type": "lines", "lines": [{"text": text, "tone": "warn"} for text in warnings]})
    return blocks


def _summary_rows(events, now):
    upcoming = _first_upcoming(events, now)
    return [_summary_row(event, now, event is upcoming) for event in events]


def _summary_row(event, now, is_upcoming):
    current = _is_current(event, now)
    tags = []
    if current:
        tags.append({"text": CURRENT_TAG, "tone": "ok"})
    elif is_upcoming:
        tags.append({"text": UPCOMING_TAG.format(minutes=_minutes_until(event, now)), "tone": "warn"})
    return {
        "time": _time_label(event, now),
        "text": event.title,
        "tone": "current" if current else "normal",
        "tags": tags,
    }


def _is_current(event, now):
    return not event.all_day and event.start <= now < event.end


def _first_upcoming(events, now):
    limit = now + timedelta(minutes=UPCOMING_WARNING_MINUTES)
    for event in events:
        if not event.all_day and now < event.start <= limit:
            return event
    return None


def _minutes_until(event, now):
    seconds = (event.start - now).total_seconds()
    return math.ceil(seconds / SECONDS_PER_MINUTE)


def _time_label(event, now):
    prefix = _day_prefix(_display_day(event, now), now)
    if event.all_day:
        return f"{prefix}{ALL_DAY_LABEL}"
    return f"{prefix}{_local(event.start, now):{TIME_FORMAT}}"


def _day_prefix(day, now):
    if day == now.date():
        return ""
    return f"{WEEKDAY_ABBREVIATIONS[day.weekday()]} "


def _display_day(event, now):
    return max(_local(event.start, now).date(), now.date())


def _local(moment, now):
    return moment.astimezone(now.tzinfo)


def _detail_sections(events, now):
    sections = []
    for day, day_events in itertools.groupby(events, key=lambda event: _display_day(event, now)):
        items = [{"id": event.id, "text": _item_text(event, now)} for event in day_events]
        sections.append({"heading": _day_heading(day, now), "items": items})
    return sections


def _day_heading(day, now):
    if day == now.date():
        return TODAY_HEADING
    if day == now.date() + timedelta(days=1):
        return TOMORROW_HEADING
    return f"{WEEKDAY_ABBREVIATIONS[day.weekday()]} {day:{DAY_FORMAT}}"


def _item_text(event, now):
    if event.all_day:
        body = f"{ALL_DAY_LABEL} {event.title}"
    else:
        start = _local(event.start, now).strftime(TIME_FORMAT)
        end = _local(event.end, now).strftime(TIME_FORMAT)
        body = f"{start}{TIME_RANGE_SEPARATOR}{end} {event.title}"
    if event.location:
        body += f"{LOCATION_SEPARATOR}{event.location}"
    return body


def _mini(events, now):
    today = now.date()
    todays = [event for event in events if _display_day(event, now) == today]
    return {"value": str(len(todays)), "text": _mini_text(todays, now)}


def _mini_text(todays, now):
    for event in todays:
        if not event.all_day and event.start > now:
            return NEXT_EVENT_TEXT.format(time=_local(event.start, now).strftime(TIME_FORMAT))
    return FREE_TODAY_TEXT if not todays else NO_NEXT_EVENT_TEXT


@dataclass(frozen=True)
class IcsEvent:
    uid: str
    summary: str
    location: str
    start: datetime | date
    end: datetime | date | None
    duration: timedelta | None
    rrule: str | None
    exdates: list[datetime | date]
    rdates: list[datetime | date]
    recurrence_id: datetime | date | None
    status: str


@dataclass(frozen=True)
class _IcsProperty:
    name: str
    params: dict[str, str]
    value: str


def parse_ics(text, zone):
    properties = [prop for prop in map(_parse_property, _unfold_lines(text)) if prop is not None]
    events = [_build_event(block, zone) for block in _event_blocks(properties)]
    return [event for event in events if event is not None], _calendar_name(properties)


def _unfold_lines(text):
    lines = []
    for raw in text.split(ICS_LINE_FEED):
        raw = raw.rstrip(ICS_CARRIAGE_RETURN)
        if raw.startswith(ICS_FOLD_CHARACTERS) and lines:
            lines[-1] += raw[1:]
        elif raw:
            lines.append(raw)
    return lines


def _parse_property(line):
    parts = _split_outside_quotes(line, ICS_PROPERTY_SEPARATOR)
    if len(parts) < 2:
        return None
    name, *raw_params = _split_outside_quotes(parts[0], ICS_PARAMETER_SEPARATOR)
    params = dict(_parse_parameter(raw) for raw in raw_params)
    return _IcsProperty(name=name.upper(), params=params, value=ICS_PROPERTY_SEPARATOR.join(parts[1:]))


def _parse_parameter(raw):
    key, _, value = raw.partition(ICS_PARAMETER_ASSIGNMENT)
    return key.strip().upper(), value.strip(ICS_QUOTE)


def _split_outside_quotes(text, separator):
    parts = [""]
    in_quotes = False
    for char in text:
        if char == ICS_QUOTE:
            in_quotes = not in_quotes
        if char == separator and not in_quotes:
            parts.append("")
        else:
            parts[-1] += char
    return parts


def _event_blocks(properties):
    block = None
    depth = 0
    for prop in properties:
        if block is None:
            if prop.name == ICS_BEGIN and prop.value.upper() == ICS_EVENT_COMPONENT:
                block, depth = [], 0
        elif prop.name == ICS_BEGIN:
            depth += 1
        elif prop.name == ICS_END and depth:
            depth -= 1
        elif prop.name == ICS_END:
            yield block
            block = None
        elif not depth:
            block.append(prop)


def _calendar_name(properties):
    for prop in properties:
        if prop.name == ICS_CALENDAR_NAME:
            return _unescape(prop.value)
    return ""


def _build_event(properties, zone):
    start = _first_moment(properties, ICS_DTSTART, zone)
    if start is None:
        return None
    return IcsEvent(
        uid=_text(properties, ICS_UID),
        summary=_text(properties, ICS_SUMMARY),
        location=_text(properties, ICS_LOCATION),
        start=start,
        end=_first_moment(properties, ICS_DTEND, zone),
        duration=_first_duration(properties),
        rrule=_first_value(properties, ICS_RRULE),
        exdates=_moments(properties, ICS_EXDATE, zone),
        rdates=_moments(properties, ICS_RDATE, zone),
        recurrence_id=_first_moment(properties, ICS_RECURRENCE_ID, zone),
        status=_text(properties, ICS_STATUS) or ICS_DEFAULT_STATUS,
    )


def _named(properties, name):
    return [prop for prop in properties if prop.name == name]


def _first_value(properties, name):
    matches = _named(properties, name)
    return matches[0].value if matches else None


def _text(properties, name):
    value = _first_value(properties, name)
    return _unescape(value) if value is not None else ""


def _unescape(value):
    return ICS_ESCAPE_PATTERN.sub(lambda match: ICS_ESCAPES[match.group(1)], value)


def _first_moment(properties, name, zone):
    matches = _named(properties, name)
    if not matches:
        return None
    return _parse_moment(matches[0].params, matches[0].value, zone)


def _moments(properties, name, zone):
    moments = []
    for prop in _named(properties, name):
        for raw in prop.value.split(ICS_VALUE_LIST_SEPARATOR):
            moment = _parse_moment(prop.params, raw.partition(ICS_PERIOD_SEPARATOR)[0], zone)
            if moment is not None:
                moments.append(moment)
    return moments


def _parse_moment(params, raw, zone):
    try:
        if params.get(ICS_VALUE_PARAMETER) == ICS_DATE_VALUE or ICS_DATE_TIME_SEPARATOR not in raw:
            return datetime.strptime(raw, ICS_DATE_FORMAT).date()
        if raw.endswith(ICS_UTC_MARKER):
            return datetime.strptime(raw[:-1], ICS_DATETIME_FORMAT).replace(tzinfo=timezone.utc)
        local = datetime.strptime(raw, ICS_DATETIME_FORMAT)
    except ValueError:
        return None
    return local.replace(tzinfo=_zone_for(params.get(ICS_TZID_PARAMETER), zone))


def _zone_for(tzid, fallback):
    zone = _load_zone(tzid or "")
    return fallback if zone is None else zone


def _first_duration(properties):
    value = _first_value(properties, ICS_DURATION)
    return None if value is None else _parse_duration(value)


def _parse_duration(raw):
    match = ICS_DURATION_PATTERN.fullmatch(raw)
    if match is None:
        return None
    amount = timedelta(
        weeks=int(match.group("weeks") or 0),
        days=int(match.group("days") or 0),
        hours=int(match.group("hours") or 0),
        minutes=int(match.group("minutes") or 0),
        seconds=int(match.group("seconds") or 0),
    )
    return -amount if match.group("sign") == "-" else amount


@dataclass(frozen=True)
class _Rule:
    frequency: str
    interval: int
    count: int | None
    until: datetime | date | None
    weekdays: list[tuple[int | None, int]]
    month_days: list[int]
    months: list[int]
    week_start: int


def _parse_rule(text, zone):
    try:
        rule = _build_rule(_rule_parts(text), zone)
    except ValueError as error:
        raise UnsupportedRule(str(error)) from None
    if _has_unsupported_combination(rule):
        raise UnsupportedRule(text)
    return rule


def _rule_parts(text):
    parts = {}
    for item in text.split(ICS_PARAMETER_SEPARATOR):
        name, assignment, value = item.partition(ICS_PARAMETER_ASSIGNMENT)
        if not assignment:
            raise ValueError(item)
        parts[name.strip().upper()] = value.strip().upper()
    unsupported = set(parts) - RULE_SUPPORTED_PARTS
    if unsupported:
        raise ValueError(ICS_VALUE_LIST_SEPARATOR.join(sorted(unsupported)))
    return parts


def _build_rule(parts, zone):
    frequency = parts.get(RULE_FREQUENCY)
    if frequency not in RULE_FREQUENCIES:
        raise ValueError(str(frequency))
    count = parts.get(RULE_COUNT)
    return _Rule(
        frequency=frequency,
        interval=_positive_integer(parts.get(RULE_INTERVAL, DEFAULT_INTERVAL)),
        count=None if count is None else _positive_integer(count),
        until=_rule_until(parts.get(RULE_UNTIL), zone),
        weekdays=_parse_weekdays(parts.get(RULE_BY_DAY, "")),
        month_days=_parse_numbers(parts.get(RULE_BY_MONTH_DAY, ""), -MAXIMUM_MONTH_DAY, MAXIMUM_MONTH_DAY),
        months=_parse_numbers(parts.get(RULE_BY_MONTH, ""), 1, MONTHS_IN_YEAR),
        week_start=WEEKDAY_CODES.index(parts.get(RULE_WEEK_START, DEFAULT_WEEK_START)),
    )


def _positive_integer(raw):
    value = int(raw)
    if value < 1:
        raise ValueError(raw)
    return value


def _rule_until(raw, zone):
    if raw is None:
        return None
    moment = _parse_moment({}, raw, zone)
    if moment is None:
        raise ValueError(raw)
    return moment


def _parse_weekdays(raw):
    weekdays = []
    for item in _split_list(raw):
        match = BY_DAY_PATTERN.fullmatch(item)
        if match is None:
            raise ValueError(item)
        position = match.group(1)
        weekdays.append((None if position is None else int(position), WEEKDAY_CODES.index(match.group(2))))
    return weekdays


def _parse_numbers(raw, minimum, maximum):
    numbers = [int(item) for item in _split_list(raw)]
    if any(number == 0 or not minimum <= number <= maximum for number in numbers):
        raise ValueError(raw)
    return numbers


def _split_list(raw):
    return [item.strip() for item in raw.split(ICS_VALUE_LIST_SEPARATOR) if item.strip()]


def _has_unsupported_combination(rule):
    positioned = any(position is not None for position, _ in rule.weekdays)
    if positioned and (rule.frequency not in POSITIONAL_FREQUENCIES or rule.month_days):
        return True
    return rule.frequency == FREQUENCY_WEEKLY and bool(rule.month_days)


def _rule_starts(rule, anchor, timezone_):
    produced = 0
    until = _naive_until(rule.until, timezone_)
    for period in range(MAXIMUM_RECURRENCE_ITERATIONS):
        try:
            days = _period_days(rule, anchor.date(), period * rule.interval)
        except (ValueError, OverflowError):
            return
        for day in days:
            start = datetime.combine(day, anchor.time())
            if start < anchor:
                continue
            if until is not None and start > until:
                return
            yield start
            produced += 1
            if rule.count is not None and produced >= rule.count:
                return
    raise UnsupportedRule(RULE_FREQUENCY)


def _naive_until(until, timezone_):
    if until is None:
        return None
    if isinstance(until, datetime) and timezone_ is not None:
        return until.astimezone(timezone_).replace(tzinfo=None)
    day = until.date() if isinstance(until, datetime) else until
    return datetime.combine(day, time.max)


def _period_days(rule, anchor_day, offset):
    handlers = {
        FREQUENCY_DAILY: _daily_days,
        FREQUENCY_WEEKLY: _weekly_days,
        FREQUENCY_MONTHLY: _monthly_days,
        FREQUENCY_YEARLY: _yearly_days,
    }
    return sorted(set(handlers[rule.frequency](rule, anchor_day, offset)))


def _daily_days(rule, anchor_day, offset):
    day = anchor_day + timedelta(days=offset)
    return [day] if _passes_limits(rule, day) else []


def _passes_limits(rule, day):
    if rule.months and day.month not in rule.months:
        return False
    if rule.month_days and day not in _month_day_dates(rule.month_days, day.year, day.month):
        return False
    return not rule.weekdays or day.weekday() in {weekday for _, weekday in rule.weekdays}


def _weekly_days(rule, anchor_day, offset):
    week_start = anchor_day - timedelta(days=(anchor_day.weekday() - rule.week_start) % DAYS_IN_WEEK)
    first = week_start + timedelta(weeks=offset)
    weekdays = {weekday for _, weekday in rule.weekdays} or {anchor_day.weekday()}
    days = [first + timedelta(days=index) for index in range(DAYS_IN_WEEK)]
    return [day for day in days if day.weekday() in weekdays and (not rule.months or day.month in rule.months)]


def _monthly_days(rule, anchor_day, offset):
    year, month = _shift_month(anchor_day.year, anchor_day.month, offset)
    if rule.months and month not in rule.months:
        return []
    return _days_of_month(rule, anchor_day, year, month)


def _yearly_days(rule, anchor_day, offset):
    year = anchor_day.year + offset
    if rule.weekdays and not rule.month_days and not rule.months:
        return _weekday_dates(date(year, 1, 1), date(year, MONTHS_IN_YEAR, MAXIMUM_MONTH_DAY), rule.weekdays)
    days = []
    for month in _yearly_months(rule, anchor_day):
        days += _days_of_month(rule, anchor_day, year, month)
    return days


def _yearly_months(rule, anchor_day):
    if rule.months:
        return rule.months
    if rule.month_days:
        return range(1, MONTHS_IN_YEAR + 1)
    return [anchor_day.month]


def _shift_month(year, month, offset):
    extra_years, month_index = divmod(month - 1 + offset, MONTHS_IN_YEAR)
    return year + extra_years, month_index + 1


def _days_of_month(rule, anchor_day, year, month):
    if rule.month_days:
        days = _month_day_dates(rule.month_days, year, month)
        return [day for day in days if _matches_weekdays(rule, day)]
    if rule.weekdays:
        return _weekday_dates(date(year, month, 1), _last_day(year, month), rule.weekdays)
    return _month_day_dates([anchor_day.day], year, month)


def _matches_weekdays(rule, day):
    return not rule.weekdays or day.weekday() in {weekday for _, weekday in rule.weekdays}


def _last_day(year, month):
    next_year, next_month = _shift_month(year, month, 1)
    return date(next_year, next_month, 1) - timedelta(days=1)


def _month_day_dates(month_days, year, month):
    length = _last_day(year, month).day
    resolved = [number if number > 0 else length + 1 + number for number in month_days]
    return [date(year, month, number) for number in resolved if 1 <= number <= length]


def _weekday_dates(first, last, weekdays):
    days = []
    for position, weekday in weekdays:
        start = first + timedelta(days=(weekday - first.weekday()) % DAYS_IN_WEEK)
        matches = [start + timedelta(weeks=index) for index in range((last - start).days // DAYS_IN_WEEK + 1)]
        days += matches if position is None else _pick_position(matches, position)
    return days


def _pick_position(matches, position):
    index = position - 1 if position > 0 else position
    return [matches[index]] if -len(matches) <= index < len(matches) else []


def _occurrence_duration(event):
    all_day = not isinstance(event.start, datetime)
    if event.end is not None and isinstance(event.end, datetime) != all_day:
        elapsed = _absolute(event.end) - _absolute(event.start)
    elif event.duration is not None:
        elapsed = event.duration
    else:
        elapsed = timedelta(days=1) if all_day else timedelta()
    return max(elapsed, timedelta())


def _absolute(moment):
    return moment.astimezone(timezone.utc) if isinstance(moment, datetime) else moment


def _occurrence_bounds(moment, duration, zone):
    if isinstance(moment, datetime):
        return moment, (moment.astimezone(timezone.utc) + duration).astimezone(moment.tzinfo)
    midnight = datetime.combine(moment, time.min)
    return midnight.replace(tzinfo=zone), (midnight + duration).replace(tzinfo=zone)


def _rule_moments(event, zone):
    if isinstance(event.start, datetime):
        anchor, timezone_ = event.start.replace(tzinfo=None), event.start.tzinfo
    else:
        anchor, timezone_ = datetime.combine(event.start, time.min), None
    if event.rrule is None:
        starts = iter([anchor])
    else:
        starts = _rule_starts(_parse_rule(event.rrule, zone), anchor, timezone_)
    for start in starts:
        yield start.replace(tzinfo=timezone_) if timezone_ else start.date()


def expand_occurrences(event, window_start, window_end, zone):
    duration = _occurrence_duration(event)
    excluded = set(event.exdates)
    found = []

    def overlaps(moment):
        start, end = _occurrence_bounds(moment, duration, zone)
        return start < window_end and end > window_start

    for moment in _rule_moments(event, zone):
        if _occurrence_bounds(moment, duration, zone)[0] >= window_end:
            break
        if moment not in excluded and overlaps(moment):
            found.append(moment)
    extra = [moment for moment in event.rdates if moment not in excluded and moment not in found and overlaps(moment)]
    return found + extra


def ics_events_in_window(events, calendar, window_start, window_end, zone):
    groups = {}
    for index, event in enumerate(events):
        groups.setdefault(event.uid or index, []).append(event)
    found, warnings = [], []
    for group in groups.values():
        try:
            found += _group_events(group, calendar, window_start, window_end, zone)
        except UnsupportedRule:
            title = next((event.summary for event in group if event.recurrence_id is None), group[0].summary)
            warnings.append(UNSUPPORTED_RULE_WARNING.format(title=title))
    return SourceResult(events=found, warnings=warnings)


def _group_events(group, calendar, window_start, window_end, zone):
    overridden = {event.recurrence_id for event in group if event.recurrence_id is not None}
    found = []
    for event in group:
        standalone = event.recurrence_id is not None
        source = replace(event, rrule=None, rdates=[], exdates=[]) if standalone else event
        if standalone and event.status.upper() == ICS_CANCELLED_STATUS:
            continue
        for moment in expand_occurrences(source, window_start, window_end, zone):
            if standalone or moment not in overridden:
                found.append(_to_event(event, moment, calendar, zone))
    return found


def _to_event(source, moment, calendar, zone):
    start, end = _occurrence_bounds(moment, _occurrence_duration(source), zone)
    return Event(
        id=_item_id(source.uid, moment),
        title=source.summary,
        start=start,
        end=end,
        all_day=not isinstance(moment, datetime),
        location=source.location,
        calendar=calendar,
        cancelled=source.status.upper() == ICS_CANCELLED_STATUS,
    )


def _item_id(uid, moment):
    start = moment.astimezone(timezone.utc).isoformat() if isinstance(moment, datetime) else moment.isoformat()
    identifier = ITEM_ID_TEMPLATE.format(uid=uid, start=start)
    if len(identifier) <= MAXIMUM_ITEM_ID_LENGTH:
        return identifier
    digest = hashlib.sha1(identifier.encode("utf-8")).hexdigest()[:ITEM_ID_DIGEST_LENGTH]
    prefix_length = MAXIMUM_ITEM_ID_LENGTH - len(ITEM_ID_DIGEST_SEPARATOR) - ITEM_ID_DIGEST_LENGTH
    return f"{identifier[:prefix_length]}{ITEM_ID_DIGEST_SEPARATOR}{digest}"


class FetchError(Exception):
    pass


class _LinkFailure(Exception):
    pass


def fetch_url(url):
    with urllib.request.urlopen(url, timeout=REQUEST_TIMEOUT_SECONDS) as response:
        body = response.read(MAXIMUM_ICS_BYTES + 1)
    if len(body) > MAXIMUM_ICS_BYTES:
        raise FetchError(OVERSIZED_RESPONSE_MESSAGE)
    return body.decode("utf-8", errors="replace")


def ics_source(config, window_start, window_end, zone, fetch=fetch_url):
    events, warnings = [], []
    failures = 0
    for number, url in enumerate(config.ics_urls, start=1):
        try:
            text = _download(url, fetch)
        except _LinkFailure:
            warnings.append(LINK_UNAVAILABLE_WARNING.format(number=number))
            failures += 1
            continue
        result = _link_result(text, number, window_start, window_end, zone)
        events += result.events
        warnings += result.warnings
    if config.ics_urls and failures == len(config.ics_urls):
        raise SourceError(ALL_LINKS_FAILED_MESSAGE)
    return SourceResult(events=events, warnings=warnings)


def _download(url, fetch):
    try:
        text = fetch(url)
    except (OSError, http.client.HTTPException, ValueError, FetchError):
        raise _LinkFailure from None
    if ICS_CALENDAR_BEGIN not in text.upper():
        raise _LinkFailure
    return text


def _link_result(text, number, window_start, window_end, zone):
    events, calendar_name = parse_ics(text, zone)
    calendar = calendar_name or ICS_CALENDAR_NAME_FALLBACK.format(number=number)
    return ics_events_in_window(events, calendar, window_start, window_end, zone)


def goa_source(config, now, window_end, zone):
    raise SourceError(NOT_IMPLEMENTED_MESSAGE)


def gcalcli_source(config, now, window_end, zone):
    raise SourceError(NOT_IMPLEMENTED_MESSAGE)


DEFAULT_SOURCES = {
    SOURCE_ICS: ics_source,
    SOURCE_GOA: goa_source,
    SOURCE_GCALCLI: gcalcli_source,
}


def main(arguments, environ=None, sources=None, now=None, stdout=None, stderr=None):
    environ = os.environ if environ is None else environ
    sources = DEFAULT_SOURCES if sources is None else sources
    now = datetime.now().astimezone() if now is None else now
    stdout = sys.stdout if stdout is None else stdout
    stderr = sys.stderr if stderr is None else stderr
    if arguments != [SNAPSHOT_COMMAND]:
        stderr.write(USAGE_MESSAGE)
        return USAGE_EXIT_CODE
    try:
        stdout.write(_snapshot_json(environ, sources, now) + "\n")
    except (ConfigError, SourceError) as error:
        stderr.write(f"erro: {error}\n")
        return FAILURE_EXIT_CODE
    return SUCCESS_EXIT_CODE


def _snapshot_json(environ, sources, now):
    config = load_config(environ)
    zone = local_zone(environ)
    moment = now.astimezone(zone)
    _, window_end = week_window(moment)
    result = sources[config.source](config, moment, window_end, zone)
    return json.dumps(build_snapshot(result, moment, config.title), ensure_ascii=False)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
