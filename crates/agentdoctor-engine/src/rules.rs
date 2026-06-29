use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    AgentProfile, Finding, FixSuggestion, Recommendation, Severity, WorkspaceFacts, path_key,
    required_agent_paths,
};

pub struct RuleContext<'a> {
    pub facts: &'a WorkspaceFacts,
    pub profile: &'a AgentProfile,
    pub project_config: Option<&'a crate::ProjectConfig>,
}

pub trait Rule {
    fn id(&self) -> &'static str;
    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding>;
}

pub fn run_rules(ctx: &RuleContext<'_>) -> Vec<Finding> {
    let rules: &[&dyn Rule] = &[
        &MissingAgentsMd,
        &MissingSelectedAgentFile,
        &MissingTestCommand,
        &MissingBuildCommand,
        &MissingBoundariesSection,
        &GenericAgentInstructions,
        &DangerousAgentInstructions,
        &MissingEnvExample,
        &MissingReadme,
        &MissingCi,
        &InvalidSkillMetadata,
        &DuplicateSkillName,
        &ShellPreapprovedSkill,
        &CommittedLocalAgentSettings,
        &SecretLikeMcpConfig,
    ];

    rules
        .iter()
        .filter(|rule| is_rule_enabled(rule.id(), ctx))
        .flat_map(|rule| rule.run(ctx))
        .collect()
}

pub fn recommendations_for(findings: &[Finding]) -> Vec<Recommendation> {
    findings
        .iter()
        .take(5)
        .map(|finding| Recommendation {
            title: finding.title.clone(),
            message: recommendation_message(finding),
        })
        .collect()
}

pub fn has_boundaries_section(facts: &WorkspaceFacts) -> bool {
    facts
        .agent_texts()
        .any(|(_, text)| contains_heading_or_phrase(text, "boundaries"))
}

pub fn has_before_finish_checklist(facts: &WorkspaceFacts) -> bool {
    facts.agent_texts().any(|(_, text)| {
        contains_heading_or_phrase(text, "before finishing")
            || contains_heading_or_phrase(text, "before finish")
    })
}

pub fn has_project_overview(facts: &WorkspaceFacts) -> bool {
    facts.agent_texts().any(|(_, text)| {
        contains_heading_or_phrase(text, "project overview")
            || contains_heading_or_phrase(text, "overview")
    })
}

pub fn has_project_structure(facts: &WorkspaceFacts) -> bool {
    facts.agent_texts().any(|(_, text)| {
        contains_heading_or_phrase(text, "project structure")
            || contains_heading_or_phrase(text, "repository structure")
    })
}

pub fn has_stack_detail(facts: &WorkspaceFacts) -> bool {
    let stack = facts.detected_stack_labels();
    facts.agent_texts().any(|(_, text)| {
        let lower = text.to_ascii_lowercase();
        contains_heading_or_phrase(text, "stack")
            || stack
                .iter()
                .any(|label| lower.contains(&label.to_ascii_lowercase()))
    })
}

pub fn has_documented_commands(facts: &WorkspaceFacts) -> bool {
    facts.agent_texts().any(|(_, text)| {
        contains_heading_or_phrase(text, "commands")
            || facts
                .commands
                .all_detected()
                .iter()
                .any(|(_, command)| text.contains(command))
    })
}

pub fn has_concrete_agent_content(facts: &WorkspaceFacts) -> bool {
    has_documented_commands(facts) && has_stack_detail(facts) && has_boundaries_section(facts)
}

fn is_rule_enabled(rule_id: &str, ctx: &RuleContext<'_>) -> bool {
    let Some(config) = ctx.project_config else {
        return true;
    };
    match rule_id {
        "AD001" => config.rules.require_agents_md,
        "AD003" => config.rules.require_test_command,
        "AD004" => config.rules.require_build_command,
        "AD005" => config.rules.require_boundaries,
        "AD006" => config.rules.detect_generic_instructions,
        "AD007" => config.rules.detect_dangerous_instructions,
        "AD008" => config.rules.require_env_example,
        _ => true,
    }
}

