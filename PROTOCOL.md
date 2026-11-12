# Mosaic widget protocol (v1)

> Versão em português: [`PROTOCOL.pt-BR.md`](PROTOCOL.pt-BR.md).

A widget is any program that Mosaic runs: Python, Node, shell, whatever you prefer. It answers the `snapshot` argument by printing a JSON document, and Mosaic draws the result as a widget. This document is the contract; the [JSON Schema](schema/protocol.schema.json) is equivalent, and tests guarantee that the two do not drift from the code (block types, tones, flags and numeric limits).

A complete widget, in Python:

```python
import json
print(json.dumps({"summary": {"title": "Hello", "blocks": [
    {"type": "lines", "lines": [{"text": "all good", "tone": "ok"}]}
]}}))
```

## 1. Invocation

Mosaic runs the widget's `command` + `args` with one extra argument:

| Invocation | When | Expected output |
| --- | --- | --- |
| `<command> snapshot` | every `interval_seconds` and when the user presses `r` | one JSON document on standard output |
| `<command> action <id>` | when the user confirms an action (parameters as JSON on standard input) | a short text message |

The standard input of `<command> action <id>` receives a JSON object with the action's `params`. For actions with `applies_to: "item"`, Mosaic adds the reserved key `item_id`, holding the `id` of the selected item; if `params` already contains `item_id`, Mosaic's value wins. `params` that is not an object arrives as `{}`. `mosaic check` warns about both cases.

The process receives Mosaic's environment plus `[widgets.env]`. If the widget declares `inherit_env = ["NAME", ...]`, it receives only `PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, `TERM`, `TZ`, `LANG`, `LANGUAGE`, `TMPDIR`, `LC_*`, `XDG_*`, the listed variables and `[widgets.env]` (plus `GIT_TERMINAL_PROMPT=0` and `SSH_ASKPASS_REQUIRE=never`), so one widget cannot see another widget's tokens, and it runs in Mosaic's configuration directory (the folder that contains `mosaic.toml`). Output is capped at **1 MiB** and the widget has `timeout_seconds` (default 5 s) to answer; a non-zero exit code, a timeout or oversized output turn into an error on the widget. An `interval_seconds` below **30** is raised to 30 (the widget gets a warning); the `r` key has no such limit.

## 2. Snapshot

```json
{
  "protocol": 1,
  "summary": { "title": "Calendar", "blocks": [ ... ] },
  "detail": [ { "heading": "Today", "blocks": [ ... ], "items": [ ... ] } ],
  "actions": [ { "id": "refresh", "label": "Refresh" } ],
  "alerts": [ { "id": "deploy-42", "title": "Deploy failed", "level": "warn" } ],
  "mini": { "value": "12", "text": "tasks", "tone": "warn" }
}
```

- **`protocol`** (optional, default 1): a positive integer. If it is greater than the supported version, the widget asks the user to update Mosaic; any other value is an error.
- **`summary.title`** (required) and **`summary.blocks`**: the content of the box on Home (section 3).
- **`detail`**: the widget's tab, a list of sections. The configuration decides which tab it goes to and the shortcut for it, with `[[tabs]]` and `tab = "Name"`. Each section has a `heading`, and optionally `blocks` (drawn first) and `items`, the selectable list of `{ "id"?, "text", "flag"?, "meta"? }`. The `id` must have 1 to 200 characters, with no control or formatting characters; an invalid `id` is ignored with a warning (the item stays, but has no per-item actions).
  - **`meta`** (map of text to text): Mosaic does not draw it. It is context for the AI widget (prioritization) and is ignored by everything else. Values that are not text are discarded.
- **`actions`**: `{ "id", "label", "description"?, "applies_to"?: "widget" | "item", "params"? }`. `params` is a JSON object sent on the action's standard input (section 1); `item_id` is a reserved key.
- **`alerts`**: section 5.
- **`mini`** (optional): the column shown in focus mode, `{ "value", "text", "tone"? }`. `value` and `text` are required; `tone` defaults to `normal`. An invalid `mini` is dropped without affecting the rest of the widget. `mosaic check` prints the line `mini: <value> <text>`.

Unknown fields are ignored, so a newer widget keeps working on an older Mosaic.

## 3. Blocks

A block is an object with a `type`, stacked in the order it appears.

| `type` | Fields | Description |
| --- | --- | --- |
| `lines` | `lines`: `{ "text", "tone"?, "flag"? }` | lines of text |
| `rows` | `rows`: `{ "text", "time"?, "tags"?, "tone"?, "flag"? }` | agendas and lists; `tags` are `{ "text", "tone"? }` aligned to the right |
| `stat` | `items`: `{ "value", "label", "tone"? }`, `hint`? | indicators side by side; they wrap onto the next line when they do not fit |
| `table` | `columns`: `{ "title", "align"?: "left" \| "right" }`, `rows`: list of lists | a cell is a text or `{ "text", "tone"?, "flag"? }` |

