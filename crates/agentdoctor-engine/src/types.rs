use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentKind {
    #[serde(rename = "codex")]
    Codex,
    #[serde(rename = "claude")]
    Claude,
    #[serde(rename = "cursor")]
    Cursor,
    #[serde(rename = "copilot")]
    Copilot,
    #[serde(rename = "gemini")]
    Gemini,
    #[serde(rename = "generic")]
    Generic,
}

impl AgentKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Cursor => "cursor",
            Self::Copilot => "copilot",
            Self::Gemini => "gemini",
            Self::Generic => "generic",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
            Self::Cursor => "Cursor",
            Self::Copilot => "GitHub Copilot",
            Self::Gemini => "Gemini CLI",
            Self::Generic => "Generic agent",
        }
    }
}

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AgentKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "codex" => Ok(Self::Codex),
            "claude" | "claude-code" | "claudecode" => Ok(Self::Claude),
            "cursor" => Ok(Self::Cursor),
            "copilot" | "github-copilot" | "githubcopilot" => Ok(Self::Copilot),
            "gemini" | "gemini-cli" | "geminicli" => Ok(Self::Gemini),
            "generic" | "agents" | "agents.md" => Ok(Self::Generic),
            other => Err(format!("unknown agent '{other}'")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfile {
    pub selected_agents: Vec<AgentKind>,
}

impl AgentProfile {
    pub fn new(selected_agents: Vec<AgentKind>) -> Self {
        let mut deduped = Vec::new();
        for agent in selected_agents {
            if !deduped.contains(&agent) {
                deduped.push(agent);
            }
        }
        if deduped.is_empty() {
            deduped.push(AgentKind::Generic);
        }
        Self {
            selected_agents: deduped,
        }
    }
}

impl Default for AgentProfile {
    fn default() -> Self {
        Self::new(vec![AgentKind::Generic])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentFileSpec {
    pub path: &'static str,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentSpec {
    pub kind: AgentKind,
    pub display_name: &'static str,
    pub description: &'static str,
    pub files: &'static [AgentFileSpec],
}

#[derive(Debug, Clone)]
pub struct ScanInput {
    pub root: PathBuf,
    pub profile: AgentProfile,
    pub project_config: Option<ProjectConfig>,
    pub options: ScanOptions,
}

#[derive(Debug, Clone)]
pub struct InitInput {
    pub root: PathBuf,
    pub profile: AgentProfile,
    pub project_config: Option<ProjectConfig>,
    pub options: ScanOptions,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub include_hidden: bool,
    pub max_file_size_bytes: u64,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            include_hidden: false,
            max_file_size_bytes: 1_048_576,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub score: Score,
    pub facts: WorkspaceFacts,
    pub findings: Vec<Finding>,
    pub recommendations: Vec<Recommendation>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Score {
    pub total: u8,
    pub categories: Vec<ScoreCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreCategory {
    pub name: String,
    pub earned: u8,
    pub max: u8,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub subject: String,
    pub points: i16,
    pub message: String,
}

impl Evidence {
    pub fn awarded(subject: impl Into<String>, points: u8, message: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            points: i16::from(points),
            message: message.into(),
        }
    }

    pub fn missed(subject: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            points: 0,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub message: String,
    pub file: Option<PathBuf>,
    pub fix: Option<FixSuggestion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Critical,
    Warning,
    Info,
    Suggestion,
}

impl Severity {
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::Warning => "Warnings",
            Self::Info => "Info",
            Self::Suggestion => "Suggestions",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FixSuggestion {
    CreateAgentsMd,
    CreateSelectedAgentFile,
    AddCommandSection,
    AddBoundariesSection,
    RemoveDangerousInstruction,
    CreateEnvExample,
    CreateReadme,
    AddCi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFacts {
    pub root: PathBuf,
    pub files: Vec<WorkspaceFile>,
    pub agent_files: Vec<AgentFile>,
    pub customizations: AgentCustomizationFacts,
    pub package_managers: Vec<PackageManager>,
    pub frameworks: Vec<Framework>,
    pub commands: CommandFacts,
    pub ci: CiFacts,
    pub env: EnvFacts,
}

impl WorkspaceFacts {
    pub fn has_path(&self, path: &str) -> bool {
        let wanted = normalize_key(path);
        self.files.iter().any(|file| path_key(&file.path) == wanted)
    }

    pub fn has_any_agent_file(&self, path: &str) -> bool {
        let wanted = normalize_key(path);
        self.agent_files
            .iter()
            .any(|file| path_key(&file.path) == wanted)
    }

    pub fn agent_texts(&self) -> impl Iterator<Item = (&AgentFile, &str)> {
        self.agent_files
            .iter()
            .filter_map(|file| file.content.as_deref().map(|content| (file, content)))
    }

    pub fn detected_stack_labels(&self) -> Vec<String> {
        self.package_managers
            .iter()
            .map(ToString::to_string)
            .chain(self.frameworks.iter().map(ToString::to_string))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFile {
    pub path: PathBuf,
    pub kind: FileKind,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFile {
    pub path: PathBuf,
    pub kind: AgentKind,
    pub size_bytes: u64,
    #[serde(skip_serializing)]
    #[serde(skip_deserializing)]
    pub content: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentCustomizationFacts {
    pub skills: Vec<SkillFile>,
    pub instruction_files: Vec<PathBuf>,
    pub prompt_files: Vec<PathBuf>,
    pub custom_agent_files: Vec<PathBuf>,
    pub command_files: Vec<PathBuf>,
    pub hook_files: Vec<PathBuf>,
    pub mcp_configs: Vec<McpConfigFile>,
    pub local_settings: Vec<PathBuf>,
}

impl AgentCustomizationFacts {
    pub fn has_any(&self) -> bool {
        !self.skills.is_empty()
            || !self.instruction_files.is_empty()
            || !self.prompt_files.is_empty()
            || !self.custom_agent_files.is_empty()
            || !self.command_files.is_empty()
            || !self.hook_files.is_empty()
            || !self.mcp_configs.is_empty()
            || !self.local_settings.is_empty()
    }

    pub fn has_invalid_skill_metadata(&self) -> bool {
        self.skills
            .iter()
            .any(|skill| !skill.missing_required_metadata().is_empty() || !skill.has_valid_name())
    }

    pub fn has_shell_preapproval(&self) -> bool {
        self.skills.iter().any(SkillFile::preapproves_shell)
    }

    pub fn has_secret_like_mcp_config(&self) -> bool {
        self.mcp_configs
            .iter()
            .any(|config| config.has_secret_like_literal)
    }

    pub fn has_local_settings(&self) -> bool {
        !self.local_settings.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    pub path: PathBuf,
    pub root: SkillRoot,
    pub declared_name: Option<String>,
    pub effective_name: String,
    pub description: Option<String>,
    pub allowed_tools: Vec<String>,
    pub size_bytes: u64,
}

impl SkillFile {
    pub fn missing_required_metadata(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.root.requires_declared_name()
            && self
                .declared_name
                .as_deref()
                .is_none_or(|name| name.trim().is_empty())
        {
            missing.push("name");
        }
        if self
            .description
            .as_deref()
            .is_none_or(|description| description.trim().is_empty())
        {
            missing.push("description");
        }
        missing
    }

    pub fn has_valid_name(&self) -> bool {
        if !self.root.requires_declared_name() {
            return true;
        }
        self.declared_name
            .as_deref()
            .is_some_and(is_valid_skill_name)
    }

    pub fn preapproves_shell(&self) -> bool {
        self.allowed_tools.iter().any(|tool| {
            let normalized = tool.trim().to_ascii_lowercase();
            if matches!(
                normalized.as_str(),
                "bash" | "shell" | "powershell" | "cmd" | "terminal"
            ) {
                return true;
            }

            shell_tool_pattern(&normalized).is_some_and(is_broad_shell_pattern)
        })
    }
}

fn shell_tool_pattern(tool: &str) -> Option<&str> {
    ["bash(", "shell(", "powershell(", "cmd("]
        .into_iter()
        .find_map(|prefix| {
            tool.strip_prefix(prefix)
                .map(|pattern| pattern.trim_end_matches(')').trim())
        })
}

fn is_broad_shell_pattern(pattern: &str) -> bool {
    pattern.is_empty() || pattern == "*" || pattern.starts_with('*')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillRoot {
    Codex,
    Claude,
    Copilot,
    Cursor,
    LegacyCodex,
}

impl SkillRoot {
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
            Self::Copilot => "GitHub Copilot",
            Self::Cursor => "Cursor",
            Self::LegacyCodex => "Codex legacy",
        }
    }

    pub const fn requires_declared_name(self) -> bool {
        !matches!(self, Self::Claude)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfigFile {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub has_secret_like_literal: bool,
}

pub fn is_valid_skill_name(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.starts_with('-') || value.ends_with('-') || value.contains("--") {
        return false;
    }
    value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    Markdown,
    Json,
    Toml,
    Yaml,
    Source,
    Config,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageManager {
    Bun,
    Pnpm,
    Npm,
    Yarn,
    Deno,
    Cargo,
    Uv,
    Poetry,
    Pip,
    Go,
}

impl fmt::Display for PackageManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Bun => "Bun",
            Self::Pnpm => "pnpm",
            Self::Npm => "npm",
            Self::Yarn => "Yarn",
            Self::Deno => "Deno",
            Self::Cargo => "Cargo",
            Self::Uv => "uv",
            Self::Poetry => "Poetry",
            Self::Pip => "pip",
            Self::Go => "Go",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Framework {
    NextJs,
    Astro,
    Vite,
    Elysia,
    FastApi,
    Prisma,
    Drizzle,
    Supabase,
    Diesel,
    Docker,
}

impl fmt::Display for Framework {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::NextJs => "Next.js",
            Self::Astro => "Astro",
            Self::Vite => "Vite",
            Self::Elysia => "Elysia",
            Self::FastApi => "FastAPI",
            Self::Prisma => "Prisma",
            Self::Drizzle => "Drizzle",
            Self::Supabase => "Supabase",
            Self::Diesel => "Diesel",
            Self::Docker => "Docker",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandFacts {
    pub install: Option<String>,
    pub dev: Option<String>,
    pub build: Option<String>,
    pub test: Option<String>,
    pub typecheck: Option<String>,
    pub lint: Option<String>,
    pub format: Option<String>,
}

impl CommandFacts {
    pub fn apply_project_commands(&mut self, commands: &ProjectCommands) {
        fill_missing(&mut self.install, commands.install.as_ref());
        fill_missing(&mut self.dev, commands.dev.as_ref());
        fill_missing(&mut self.build, commands.build.as_ref());
        fill_missing(&mut self.test, commands.test.as_ref());
        fill_missing(&mut self.typecheck, commands.typecheck.as_ref());
        fill_missing(&mut self.lint, commands.lint.as_ref());
        fill_missing(&mut self.format, commands.format.as_ref());
    }

    pub fn all_detected(&self) -> Vec<(&'static str, &str)> {
        [
            ("install", self.install.as_deref()),
            ("dev", self.dev.as_deref()),
            ("build", self.build.as_deref()),
            ("test", self.test.as_deref()),
            ("typecheck", self.typecheck.as_deref()),
            ("lint", self.lint.as_deref()),
            ("format", self.format.as_deref()),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.map(|command| (name, command)))
        .collect()
    }
}

fn fill_missing(slot: &mut Option<String>, configured: Option<&String>) {
    if slot.is_none() {
        *slot = configured.cloned();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CiFacts {
    pub workflows: Vec<PathBuf>,
    pub has_ci: bool,
    pub has_test: bool,
    pub has_build: bool,
    pub has_lint: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvFacts {
    pub has_env_example: bool,
    pub uses_env: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectConfig {
    pub version: u32,
    pub project: ProjectSection,
    pub agents: ProjectAgents,
    pub score: ProjectScore,
    pub commands: ProjectCommands,
    pub paths: ProjectPaths,
    pub rules: RuleSettings,
}

impl ProjectConfig {
    pub fn selected_agents(&self) -> Option<Vec<AgentKind>> {
        if self.agents.enabled.is_empty() {
            None
        } else {
            Some(self.agents.enabled.clone())
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSection {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectAgents {
    pub enabled: Vec<AgentKind>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectScore {
    pub minimum: Option<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectCommands {
    pub install: Option<String>,
    pub dev: Option<String>,
    pub build: Option<String>,
    pub test: Option<String>,
    pub typecheck: Option<String>,
    pub lint: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectPaths {
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleSettings {
    pub require_agents_md: bool,
    pub require_test_command: bool,
    pub require_build_command: bool,
    pub require_boundaries: bool,
    pub require_env_example: bool,
    pub detect_generic_instructions: bool,
    pub detect_dangerous_instructions: bool,
}

impl Default for RuleSettings {
    fn default() -> Self {
        Self {
            require_agents_md: true,
            require_test_command: true,
            require_build_command: true,
            require_boundaries: true,
            require_env_example: true,
            detect_generic_instructions: true,
            detect_dangerous_instructions: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitPlan {
    pub changes: Vec<PlannedChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum PlannedChange {
    CreateFile { path: PathBuf, content: String },
    SkipExisting { path: PathBuf, reason: String },
}

pub fn path_key(path: &Path) -> String {
    normalize_key(&path.to_string_lossy())
}

pub fn normalize_key(value: &str) -> String {
    value.replace('\\', "/").to_ascii_lowercase()
}