fn recommendation_message(finding: &Finding) -> String {
    match finding.id.as_str() {
        "AD001" => "Create a canonical AGENTS.md with project-specific commands and boundaries."
            .to_string(),
        "AD002" => "Create the selected agent adapter files or remove those agents from config."
            .to_string(),
        "AD003" => "Add a test command to AGENTS.md or .agentdoctor.toml.".to_string(),
        "AD004" => "Add a build command to AGENTS.md or .agentdoctor.toml.".to_string(),
        "AD005" => "Document files, directories, and workflows agents should not change casually."
            .to_string(),
        "AD006" => {
            "Replace vague instructions with stack details, commands, and repository boundaries."
                .to_string()
        }
        "AD007" => "Remove instructions that tell agents to bypass tests, validation, or security."
            .to_string(),
        "AD008" => "Add a sanitized .env.example with variable names only.".to_string(),
        "AD009" => "Add a README with purpose, setup, and development commands.".to_string(),
        "AD010" => {
            "Add CI that runs the same checks expected from contributors and agents.".to_string()
        }
        "AD011" => "Add valid skill front matter with required name and description metadata."
            .to_string(),
        "AD012" => "Rename duplicate skills so each reusable workflow has a clear identifier."
            .to_string(),
        "AD013" => "Avoid pre-approving broad shell tools inside repo skills.".to_string(),
        "AD014" => {
            "Remove local-only agent settings from the repository or replace them with shared config."
                .to_string()
        }
        "AD015" => "Move secret values out of MCP config and reference environment variables instead."
            .to_string(),
        _ => finding.message.clone(),
    }
}

struct MissingAgentsMd;

impl Rule for MissingAgentsMd {
    fn id(&self) -> &'static str {
        "AD001"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        if ctx.facts.has_path("AGENTS.md") {
            return Vec::new();
        }
        vec![Finding {
            id: self.id().to_string(),
            severity: Severity::Critical,
            title: "Missing AGENTS.md".to_string(),
            message: "No canonical AGENTS.md file was found at the repository root.".to_string(),
            file: Some(PathBuf::from("AGENTS.md")),
            fix: Some(FixSuggestion::CreateAgentsMd),
        }]
    }
}

struct MissingSelectedAgentFile;

impl Rule for MissingSelectedAgentFile {
    fn id(&self) -> &'static str {
        "AD002"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        let mut missing = Vec::new();
        for (agent, path) in required_agent_paths(ctx.profile) {
            if path_key(&path) == "agents.md" {
                continue;
            }
            if !ctx.facts.has_any_agent_file(&path.to_string_lossy()) {
                missing.push(format!("{} ({})", agent.display_name(), path.display()));
            }
        }

        if missing.is_empty() {
            return Vec::new();
        }

        vec![Finding {
            id: self.id().to_string(),
            severity: Severity::Warning,
            title: "Missing selected agent file".to_string(),
            message: format!("Selected agent files are missing: {}.", missing.join(", ")),
            file: None,
            fix: Some(FixSuggestion::CreateSelectedAgentFile),
        }]
    }
}

struct MissingTestCommand;

impl Rule for MissingTestCommand {
    fn id(&self) -> &'static str {
        "AD003"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        if ctx.facts.commands.test.is_some() {
            return Vec::new();
        }
        vec![Finding {
            id: self.id().to_string(),
            severity: Severity::Critical,
            title: "No test command detected".to_string(),
            message: "AgentDoctor could not detect a test command from project files or config."
                .to_string(),
            file: None,
            fix: Some(FixSuggestion::AddCommandSection),
        }]
    }
}

struct MissingBuildCommand;

impl Rule for MissingBuildCommand {
    fn id(&self) -> &'static str {
        "AD004"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        if ctx.facts.commands.build.is_some() {
            return Vec::new();
        }
        vec![Finding {
            id: self.id().to_string(),
            severity: Severity::Warning,
            title: "No build command detected".to_string(),
            message: "AgentDoctor could not detect a build command from project files or config."
                .to_string(),
            file: None,
            fix: Some(FixSuggestion::AddCommandSection),
        }]
    }
}

