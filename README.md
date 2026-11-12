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
 ┌ GitHub ───────────────────────┐ ┌ Calendar ─────────────────────┐
 │ 3 to review · 2 mine          │ │ 10:30 Code review         now │
 │ 12d acme/api #10 …      [new] │ │ 12:00 Lunch                   │
 │ 11d acme/api #11 … [new push] │ │ 14:00 1:1 with the boss       │
 └───────────────────────────────┘ └───────────────────────────────┘
 ┌ GitLab ───────────────────────┐ ┌ Jira ─────────────────────────┐
 │ 1 to review · 4 mine          │ │ PROJ-12 Fix the flaky test    │
 │ 9d  group/web !42 …     [new] │ │ PROJ-15 Ship the thing        │
 └───────────────────────────────┘ └───────────────────────────────┘
  ←↑↓→ move  Enter open  Tab next tab  1-9 go to tab  A-Z jump  ? help  q quit
```

<p align="center"><i>(An artist's impression. Run <code>scripts/demo.sh</code> to see the real thing, with fake data.)</i></p>

## Why

Your day lives in a dozen tabs: pull requests waiting for you, tickets, meetings, that one kanban folder you swore you'd keep tidy. Mosaic puts all of it in one terminal, refreshes it in the background, and **taps you on the shoulder only when something actually changed**.

And because every widget is just a program that prints JSON, adding a new one takes about five minutes and zero frameworks.

## Install

**One line (Linux x86_64 and aarch64, macOS Apple Silicon and Intel)**: no `sudo`, no package manager. It downloads the binary for your system (a static build on Linux, so it runs on any distribution), checks its SHA-256 and installs it in `~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/flisbao/mosaic/main/install.sh | sh
```

`MOSAIC_VERSION=v0.0.11` installs a specific version and `MOSAIC_INSTALL_DIR=/some/dir` changes the destination. If the destination is not in your `PATH`, the script prints the line to add. It needs `curl` or `wget`, `tar` and `sha256sum` or `shasum`. Prefer to read it first? It is [`install.sh`](install.sh), a short POSIX script.

**Homebrew** (available once the tap `flisbao/homebrew-mosaic` is published; the formula is attached to every release as `mosaic.rb`):

```sh
brew install flisbao/mosaic/mosaic
```

**Ubuntu 24.04 or newer** (the `.deb` from the [latest release](https://github.com/flisbao/mosaic/releases/latest)):

```sh
sudo dpkg -i mosaic_*_amd64.deb
```

**Tarball**: grab the `.tar.gz` for your system from the release, check it against the `.sha256` (or against `SHA256SUMS`, which lists every file), and drop `mosaic` somewhere in your `PATH`. Targets: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` and `x86_64-apple-darwin`.

**From source** (Rust 1.88+):

```sh
git clone https://github.com/flisbao/mosaic && cd mosaic
cargo install --path .
```

## Update

```sh
mosaic update --check    # says whether there is a newer version
mosaic update            # updates, asking [y/N] first (--yes skips the question)
```

`mosaic update` looks at how the running binary was installed. For a binary from `install.sh` or a manual tarball it downloads the tarball for your system, checks the SHA-256, extracts it next to the current executable and swaps it atomically; it never uses `sudo`, so a read-only folder gets an explanation instead. A `.deb` install is left alone: it prints the URL of the new `.deb` and the `sudo apt install ./...` command. Homebrew gets `brew upgrade mosaic` and `cargo install` builds get the cargo command. Nothing is changed unless you answer `y` (or pass `--yes`).

**Update notice.** When the panel opens, a background thread asks `github.com` (the redirect of `releases/latest`, a single request) for the newest version, at most once every 24 hours, and shows `version X available; run mosaic update` in the notice line. No data about you is sent, it never blocks the screen and a failure is silent. Turn it off with `check = false` under `[update]` in `mosaic.toml` or with `MOSAIC_NO_UPDATE_CHECK=1` in the environment. `MOSAIC_DOWNLOAD_BASE` points both the installer and `mosaic update` at a mirror of the releases page.

Then just run:

```sh
mosaic
```

On the first run Mosaic creates `~/.config/mosaic/` with a short `mosaic.toml`: two example widgets that only need `python3`. Add service widgets with `mosaic plugins setup <name>`, or run `mosaic --default-config` to see every option, commented. A key Mosaic does not know is ignored with a warning, in the panel and in `mosaic check --config`. Edit, press `R`, done. To add a widget without leaving the panel, press `+`.

> **Just want to look around?** `scripts/demo.sh` (from the repo) starts Mosaic with fake data and an isolated config. It never touches `~/.config/mosaic`.

<!-- demo: put the GIF recorded with `vhs scripts/demo.tape` here (manual step) -->

