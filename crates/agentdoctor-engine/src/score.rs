use crate::{Evidence, Score, ScoreCategory, rules, rules::RuleContext};

pub fn calculate_score(ctx: &RuleContext<'_>) -> Score {
    let categories = vec![
        agent_files(ctx),
        project_detail(ctx),
        commands(ctx),
        safety_boundaries(ctx),
        repo_hygiene(ctx),
        automation_ci(ctx),
    ];

    let total = categories.iter().map(|category| category.earned).sum();
    Score { total, categories }
}

fn agent_files(ctx: &RuleContext<'_>) -> ScoreCategory {
    let mut earned = 0;
    let mut evidence = Vec::new();

    let selected_paths = crate::required_agent_paths(ctx.profile);
    let selected_covered = selected_paths
        .iter()
        .all(|(_, path)| ctx.facts.has_any_agent_file(&path.to_string_lossy()));
    if selected_covered {
        earned += 15;
        evidence.push(Evidence::awarded(
            "selected-agent-files",
            15,
            "All selected agent files are present.",
        ));
    } else {
        evidence.push(Evidence::missed(
            "selected-agent-files",
            "One or more selected agent files are missing.",
        ));
    }

    if ctx.facts.has_any_agent_file("AGENTS.md") {
        earned += 5;
        evidence.push(Evidence::awarded(
            "agents-md",
            5,
            "Canonical AGENTS.md exists.",
        ));
    } else {
        evidence.push(Evidence::missed(
            "agents-md",
            "Canonical AGENTS.md is missing.",
        ));
    }

    if has_healthy_agent_customizations(ctx) {
        earned += 5;
        let message = if ctx.facts.customizations.has_any() {
            "Detected agent customizations are structurally healthy."
        } else {
            "No advanced agent customizations detected."
        };
        evidence.push(Evidence::awarded("agent-customizations", 5, message));
    } else {
        evidence.push(Evidence::missed(
            "agent-customizations",
            "One or more agent customization files need attention.",
        ));
    }

    category("Agent files", earned, 25, evidence)
}

fn project_detail(ctx: &RuleContext<'_>) -> ScoreCategory {
    let mut earned = 0;
    let mut evidence = Vec::new();

    award_bool(
        rules::has_project_overview(ctx.facts),
        &mut earned,
        &mut evidence,
        "project-overview",
        5,
        "Project overview exists in agent instructions.",
        "Project overview is missing from agent instructions.",
    );
    award_bool(
        rules::has_stack_detail(ctx.facts),
        &mut earned,
        &mut evidence,
        "stack-detail",
        5,
        "Detected stack is documented in agent instructions.",
        "Detected stack is not documented in agent instructions.",
    );
    award_bool(
        rules::has_project_structure(ctx.facts),
        &mut earned,
        &mut evidence,
        "project-structure",
        5,
        "Project structure is documented.",
        "Project structure is not documented.",
    );
    award_bool(
        rules::has_concrete_agent_content(ctx.facts),
        &mut earned,
        &mut evidence,
        "non-generic-content",
        5,
        "Agent instructions include concrete project context.",
        "Agent instructions appear too generic.",
    );

    category("Project-specific detail", earned, 20, evidence)
}

fn commands(ctx: &RuleContext<'_>) -> ScoreCategory {
    let mut earned = 0;
    let mut evidence = Vec::new();
    let command_facts = &ctx.facts.commands;

    award_option(
        command_facts.install.as_deref(),
        &mut earned,
        &mut evidence,
        "install-command",
        4,
    );
    award_option(
        command_facts.dev.as_deref(),
        &mut earned,
        &mut evidence,
        "dev-command",
        4,
    );
    award_option(
        command_facts.build.as_deref(),
        &mut earned,
        &mut evidence,
        "build-command",
        4,
    );
    award_option(
        command_facts.test.as_deref(),
        &mut earned,
        &mut evidence,
        "test-command",
        4,
    );
    if let Some(command) = command_facts
        .typecheck
        .as_deref()
        .or(command_facts.lint.as_deref())
    {
        earned += 4;
        evidence.push(Evidence::awarded(
            "typecheck-or-lint-command",
            4,
            format!("Detected quality command: {command}."),
        ));
    } else {
        evidence.push(Evidence::missed(
            "typecheck-or-lint-command",
            "No typecheck or lint command detected.",
        ));
    }

    category("Commands", earned, 20, evidence)
}

fn safety_boundaries(ctx: &RuleContext<'_>) -> ScoreCategory {
    let mut earned = 0;
    let mut evidence = Vec::new();
    award_bool(
        rules::has_boundaries_section(ctx.facts),
        &mut earned,
        &mut evidence,
        "boundaries-section",
        5,
        "Boundaries section exists.",
        "Boundaries section is missing.",
    );
    award_bool(
        has_protected_path_reference(ctx),
        &mut earned,
        &mut evidence,
        "protected-paths",
        5,
        "Protected paths or sensitive workflows are documented.",
        "Protected paths are not documented.",
    );
    award_bool(
        rules::has_before_finish_checklist(ctx.facts),
        &mut earned,
        &mut evidence,
        "before-finish-checklist",
        5,
        "Before-finishing checklist exists.",
        "Before-finishing checklist is missing.",
    );
    category("Safety boundaries", earned, 15, evidence)
}