struct MissingBoundariesSection;

impl Rule for MissingBoundariesSection {
    fn id(&self) -> &'static str {
        "AD005"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        if has_boundaries_section(ctx.facts) {
            return Vec::new();
        }
        vec![Finding {
            id: self.id().to_string(),
            severity: Severity::Warning,
            title: "Missing boundaries section".to_string(),
            message: "Agent instructions do not document files or workflows that require care."
                .to_string(),
            file: Some(PathBuf::from("AGENTS.md")),
            fix: Some(FixSuggestion::AddBoundariesSection),
        }]
    }
}

struct GenericAgentInstructions;

impl Rule for GenericAgentInstructions {
    fn id(&self) -> &'static str {
        "AD006"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        if has_concrete_agent_content(ctx.facts) {
            return Vec::new();
        }
        let phrases = [
            "follow best practices",
            "write clean code",
            "make it modern",
            "use your judgment",
            "do whatever is needed",
            "improve the codebase",
            "ensure scalability",
            "be professional",
            "use latest technologies",
        ];

        ctx.facts
            .agent_texts()
            .filter_map(|(file, text)| {
                let lower = text.to_ascii_lowercase();
                phrases
                    .iter()
                    .any(|phrase| lower.contains(phrase))
                    .then(|| Finding {
                        id: self.id().to_string(),
                        severity: Severity::Warning,
                        title: "Agent instructions look generic".to_string(),
                        message: format!(
                            "{} contains vague guidance without enough project-specific detail.",
                            file.path.display()
                        ),
                        file: Some(file.path.clone()),
                        fix: Some(FixSuggestion::AddCommandSection),
                    })
            })
            .collect()
    }
}

struct DangerousAgentInstructions;

impl Rule for DangerousAgentInstructions {
    fn id(&self) -> &'static str {
        "AD007"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        let phrases = [
            "skip tests",
            "ignore failing tests",
            "ignore type errors",
            "delete migrations",
            "rewrite everything",
            "remove validation",
            "disable authentication",
            "bypass security",
            "use force",
            "never ask for confirmation",
        ];

        ctx.facts
            .agent_texts()
            .filter_map(|(file, text)| {
                let lower = text.to_ascii_lowercase();
                phrases
                    .iter()
                    .any(|phrase| lower.contains(phrase))
                    .then(|| Finding {
                        id: self.id().to_string(),
                        severity: Severity::Critical,
                        title: "Dangerous agent instruction detected".to_string(),
                        message: format!(
                            "{} contains guidance that may bypass tests, validation, or security.",
                            file.path.display()
                        ),
                        file: Some(file.path.clone()),
                        fix: Some(FixSuggestion::RemoveDangerousInstruction),
                    })
            })
            .collect()
    }
}

struct MissingEnvExample;

impl Rule for MissingEnvExample {
    fn id(&self) -> &'static str {
        "AD008"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        if !ctx.facts.env.uses_env || ctx.facts.env.has_env_example {
            return Vec::new();
        }
        vec![Finding {
            id: self.id().to_string(),
            severity: Severity::Warning,
            title: "Missing .env.example".to_string(),
            message: "Environment variable usage was detected, but no .env.example exists."
                .to_string(),
            file: Some(PathBuf::from(".env.example")),
            fix: Some(FixSuggestion::CreateEnvExample),
        }]
    }
}

struct MissingReadme;

impl Rule for MissingReadme {
    fn id(&self) -> &'static str {
        "AD009"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        if ctx.facts.has_path("README.md") {
            return Vec::new();
        }
        vec![Finding {
            id: self.id().to_string(),
            severity: Severity::Info,
            title: "Missing README".to_string(),
            message: "No README.md was found at the repository root.".to_string(),
            file: Some(PathBuf::from("README.md")),
            fix: Some(FixSuggestion::CreateReadme),
        }]
    }
}

struct MissingCi;