Run `mosaic --help` for the commands and the first steps, and `mosaic --version` to see which version you have installed. Any other unknown argument is an error instead of opening the panel.

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
| `+` | Add a widget: choose an installed plugin, answer its questions, review the block and save |
| `p` / `x` | Start or pause the pomodoro / reset it |
| `i` | Ask an AI what to do next (if `[llm]` is on) |
| `PgUp` `PgDn` `Home` `End` | Move faster inside an open widget |
| `Esc` | Back |
| `?` | Help |
| `q` or `Ctrl+C` | Quit (gracefully: providers are cleaned up) |

`Alt` needs your terminal to send `Esc` before the letter (on macOS, enable "Option as Meta").

## Language

The interface is in English by default; Brazilian Portuguese (`pt-BR`) is available. Mosaic picks the language in this order:

1. `language` under `[ui]` in `mosaic.toml`: `"en"` or `"pt-BR"`.
2. The `MOSAIC_LANGUAGE` environment variable (same values).
3. The locale: the first non-empty of `LC_ALL`, `LC_MESSAGES` and `LANG`. A value starting with `pt` means pt-BR; anything else means English.

```toml
[ui]
language = "pt-BR"
```

An invalid value in `[ui] language` becomes a warning and Mosaic falls back to the detection above. Press `R` to reload `mosaic.toml` and apply a new language. The official plugins follow the same language: Mosaic passes `MOSAIC_LANGUAGE` (`en` or `pt-BR`) to every provider and action (see [`PROTOCOL.md`](PROTOCOL.md)), and a plugin without the variable answers in English. Data that comes from a service, such as an issue title, is never translated. `install.sh` prints English only, since it runs before Mosaic exists.

## Themes

By default Mosaic uses your terminal's own colors. Pick a built-in theme in `mosaic.toml` (press `R` to reload it):

```toml
[theme]
name = "tokyo-night"
# paint_background = false        # keep your terminal background (normal text then uses the terminal's own color)
# [theme.colors]                   # override single roles
# accent = "#ff9e64"
```

