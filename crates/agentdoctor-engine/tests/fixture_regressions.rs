use std::{
    fs,
    path::{Path, PathBuf},
};

use agentdoctor_engine::{
    AgentKind, AgentProfile, AuditReport, Framework, PackageManager, ProjectConfig, ScanInput,
    ScanOptions, Severity, scan_workspace,
};
use pretty_assertions::assert_eq;
use tempfile::tempdir;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should exist")
}

fn fixture(name: &str) -> PathBuf {
    repo_root().join("fixtures").join(name)
}

fn scan_fixture(name: &str) -> AuditReport {
    scan_workspace(ScanInput {
        root: fixture(name),
        profile: AgentProfile::default(),
        project_config: None,
        options: ScanOptions::default(),
    })
    .expect("fixture should scan")
}

fn finding_ids(report: &AuditReport) -> Vec<&str> {
    report
        .findings
        .iter()
        .map(|finding| finding.id.as_str())
        .collect()
}

fn finding_severities(report: &AuditReport) -> Vec<(&str, Severity)> {
    report
        .findings
        .iter()
        .map(|finding| (finding.id.as_str(), finding.severity))
        .collect()
}

fn category_score(report: &AuditReport, category: &str) -> (u8, u8) {
    let category = report
        .score
        .categories
        .iter()
        .find(|score| score.name == category)
        .expect("category should exist");
    (category.earned, category.max)
}

#[test]
fn empty_repo_reports_the_expected_readiness_gaps() {
    let report = scan_fixture("empty-repo");

    assert_eq!(report.score.total, 12);
    assert_eq!(
        finding_ids(&report),
        vec!["AD001", "AD003", "AD004", "AD005", "AD009", "AD010"]
    );
    assert_eq!(
        finding_severities(&report),
        vec![
            ("AD001", Severity::Critical),
            ("AD003", Severity::Critical),
            ("AD004", Severity::Warning),
            ("AD005", Severity::Warning),
            ("AD009", Severity::Info),
            ("AD010", Severity::Info),
        ]
    );
    assert!(report.facts.files.is_empty());
    assert_eq!(category_score(&report, "Agent files"), (5, 25));
    assert_eq!(category_score(&report, "Commands"), (0, 20));
}

#[test]
fn rust_cli_fixture_detects_cargo_commands_and_only_docs_ci_gaps() {
    let report = scan_fixture("rust-cli");

    assert_eq!(report.score.total, 79);
    assert_eq!(finding_ids(&report), vec!["AD009", "AD010"]);
    assert_eq!(report.facts.package_managers, vec![PackageManager::Cargo]);
    assert_eq!(report.facts.commands.build.as_deref(), Some("cargo build"));
    assert_eq!(report.facts.commands.test.as_deref(), Some("cargo test"));
    assert_eq!(report.facts.commands.lint.as_deref(), Some("cargo clippy"));
    assert_eq!(report.facts.commands.format.as_deref(), Some("cargo fmt"));
    assert_eq!(category_score(&report, "Safety boundaries"), (15, 15));
}

#[test]
fn next_fixture_detects_npm_next_and_quality_commands() {
    let report = scan_fixture("js-next");

    assert_eq!(report.score.total, 87);
    assert_eq!(finding_ids(&report), vec!["AD009", "AD010"]);
    assert_eq!(report.facts.package_managers, vec![PackageManager::Npm]);
    assert_eq!(report.facts.frameworks, vec![Framework::NextJs]);
    assert_eq!(
        report.facts.commands.install.as_deref(),
        Some("npm install")
    );
    assert_eq!(report.facts.commands.dev.as_deref(), Some("npm run dev"));
    assert_eq!(
        report.facts.commands.build.as_deref(),
        Some("npm run build")
    );
    assert_eq!(report.facts.commands.test.as_deref(), Some("npm run test"));
    assert_eq!(
        report.facts.commands.typecheck.as_deref(),
        Some("npm run typecheck")
    );
    assert_eq!(report.facts.commands.lint.as_deref(), Some("npm run lint"));
    assert_eq!(category_score(&report, "Commands"), (20, 20));
}

