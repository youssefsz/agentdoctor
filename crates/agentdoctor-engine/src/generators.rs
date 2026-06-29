use std::path::PathBuf;

use crate::{
    AgentKind, CommandFacts, EngineError, FileKind, InitInput, InitPlan, PlannedChange,
    ProjectConfig, ScanInput, WorkspaceFacts, required_agent_paths, scan_workspace,
};

pub fn generate_init_plan(input: InitInput) -> Result<InitPlan, EngineError> {
    let report = scan_workspace(ScanInput {
        root: input.root.clone(),
        profile: input.profile.clone(),
        project_config: input.project_config.clone(),
        options: input.options.clone(),
    })?;
    let facts = &report.facts;
    let mut changes = Vec::new();

    push_agents_md(facts, &mut changes);
    push_agent_adapters(facts, &input, &mut changes);
    push_project_config(facts, input.project_config.as_ref(), &input, &mut changes);

    Ok(InitPlan { changes })
}

fn push_agents_md(facts: &WorkspaceFacts, changes: &mut Vec<PlannedChange>) {
    let path = PathBuf::from("AGENTS.md");
    if facts.has_path("AGENTS.md") {
        changes.push(PlannedChange::SkipExisting {
            path,
            reason: "AGENTS.md already exists.".to_string(),
        });
        return;
    }
    changes.push(PlannedChange::CreateFile {
        path,
        content: generate_agents_md(facts),
    });
}

fn push_agent_adapters(
    facts: &WorkspaceFacts,
    input: &InitInput,
    changes: &mut Vec<PlannedChange>,
) {
    for (agent, path) in required_agent_paths(&input.profile) {
        if agent == AgentKind::Codex
            || agent == AgentKind::Generic
            || crate::path_key(&path) == "agents.md"
        {
            continue;
        }
        let path_text = path.to_string_lossy();
        if facts.has_path(&path_text) {
            changes.push(PlannedChange::SkipExisting {
                path,
                reason: "Selected agent file already exists.".to_string(),
            });
        } else {
            changes.push(PlannedChange::CreateFile {
                path: path.clone(),
                content: generate_adapter(agent),
            });
        }
    }
}

fn push_project_config(
    facts: &WorkspaceFacts,
    existing: Option<&ProjectConfig>,
    input: &InitInput,
    changes: &mut Vec<PlannedChange>,
) {
    let path = PathBuf::from(".agentdoctor.toml");
    if existing.is_some() || facts.has_path(".agentdoctor.toml") {
        changes.push(PlannedChange::SkipExisting {
            path,
            reason: ".agentdoctor.toml already exists.".to_string(),
        });
        return;
    }

    changes.push(PlannedChange::CreateFile {
        path,
        content: generate_project_config(facts, input),
    });
}

fn generate_agents_md(facts: &WorkspaceFacts) -> String {
    let stack = facts.detected_stack_labels();
    let commands = facts.commands.all_detected();
    let structure = describe_structure(facts);

    let mut content = String::new();
    content.push_str("# Agent Instructions\n\n");
    content.push_str("## Project overview\n");
    if stack.is_empty() {
        content.push_str("This repository has not exposed enough files for AgentDoctor to identify a primary stack.\n\n");
    } else {
        content.push_str("This repository appears to use ");
        content.push_str(&stack.join(", "));
        content.push_str(".\n\n");
    }

    content.push_str("## Stack\n");
    if stack.is_empty() {
        content.push_str("- Stack not detected from repository files.\n\n");
    } else {
        for label in &stack {
            content.push_str("- ");
            content.push_str(label);
            content.push('\n');
        }
        content.push('\n');
    }

    content.push_str("## Commands\n");
    if commands.is_empty() {
        content.push_str("- No commands were detected. Add install, build, test, and lint commands when known.\n\n");
    } else {
        for (name, command) in commands {
            content.push_str("- ");
            content.push_str(name);
            content.push_str(": `");
            content.push_str(command);
            content.push_str("`\n");
        }
        content.push('\n');
    }

    content.push_str("## Project structure\n");
    if structure.is_empty() {
        content.push_str("- Root files contain the primary project configuration.\n\n");
    } else {
        for item in structure {
            content.push_str("- ");
            content.push_str(&item);
            content.push('\n');
        }
        content.push('\n');
    }

    content.push_str("## Boundaries\n");
    content.push_str("- Do not edit secrets or local environment files such as `.env`.\n");
    content.push_str(
        "- Do not modify generated or build output directories unless explicitly requested.\n",
    );
    if facts
        .files
        .iter()
        .any(|file| crate::path_key(&file.path).contains("migration"))
    {
        content.push_str("- Treat database migrations as append-only unless the user explicitly asks otherwise.\n");
    }
    content.push('\n');

    content.push_str("## Code style\n");
    content.push_str("- Follow the style already present in the touched files.\n");
    content.push_str("- Keep changes scoped to the requested task.\n\n");

    content.push_str("## Before finishing\n");
    if let Some(command) = facts.commands.format.as_deref() {
        content.push_str("- Run `");
        content.push_str(command);
        content.push_str("` when formatting is needed.\n");
    }
    if let Some(command) = facts
        .commands
        .lint
        .as_deref()
        .or(facts.commands.typecheck.as_deref())
    {
        content.push_str("- Run `");
        content.push_str(command);
        content.push_str("` when code changes affect checked files.\n");
    }
    if let Some(command) = facts.commands.test.as_deref() {
        content.push_str("- Run `");
        content.push_str(command);
        content.push_str("` when behavior changes.\n");
    }
    if facts.commands.test.is_none()
        && facts.commands.lint.is_none()
        && facts.commands.typecheck.is_none()
    {
        content.push_str(
            "- Document the verification performed and any checks that could not be run.\n",
        );
    }

    content
}

