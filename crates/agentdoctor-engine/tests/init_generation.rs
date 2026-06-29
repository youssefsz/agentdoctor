use std::path::{Path, PathBuf};

use agentdoctor_engine::{
    AgentKind, AgentProfile, InitInput, PlannedChange, ScanOptions, generate_init_plan,
};
use pretty_assertions::assert_eq;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should exist")
}

fn fixture(name: &str) -> PathBuf {
    repo_root().join("fixtures").join(name)
}

fn init_plan(name: &str, agents: Vec<AgentKind>) -> Vec<PlannedChange> {
    generate_init_plan(InitInput {
        root: fixture(name),
        profile: AgentProfile::new(agents),
        project_config: None,
        options: ScanOptions::default(),
    })
    .expect("init plan should generate")
    .changes
}

fn created_content<'a>(changes: &'a [PlannedChange], expected_path: &str) -> &'a str {
    changes
        .iter()
        .find_map(|change| match change {
            PlannedChange::CreateFile { path, content }
                if path == &PathBuf::from(expected_path) =>
            {
                Some(content.as_str())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing created file {expected_path}"))
}

fn skipped_reason<'a>(changes: &'a [PlannedChange], expected_path: &str) -> &'a str {
    changes
        .iter()
        .find_map(|change| match change {
            PlannedChange::SkipExisting { path, reason }
                if path == &PathBuf::from(expected_path) =>
            {
                Some(reason.as_str())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing skipped file {expected_path}"))
}

#[test]
fn empty_repo_generates_exact_canonical_agents_md_and_adapters() {
    let changes = init_plan(
        "empty-repo",
        vec![AgentKind::Codex, AgentKind::Claude, AgentKind::Cursor],
    );

    assert_eq!(
        created_content(&changes, "AGENTS.md"),
        "# Agent Instructions\n\n## Project overview\nThis repository has not exposed enough files for AgentDoctor to identify a primary stack.\n\n## Stack\n- Stack not detected from repository files.\n\n## Commands\n- No commands were detected. Add install, build, test, and lint commands when known.\n\n## Project structure\n- Root files contain the primary project configuration.\n\n## Boundaries\n- Do not edit secrets or local environment files such as `.env`.\n- Do not modify generated or build output directories unless explicitly requested.\n\n## Code style\n- Follow the style already present in the touched files.\n- Keep changes scoped to the requested task.\n\n## Before finishing\n- Document the verification performed and any checks that could not be run.\n"
    );
    assert_eq!(
        created_content(&changes, "CLAUDE.md"),
        "# Claude Instructions\n\nRead `AGENTS.md` in this repository before making code changes.\nFollow the repository-specific instructions there.\n"
    );
    assert_eq!(
        created_content(&changes, ".cursor/rules/project.mdc"),
        "---\ndescription: AgentDoctor generated project rule\nglobs: [\"**/*\"]\nalwaysApply: true\n---\n\nRead `AGENTS.md` in this repository before making code changes.\nFollow the repository-specific instructions there.\n"
    );
}

#[test]
fn empty_repo_generates_exact_project_config_for_selected_agents() {
    let changes = init_plan(
        "empty-repo",
        vec![AgentKind::Codex, AgentKind::Claude, AgentKind::Cursor],
    );

    assert_eq!(
        created_content(&changes, ".agentdoctor.toml"),
        "version = 1\n\n[agents]\nenabled = [\"codex\", \"claude\", \"cursor\"]\n\n[score]\nminimum = 80\n\n[commands]\n\n[paths]\nignore = [\"node_modules\", \"target\", \"dist\", \".next\", \"coverage\"]\n\n[rules]\nrequire_agents_md = true\nrequire_test_command = true\nrequire_build_command = true\nrequire_boundaries = true\nrequire_env_example = true\ndetect_generic_instructions = true\ndetect_dangerous_instructions = true\n"
    );
}

#[test]
fn rust_cli_init_plan_preserves_existing_agents_md_and_generates_detected_commands() {
    let changes = init_plan(
        "rust-cli",
        vec![AgentKind::Codex, AgentKind::Claude, AgentKind::Cursor],
    );

    assert_eq!(
        skipped_reason(&changes, "AGENTS.md"),
        "AGENTS.md already exists."
    );
    assert_eq!(
        created_content(&changes, ".agentdoctor.toml"),
        "version = 1\n\n[agents]\nenabled = [\"codex\", \"claude\", \"cursor\"]\n\n[score]\nminimum = 80\n\n[commands]\nbuild = \"cargo build\"\ntest = \"cargo test\"\nlint = \"cargo clippy\"\nformat = \"cargo fmt\"\n\n[paths]\nignore = [\"node_modules\", \"target\", \"dist\", \".next\", \"coverage\"]\n\n[rules]\nrequire_agents_md = true\nrequire_test_command = true\nrequire_build_command = true\nrequire_boundaries = true\nrequire_env_example = true\ndetect_generic_instructions = true\ndetect_dangerous_instructions = true\n"
    );
}
