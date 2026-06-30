use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn command(config_home: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("agentdoctor").expect("binary");
    command.env("AGENTDOCTOR_CONFIG_HOME", config_home);
    command.env("CI", "true");
    command
}

#[test]
fn scan_json_outputs_valid_json_only() {
    let repo = tempdir().expect("repo");
    let config_home = tempdir().expect("config");
    fs::write(
        repo.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");

    let output = command(config_home.path())
        .args(["scan", "--format", "json"])
        .arg(repo.path())
        .output()
        .expect("run");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    assert!(value["score"]["total"].is_number());
}

#[test]
fn bare_command_falls_back_to_scan_output_in_ci() {
    let repo = tempdir().expect("repo");
    let config_home = tempdir().expect("config");
    fs::write(
        repo.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");

    command(config_home.path())
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("AI Agent Readiness"));
}

#[test]
fn scan_json_does_not_prompt_without_global_config() {
    let repo = tempdir().expect("repo");
    let config_home = tempdir().expect("config");

    command(config_home.path())
        .args(["scan", "--format", "json"])
        .arg(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"findings\""));
}

#[test]
fn init_dry_run_does_not_write_files() {
    let repo = tempdir().expect("repo");
    let config_home = tempdir().expect("config");

    command(config_home.path())
        .args(["init", "--dry-run", "--agents", "codex,claude"])
        .arg(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Create AGENTS.md"))
        .stdout(predicate::str::contains("Create CLAUDE.md"));

    assert!(!repo.path().join("AGENTS.md").exists());
    assert!(!repo.path().join("CLAUDE.md").exists());
}

#[test]
fn config_reset_uses_test_config_directory() {
    let config_home = tempdir().expect("config");

    command(config_home.path())
        .args(["config", "agents", "--set", "codex,claude"])
        .assert()
        .success();

    assert!(config_home.path().join("config.toml").exists());

    command(config_home.path())
        .args(["config", "reset"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Global config reset."));

    assert!(!config_home.path().join("config.toml").exists());
}

#[test]
fn scan_threshold_failure_returns_exit_1_and_keeps_stdout_valid_json() {
    let repo = tempdir().expect("repo");
    let config_home = tempdir().expect("config");
    fs::write(
        repo.path().join("Cargo.toml"),
        "[package]\nname = \"threshold-demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    fs::write(
        repo.path().join(".agentdoctor.toml"),
        "version = 1\n[score]\nminimum = 100\n",
    )
    .expect("write project config");

    let output = command(config_home.path())
        .args(["scan", "--format", "json"])
        .arg(repo.path())
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    assert!(value["score"]["total"].as_u64().expect("numeric score") < 100);
    assert!(String::from_utf8_lossy(&output.stderr).contains("below configured minimum"));
}

#[test]
fn project_config_agents_override_global_agent_config() {
    let repo = tempdir().expect("repo");
    let config_home = tempdir().expect("config");

    command(config_home.path())
        .args(["config", "agents", "--set", "generic"])
        .assert()
        .success();
    fs::write(
        repo.path().join(".agentdoctor.toml"),
        "version = 1\n[agents]\nenabled = [\"claude\"]\n",
    )
    .expect("write project config");

    let output = command(config_home.path())
        .args(["scan", "--format", "json"])
        .arg(repo.path())
        .output()
        .expect("run");

    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    let ids = value["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .map(|finding| finding["id"].as_str().expect("finding id"))
        .collect::<Vec<_>>();
    assert!(ids.contains(&"AD002"));
}

#[test]
fn config_show_outputs_saved_agents_as_toml() {
    let config_home = tempdir().expect("config");

    command(config_home.path())
        .args(["config", "agents", "--set", "codex,claude"])
        .assert()
        .success();

    let output = command(config_home.path())
        .args(["config", "show"])
        .output()
        .expect("run");

    assert!(output.status.success());
    let value: toml::Value =
        toml::from_str(&String::from_utf8(output.stdout).expect("utf8")).expect("valid toml");
    let agents = value["selected_agents"]
        .as_array()
        .expect("selected_agents array")
        .iter()
        .map(|agent| agent.as_str().expect("agent string"))
        .collect::<Vec<_>>();
    assert_eq!(agents, vec!["codex", "claude"]);
}

#[test]
fn invalid_agent_list_returns_usage_exit_code() {
    let repo = tempdir().expect("repo");
    let config_home = tempdir().expect("config");

    command(config_home.path())
        .args(["scan", "--agents", "not-real"])
        .arg(repo.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unknown agent 'not-real'"));
}

#[test]
fn init_without_dry_run_is_rejected_and_does_not_write_files() {
    let repo = tempdir().expect("repo");
    let config_home = tempdir().expect("config");

    command(config_home.path())
        .args(["init", "--agents", "codex"])
        .arg(repo.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "v0.1 supports only `agentdoctor init --dry-run`",
        ));

    assert!(!repo.path().join("AGENTS.md").exists());
    assert!(!repo.path().join(".agentdoctor.toml").exists());
}

#[test]
fn help_lists_lifecycle_commands() {
    let config_home = tempdir().expect("config");

    command(config_home.path())
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("upgrade"))
        .stdout(predicate::str::contains("uninstall"));
}

#[test]
fn uninstall_requires_confirmation_in_non_interactive_mode() {
    let config_home = tempdir().expect("config");

    command(config_home.path())
        .args(["--no-interactive", "uninstall"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "uninstall requires --yes when --no-interactive is set",
        ));
}

#[test]
fn upgrade_help_documents_force_and_repo_options() {
    let config_home = tempdir().expect("config");

    command(config_home.path())
        .args(["upgrade", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--force"))
        .stdout(predicate::str::contains("--repo"));
}
