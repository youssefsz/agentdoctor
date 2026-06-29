# Configuration

AgentDoctor has two configuration layers:

- Global user config managed by `agentdoctor config`.
- Project config in `.agentdoctor.toml`.

Precedence:

```text
CLI flags > .agentdoctor.toml > global config > safe defaults
```

## Global Config

Use global config for personal defaults, especially selected agents:

```bash
agentdoctor config agents --set codex,claude,cursor
agentdoctor config agents
agentdoctor config show
agentdoctor config reset
```

The global config path is platform-native. It is typically:

- Windows: `%APPDATA%\agentdoctor\config.toml`
- macOS: `~/Library/Application Support/agentdoctor/config.toml`
- Linux: `~/.config/agentdoctor/config.toml`

Tests and scripts can override the config directory:

```bash
AGENTDOCTOR_CONFIG_HOME=/tmp/agentdoctor-config agentdoctor config show
```

When `AGENTDOCTOR_CONFIG_HOME` is set, AgentDoctor reads and writes
`config.toml` directly inside that directory.

Example global config:

```toml
version = 1
onboarding_completed = true
selected_agents = ["codex", "claude", "cursor"]
default_output = "pretty"
progress = "auto"
color = "auto"
```

## Project Config

Use `.agentdoctor.toml` for shared repository defaults:

```toml
version = 1

[agents]
enabled = ["codex"]

[score]
minimum = 80

[commands]
format = "cargo fmt --all"
lint = "cargo clippy --workspace --all-targets -- -D warnings"
test = "cargo test --workspace"
build = "cargo build --workspace --release"

[paths]
ignore = ["target", "dist", ".next", "coverage", "node_modules"]

[rules]
require_agents_md = true
require_test_command = true
require_build_command = true
require_boundaries = true
require_env_example = true
detect_generic_instructions = true
detect_dangerous_instructions = true
```

Project config is loaded only from the scan root. It does not search parent
directories.

## Agent Selection

Supported agent names:

- `codex`
- `claude`
- `cursor`
- `copilot`
- `gemini`
- `generic`

Examples:

```bash
agentdoctor scan --agents codex,copilot
agentdoctor init --dry-run --agents codex,claude,cursor
```

If no selected agents are configured, AgentDoctor defaults to `generic`.

## Score Minimum

When `[score].minimum` is set, `scan` still prints the report. If the score is
below the minimum, the process exits with code `1`.

This is useful for CI:

```bash
agentdoctor scan --format json --no-interactive
```

