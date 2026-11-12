pub(super) const DEFAULT_CONFIG: &str = r##"# Mosaic: each [[widgets]] block below is a widget of the panel.
# After editing this file, press R in the panel to reload it.
#
#   mosaic plugins install jira     copies a service plugin (see `mosaic plugins list`)
#   mosaic plugins setup jira       asks the questions and writes the widget here
#   mosaic check --config           checks this file
#   mosaic --default-config         shows every option, commented

[[widgets]]
id = "example-python"
command = "python3"
args = ["plugins/example.py"]

[[widgets]]
id = "kanban"
command = "python3"
args = ["plugins/kanban.py"]
interval_seconds = 30
llm_sections = ["inbox", "in progress", "acompanhar_jira"]

[widgets.env]
KANBAN_DIR = "~/kanban"
"##;

pub(super) const REFERENCE_CONFIG: &str = r##"[[widgets]]
id = "kanban"
command = "python3"
args = ["plugins/kanban.py"]
interval_seconds = 30
llm_sections = ["inbox", "in progress", "acompanhar_jira"]

[widgets.env]
KANBAN_DIR = "~/kanban"

[[widgets]]
id = "example-python"
command = "python3"
args = ["plugins/example.py"]

[[widgets]]
id = "example-node"
command = "node"
args = ["plugins/example.js"]

[pomodoro]
enabled = true
focus_minutes = 25
short_break_minutes = 5
long_break_minutes = 15
cycles_before_long_break = 4
suggestions = ["Drink some water", "Stand up and stretch", "Have a coffee"]
bell = true
notify_command = ""

[llm]
enabled = true
harness = "claude"
timeout_seconds = 120
confirm_before_send = true
priority_prompt = "prompts/prioridade.md"

# --- Interface language (default: the system one; "en" or "pt-BR") ---
# Order of choice: this field, the MOSAIC_LANGUAGE variable, then LC_ALL/LC_MESSAGES/LANG.
# [ui]
# language = "en"

# --- Color theme (uncomment to change it; `mosaic themes` lists the presets with samples) ---
# Without this section Mosaic uses your terminal colors (the "default" theme). The colored themes
# paint the background of the whole screen and need a terminal with truecolor or 256 colors.
# [theme]
# name = "tokyo-night"
# # paint_background = false   # does not paint the background (normal text uses the terminal color; the other roles follow the theme)
# # [theme.colors]             # overrides roles: foreground, background, muted, ok, warn, danger, accent, on_accent
# # accent = "#ff9e64"

# --- New version notice (on by default) ---
# When the panel opens, at most once every 24 hours, Mosaic makes a single request to
# github.com (releases/latest) to find out whether there is a new version, sending none of your
# data, and shows it in the notice bar. To turn it off, uncomment below or export MOSAIC_NO_UPDATE_CHECK=1.
# [update]
# check = false

# --- Service plugins (Jira, GitHub, GitLab, Azure DevOps, Azure Pipelines, calendar) ---
# They do not ship in the binary: install them with `mosaic plugins install` (all) or
# `mosaic plugins install jira github` (only some). `--force` updates the ones already installed.
# Then uncomment the widget you want below.

# --- Jira (uncomment and adjust; step by step in plugins/jira/README.md in the repository) ---
# Each widget defines its own JQL in args; Mosaic appends `snapshot`.
# Keep the token in an environment variable, for example:
#   export JIRA_TOKEN_EMPRESA="your-api-token"
#
# My open tasks.
# [[widgets]]
# id = "jira-minhas-tarefas"
# command = "python3"
# args = ["plugins/jira/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC", "--title", "My tasks", "--view", "lines"]
# interval_seconds = 120
# timeout_seconds = 45
# llm_sections = ["My tasks"]
# inherit_env = ["JIRA_TOKEN_EMPRESA"]
#
# [widgets.env]
# JIRA_SITE = "company.atlassian.net"
# JIRA_EMAIL = "you@company.com"
# JIRA_TOKEN_ENV = "JIRA_TOKEN_EMPRESA"
#
# What each person on the team is working on right now.
# [[widgets]]
# id = "jira-time-em-andamento"
# command = "python3"
# args = ["plugins/jira/jira.py", "--jql", "project = VHPJ AND statusCategory = \"In Progress\"", "--title", "Team in progress", "--group-by", "assignee", "--view", "table", "--columns", "key,summary,status"]
# interval_seconds = 120
# timeout_seconds = 45
# inherit_env = ["JIRA_TOKEN_EMPRESA"]
#
# [widgets.env]
# JIRA_SITE = "company.atlassian.net"
# JIRA_EMAIL = "you@company.com"
# JIRA_TOKEN_ENV = "JIRA_TOKEN_EMPRESA"

