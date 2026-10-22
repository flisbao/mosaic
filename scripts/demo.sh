#!/usr/bin/env bash
# Demo do mosaic com dados falsos: config isolado, sem tocar ~/.config/mosaic.
set -euo pipefail
REPO=$(cd "$(dirname "$0")/.." && pwd)
DEMO=$(mktemp -d)
mkdir -p "$DEMO/xdg/mosaic" "$DEMO/state"
PORT_FILE="$DEMO/port"
FAKE_PR_TOKEN=segredo python3 "$REPO/plugins/tests/fake_pull_request_server.py" > "$PORT_FILE" &
SERVER=$!
trap 'kill $SERVER 2>/dev/null; rm -rf "$DEMO"' EXIT
until [ -s "$PORT_FILE" ]; do sleep 0.1; done
PORT=$(head -1 "$PORT_FILE")
export FAKE_TOKEN=segredo
cat > "$DEMO/xdg/mosaic/config.toml" <<TOML
[[tabs]]
name = "PRs"

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
