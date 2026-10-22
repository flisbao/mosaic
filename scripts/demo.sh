#!/usr/bin/env bash
# Demo do mosaic com dados falsos: config isolado, sem tocar ~/.config/mosaic.
set -euo pipefail
REPO=$(cd "$(dirname "$0")/.." && pwd)
DEMO=$(mktemp -d)
mkdir -p "$DEMO/xdg/mosaic" "$DEMO/state"
PORT_FILE="$DEMO/port"
FAKE_PR_TOKEN=segredo python3 "$REPO/plugins/tests/fake_pull_request_server.py" > "$PORT_FILE" &
SERVER=$!
JIRA_PORT_FILE="$DEMO/jira-port"
FAKE_JIRA_EMAIL=demo@exemplo.com FAKE_JIRA_TOKEN=segredo python3 "$REPO/plugins/tests/fake_jira_server.py" > "$JIRA_PORT_FILE" &
JIRA_SERVER=$!
trap 'kill $SERVER $JIRA_SERVER 2>/dev/null; rm -rf "$DEMO"' EXIT
until [ -s "$PORT_FILE" ] && [ -s "$JIRA_PORT_FILE" ]; do sleep 0.1; done
PORT=$(head -1 "$PORT_FILE")
JIRA_PORT=$(head -1 "$JIRA_PORT_FILE")
export FAKE_TOKEN=segredo
cat > "$DEMO/xdg/mosaic/config.toml" <<TOML
[[tabs]]
name = "PRs"

[[tabs]]
name = "Jira"

[[tabs]]
name = "Galeria"

[[widgets]]
id = "github-prs"
command = "python3"
args = ["$REPO/plugins/github/github_prs.py"]
interval_seconds = 30
tab = "PRs"
inherit_env = ["FAKE_TOKEN"]
[widgets.env]
GITHUB_API_URL = "http://127.0.0.1:$PORT"
GITHUB_TOKEN_ENV = "FAKE_TOKEN"
GITHUB_STATE_DIR = "$DEMO/state"

[[widgets]]
id = "gitlab-mrs"
command = "python3"
args = ["$REPO/plugins/gitlab/gitlab_mrs.py"]
interval_seconds = 30
tab = "PRs"
inherit_env = ["FAKE_TOKEN"]
[widgets.env]
GITLAB_URL = "http://127.0.0.1:$PORT"
GITLAB_TOKEN_ENV = "FAKE_TOKEN"
GITLAB_STATE_DIR = "$DEMO/state"

[[widgets]]
id = "azure-devops-prs"
command = "python3"
args = ["$REPO/plugins/azure-devops/azure_devops.py"]
interval_seconds = 30
tab = "PRs"
inherit_env = ["FAKE_TOKEN"]
[widgets.env]
AZDO_BASE_URL = "http://127.0.0.1:$PORT"
AZDO_ORGANIZATION = "org"
AZDO_TOKEN_ENV = "FAKE_TOKEN"
AZDO_STATE_DIR = "$DEMO/state"

[[widgets]]
id = "jira-minhas-tarefas"
command = "python3"
args = ["$REPO/plugins/jira/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done", "--title", "Minhas tarefas"]
interval_seconds = 30
tab = "Jira"
inherit_env = ["FAKE_TOKEN"]
[widgets.env]
JIRA_SITE = "demo.atlassian.net"
JIRA_BASE_URL = "http://127.0.0.1:$JIRA_PORT"
JIRA_EMAIL = "demo@exemplo.com"
JIRA_TOKEN_ENV = "FAKE_TOKEN"
JIRA_STATE_DIR = "$DEMO/state"

[[widgets]]
id = "jira-time"
command = "python3"
args = ["$REPO/plugins/jira/jira.py", "--jql", "project = VHPJ AND statusCategory != Done", "--title", "Time", "--group-by", "assignee"]
interval_seconds = 30
tab = "Jira"
inherit_env = ["FAKE_TOKEN"]
[widgets.env]
JIRA_SITE = "demo.atlassian.net"
JIRA_BASE_URL = "http://127.0.0.1:$JIRA_PORT"
JIRA_EMAIL = "demo@exemplo.com"
JIRA_TOKEN_ENV = "FAKE_TOKEN"
JIRA_STATE_DIR = "$DEMO/state"

[[widgets]]
id = "jira-sprint"
command = "python3"
args = ["$REPO/plugins/jira/jira.py", "--jql", "sprint in openSprints() AND project = VHPJ", "--title", "Sprint", "--group-by", "status"]
interval_seconds = 30
tab = "Jira"
inherit_env = ["FAKE_TOKEN"]
[widgets.env]
JIRA_SITE = "demo.atlassian.net"
JIRA_BASE_URL = "http://127.0.0.1:$JIRA_PORT"
JIRA_EMAIL = "demo@exemplo.com"
JIRA_TOKEN_ENV = "FAKE_TOKEN"
JIRA_STATE_DIR = "$DEMO/state"

[[widgets]]
id = "gallery-python"
command = "python3"
args = ["$REPO/plugins/gallery.py"]
span = 2
tab = "Galeria"

[pomodoro]
enabled = true

[llm]
enabled = false
TOML
cd "$REPO"
cargo build --release --quiet
XDG_CONFIG_HOME="$DEMO/xdg" ./target/release/mosaic