fn generate_adapter(agent: AgentKind) -> String {
    match agent {
        AgentKind::Claude => "# Claude Instructions\n\nRead `AGENTS.md` in this repository before making code changes.\nFollow the repository-specific instructions there.\n".to_string(),
        AgentKind::Gemini => "# Gemini Instructions\n\nRead `AGENTS.md` in this repository before making code changes.\nFollow the repository-specific instructions there.\n".to_string(),
        AgentKind::Copilot => "# GitHub Copilot Instructions\n\nRead `AGENTS.md` in this repository before making code suggestions.\nFollow the repository-specific instructions there.\n".to_string(),
        AgentKind::Cursor => "---\ndescription: AgentDoctor generated project rule\nglobs: [\"**/*\"]\nalwaysApply: true\n---\n\nRead `AGENTS.md` in this repository before making code changes.\nFollow the repository-specific instructions there.\n".to_string(),
        AgentKind::Codex | AgentKind::Generic => String::new(),
    }
}

fn generate_project_config(facts: &WorkspaceFacts, input: &InitInput) -> String {
    let commands = &facts.commands;
    let agents = input
        .profile
        .selected_agents
        .iter()
        .map(|agent| format!("\"{}\"", agent.as_str()))
        .collect::<Vec<_>>()
        .join(", ");

    let mut content = String::new();
    content.push_str("version = 1\n\n");
    content.push_str("[agents]\n");
    content.push_str("enabled = [");
    content.push_str(&agents);
    content.push_str("]\n\n");
    content.push_str("[score]\nminimum = 80\n\n");
    content.push_str("[commands]\n");
    write_command_lines(commands, &mut content);
    content.push_str(
        "\n[paths]\nignore = [\"node_modules\", \"target\", \"dist\", \".next\", \"coverage\"]\n\n",
    );
    content.push_str("[rules]\n");
    content.push_str("require_agents_md = true\n");
    content.push_str("require_test_command = true\n");
    content.push_str("require_build_command = true\n");
    content.push_str("require_boundaries = true\n");
    content.push_str("require_env_example = true\n");
    content.push_str("detect_generic_instructions = true\n");
    content.push_str("detect_dangerous_instructions = true\n");
    content
}

fn write_command_lines(commands: &CommandFacts, content: &mut String) {
    for (name, command) in commands.all_detected() {
        content.push_str(name);
        content.push_str(" = \"");
        content.push_str(&command.replace('\\', "\\\\").replace('"', "\\\""));
        content.push_str("\"\n");
    }
}

fn describe_structure(facts: &WorkspaceFacts) -> Vec<String> {
    let mut items = Vec::new();
    let has_crates = facts
        .files
        .iter()
        .any(|file| crate::path_key(&file.path).starts_with("crates/"));
    let has_src = facts
        .files
        .iter()
        .any(|file| crate::path_key(&file.path).starts_with("src/"));
    let has_tests = facts.files.iter().any(|file| {
        let key = crate::path_key(&file.path);
        key.starts_with("tests/") || key.contains("/tests/")
    });
    let has_fixtures = facts
        .files
        .iter()
        .any(|file| crate::path_key(&file.path).starts_with("fixtures/"));
    if has_crates {
        items.push("`crates/` contains Rust workspace crates.".to_string());
    }
    if has_src {
        items.push("`src/` contains application or library source code.".to_string());
    }
    if has_tests {
        items.push("`tests/` contains integration or scenario tests.".to_string());
    }
    if has_fixtures {
        items.push("`fixtures/` contains sample repositories or test data.".to_string());
    }
    if facts.files.iter().any(|file| file.kind == FileKind::Json) {
        items.push("JSON files define JavaScript or tool configuration.".to_string());
    }
    items
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::{AgentKind, AgentProfile, InitInput, ScanOptions};

    use super::*;

    #[test]
    fn init_plan_generates_deterministic_agents_md() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .expect("write cargo");

        let input = InitInput {
            root: dir.path().to_path_buf(),
            profile: AgentProfile::default(),
            project_config: None,
            options: ScanOptions::default(),
        };
        let first = generate_init_plan(input.clone()).expect("first");
        let second = generate_init_plan(input).expect("second");

        assert_eq!(
            format!("{:?}", first.changes),
            format!("{:?}", second.changes)
        );
    }

    #[test]
    fn init_plan_creates_selected_adapters() {
        let dir = tempdir().expect("tempdir");
        let input = InitInput {
            root: dir.path().to_path_buf(),
            profile: AgentProfile::new(vec![AgentKind::Claude, AgentKind::Cursor]),
            project_config: None,
            options: ScanOptions::default(),
        };
        let plan = generate_init_plan(input).expect("plan");

        assert!(plan.changes.iter().any(|change| matches!(
            change,
            PlannedChange::CreateFile { path, .. } if path == &PathBuf::from("CLAUDE.md")
        )));
        assert!(plan.changes.iter().any(|change| matches!(
            change,
            PlannedChange::CreateFile { path, .. } if path == &PathBuf::from(".cursor/rules/project.mdc")
        )));
    }
}