fn repo_hygiene(ctx: &RuleContext<'_>) -> ScoreCategory {
    let mut earned = 0;
    let mut evidence = Vec::new();
    award_bool(
        ctx.facts.has_path("README.md"),
        &mut earned,
        &mut evidence,
        "readme",
        3,
        "README.md exists.",
        "README.md is missing.",
    );
    if !ctx.facts.env.uses_env || ctx.facts.env.has_env_example {
        earned += 4;
        let message = if ctx.facts.env.uses_env {
            ".env.example exists for detected environment usage."
        } else {
            "No environment variable usage detected."
        };
        evidence.push(Evidence::awarded("env-example", 4, message));
    } else {
        evidence.push(Evidence::missed(
            "env-example",
            "Environment variable usage was detected without .env.example.",
        ));
    }
    award_bool(
        has_common_generated_dirs_ignored(ctx),
        &mut earned,
        &mut evidence,
        "generated-dirs",
        3,
        "Common generated/build directories are not present in scan results.",
        "Common generated/build directories appear in scan results.",
    );
    category("Repo hygiene", earned, 10, evidence)
}

fn automation_ci(ctx: &RuleContext<'_>) -> ScoreCategory {
    let mut earned = 0;
    let mut evidence = Vec::new();
    award_bool(
        ctx.facts.ci.has_ci,
        &mut earned,
        &mut evidence,
        "ci-present",
        5,
        "CI workflow detected.",
        "No CI workflow detected.",
    );
    award_bool(
        ctx.facts.ci.has_test || ctx.facts.ci.has_build || ctx.facts.ci.has_lint,
        &mut earned,
        &mut evidence,
        "ci-checks",
        5,
        "CI includes test, build, or lint checks.",
        "CI checks were not detected.",
    );
    category("Automation/CI", earned, 10, evidence)
}

fn award_bool(
    condition: bool,
    earned: &mut u8,
    evidence: &mut Vec<Evidence>,
    subject: &'static str,
    points: u8,
    awarded: &'static str,
    missed: &'static str,
) {
    if condition {
        *earned += points;
        evidence.push(Evidence::awarded(subject, points, awarded));
    } else {
        evidence.push(Evidence::missed(subject, missed));
    }
}

fn award_option(
    command: Option<&str>,
    earned: &mut u8,
    evidence: &mut Vec<Evidence>,
    subject: &'static str,
    points: u8,
) {
    if let Some(command) = command {
        *earned += points;
        evidence.push(Evidence::awarded(
            subject,
            points,
            format!("Detected command: {command}."),
        ));
    } else {
        evidence.push(Evidence::missed(subject, "Command was not detected."));
    }
}

fn category(name: &'static str, earned: u8, max: u8, evidence: Vec<Evidence>) -> ScoreCategory {
    ScoreCategory {
        name: name.to_string(),
        earned: earned.min(max),
        max,
        evidence,
    }
}

fn has_protected_path_reference(ctx: &RuleContext<'_>) -> bool {
    ctx.facts.agent_texts().any(|(_, text)| {
        let lower = text.to_ascii_lowercase();
        [
            "migration",
            ".env",
            "secret",
            "generated",
            "lockfile",
            "target/",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
    })
}

fn has_common_generated_dirs_ignored(ctx: &RuleContext<'_>) -> bool {
    !ctx.facts.files.iter().any(|file| {
        let key = crate::path_key(&file.path);
        key.starts_with("node_modules/")
            || key.starts_with("target/")
            || key.starts_with(".next/")
            || key.starts_with("dist/")
            || key.starts_with("coverage/")
    })
}

fn has_healthy_agent_customizations(ctx: &RuleContext<'_>) -> bool {
    let customizations = &ctx.facts.customizations;
    !customizations.has_invalid_skill_metadata()
        && !customizations.has_shell_preapproval()
        && !customizations.has_secret_like_mcp_config()
        && !customizations.has_local_settings()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{
        AgentFile, AgentKind, AgentProfile, CommandFacts, EnvFacts, FileKind, WorkspaceFacts,
        WorkspaceFile,
    };

    use super::*;

    #[test]
    fn score_includes_evidence_for_categories() {
        let facts = WorkspaceFacts {
            root: PathBuf::from("."),
            files: vec![WorkspaceFile {
                path: PathBuf::from("AGENTS.md"),
                kind: FileKind::Markdown,
                size_bytes: 1,
            }],
            agent_files: vec![AgentFile {
                path: PathBuf::from("AGENTS.md"),
                kind: AgentKind::Generic,
                size_bytes: 1,
                content: Some(
                    "## Project overview\n## Stack\nRust\n## Commands\ncargo test\n## Project structure\n## Boundaries\nDo not edit .env.\n## Before finishing"
                        .to_string(),
                ),
            }],
            customizations: Default::default(),
            package_managers: vec![crate::PackageManager::Cargo],
            frameworks: Vec::new(),
            commands: CommandFacts {
                build: Some("cargo build".to_string()),
                test: Some("cargo test".to_string()),
                lint: Some("cargo clippy".to_string()),
                format: Some("cargo fmt".to_string()),
                ..CommandFacts::default()
            },
            ci: Default::default(),
            env: EnvFacts::default(),
        };
        let profile = AgentProfile::default();
        let score = calculate_score(&RuleContext {
            facts: &facts,
            profile: &profile,
            project_config: None,
        });

        assert!(score.total > 50);
        assert!(
            score
                .categories
                .iter()
                .all(|category| !category.evidence.is_empty())
        );
    }
}