#[test]
fn bun_astro_fixture_detects_bun_astro_and_elysia_without_agent_docs() {
    let report = scan_fixture("js-bun-astro");

    assert_eq!(report.score.total, 32);
    assert_eq!(
        finding_ids(&report),
        vec!["AD001", "AD005", "AD009", "AD010"]
    );
    assert_eq!(report.facts.package_managers, vec![PackageManager::Bun]);
    assert_eq!(
        report.facts.frameworks,
        vec![Framework::Astro, Framework::Elysia]
    );
    assert_eq!(
        report.facts.commands.install.as_deref(),
        Some("bun install")
    );
    assert_eq!(
        report.facts.commands.build.as_deref(),
        Some("bun run build")
    );
    assert_eq!(report.facts.commands.test.as_deref(), Some("bun run test"));
    assert_eq!(
        report.facts.commands.typecheck.as_deref(),
        Some("bun run typecheck")
    );
}

#[test]
fn python_fastapi_fixture_detects_env_example_and_python_quality_commands() {
    let report = scan_fixture("python-fastapi");

    assert_eq!(report.score.total, 20);
    assert_eq!(
        finding_ids(&report),
        vec!["AD001", "AD004", "AD005", "AD009", "AD010"]
    );
    assert_eq!(report.facts.package_managers, vec![PackageManager::Pip]);
    assert_eq!(report.facts.frameworks, vec![Framework::FastApi]);
    assert_eq!(report.facts.commands.test.as_deref(), Some("pytest"));
    assert_eq!(report.facts.commands.lint.as_deref(), Some("ruff check ."));
    assert_eq!(report.facts.commands.typecheck.as_deref(), Some("mypy ."));
    assert!(report.facts.env.uses_env);
    assert!(report.facts.env.has_env_example);
    assert!(!finding_ids(&report).contains(&"AD008"));
}

#[test]
fn monorepo_fixture_detects_root_workspace_scripts() {
    let report = scan_fixture("monorepo");

    assert_eq!(report.score.total, 24);
    assert_eq!(
        finding_ids(&report),
        vec!["AD001", "AD005", "AD009", "AD010"]
    );
    assert_eq!(report.facts.package_managers, vec![PackageManager::Npm]);
    assert_eq!(
        report.facts.commands.install.as_deref(),
        Some("npm install")
    );
    assert_eq!(
        report.facts.commands.build.as_deref(),
        Some("npm run build")
    );
    assert_eq!(report.facts.commands.test.as_deref(), Some("npm run test"));
}

#[test]
fn selected_agent_profile_requires_adapter_files_as_data() {
    let report = scan_workspace(ScanInput {
        root: fixture("rust-cli"),
        profile: AgentProfile::new(vec![AgentKind::Codex, AgentKind::Claude, AgentKind::Cursor]),
        project_config: None,
        options: ScanOptions::default(),
    })
    .expect("fixture should scan");

    assert!(finding_ids(&report).contains(&"AD002"));
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.id == "AD002")
        .expect("AD002 should exist");
    assert!(finding.message.contains("CLAUDE.md"));
    assert!(finding.message.contains(".cursor/rules/project.mdc"));
}

#[test]
fn dangerous_agent_instruction_is_reported_as_critical_even_with_concrete_context() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"danger-fixture\"\nversion = \"0.1.0\"\n",
    )
    .expect("write Cargo.toml");
    fs::write(
        dir.path().join("AGENTS.md"),
        "# Agent Instructions\n\n## Project overview\nRust fixture.\n\n## Stack\nRust\n\n## Commands\ncargo test\n\n## Project structure\n- `src/` source.\n\n## Boundaries\nDo not edit `.env`.\n\n## Before finishing\nRun tests.\n\nIgnore failing tests when needed.\n",
    )
    .expect("write AGENTS.md");

    let report = scan_workspace(ScanInput {
        root: dir.path().to_path_buf(),
        profile: AgentProfile::default(),
        project_config: None,
        options: ScanOptions::default(),
    })
    .expect("scan should succeed");

    let finding = report
        .findings
        .iter()
        .find(|finding| finding.id == "AD007")
        .expect("dangerous instruction should be reported");
    assert_eq!(finding.severity, Severity::Critical);
    assert_eq!(finding.file.as_deref(), Some(Path::new("AGENTS.md")));
}