```json
{ "type": "rows", "rows": [
  { "time": "10:30", "text": "Review", "tone": "current",
    "tags": [ { "text": "now", "tone": "ok" } ] }
] }
{ "type": "table",
  "columns": [ { "title": "Person" }, { "title": "In progress", "align": "right" } ],
  "rows": [ [ "Ana", "3" ], [ { "text": "Bia", "tone": "danger", "flag": "attention" }, "5" ] ] }
```

**Tones:** `normal`, `muted`, `ok`, `warn`, `danger` and `current` (highlight, in bold). Colors come from Mosaic's theme (`[theme]` in `mosaic.toml`, see the README): the widget only picks the tone. In the `default` theme (the terminal's own colors) they are: `normal` the terminal default, `muted` gray, `ok` green, `warn` yellow, `danger` red and `current` cyan; in a colored theme each tone uses the role of the same name in the theme's palette.
**Flags:** `updated` (`●`) and `attention` (`⚠`).

## 4. Limits, text and errors

| Limit | Value |
| --- | --- |
| blocks per widget | 20 |
| lines per block (`lines`, `rows` and `table` rows) | 200 |
| `table` columns | 10 |
| `stat` indicators | 8 |
| tags per `rows` row | 3 |
| characters per text | 500 |
| characters in `mini.value` | 8 |
| characters in `mini.text` | 24 |
| new alerts per read | 5 |

Excess is truncated (not rejected) and `mosaic check` warns about it; this is why the JSON Schema declares no `maxLength` for text.

**Text:** before it is used, all widget text loses Unicode control and formatting characters (escape, NUL, text direction), line breaks and tabs become a single space, and repeated spaces are collapsed.

**Errors:**
- Invalid JSON, a root that is not an object or a missing `summary.title`: the whole widget shows the error.
- Malformed block (not an object, no `type`, a list that is not a list, a line without `text`, a value of the wrong type): only that block is dropped and the widget gets the line `⚠ invalid block: <path>: <reason>`. See the path in `mosaic check`, for example `summary.blocks[2].rows[1].text`.
- Unknown block type, tone or flag: ignored, with a warning in `mosaic check`. Unknown field: a warning at the root, in `summary`, in sections, items, actions and `mini`; inside the entries of a block (`lines`, `rows`, `stat`, `table`) it is silently ignored.

## 5. Alerts (`alerts`)

`{ "id", "title", "body"?, "level"?: "info" | "warn" }`, with an `id` of 1 to 100 characters.

- The first read of each widget after Mosaic opens only remembers the ids; it does not notify. After that, each new `id` notifies **once**.
- Generate stable ids: the same event, the same id. Reusing an id hides the next event.
- `info` shows a box. `warn` also rings the bell and runs the pomodoro's `notify_command`.
- Simultaneous alerts go into a queue; any key dismisses the one at the front.

## 6. Testing your widget: `mosaic check`

```sh
mosaic check <command> [args...] [--env NAME=value]... [--timeout SECONDS] [--cwd DIRECTORY] [-- provider-arguments...]
```

Runs `snapshot` with the same output, time and protocol rules as Mosaic (`--timeout` has the same default as `timeout_seconds`: 5 s), without opening the terminal. By default the provider runs in the **current** directory (careful: `mosaic check python3 plugins/x.py` runs the `plugins/x.py` of the folder you are in, which may be a third-party repository; only validate widgets you trust); use `--cwd ~/.config/mosaic` to reproduce the directory in which Mosaic runs it. To forward the provider's own options, put `--` before them; from that separator on, every argument is passed through without interpretation by Mosaic. The command prints the problems and a 60-column preview of the widget:

```text
ERROR summary.blocks[0].rows[2].text: required field missing
WARNING summary.extra: unknown field

Calendar
09:00 Daily
```

Exit `0` with no errors (warnings are allowed), `1` with errors and `2` for incorrect usage, which makes it suitable for testing the widget in continuous integration. The widgets `plugins/gallery.py` and `plugins/gallery.js` show every block and are a good starting point.

## 7. Language

Mosaic passes the environment variable `MOSAIC_LANGUAGE`, with the value `en` or `pt-BR`, to every provider (`snapshot`) and to every action, carrying the language chosen for the interface. It also reaches widgets that declare `inherit_env`, which receive it without listing it.

- The provider uses **English** when the variable is missing or has any other value.
- Only the plugin's own fixed texts are translated (titles, labels, messages, tags). Data coming from the service (an issue title, a pipeline name) is left as it arrived.
- Stable ids (of items, alerts, sections and actions) do not depend on the language: switching the language must not change any `id`, otherwise an alert already seen shows up again and per-item actions can no longer find their item.
