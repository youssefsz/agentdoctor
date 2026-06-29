<div align="center">

# AgentDoctor

**Audit and prepare repositories for AI coding agents.**

[![CI](https://img.shields.io/github/actions/workflow/status/youssefsz/agentdoctor/ci.yml?branch=main&label=ci&style=for-the-badge)](https://github.com/youssefsz/agentdoctor/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/youssefsz/agentdoctor?style=for-the-badge)](https://github.com/youssefsz/agentdoctor/releases)
[![License](https://img.shields.io/github/license/youssefsz/agentdoctor?style=for-the-badge)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=for-the-badge)](#installation)

Created by [Youssef Dhibi](https://youssef.tn).

</div>

## Overview

AgentDoctor is a native Rust CLI that audits whether a repository is ready for
AI coding agents such as Codex, Claude Code, Cursor, GitHub Copilot, Gemini CLI,
and generic tools that read `AGENTS.md`.

It detects missing or weak agent instructions, project stack evidence,
development commands, CI coverage, environment-file hygiene, and risky
instructions that tell agents to bypass verification. It also validates modern
agent customization surfaces such as skills, prompts, custom agents, hooks, and
MCP configuration when those files are present.

AgentDoctor does **not** call AI APIs, send telemetry, or access the network at
runtime.

## Features

- Scans repositories without requiring project-specific setup.
- Detects agent instruction files:
  - `AGENTS.md`
  - `CLAUDE.md`
  - `GEMINI.md`
  - `.cursor/rules/project.mdc`
  - `.github/copilot-instructions.md`
- Detects agent customization files:
  - `.agents/skills/*/SKILL.md`
  - `.claude/skills/*/SKILL.md`
  - `.github/skills/*/SKILL.md`
  - `.cursor/skills/*/SKILL.md`
  - `.github/instructions/*.instructions.md`
  - `.github/prompts/*.prompt.md`
  - `.github/agents/**/*.md`
  - `.claude/commands/**/*.md`
  - `.claude/agents/**/*.md`
  - `.codex/agents/*.toml`
  - `.mcp.json`, `.vscode/mcp.json`, `.cursor/mcp.json`, `.codex/config.toml`
- Reports invalid skill metadata, duplicate skill names, broad shell
  pre-approval in skills, committed local agent settings, and secret-like
  literals in MCP config without printing secret values.
- Detects common stacks and commands for Rust, JavaScript/TypeScript, Python,
  Go, Docker, CI, and database tooling.
- Produces human-readable and machine-readable reports.
- Scores AI-agent readiness out of 100 with evidence.
- Generates deterministic repo setup plans with `init --dry-run`.
- Keeps scan logic in a reusable Rust engine, separate from CLI output.

## Installation

### Windows

Run from PowerShell:

```powershell
irm https://raw.githubusercontent.com/youssefsz/agentdoctor/main/install.ps1 | iex
```

Or from a local checkout:

```powershell
.\scripts\install-local.ps1
```

### macOS and Linux

Run:

```bash
curl -fsSL https://raw.githubusercontent.com/youssefsz/agentdoctor/main/install.sh | sh
```

The Unix installer shows colored step progress in interactive terminals,
installs to a user-writable directory by default, updates shell profiles when
needed, and verifies the installed binary. It does not use `sudo` unless you
explicitly request a system install:

```bash
curl -fsSL https://raw.githubusercontent.com/youssefsz/agentdoctor/main/install.sh | AGENTDOCTOR_INSTALL_MODE=system sh
```

You can also choose a custom user-writable directory:

```bash
curl -fsSL https://raw.githubusercontent.com/youssefsz/agentdoctor/main/install.sh | AGENTDOCTOR_INSTALL_DIR="$HOME/.local/bin" sh
```

### From Source

```bash
cargo install --path crates/agentdoctor-cli --locked --force
```

After installation:

```bash
agentdoctor --version
agentdoctor scan --no-interactive
```

## Quick Start

Scan the current repository:

```bash
agentdoctor scan
```

Emit JSON only:

```bash
agentdoctor scan --format json --no-interactive
```

Preview generated setup files:

```bash
agentdoctor init --dry-run --agents codex,claude,cursor
```

Configure your default agents:

```bash
agentdoctor config agents --set codex,claude,cursor
agentdoctor config show
```

## Documentation

- [CLI reference](docs/cli.md): every command, flag, example, and exit code.
- [Scoring and findings](docs/scoring.md): score categories, evidence, and
  `AD001`-`AD015`.
- [Configuration](docs/config.md): global config, `.agentdoctor.toml`, and
  precedence.

## Example Output

```text
AgentDoctor 0.1.4

Detected: Cargo

AI Agent Readiness: 79/100

Info
  i AD009 Missing README
  i AD010 Missing CI

Score breakdown
  - Agent files: 25/25
  - Project-specific detail: 20/20
  - Commands: 12/20
  - Safety boundaries: 15/15
  - Repo hygiene: 7/10
  - Automation/CI: 0/10
```

## Commands

```bash
agentdoctor [--no-interactive] <command>
agentdoctor --help
agentdoctor --version

agentdoctor scan [path] [--format pretty|json] [--agents codex,claude,cursor] [--no-progress] [--no-interactive]
agentdoctor init [path] --dry-run [--agents codex,claude,cursor] [--no-interactive]
agentdoctor upgrade [--repo owner/name] [--force]
agentdoctor uninstall [--yes] [--remove-config]
agentdoctor config show [--no-interactive]
agentdoctor config agents [--no-interactive]
agentdoctor config agents --set codex,claude [--no-interactive]
agentdoctor config reset [--no-interactive]
```

See the [CLI reference](docs/cli.md) for full command behavior and exit codes.

## Finding IDs

AgentDoctor uses stable finding IDs so reports can be tracked in CI and tests.
See [Scoring and findings](docs/scoring.md) for severity, score evidence, and
rule details.

| ID | Meaning |
| --- | --- |
| `AD001` | Missing canonical `AGENTS.md` |
| `AD002` | Missing selected agent adapter file |
| `AD003` | Missing test command |
| `AD004` | Missing build command |
| `AD005` | Missing agent boundaries section |
| `AD006` | Agent instructions are too generic |
| `AD007` | Dangerous agent instruction detected |
| `AD008` | Missing `.env.example` for detected environment usage |
| `AD009` | Missing README |
| `AD010` | Missing CI |
| `AD011` | Invalid skill metadata |
| `AD012` | Duplicate skill name |
| `AD013` | Skill pre-approves broad shell tools |
| `AD014` | Local-only agent settings are committed |
| `AD015` | MCP config may contain a secret-like literal |

## Configuration

AgentDoctor uses two configuration layers:

- Global user config, managed with `agentdoctor config`.
- Project config, stored in `.agentdoctor.toml`.

Precedence:

```text
CLI flags > .agentdoctor.toml > global config > safe defaults
```

Tests and CI can isolate global config with `AGENTDOCTOR_CONFIG_HOME`.

## Development

Requirements:

- Rust 2024-capable stable toolchain.
- On Windows, either Visual Studio Build Tools for MSVC or the GNU toolchain with
  MinGW.

Run the full check suite:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

Project layout:

```text
crates/
  agentdoctor-engine/  reusable scan, rule, score, and generation engine
  agentdoctor-config/  global and project configuration
  agentdoctor-report/  pretty and JSON report rendering
  agentdoctor-cli/     command-line interface
fixtures/              regression fixtures
scripts/               local development helpers
```

## Release Builds

Tagged releases build archives for:

- Windows x86_64 GNU
- Linux x86_64 GNU
- Linux ARM64 GNU
- macOS x86_64
- macOS ARM64

Create a release by pushing a tag:

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

The release workflow uploads platform archives and checksums to GitHub Releases.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md)
before opening an issue or pull request.

## Security

Please report security issues through the process in [SECURITY.md](SECURITY.md).

## License

AgentDoctor is released under the [MIT License](LICENSE).
