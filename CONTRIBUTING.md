# Contributing

Thanks for considering a contribution to AgentDoctor.

AgentDoctor is intentionally conservative: the reusable engine owns scanning,
rules, scoring, and generation, while the CLI owns command parsing and terminal
behavior. Please keep that boundary intact.

## Development Setup

```bash
git clone https://github.com/youssefsz/agentdoctor.git
cd agentdoctor
cargo test --workspace
```

On Windows, use either Visual Studio Build Tools for the MSVC toolchain or the
GNU Rust toolchain with MinGW.

## Required Checks

Run these before opening a pull request:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

## Test Expectations

Behavior changes should include tests at the right level:

- Rule, scoring, and detection behavior: engine tests.
- Generated files: deterministic generation tests.
- Report output: report integration tests.
- CLI behavior, exit codes, and config precedence: CLI integration tests.

The fixture repositories in `fixtures/` are regression inputs. Update the
expected assertions when behavior intentionally changes.

## Pull Request Guidelines

- Keep changes focused.
- Avoid unrelated refactors.
- Do not add telemetry, network access, AI APIs, or large parser frameworks
  without prior discussion.
- Update README or docs when command behavior changes.
- Explain any check you could not run.

## Release Process

Releases are tag-driven:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The release workflow builds platform archives and publishes them to GitHub
Releases.