impl Rule for MissingCi {
    fn id(&self) -> &'static str {
        "AD010"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        if ctx.facts.ci.has_ci {
            return Vec::new();
        }
        vec![Finding {
            id: self.id().to_string(),
            severity: Severity::Info,
            title: "Missing CI".to_string(),
            message: "No CI workflow file was detected.".to_string(),
            file: Some(PathBuf::from(".github/workflows/ci.yml")),
            fix: Some(FixSuggestion::AddCi),
        }]
    }
}

struct InvalidSkillMetadata;

impl Rule for InvalidSkillMetadata {
    fn id(&self) -> &'static str {
        "AD011"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        ctx.facts
            .customizations
            .skills
            .iter()
            .filter(|skill| {
                !skill.missing_required_metadata().is_empty() || !skill.has_valid_name()
            })
            .map(|skill| {
                let mut problems = skill.missing_required_metadata();
                if !skill.has_valid_name() {
                    problems.push("kebab-case name");
                }
                Finding {
                    id: self.id().to_string(),
                    severity: Severity::Warning,
                    title: "Invalid skill metadata".to_string(),
                    message: format!(
                        "{} skill metadata is missing or invalid: {}.",
                        skill.root.display_name(),
                        problems.join(", ")
                    ),
                    file: Some(skill.path.clone()),
                    fix: None,
                }
            })
            .collect()
    }
}

struct DuplicateSkillName;

impl Rule for DuplicateSkillName {
    fn id(&self) -> &'static str {
        "AD012"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        let mut by_name: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        for skill in &ctx.facts.customizations.skills {
            by_name
                .entry(skill.effective_name.to_ascii_lowercase())
                .or_default()
                .push(skill.path.clone());
        }

        by_name
            .into_iter()
            .filter(|(_, paths)| paths.len() > 1)
            .map(|(name, paths)| Finding {
                id: self.id().to_string(),
                severity: Severity::Suggestion,
                title: "Duplicate skill name".to_string(),
                message: format!(
                    "Skill name '{name}' is declared by multiple files: {}.",
                    paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                file: paths.first().cloned(),
                fix: None,
            })
            .collect()
    }
}

struct ShellPreapprovedSkill;

impl Rule for ShellPreapprovedSkill {
    fn id(&self) -> &'static str {
        "AD013"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        ctx.facts
            .customizations
            .skills
            .iter()
            .filter(|skill| skill.preapproves_shell())
            .map(|skill| Finding {
                id: self.id().to_string(),
                severity: Severity::Warning,
                title: "Skill pre-approves shell tools".to_string(),
                message: format!(
                    "{} pre-approves a broad shell-like tool. Keep shell execution behind normal approval unless the scope is narrowly justified.",
                    skill.path.display()
                ),
                file: Some(skill.path.clone()),
                fix: None,
            })
            .collect()
    }
}

struct CommittedLocalAgentSettings;

impl Rule for CommittedLocalAgentSettings {
    fn id(&self) -> &'static str {
        "AD014"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        ctx.facts
            .customizations
            .local_settings
            .iter()
            .map(|path| Finding {
                id: self.id().to_string(),
                severity: Severity::Warning,
                title: "Local agent settings are committed".to_string(),
                message: format!(
                    "{} looks like a local-only agent settings file and may contain personal paths, permissions, or secrets.",
                    path.display()
                ),
                file: Some(path.clone()),
                fix: None,
            })
            .collect()
    }
}

struct SecretLikeMcpConfig;

impl Rule for SecretLikeMcpConfig {
    fn id(&self) -> &'static str {
        "AD015"
    }

    fn run(&self, ctx: &RuleContext<'_>) -> Vec<Finding> {
        ctx.facts
            .customizations
            .mcp_configs
            .iter()
            .filter(|config| config.has_secret_like_literal)
            .map(|config| Finding {
                id: self.id().to_string(),
                severity: Severity::Warning,
                title: "MCP config may contain a secret".to_string(),
                message: format!(
                    "{} contains a secret-like key with a literal value. AgentDoctor does not print the value; move it to an environment variable.",
                    config.path.display()
                ),
                file: Some(config.path.clone()),
                fix: None,
            })
            .collect()
    }
}