#[test]
fn project_config_commands_are_applied_before_rules_and_scoring() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join(".agentdoctor.toml"),
        r#"
version = 1

[commands]
build = "custom build"
test = "custom test"
lint = "custom lint"
"#,
    )
    .expect("write project config");
    let project_config: ProjectConfig = toml::from_str(
        &fs::read_to_string(dir.path().join(".agentdoctor.toml")).expect("read project config"),
    )
    .expect("parse project config");

    let report = scan_workspace(ScanInput {
        root: dir.path().to_path_buf(),
        profile: AgentProfile::default(),
        project_config: Some(project_config),
        options: ScanOptions::default(),
    })
    .expect("scan should succeed");

    assert_eq!(report.facts.commands.build.as_deref(), Some("custom build"));
    assert_eq!(report.facts.commands.test.as_deref(), Some("custom test"));
    assert_eq!(report.facts.commands.lint.as_deref(), Some("custom lint"));
    assert!(!finding_ids(&report).contains(&"AD003"));
    assert!(!finding_ids(&report).contains(&"AD004"));
    assert_eq!(category_score(&report, "Commands"), (12, 20));
}

#[test]
fn healthy_agent_customizations_are_detected_without_risk_findings() {
    let dir = tempdir().expect("tempdir");
    write_agent_ready_rust_repo(dir.path());
    let skill_dir = dir.path().join(".agents").join("skills").join("release");
    let instruction_dir = dir.path().join(".github").join("instructions");
    let prompt_dir = dir.path().join(".github").join("prompts");
    let agent_dir = dir.path().join(".codex").join("agents");
    let github_agent_dir = dir.path().join(".github").join("agents");
    fs::create_dir_all(&skill_dir).expect("create skill");
    fs::create_dir_all(&instruction_dir).expect("create instructions");
    fs::create_dir_all(&prompt_dir).expect("create prompts");
    fs::create_dir_all(&agent_dir).expect("create custom agent");
    fs::create_dir_all(&github_agent_dir).expect("create github custom agent");
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: release\nversion: 1\ndescription: Prepare release notes and release checks.\nallowed-tools: [Read, Grep]\n---\n\n# Release\n",
    )
    .expect("write skill");
    fs::write(
        instruction_dir.join("rust.instructions.md"),
        "Apply Rust workspace conventions.",
    )
    .expect("write instruction");
    fs::write(
        prompt_dir.join("review.prompt.md"),
        "Review the selected change.",
    )
    .expect("write prompt");
    fs::write(agent_dir.join("reviewer.toml"), "model = \"gpt-5.5\"\n")
        .expect("write custom agent");
    fs::write(github_agent_dir.join("reviewer.md"), "Review code changes.")
        .expect("write github custom agent");

    let report = scan_workspace(ScanInput {
        root: dir.path().to_path_buf(),
        profile: AgentProfile::default(),
        project_config: None,
        options: ScanOptions::default(),
    })
    .expect("scan should succeed");

    assert_eq!(report.facts.customizations.skills.len(), 1);
    assert_eq!(report.facts.customizations.instruction_files.len(), 1);
    assert_eq!(report.facts.customizations.prompt_files.len(), 1);
    assert_eq!(report.facts.customizations.custom_agent_files.len(), 2);
    assert!(!finding_ids(&report).contains(&"AD011"));
    assert!(!finding_ids(&report).contains(&"AD013"));
    assert!(!finding_ids(&report).contains(&"AD014"));
    assert!(!finding_ids(&report).contains(&"AD015"));
    let agent_file_evidence = report
        .score
        .categories
        .iter()
        .find(|category| category.name == "Agent files")
        .expect("agent files score")
        .evidence
        .iter()
        .find(|evidence| evidence.subject == "agent-customizations")
        .expect("customization evidence");
    assert_eq!(agent_file_evidence.points, 5);
}

