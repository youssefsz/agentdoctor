# CLI Reference

AgentDoctor is CLI-first. All commands are safe to run from a repository root.
In v0.1, repository setup is dry-run only; the `config` command can write global
user config outside the repository.

## Global Options

```bash
agentdoctor [--no-interactive] <command>
agentdoctor --help
agentdoctor --version
```

`--no-interactive` disables prompts. Use it in CI, scripts, and automation.

## scan

```bash
agentdoctor scan [path] [--format pretty|json] [--agents <list>] [--no-progress] [--no-interactive]
```

Scans a repository and prints an audit report.

Options:

- `path`: repository path to scan. Defaults to the current directory.
- `--format pretty|json`: output format. Defaults to `pretty`.
- `--agents <list>`: comma-separated selected agents. Supported values are
  `codex`, `claude`, `cursor`, `copilot`, `gemini`, and `generic`.
- `--no-progress`: accepted for script-friendly scans. v0.1 does not render a
  progress UI.
- `--no-interactive`: skip first-run onboarding prompts.

Examples:

```bash
agentdoctor scan
agentdoctor scan . --agents codex,claude,cursor
agentdoctor scan --format json --no-interactive
```

JSON output is valid JSON only on stdout. Human text, prompts, warnings, and
colors must not be written to stdout in JSON mode.

## init

```bash
agentdoctor init [path] --dry-run [--agents <list>] [--no-interactive]
```

Builds a deterministic setup plan for agent instruction files and
`.agentdoctor.toml`.

v0.1 supports dry-run only. Running `init` without `--dry-run` exits with a
usage error and does not write files.

Examples:

```bash
agentdoctor init --dry-run
agentdoctor init . --dry-run --agents codex,claude,cursor
```

## upgrade

```bash
agentdoctor upgrade [--repo <owner/name>] [--force]
```

Checks the latest GitHub Release, compares it with the current binary version,
downloads the matching platform archive, and replaces the current executable.
Upgrade uses PowerShell for downloads on Windows and `curl` on macOS/Linux.

Options:

- `--repo <owner/name>`: release repository. Defaults to
  `youssefsz/agentdoctor`. The `AGENTDOCTOR_REPO` environment variable can also
  override this.
- `--force`: reinstall from the latest release even when the current version is
  already up to date.

Supported release targets:

- Windows x86_64: `x86_64-pc-windows-gnu.zip`
- Linux x86_64: `x86_64-unknown-linux-gnu.tar.gz`
- Linux ARM64: `aarch64-unknown-linux-gnu.tar.gz`
- macOS x86_64: `x86_64-apple-darwin.tar.gz`
- macOS ARM64: `aarch64-apple-darwin.tar.gz`

Examples:

```bash
agentdoctor upgrade
agentdoctor upgrade --force
agentdoctor upgrade --repo youssefsz/agentdoctor
```

If the current version is already the latest release, AgentDoctor prints that it
is already up to date and does not download or reinstall the binary.

## uninstall

```bash
agentdoctor uninstall [--yes] [--remove-config]
```

Removes the currently running AgentDoctor executable.

Options:

- `--yes`: skip the confirmation prompt. Required with `--no-interactive`.
- `--remove-config`: also remove global AgentDoctor config when it exists.

Examples:

```bash
agentdoctor uninstall
agentdoctor --no-interactive uninstall --yes
agentdoctor uninstall --yes --remove-config
```

Uninstall does not remove PATH entries because install directories such as
`~/.local/bin` and `%USERPROFILE%\.local\bin` may be shared with other tools. On
Windows, executable deletion is completed after the process exits.

## config

```bash
agentdoctor config show [--no-interactive]
agentdoctor config agents [--no-interactive]
agentdoctor config agents --set <list> [--no-interactive]
agentdoctor config reset [--no-interactive]
```

Commands:

- `config show`: print global config as TOML, or report that none exists.
- `config agents`: print selected global agents, or `generic` when unset.
- `config agents --set <list>`: save selected global agents.
- `config reset`: remove the global config file when it exists.

Examples:

```bash
agentdoctor config agents --set codex,claude,cursor
agentdoctor config agents
agentdoctor config show
agentdoctor config reset
```

## Exit Codes

- `0`: success.
- `1`: scan completed, but the score is below the configured minimum.
- `2`: usage error, such as an invalid agent list or `init` without
  `--dry-run`.
- `3`: config or engine error, such as invalid TOML or an invalid scan root.
- `4`: unexpected error.
