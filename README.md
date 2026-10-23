<h1 align="center">🧩 Mosaic</h1>

<p align="center">
  <b>A terminal dashboard built from tiny programs.</b><br>
  If it can print JSON, it can be a widget.
</p>

<p align="center">
  <a href="https://github.com/flisbao/mosaic/releases/latest"><img alt="release" src="https://img.shields.io/github/v/release/flisbao/mosaic?color=cyan"></a>
  <img alt="license" src="https://img.shields.io/badge/license-MIT-green">
  <img alt="platforms" src="https://img.shields.io/badge/platforms-Linux%20%7C%20macOS-blue">
  <img alt="made with" src="https://img.shields.io/badge/made%20with-Rust%20%F0%9F%A6%80-orange">
</p>

```text
 10:42  Fri 23/10                                              🍅 18:31 focus
  1 PRs   2 Jira   3 Gallery
 ┌ GitHub ───────────────────────┐ ┌ Agenda ───────────────────────┐
 │ 3 to review · 2 mine          │ │ 10:30 Code review         now │
 │ 12d acme/api #10 …     [novo] │ │ 12:00 Lunch                   │
 │ 11d acme/api #11 … [push novo]│ │ 14:00 1:1 with the boss       │
 └───────────────────────────────┘ └───────────────────────────────┘
 ┌ GitLab ───────────────────────┐ ┌ Jira ─────────────────────────┐
 │ 1 to review · 4 mine          │ │ PROJ-12 Fix the flaky test    │
 │ 9d  group/web !42 …    [novo] │ │ PROJ-15 Ship the thing        │
 └───────────────────────────────┘ └───────────────────────────────┘
  ←↑↓→ move  Enter open  Tab next tab  1-9 go to tab  A-Z jump  ? help  q quit
```