fn contains_heading_or_phrase(text: &str, phrase: &str) -> bool {
    let phrase = phrase.to_ascii_lowercase();
    text.lines().any(|line| {
        let line = line.trim().trim_start_matches('#').trim();
        line.to_ascii_lowercase().contains(&phrase)
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        AgentFile, AgentKind, CommandFacts, EnvFacts, FileKind, ScanOptions, WorkspaceFile,
    };

    use super::*;

    fn facts(agent_content: Option<&str>) -> WorkspaceFacts {
        WorkspaceFacts {
            root: PathBuf::from("."),
            files: vec![WorkspaceFile {
                path: PathBuf::from("AGENTS.md"),
                kind: FileKind::Markdown,
                size_bytes: 1,
            }],
            agent_files: agent_content
                .map(|content| AgentFile {
                    path: PathBuf::from("AGENTS.md"),
                    kind: AgentKind::Generic,
                    size_bytes: content.len() as u64,
                    content: Some(content.to_string()),
                })
                .into_iter()
                .collect(),
            customizations: Default::default(),
            package_managers: Vec::new(),
            frameworks: Vec::new(),
            commands: CommandFacts::default(),
            ci: Default::default(),
            env: EnvFacts::default(),
        }
    }

    fn context(facts: &WorkspaceFacts) -> RuleContext<'_> {
        let profile = Box::leak(Box::new(AgentProfile::default()));
        RuleContext {
            facts,
            profile,
            project_config: None,
        }
    }

    #[test]
    fn missing_agents_md_produces_ad001() {
        let mut facts = facts(None);
        facts.files.clear();
        let findings = MissingAgentsMd.run(&context(&facts));
        assert_eq!(findings[0].id, "AD001");
    }

    #[test]
    fn generic_phrase_produces_ad006_without_concrete_context() {
        let facts = facts(Some("Follow best practices and write clean code."));
        let findings = GenericAgentInstructions.run(&context(&facts));
        assert_eq!(findings[0].id, "AD006");
    }

    #[test]
    fn generic_phrase_is_allowed_when_context_is_concrete() {
        let mut facts = facts(Some(
            "# Agent Instructions

## Stack
Rust

## Commands
cargo test

## Boundaries
Do not edit generated files.

Follow best practices.",
        ));
        facts.commands.test = Some("cargo test".to_string());
        let findings = GenericAgentInstructions.run(&context(&facts));
        assert!(findings.is_empty());
    }

    #[test]
    fn test_command_found_does_not_produce_ad003() {
        let mut facts = facts(None);
        facts.commands.test = Some("cargo test".to_string());
        let findings = MissingTestCommand.run(&context(&facts));
        assert!(findings.is_empty());
    }

    #[test]
    fn selected_claude_profile_requires_claude_file() {
        let facts = facts(Some("## Commands\ncargo test"));
        let profile = AgentProfile::new(vec![AgentKind::Claude]);
        let ctx = RuleContext {
            facts: &facts,
            profile: &profile,
            project_config: None,
        };
        let findings = MissingSelectedAgentFile.run(&ctx);
        assert_eq!(findings[0].id, "AD002");
    }

    #[test]
    fn env_usage_without_example_produces_ad008() {
        let mut facts = facts(None);
        facts.env.uses_env = true;
        let findings = MissingEnvExample.run(&context(&facts));
        assert_eq!(findings[0].id, "AD008");
    }

    #[test]
    fn disabled_rule_is_skipped() {
        let facts = facts(None);
        let config = crate::ProjectConfig {
            rules: crate::RuleSettings {
                require_test_command: false,
                ..crate::RuleSettings::default()
            },
            ..crate::ProjectConfig::default()
        };
        let ctx = RuleContext {
            facts: &facts,
            profile: &AgentProfile::default(),
            project_config: Some(&config),
        };
        let findings = run_rules(&ctx);
        assert!(!findings.iter().any(|finding| finding.id == "AD003"));
    }

    #[test]
    fn default_scan_options_are_reasonable() {
        let options = ScanOptions::default();
        assert!(!options.include_hidden);
        assert!(options.max_file_size_bytes >= 1_000_000);
    }
}