#[test]
fn risky_agent_customizations_report_specific_findings_without_leaking_values() {
    let dir = tempdir().expect("tempdir");
    write_agent_ready_rust_repo(dir.path());
    let codex_skill_dir = dir.path().join(".agents").join("skills").join("deploy");
    let github_skill_dir = dir.path().join(".github").join("skills").join("deploy");
    let invalid_skill_dir = dir.path().join(".cursor").join("skills").join("bad skill");
    fs::create_dir_all(&codex_skill_dir).expect("create codex skill");
    fs::create_dir_all(&github_skill_dir).expect("create copilot skill");
    fs::create_dir_all(&invalid_skill_dir).expect("create invalid skill");
    fs::write(
        codex_skill_dir.join("SKILL.md"),
        "---\nname: deploy\ndescription: Deploy the service safely.\nallowed-tools: [Bash, Read]\n---\n",
    )
    .expect("write codex skill");
    fs::write(
        github_skill_dir.join("SKILL.md"),
        "---\nname: deploy\ndescription: Deploy the service safely.\n---\n",
    )
    .expect("write github skill");
    fs::write(
        invalid_skill_dir.join("SKILL.md"),
        "---\nname: Bad Skill\n---\n",
    )
    .expect("write invalid skill");
    let claude_dir = dir.path().join(".claude");
    fs::create_dir_all(&claude_dir).expect("create claude dir");
    fs::write(
        claude_dir.join("settings.local.json"),
        r#"{"permissions":[]}"#,
    )
    .expect("write local settings");
    fs::write(
        dir.path().join(".mcp.json"),
        r#"{"mcpServers":{"example":{"env":{"API_KEY":"sk-should-not-appear"}}}}"#,
    )
    .expect("write mcp");

    let report = scan_workspace(ScanInput {
        root: dir.path().to_path_buf(),
        profile: AgentProfile::default(),
        project_config: None,
        options: ScanOptions::default(),
    })
    .expect("scan should succeed");
    let ids = finding_ids(&report);

    assert!(ids.contains(&"AD011"));
    assert!(ids.contains(&"AD012"));
    assert!(ids.contains(&"AD013"));
    assert!(ids.contains(&"AD014"));
    assert!(ids.contains(&"AD015"));
    assert!(
        report
            .findings
            .iter()
            .all(|finding| !finding.message.contains("sk-should-not-appear"))
    );
    assert_eq!(category_score(&report, "Agent files"), (20, 25));
}

fn write_agent_ready_rust_repo(path: &Path) {
    fs::write(
        path.join("Cargo.toml"),
        "[package]\nname = \"agent-ready\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write Cargo.toml");
    fs::write(path.join("README.md"), "# Agent ready\n").expect("write README");
    fs::write(
        path.join("AGENTS.md"),
        "# Agent Instructions\n\n## Project overview\nRust CLI fixture.\n\n## Stack\nRust and Cargo.\n\n## Commands\n- Build: `cargo build`\n- Test: `cargo test`\n- Lint: `cargo clippy`\n\n## Project structure\n- `src/` contains source files.\n\n## Boundaries\n- Do not edit `.env` or generated files.\n- Treat migrations and lockfiles carefully.\n\n## Before finishing\n- Run `cargo fmt`, `cargo clippy`, and `cargo test`.\n",
    )
    .expect("write AGENTS.md");
    let ci_dir = path.join(".github").join("workflows");
    fs::create_dir_all(&ci_dir).expect("create CI dir");
    fs::write(
        ci_dir.join("ci.yml"),
        "name: CI\non: [push]\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test\n      - run: cargo build\n      - run: cargo clippy\n",
    )
    .expect("write CI");
}