<p align="center"><i>(An artist's impression. Run <code>scripts/demo.sh</code> to see the real thing, with fake data. Heads-up: the interface text is in Portuguese for now.)</i></p>

## Why

Your day lives in a dozen tabs: pull requests waiting for you, tickets, meetings, that one kanban folder you swore you'd keep tidy. Mosaic puts all of it in one terminal, refreshes it in the background, and **taps you on the shoulder only when something actually changed**.

And because every widget is just a program that prints JSON, adding a new one takes about five minutes and zero frameworks.

## Install

**Ubuntu 24.04 or newer** (the `.deb` from the [latest release](https://github.com/flisbao/mosaic/releases/latest)):

```sh
sudo dpkg -i mosaic_*_amd64.deb
```

**Linux x86_64, macOS (Apple Silicon and Intel)**: grab the matching `.tar.gz` from the release, check it against the `.sha256`, and drop `mosaic` somewhere in your `PATH`.

**From source** (Rust 1.88+):

```sh
git clone https://github.com/flisbao/mosaic && cd mosaic
cargo install --path .
```

Then just run:

```sh
mosaic
```

On the first run Mosaic creates `~/.config/mosaic/` with a `mosaic.toml`, a few example widgets and **ready-to-uncomment blocks** for Jira, Google Calendar, GitHub, GitLab and Azure DevOps. Edit, press `R`, done.

> **Just want to look around?** `scripts/demo.sh` (from the repo) starts Mosaic with fake data and an isolated config. It never touches `~/.config/mosaic`.

## Keys

Press `?` at any time: the help is always one key away, and the bottom line shows the keys that make sense right now.

| Key | What it does |
| --- | --- |
| `1`-`9` | Jump to that tab |
| `Tab` / `BackTab` | Next / previous tab |
| `←` `↑` `→` `↓` or `h` `j` `k` `l` | Move between widgets (or items, when one is open) |
| `Enter` | Open the focused widget |
| `Shift+letter` or `Alt+letter` | Jump straight to the widget with that shortcut letter (`A`-`Z`, except `R`) |
| `a` | Actions menu for the focused widget or item |
| `r` | Refresh everything now |
| `R` | Reload `mosaic.toml` |
| `p` / `x` | Start or pause the pomodoro / reset it |
| `i` | Ask an AI what to do next (if `[llm]` is on) |
| `PgUp` `PgDn` `Home` `End` | Move faster inside an open widget |
| `Esc` | Back |
| `?` | Help |
| `q` or `Ctrl+C` | Quit (gracefully: providers are cleaned up) |

`Alt` needs your terminal to send `Esc` before the letter (on macOS, enable "Option as Meta").

## Batteries included

| Widget | What it shows | Setup guide |
| --- | --- | --- |
| **Jira** | Any JQL you like: lines, tables or just counters, grouped by assignee, status or priority | [`plugins/jira`](plugins/jira/README.md) |
| **GitHub** | PRs waiting for your review and your own PRs, tagged `novo`, `push novo`, `aprovado`, `conflito`… | [`plugins/github`](plugins/github/README.md) |
| **GitLab** | Same idea for merge requests | [`plugins/gitlab`](plugins/gitlab/README.md) |
| **Azure DevOps** | Same idea for Azure PRs | [`plugins/azure-devops`](plugins/azure-devops/README.md) |
| **Calendar** | Your week, from Google Calendar via secret ICS link, GNOME Online Accounts or `gcalcli` | [`plugins/calendar`](plugins/calendar/README.md) |
| **Kanban** | A folder of Markdown files per column, with a "move task" action | `plugins/kanban.py` |
| **Gallery** | Every block type, for the curious (and for copy-pasting) | `plugins/gallery.py` |

The pull request widgets remember what you have already reviewed, so a fresh push to a PR you reviewed lights up as `push novo`. If the network drops, they show the last good data with an `(offline)` note instead of an empty box.

## Write your own widget

A widget answers `snapshot` by printing a JSON document. That's the whole contract:

```python
import json
print(json.dumps({"summary": {"title": "Hello", "blocks": [
    {"type": "lines", "lines": [{"text": "all good", "tone": "ok"}]}
]}}))
```

```toml
[[widgets]]
id = "hello"
command = "python3"
args = ["hello.py"]
interval_seconds = 60
```

Four block types (`lines`, `rows`, `stat`, `table`), six tones, two flags, optional drill-down `detail` with selectable items, `actions`, `alerts` that notify once, and a tiny `mini` summary. Any language works: Python, Node, shell, Go, a binary you wrote on a Tuesday.

Check your work without opening the TUI:

```sh
mosaic check python3 hello.py
```

It validates the output against the protocol and prints a 60-column preview. It exits `0`, `1` or `2`, so it works in CI too. The full contract is in [`PROTOCOL.md`](PROTOCOL.md) (in Portuguese for now) and the JSON Schema in [`schema/protocol.schema.json`](schema/protocol.schema.json).

## Tabs and layout

```toml
[[tabs]]
name = "Work"

[[tabs]]
name = "Personal"

[[widgets]]
id = "jira-mine"
command = "python3"
args = ["plugins/jira/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done", "--title", "My tasks"]
interval_seconds = 120
tab = "Work"
max_items = 5
```

Without `[[tabs]]` everything lives on a single tab. Check the file any time with `mosaic check --config`.

## Pomodoro and AI

- **Pomodoro**: `p` starts it, a bell and an optional `notify_command` tell you when to stand up. Configure it under `[pomodoro]`.
- **AI recommendation**: press `i` and Mosaic sends a summary of your widgets to `claude` or `opencode` (via stdin, never as an argument) and shows you a recommendation. It asks before sending anything unless you turn that off. Widgets choose what the AI sees with `llm_sections`.

## Built to be a good citizen

- **No daemon.** Each refresh runs your program, reads its JSON and lets it exit. Timeouts and the 1 MiB output cap end it with `SIGKILL` (the whole process group).
- **A broken widget breaks only itself.** It shows its error and keeps the last good data.
- **Secrets stay out of sight.** Providers read the *name* of an environment variable or a token file, never a token in the command line, and `inherit_env` lets a widget see only the variables it needs.
- **The UI never waits for a provider.** Each widget has its own thread.

## Claude Code skills

Mosaic ships [Claude Code](https://claude.com/claude-code) skills that interview you and configure each widget for you (without ever asking you to paste a token), plus one for writing new widgets. The `.deb` puts them in `/usr/share/mosaic/skills`. Link them into your Claude Code with:

```sh
mosaic skills install
```

It creates symlinks in `~/.claude/skills/`, never overwrites anything of yours and is safe to run again. Without the package, they live in [`.claude/skills`](.claude/skills) in this repository.

## Contributing

Want to add a plugin? Welcome! There is one house rule: **a new plugin comes with its own skill**, so the next person (or agent) can configure and maintain it. [`CONTRIBUTING.md`](CONTRIBUTING.md) has the recipe, and `python3 -m unittest discover -s plugins/tests` will remind you if you forget.

Portuguese readers: [`README.pt-BR.md`](README.pt-BR.md) has the original, more detailed notes.

## License

[MIT](LICENSE)