# --- Google Calendar (uncomment one option; step by step in plugins/calendar/README.md in the repository) ---
# Secret ICS link, kept in an environment variable:
#   export CALENDAR_ICS_PERSONAL="https://calendar.google.com/calendar/ical/.../basic.ics"
# [[widgets]]
# id = "agenda"
# command = "python3"
# args = ["plugins/calendar/calendar_widget.py"]
# interval_seconds = 300
# timeout_seconds = 20
# inherit_env = ["CALENDAR_ICS_PERSONAL"]
#
# [widgets.env]
# CALENDAR_SOURCE = "ics"
# CALENDAR_ICS_URL_ENV = "CALENDAR_ICS_PERSONAL"
#
# GNOME Online Accounts: replace the two lines of [widgets.env] with CALENDAR_SOURCE = "goa"
# and use inherit_env = ["DBUS_SESSION_BUS_ADDRESS"].
# gcalcli: CALENDAR_SOURCE = "gcalcli" (run `gcalcli init` first).

# --- Pull requests (uncomment yours; step by step in plugins/<github|azure-devops|azure-pipelines|gitlab>/README.md in the repository) ---
#   export GITHUB_TOKEN_PRS="..."   export GITLAB_TOKEN_MRS="..."   (Azure DevOps uses az login: no token)
# [[widgets]]
# id = "github-prs"
# command = "python3"
# args = ["plugins/github/github_prs.py"]
# interval_seconds = 120
# timeout_seconds = 30
# inherit_env = ["GITHUB_TOKEN_PRS"]
#
# [widgets.env]
# GITHUB_TOKEN_ENV = "GITHUB_TOKEN_PRS"
#
# [[widgets]]
# id = "azure-devops-prs"
# command = "python3"
# args = ["plugins/azure-devops/azure_devops.py"]
# interval_seconds = 120
# timeout_seconds = 30
# inherit_env = []
#
# [widgets.env]
# AZDO_ORGANIZATION = "my-org"
# AZDO_AUTH_METHOD = "azure-cli"
#
# [[widgets]]
# id = "azure-pipelines"
# command = "python3"
# args = ["plugins/azure-pipelines/azure_pipelines.py"]
# interval_seconds = 900
# timeout_seconds = 60
# inherit_env = []
#
# [widgets.env]
# AZDO_ORGANIZATION = "my-org"
# AZDO_PROJECTS = "my-project"
# AZDO_AUTH_METHOD = "azure-cli"
#
# [[widgets]]
# id = "gitlab-mrs"
# command = "python3"
# args = ["plugins/gitlab/gitlab_mrs.py"]
# interval_seconds = 120
# timeout_seconds = 30
# inherit_env = ["GITLAB_TOKEN_MRS"]
#
# [widgets.env]
# GITLAB_TOKEN_ENV = "GITLAB_TOKEN_MRS"

# --- Block gallery (uncomment to see every block; see PROTOCOL.md) ---
# [[widgets]]
# id = "gallery-python"
# command = "python3"
# args = ["plugins/gallery.py"]
# span = 2
#
# [[widgets]]
# id = "gallery-node"
# command = "node"
# args = ["plugins/gallery.js"]
# span = 2
"##;