Built-in themes: `tokyo-night`, `tokyo-night-storm`, `solarized-dark`, `solarized-light`, `gruvbox-dark`, `gruvbox-light`, `dracula`, `nord`, `catppuccin-mocha`, `one-dark`, `monokai` and `everforest-dark` (plus `default`, the terminal's colors). Run `mosaic themes` to list them (with color swatches on truecolor terminals), or `mosaic themes nord` to see the roles, hex values and contrast ratios.

A colored theme paints the whole screen background and needs a terminal with truecolor (`COLORTERM=truecolor`) or 256 colors; on a basic terminal Mosaic falls back to `default` and tells you. Widgets are unaffected: they only choose a tone (`normal`, `muted`, `ok`, `warn`, `danger`, `current`) and the theme decides how it looks.

## Batteries included

| Widget | What it shows | Setup guide |
| --- | --- | --- |
| **Jira** | Any JQL you like: lines, tables or just counters, grouped by assignee, status or priority | [`plugins/jira`](plugins/jira/README.md) |
| **GitHub** | PRs waiting for your review and your own PRs, tagged `new`, `new push`, `approved`, `conflict`… | [`plugins/github`](plugins/github/README.md) |
| **GitLab** | Same idea for merge requests | [`plugins/gitlab`](plugins/gitlab/README.md) |
| **Azure DevOps** | Same idea for Azure PRs | [`plugins/azure-devops`](plugins/azure-devops/README.md) |
| **Azure Pipelines** | Your slowest build pipelines (average duration) and, on Enter, the steps that take the longest in each | [`plugins/azure-pipelines`](plugins/azure-pipelines/README.md) |
| **Calendar** | Your week, from Google Calendar via secret ICS link, GNOME Online Accounts or `gcalcli` | [`plugins/calendar`](plugins/calendar/README.md) |
| **Kanban** | A folder of Markdown files per column, with a "move task" action | `plugins/kanban.py` |
| **Gallery** | Every block type, for the curious (and for copy-pasting) | `plugins/gallery.py` |

Only Kanban, `example` and `gallery` ship inside the binary. Install service plugins directly from a public GitHub repository with **Git** and Mosaic 0.0.10 or later:

```sh
mosaic plugins list                                      # six official shortcuts, no network
mosaic plugins install jira github                       # official shortcuts
mosaic plugins install flisbao/mosaic/plugins/jira        # owner/repo/subdir
mosaic plugins install owner/repo                         # setup.toml at repository root
mosaic plugins install jira --force                      # replace local edits, after confirmation
mosaic plugins install jira --yes                        # skip the confirmation for automation
mosaic plugins setup jira                                # write a widget through the terminal interview
```

Remote installation shows source, plugin version, minimum Mosaic version and destination, then asks `[y/N]`; Enter or EOF cancels. `--force` permits replacement and still requires confirmation unless `--yes` is present. Each plugin declares `version` and `min_mosaic_version` in its `setup.toml`; incompatible versions are rejected even with `--force`. Git follows the repository's default branch; installations do not pin a revision. A third-party plugin uses `owner/repo[/subdir]`; only the six names listed by `plugins list` have shortcuts. A full reference must be installed on its own; multiple official shortcuts can be installed together.

The old collective **mosaic-plugins** package is retired. Existing installations remain usable and are never removed automatically. To copy its files offline, run `mosaic plugins install --from /usr/share/mosaic/plugins jira`; `--from DIR` also accepts a local checkout's `plugins` directory. The panel and local installation do not need Git. `--to DIR` changes the plugins destination. Remote packages contain a regular script, `SKILL.md` and a versioned `setup.toml`. The installer copies only direct `.py`/`.js` files and `README.md`, `SKILL.md`, `setup.toml`; it does not install dependencies or run the plugin. Python scripts need `python3`; JavaScript scripts need `node`, which the setup selects from the script extension. The manifest's `script` describes the installed path (`plugins/<name>/<file>.py`), while the source file lives directly in the selected repository folder. The installer links the skill into `~/.claude/skills` when Claude Code is present; `--no-skill` skips linking and `--skills-dir DIR` selects another directory.

`mosaic plugins setup <name>` asks one question at a time, shows the finished `[[widgets]]` block and only appends it to `mosaic.toml` after confirmation. It never asks for a token, only the environment variable name or file path. `--print` shows the block, `--advanced` asks optional settings and `--config-dir DIR` selects another configuration folder. Run it again with another id for a second widget, or copy an example from `mosaic --default-config`.

Inside the panel, `+` opens the same interview as a modal: it lists the installed plugins that have a `setup.toml`, asks the same questions with the same validation (an invalid answer shows its message and keeps the question), shows the `[[widgets]]` block and, on `Enter`, appends it to `mosaic.toml` and reloads like `R`. `Esc` (or `Ctrl+C`) cancels at any step and writes nothing. Without installed plugins the modal shows `mosaic plugins install <name>` and the official names; installing from inside the panel is not supported. An empty panel says `no widgets yet` and points to `+`.

The Azure DevOps widgets sign in with Microsoft Entra ID (`az login`, device code, service principal or managed identity); personal access tokens are no longer supported.

The pull request widgets remember what you have already reviewed, so a fresh push to a PR you reviewed lights up as `new push`. If the network drops, they show the last good data with an `(offline)` note instead of an empty box.

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

It validates the output against the protocol and prints a 60-column preview. It exits `0`, `1` or `2`, so it works in CI too. The full contract is in [`PROTOCOL.md`](PROTOCOL.md) (a Portuguese translation is in [`PROTOCOL.pt-BR.md`](PROTOCOL.pt-BR.md)) and the JSON Schema in [`schema/protocol.schema.json`](schema/protocol.schema.json).

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

A tab is a "panel": the widgets whose `tab` names it live there, and a widget without `tab` goes to the first tab. You do not have to edit the file by hand to use them:

```sh
mosaic tabs list                 # each tab with its widgets
mosaic tabs add Studies          # declares a new tab
mosaic plugins setup jira        # also asks which tab the widget goes to (or creates a new one)
```

Careful with the first tab: when the file declares no `[[tabs]]` yet, the first one you create becomes the home of every widget that already exists (they have no `tab`), and Mosaic warns you before writing.

## Pomodoro and AI

- **Pomodoro**: `p` starts it, a bell and an optional `notify_command` tell you when to stand up. Configure it under `[pomodoro]`.
- **AI recommendation**: press `i` and Mosaic sends a summary of your widgets to `claude` or `opencode` (via stdin, never as an argument) and shows you a recommendation. It asks before sending anything unless you turn that off. Widgets choose what the AI sees with `llm_sections`.

## Built to be a good citizen

- **No daemon.** Each refresh runs your program, reads its JSON and lets it exit. Timeouts and the 1 MiB output cap end it with `SIGKILL` (the whole process group).
- **A broken widget breaks only itself.** It shows its error and keeps the last good data.
- **Secrets stay out of sight.** Providers read the *name* of an environment variable or a token file, never a token in the command line, and `inherit_env` lets a widget see only the variables it needs.
- **The UI never waits for a provider.** Each widget has its own thread.

## Claude Code skills

Mosaic ships [Claude Code](https://claude.com/claude-code) skills that interview you and configure each widget for you (without ever asking you to paste a token). Each plugin brings its own, in `plugins/<name>/SKILL.md`, and `mosaic plugins install` links it into `~/.claude/skills/` (it never overwrites anything of yours and is safe to run again). The skill that teaches how to write new widgets comes with the core package in `/usr/share/mosaic/skills`; link it with:

```sh
mosaic skills install
```

With a local checkout, the skills also live in this repository: [`plugins/<name>/SKILL.md`](plugins) (linked from [`.claude/skills`](.claude/skills)) and `.claude/skills/mosaic-widget-authoring`.

## Contributing

Want to add a plugin? Welcome! There is one house rule: **a new plugin comes with its own skill**, so the next person (or agent) can configure and maintain it. [`CONTRIBUTING.md`](CONTRIBUTING.md) has the recipe, and `python3 -m unittest discover -s plugins/tests` will remind you if you forget.

Portuguese readers: [`README.pt-BR.md`](README.pt-BR.md) has the original, more detailed notes.

## License

[MIT](LICENSE)
