#![forbid(unsafe_code)]

use std::{
    fs,
    io::{self, Stdout},
    path::PathBuf,
    path::{Component, Path},
    process::ExitCode,
    time::{Duration, Instant},
};

use agentdoctor_engine::{
    AgentKind, AgentProfile, AuditReport, Evidence, Finding, InitInput, InitPlan, PlannedChange,
    ProjectCommands, ProjectConfig, ProjectPaths, ProjectScore, RuleSettings, ScanInput,
    ScanOptions, Severity, agent_specs, generate_init_plan, scan_workspace,
};
use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

const ASCII_LOGO: &str = r"    _                    _   ____             _
   / \   __ _  ___ _ __ | |_|  _ \  ___   ___| |_ ___  _ __
  / _ \ / _` |/ _ \ '_ \| __| | | |/ _ \ / __| __/ _ \| '__|
 / ___ \ (_| |  __/ | | | |_| |_| | (_) | (__| || (_) | |
/_/   \_\__, |\___|_| |_|\__|____/ \___/ \___|\__\___/|_|
        |___/";

const MIN_WIDTH: u16 = 100;
const MIN_HEIGHT: u16 = 34;
const APP_MAX_WIDTH: u16 = 160;
const APP_MAX_HEIGHT: u16 = 48;
const RESCAN_ANIMATION: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Scan,
    Findings,
    Evidence,
    InitPlan,
    Config,
}

impl View {
    const ALL: [Self; 5] = [
        Self::Scan,
        Self::Findings,
        Self::Evidence,
        Self::InitPlan,
        Self::Config,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Scan => "scan summary",
            Self::Findings => "findings",
            Self::Evidence => "evidence",
            Self::InitPlan => "init dry-run",
            Self::Config => "config",
        }
    }

    const fn key(self) -> &'static str {
        match self {
            Self::Scan => "s",
            Self::Findings => "f",
            Self::Evidence => "e",
            Self::InitPlan => "i",
            Self::Config => "c",
        }
    }

    fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|view| *view == self)
            .unwrap_or_default();
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone)]
enum Overlay {
    Help,
    Detail {
        title: String,
        lines: Vec<Line<'static>>,
    },
    Confirm {
        title: String,
        message: String,
        action: PendingAction,
    },
    CommandInput {
        command: CommandOverride,
        input: String,
    },
}

#[derive(Debug, Clone, Copy)]
enum PendingAction {
    ApplyInitChange(usize),
    ApplyAllInitChanges,
    ResetProjectConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandOverride {
    Install,
    Dev,
    Build,
    Test,
    Typecheck,
    Lint,
    Format,
}

impl CommandOverride {
    const ALL: [Self; 7] = [
        Self::Install,
        Self::Dev,
        Self::Build,
        Self::Test,
        Self::Typecheck,
        Self::Lint,
        Self::Format,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Dev => "dev",
            Self::Build => "build",
            Self::Test => "test",
            Self::Typecheck => "typecheck",
            Self::Lint => "lint",
            Self::Format => "format",
        }
    }

    fn get(self, commands: &ProjectCommands) -> Option<&String> {
        match self {
            Self::Install => commands.install.as_ref(),
            Self::Dev => commands.dev.as_ref(),
            Self::Build => commands.build.as_ref(),
            Self::Test => commands.test.as_ref(),
            Self::Typecheck => commands.typecheck.as_ref(),
            Self::Lint => commands.lint.as_ref(),
            Self::Format => commands.format.as_ref(),
        }
    }

    fn set(self, commands: &mut ProjectCommands, value: Option<String>) {
        match self {
            Self::Install => commands.install = value,
            Self::Dev => commands.dev = value,
            Self::Build => commands.build = value,
            Self::Test => commands.test = value,
            Self::Typecheck => commands.typecheck = value,
            Self::Lint => commands.lint = value,
            Self::Format => commands.format = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleToggle {
    RequireAgentsMd,
    RequireTestCommand,
    RequireBuildCommand,
    RequireBoundaries,
    RequireEnvExample,
    DetectGenericInstructions,
    DetectDangerousInstructions,
}

impl RuleToggle {
    const ALL: [Self; 7] = [
        Self::RequireAgentsMd,
        Self::RequireTestCommand,
        Self::RequireBuildCommand,
        Self::RequireBoundaries,
        Self::RequireEnvExample,
        Self::DetectGenericInstructions,
        Self::DetectDangerousInstructions,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::RequireAgentsMd => "require_agents_md",
            Self::RequireTestCommand => "require_test_command",
            Self::RequireBuildCommand => "require_build_command",
            Self::RequireBoundaries => "require_boundaries",
            Self::RequireEnvExample => "require_env_example",
            Self::DetectGenericInstructions => "detect_generic_instructions",
            Self::DetectDangerousInstructions => "detect_dangerous_instructions",
        }
    }

    fn get(self, rules: &RuleSettings) -> bool {
        match self {
            Self::RequireAgentsMd => rules.require_agents_md,
            Self::RequireTestCommand => rules.require_test_command,
            Self::RequireBuildCommand => rules.require_build_command,
            Self::RequireBoundaries => rules.require_boundaries,
            Self::RequireEnvExample => rules.require_env_example,
            Self::DetectGenericInstructions => rules.detect_generic_instructions,
            Self::DetectDangerousInstructions => rules.detect_dangerous_instructions,
        }
    }

    fn set(self, rules: &mut RuleSettings, value: bool) {
        match self {
            Self::RequireAgentsMd => rules.require_agents_md = value,
            Self::RequireTestCommand => rules.require_test_command = value,
            Self::RequireBuildCommand => rules.require_build_command = value,
            Self::RequireBoundaries => rules.require_boundaries = value,
            Self::RequireEnvExample => rules.require_env_example = value,
            Self::DetectGenericInstructions => rules.detect_generic_instructions = value,
            Self::DetectDangerousInstructions => rules.detect_dangerous_instructions = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigRow {
    Agent(AgentKind),
    ScoreMinimum,
    Command(CommandOverride),
    Rule(RuleToggle),
    PathsIgnore,
    ProjectFile,
}

#[derive(Debug)]
struct App {
    root: PathBuf,
    profile: AgentProfile,
    project_config: Option<ProjectConfig>,
    report: AuditReport,
    init_plan: InitPlan,
    view: View,
    selected_row: usize,
    status: String,
    tick: u64,
    rescan_started: Option<Instant>,
    overlay: Option<Overlay>,
    config_dirty: bool,
}

impl App {
    fn new(
        root: PathBuf,
        profile: AgentProfile,
        project_config: Option<ProjectConfig>,
    ) -> anyhow::Result<Self> {
        let report = scan(root.clone(), profile.clone(), project_config.clone())?;
        let init_plan = init_plan(root.clone(), profile.clone(), project_config.clone())?;
        Ok(Self {
            root,
            profile,
            project_config,
            report,
            init_plan,
            view: View::Scan,
            selected_row: 0,
            status: "scan completed - exit 0".to_string(),
            tick: 0,
            rescan_started: None,
            overlay: None,
            config_dirty: false,
        })
    }

    fn start_rescan(&mut self) {
        if self.rescan_started.is_none() {
            self.rescan_started = Some(Instant::now());
            self.status = "rescanning".to_string();
        }
    }

    fn finish_rescan_if_ready(&mut self) {
        let Some(started) = self.rescan_started else {
            return;
        };
        if started.elapsed() >= RESCAN_ANIMATION {
            self.rescan_started = None;
            self.rescan();
        }
    }

    fn rescan(&mut self) {
        match scan(
            self.root.clone(),
            self.profile.clone(),
            self.project_config.clone(),
        )
        .and_then(|report| {
            init_plan(
                self.root.clone(),
                self.profile.clone(),
                self.project_config.clone(),
            )
            .map(|plan| (report, plan))
        }) {
            Ok((report, plan)) => {
                self.report = report;
                self.init_plan = plan;
                self.selected_row = 0;
                self.status = "scan refreshed - exit 0".to_string();
            }
            Err(error) => {
                self.status = format!("rescan failed: {error}");
            }
        }
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
        loop {
            self.tick = self.tick.saturating_add(1);
            self.finish_rescan_if_ready();
            terminal.draw(|frame| self.render(frame))?;
            if event::poll(Duration::from_millis(120))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
                && self.handle_key(key)
            {
                break;
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.handle_overlay_key(key) {
            return false;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Char('?') => self.overlay = Some(Overlay::Help),
            _ if self.rescan_started.is_some() => return false,
            KeyCode::Tab => {
                self.view = self.view.next();
                self.selected_row = 0;
            }
            KeyCode::Char('s') if self.view == View::Config => self.save_project_config(),
            KeyCode::Char('s') => self.set_view(View::Scan),
            KeyCode::Char('f') => self.set_view(View::Findings),
            KeyCode::Char('e') => self.set_view(View::Evidence),
            KeyCode::Char('i') => self.set_view(View::InitPlan),
            KeyCode::Char('c') => self.set_view(View::Config),
            KeyCode::Char('r') => self.start_rescan(),
            KeyCode::Enter => self.open_selected_detail(),
            KeyCode::Char(' ') => self.toggle_selected_config_row(),
            KeyCode::Char('+') | KeyCode::Char('=') => self.adjust_score_minimum(5),
            KeyCode::Char('-') => self.adjust_score_minimum(-5),
            KeyCode::Char('R') if self.view == View::Config => self.confirm_reset_project_config(),
            KeyCode::Char('a') if self.view == View::InitPlan => self.confirm_apply_selected_init(),
            KeyCode::Char('A') if self.view == View::InitPlan => self.confirm_apply_all_init(),
            KeyCode::Down | KeyCode::Char('j') => self.move_row(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_row(-1),
            _ => {}
        }
        false
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) -> bool {
        let Some(overlay) = self.overlay.as_mut() else {
            return false;
        };

        match overlay {
            Overlay::Help | Overlay::Detail { .. } => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                    self.overlay = None;
                    true
                }
                _ => true,
            },
            Overlay::Confirm { action, .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let action = *action;
                    self.overlay = None;
                    self.run_pending_action(action);
                    true
                }
                KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.overlay = None;
                    true
                }
                _ => true,
            },
            Overlay::CommandInput { command, input } => match key.code {
                KeyCode::Enter => {
                    let command = *command;
                    let value = if input.trim().is_empty() {
                        None
                    } else {
                        Some(input.trim().to_string())
                    };
                    self.overlay = None;
                    self.set_command_override(command, value);
                    true
                }
                KeyCode::Esc => {
                    self.overlay = None;
                    true
                }
                KeyCode::Backspace => {
                    input.pop();
                    true
                }
                KeyCode::Char(character) => {
                    input.push(character);
                    true
                }
                _ => true,
            },
        }
    }

    fn set_view(&mut self, view: View) {
        self.view = view;
        self.selected_row = 0;
    }

    fn move_row(&mut self, delta: isize) {
        let count = self.row_count();
        if count == 0 {
            self.selected_row = 0;
            return;
        }
        let last = count.saturating_sub(1);
        self.selected_row = if delta.is_negative() {
            self.selected_row.saturating_sub(delta.unsigned_abs())
        } else {
            self.selected_row.saturating_add(delta as usize).min(last)
        };
    }

    fn open_selected_detail(&mut self) {
        match self.view {
            View::Scan => {
                let selected = command_evidence(&self.report)
                    .nth(self.selected_row)
                    .cloned();
                self.open_evidence_detail(selected);
            }
            View::Evidence => {
                self.open_evidence_detail(self.report.evidence.get(self.selected_row).cloned())
            }
            View::Findings => {
                self.open_finding_detail(self.report.findings.get(self.selected_row).cloned())
            }
            View::InitPlan => {
                self.open_init_detail(self.init_plan.changes.get(self.selected_row).cloned())
            }
            View::Config => self.open_config_detail(),
        }
    }

    fn open_evidence_detail(&mut self, evidence: Option<Evidence>) {
        let Some(evidence) = evidence else {
            return;
        };
        self.overlay = Some(Overlay::Detail {
            title: evidence.subject.clone(),
            lines: vec![
                detail_line("subject", evidence.subject.as_str(), Palette::Text),
                detail_line(
                    "points",
                    &format!("{}", evidence.points),
                    evidence_palette(&evidence),
                ),
                detail_line("message", evidence.message.as_str(), Palette::Muted),
                Line::raw(""),
                Line::styled(
                    "Evidence explains why a score category awarded or withheld points.",
                    Style::default().fg(Palette::Dim.color()),
                ),
            ],
        });
    }

    fn open_finding_detail(&mut self, finding: Option<Finding>) {
        let Some(finding) = finding else {
            return;
        };
        let mut lines = vec![
            detail_line("id", finding.id.as_str(), Palette::Text),
            detail_line(
                "severity",
                severity_marker(finding.severity),
                severity_palette(finding.severity),
            ),
            detail_line("title", finding.title.as_str(), Palette::Text),
            detail_line("message", finding.message.as_str(), Palette::Muted),
        ];
        if let Some(file) = finding.file.as_ref() {
            lines.push(detail_line(
                "file",
                &file.display().to_string(),
                Palette::Text,
            ));
        }
        if let Some(fix) = finding.fix.as_ref() {
            lines.push(detail_line("fix", &format!("{fix:?}"), Palette::Green));
        }
        self.overlay = Some(Overlay::Detail {
            title: "finding detail".to_string(),
            lines,
        });
    }

    fn open_init_detail(&mut self, change: Option<PlannedChange>) {
        let Some(change) = change else {
            return;
        };
        match &change {
            PlannedChange::CreateFile { path, content } => {
                let mut lines = vec![
                    detail_line("action", "create file", Palette::Green),
                    detail_line("path", &path.display().to_string(), Palette::Text),
                    detail_line("apply", "press a for selected, A for all", Palette::Muted),
                    Line::raw(""),
                    heading("preview"),
                ];
                lines.extend(content.lines().take(18).map(|line| {
                    Line::styled(
                        line.to_string(),
                        Style::default().fg(Palette::Muted.color()),
                    )
                }));
                if content.lines().count() > 18 {
                    lines.push(Line::styled(
                        "... preview truncated",
                        Style::default().fg(Palette::Dim.color()),
                    ));
                }
                self.overlay = Some(Overlay::Detail {
                    title: "init preview".to_string(),
                    lines,
                });
            }
            PlannedChange::SkipExisting { path, reason } => {
                self.overlay = Some(Overlay::Detail {
                    title: "init skip".to_string(),
                    lines: vec![
                        detail_line("action", "skip existing", Palette::Amber),
                        detail_line("path", &path.display().to_string(), Palette::Text),
                        detail_line("reason", reason.as_str(), Palette::Muted),
                    ],
                });
            }
        }
    }

    fn open_config_detail(&mut self) {
        let Some(row) = self.selected_config_row() else {
            return;
        };
        match row {
            ConfigRow::Command(command) => {
                let value = self
                    .project_config
                    .as_ref()
                    .and_then(|config| command.get(&config.commands))
                    .cloned()
                    .unwrap_or_default();
                self.overlay = Some(Overlay::CommandInput {
                    command,
                    input: value,
                });
            }
            ConfigRow::Agent(agent) => {
                self.overlay = Some(Overlay::Detail {
                    title: format!("agent: {}", agent.as_str()),
                    lines: vec![
                        detail_line("agent", agent.display_name(), Palette::Text),
                        detail_line("toggle", "press space to enable or disable", Palette::Green),
                        detail_line("save", "press s to write .agentdoctor.toml", Palette::Muted),
                    ],
                });
            }
            ConfigRow::ScoreMinimum => {
                self.overlay = Some(Overlay::Detail {
                    title: "score minimum".to_string(),
                    lines: vec![
                        detail_line("change", "press + or -", Palette::Green),
                        detail_line("save", "press s to write .agentdoctor.toml", Palette::Muted),
                    ],
                });
            }
            ConfigRow::Rule(rule) => {
                self.overlay = Some(Overlay::Detail {
                    title: rule.label().to_string(),
                    lines: vec![
                        detail_line("toggle", "press space to enable or disable", Palette::Green),
                        detail_line("save", "press s to write .agentdoctor.toml", Palette::Muted),
                    ],
                });
            }
            ConfigRow::PathsIgnore | ConfigRow::ProjectFile => {
                self.overlay = Some(Overlay::Detail {
                    title: "project config".to_string(),
                    lines: vec![
                        detail_line(
                            "path",
                            &project_config_path(&self.root).display().to_string(),
                            Palette::Text,
                        ),
                        detail_line("save", "press s to write current settings", Palette::Green),
                        detail_line("reset", "press R to remove project config", Palette::Amber),
                    ],
                });
            }
        }
    }

    fn toggle_selected_config_row(&mut self) {
        if self.view != View::Config {
            return;
        }
        match self.selected_config_row() {
            Some(ConfigRow::Agent(agent)) => self.toggle_agent(agent),
            Some(ConfigRow::Rule(rule)) => self.toggle_rule(rule),
            _ => {}
        }
    }

    fn adjust_score_minimum(&mut self, delta: i16) {
        if self.view != View::Config || self.selected_config_row() != Some(ConfigRow::ScoreMinimum)
        {
            return;
        }
        let current = self
            .project_config
            .as_ref()
            .and_then(|config| config.score.minimum)
            .unwrap_or(80);
        let next = (i16::from(current) + delta).clamp(0, 100) as u8;
        self.project_config_mut().score.minimum = Some(next);
        self.mark_config_changed();
    }

    fn toggle_agent(&mut self, agent: AgentKind) {
        let config = self.project_config_mut();
        if config.agents.enabled.contains(&agent) {
            config.agents.enabled.retain(|enabled| *enabled != agent);
        } else {
            config.agents.enabled.push(agent);
        }
        self.profile = AgentProfile::new(config.agents.enabled.clone());
        self.mark_config_changed();
    }

    fn toggle_rule(&mut self, rule: RuleToggle) {
        let config = self.project_config_mut();
        let current = rule.get(&config.rules);
        rule.set(&mut config.rules, !current);
        self.mark_config_changed();
    }

    fn set_command_override(&mut self, command: CommandOverride, value: Option<String>) {
        command.set(&mut self.project_config_mut().commands, value);
        self.mark_config_changed();
    }

    fn save_project_config(&mut self) {
        let config = self.project_config_mut().clone();
        match save_project_config_file(&self.root, &config) {
            Ok(path) => {
                self.config_dirty = false;
                self.status = format!("saved {}", path.display());
            }
            Err(error) => {
                self.status = format!("save failed: {error}");
            }
        }
    }

    fn confirm_reset_project_config(&mut self) {
        self.overlay = Some(Overlay::Confirm {
            title: "reset project config".to_string(),
            message: format!("Remove {}?", project_config_path(&self.root).display()),
            action: PendingAction::ResetProjectConfig,
        });
    }

    fn confirm_apply_selected_init(&mut self) {
        let Some(PlannedChange::CreateFile { path, .. }) =
            self.init_plan.changes.get(self.selected_row)
        else {
            self.status = "selected init row cannot be applied".to_string();
            return;
        };
        self.overlay = Some(Overlay::Confirm {
            title: "apply init file".to_string(),
            message: format!("Create {}?", path.display()),
            action: PendingAction::ApplyInitChange(self.selected_row),
        });
    }

    fn confirm_apply_all_init(&mut self) {
        let count = self
            .init_plan
            .changes
            .iter()
            .filter(|change| matches!(change, PlannedChange::CreateFile { .. }))
            .count();
        if count == 0 {
            self.status = "no init files to apply".to_string();
            return;
        }
        self.overlay = Some(Overlay::Confirm {
            title: "apply all init files".to_string(),
            message: format!("Create {count} file(s) from the current init plan?"),
            action: PendingAction::ApplyAllInitChanges,
        });
    }

    fn run_pending_action(&mut self, action: PendingAction) {
        match action {
            PendingAction::ApplyInitChange(index) => self.apply_init_change(index),
            PendingAction::ApplyAllInitChanges => self.apply_all_init_changes(),
            PendingAction::ResetProjectConfig => self.reset_project_config(),
        }
    }

    fn apply_init_change(&mut self, index: usize) {
        match self.init_plan.changes.get(index).cloned() {
            Some(PlannedChange::CreateFile { path, content }) => {
                match write_project_file(&self.root, &path, &content) {
                    Ok(()) => {
                        self.reload_project_config_from_disk();
                        self.rescan();
                        self.status = format!("created {}", path.display());
                    }
                    Err(error) => self.status = format!("apply failed: {error}"),
                }
            }
            _ => self.status = "selected init row cannot be applied".to_string(),
        }
    }

    fn apply_all_init_changes(&mut self) {
        let mut created = 0usize;
        for change in &self.init_plan.changes {
            if let PlannedChange::CreateFile { path, content } = change {
                match write_project_file(&self.root, path, content) {
                    Ok(()) => created = created.saturating_add(1),
                    Err(error) => {
                        self.status = format!("apply failed: {error}");
                        return;
                    }
                }
            }
        }
        self.reload_project_config_from_disk();
        self.rescan();
        self.status = format!("created {created} init file(s)");
    }

    fn reset_project_config(&mut self) {
        let path = project_config_path(&self.root);
        match remove_project_config_file(&path) {
            Ok(removed) => {
                self.project_config = None;
                self.profile = AgentProfile::default();
                self.config_dirty = false;
                self.rescan();
                self.status = if removed {
                    "project config reset".to_string()
                } else {
                    "no project config to reset".to_string()
                };
            }
            Err(error) => self.status = format!("reset failed: {error}"),
        }
    }

    fn project_config_mut(&mut self) -> &mut ProjectConfig {
        let default_config = default_project_config(&self.report, &self.profile);
        self.project_config.get_or_insert(default_config)
    }

    fn reload_project_config_from_disk(&mut self) {
        if self.config_dirty {
            return;
        }
        if let Ok(config) = load_project_config_file(&self.root) {
            self.project_config = config;
        }
    }

    fn mark_config_changed(&mut self) {
        let view = self.view;
        let selected_row = self.selected_row;
        self.config_dirty = true;
        self.rescan();
        self.view = view;
        self.selected_row = selected_row.min(self.row_count().saturating_sub(1));
        self.status = "config changed - press s to save".to_string();
    }

    fn config_rows(&self) -> Vec<ConfigRow> {
        let mut rows = agent_specs()
            .iter()
            .map(|spec| ConfigRow::Agent(spec.kind))
            .collect::<Vec<_>>();
        rows.push(ConfigRow::ScoreMinimum);
        rows.extend(CommandOverride::ALL.into_iter().map(ConfigRow::Command));
        rows.extend(RuleToggle::ALL.into_iter().map(ConfigRow::Rule));
        rows.push(ConfigRow::PathsIgnore);
        rows.push(ConfigRow::ProjectFile);
        rows
    }

    fn selected_config_row(&self) -> Option<ConfigRow> {
        self.config_rows().get(self.selected_row).copied()
    }

    fn row_count(&self) -> usize {
        match self.view {
            View::Scan => command_evidence(&self.report).count(),
            View::Evidence => self.report.evidence.len(),
            View::Findings => self.report.findings.len(),
            View::InitPlan => self.init_plan.changes.len(),
            View::Config => self.config_rows().len(),
        }
    }

    fn render(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
            render_too_small(frame, area);
            return;
        }

        let shell = render_app_shell(frame, area);
        let content = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(1)])
            .split(shell)[0];

        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),
                Constraint::Length(2),
                Constraint::Min(20),
                Constraint::Length(2),
            ])
            .split(content);

        self.render_logo(frame, vertical[0]);
        self.render_command_line(frame, vertical[1]);

        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(sidebar_width(vertical[2].width)),
                Constraint::Min(64),
            ])
            .split(vertical[2]);
        self.render_sidebar(frame, body[0]);
        self.render_main(frame, body[1]);
        self.render_footer(frame, vertical[3]);

        if self.rescan_started.is_some() {
            self.render_rescan_popup(frame, area);
        }
        if let Some(overlay) = &self.overlay {
            render_overlay(frame, area, overlay);
        }
    }

    fn render_logo(&self, frame: &mut Frame<'_>, area: Rect) {
        let lines = ASCII_LOGO
            .lines()
            .map(|line| Line::styled(line, Style::default().fg(Palette::Green.color())))
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_command_line(&self, frame: &mut Frame<'_>, area: Rect) {
        let clean = self.report.findings.is_empty();
        let status = if self.rescan_started.is_some() {
            "rescanning"
        } else if clean {
            "clean"
        } else {
            "findings"
        };
        let line = Line::from(vec![
            Span::styled("$ ", Style::default().fg(Palette::Dim.color())),
            Span::raw("agentdoctor "),
            Span::styled(
                truncate_path(&self.root, command_path_width(area.width)),
                Style::default().fg(Palette::Dim.color()),
            ),
            Span::raw("   "),
            Span::styled(
                format!("* {status}"),
                Style::default().fg(if clean {
                    Palette::Green.color()
                } else {
                    Palette::Amber.color()
                }),
            ),
            Span::styled(
                "   cargo   offline",
                Style::default().fg(Palette::Muted.color()),
            ),
        ]);
        frame.render_widget(Paragraph::new(vec![line, divider_line(area.width)]), area);
    }

    fn render_sidebar(&self, frame: &mut Frame<'_>, area: Rect) {
        let [content, rule] = split_right_rule(area);
        let mut lines = vec![heading("views")];
        for view in View::ALL {
            let selected = view == self.view;
            lines.push(Line::from(vec![
                Span::styled(
                    if selected { "> " } else { "  " },
                    Style::default().fg(Palette::Green.color()),
                ),
                Span::styled(
                    view.label(),
                    Style::default().fg(if selected {
                        Palette::Text.color()
                    } else {
                        Palette::Muted.color()
                    }),
                ),
                Span::styled(
                    format!(" {:>2}", view.key()),
                    Style::default().fg(Palette::Dim.color()),
                ),
            ]));
        }

        lines.push(Line::raw(""));
        lines.push(heading("detected"));
        lines.push(kv_line("stack", stack_label(&self.report)));
        lines.push(kv_line("agent", agent_surface_label(&self.report)));
        lines.push(kv_line(
            "ci",
            if self.report.facts.ci.has_ci {
                "strict checks".to_string()
            } else {
                "none".to_string()
            },
        ));
        lines.push(Line::raw(""));
        lines.push(heading("profile"));
        lines.push(kv_line(
            "selected",
            self.profile
                .selected_agents
                .iter()
                .map(|agent| agent.as_str())
                .collect::<Vec<_>>()
                .join(","),
        ));
        lines.push(kv_line("output", "pretty / json".to_string()));
        lines.push(kv_line("network", "no scan access".to_string()));

        frame.render_widget(Paragraph::new(lines), content);
        render_vertical_rule(frame, rule);
    }

    fn render_main(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Clear, area);
        match self.view {
            View::Scan => self.render_scan(frame, area),
            View::Findings => self.render_findings(frame, area),
            View::Evidence => self.render_evidence(frame, area),
            View::InitPlan => self.render_init_plan(frame, area),
            View::Config => self.render_config(frame, area),
        }
    }

    fn render_scan(&self, frame: &mut Frame<'_>, area: Rect) {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Length(1),
                Constraint::Length(8),
                Constraint::Length(1),
                Constraint::Min(7),
            ])
            .split(area);

        self.render_score_headline(frame, vertical[0]);
        frame.render_widget(Paragraph::new(divider_line(vertical[1].width)), vertical[1]);
        self.render_score_bars(frame, vertical[2]);
        frame.render_widget(Paragraph::new(divider_line(vertical[3].width)), vertical[3]);
        self.render_scan_bottom(frame, vertical[4]);
    }

    fn render_score_headline(&self, frame: &mut Frame<'_>, area: Rect) {
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(20),
                Constraint::Min(30),
                Constraint::Length(23),
            ])
            .split(area);

        frame.render_widget(
            Paragraph::new(big_score_lines(self.report.score.total)),
            horizontal[0],
        );

        let subtitle = if self.report.findings.is_empty() {
            if self.report.score.total == 100 {
                "No findings. Repository is fully ready.".to_string()
            } else {
                format!(
                    "No findings. {} score evidence notes remain.",
                    missed_evidence_count(&self.report)
                )
            }
        } else {
            format!(
                "{} finding(s). Open findings for fixes.",
                self.report.findings.len()
            )
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::styled(
                    "AI Agent Readiness",
                    Style::default()
                        .fg(Palette::Text.color())
                        .add_modifier(Modifier::BOLD),
                ),
                Line::styled(subtitle, Style::default().fg(Palette::Muted.color())),
            ]),
            horizontal[1],
        );

        frame.render_widget(Paragraph::new(finding_strip(&self.report)), horizontal[2]);
    }

    fn render_score_bars(&self, frame: &mut Frame<'_>, area: Rect) {
        let constraints = self
            .report
            .score
            .categories
            .iter()
            .map(|_| Constraint::Length(1))
            .collect::<Vec<_>>();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        for (index, category) in self.report.score.categories.iter().enumerate() {
            let row = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(26),
                    Constraint::Min(20),
                    Constraint::Length(8),
                ])
                .split(rows[index]);
            frame.render_widget(
                Paragraph::new(category.name.as_str())
                    .style(Style::default().fg(Palette::Text.color())),
                row[0],
            );
            let palette = if category.earned == category.max {
                Palette::Green
            } else if category.earned.saturating_mul(100) / category.max.max(1) >= 60 {
                Palette::Amber
            } else {
                Palette::Red
            };
            frame.render_widget(
                Paragraph::new(ascii_bar(category.earned, category.max, row[1].width))
                    .style(Style::default().fg(palette.color())),
                row[1],
            );
            frame.render_widget(
                Paragraph::new(format!("{}/{}", category.earned, category.max))
                    .style(Style::default().fg(Palette::Muted.color()))
                    .alignment(Alignment::Right),
                row[2],
            );
        }
    }

    fn render_scan_bottom(&self, frame: &mut Frame<'_>, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .spacing(2)
            .split(area);
        self.render_evidence_rows(
            frame,
            columns[0],
            "evidence: commands",
            command_evidence(&self.report),
        );
        self.render_next_action(frame, columns[1]);
    }

    fn render_next_action(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines = vec![Line::from(vec![
            Span::styled("next action", Style::default().fg(Palette::Muted.color())),
            Span::styled("   ready", Style::default().fg(Palette::Green.color())),
        ])];

        if self.report.findings.is_empty() {
            lines.push(note_line(
                "ok",
                "Repository is ready for agent work.",
                Palette::Green,
            ));
            if self.report.score.total < 100 {
                lines.push(note_line(
                    "--",
                    "Add missing command evidence only if a perfect score matters.",
                    Palette::Amber,
                ));
            }
            lines.push(note_line(
                "ok",
                "JSON output remains safe for CI.",
                Palette::Green,
            ));
        } else {
            for finding in self.report.findings.iter().take(3) {
                lines.push(note_line(
                    severity_marker(finding.severity),
                    finding.title.as_str(),
                    severity_palette(finding.severity),
                ));
            }
        }

        lines.push(Line::raw(""));
        lines.push(heading("detected commands"));
        for (name, command) in self.report.facts.commands.all_detected() {
            lines.push(kv_line(name, command.to_string()));
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
    }

    fn render_findings(&self, frame: &mut Frame<'_>, area: Rect) {
        let viewport = list_viewport(area);
        let total = self.report.findings.len();
        let offset = scroll_offset(self.selected_row, total, viewport);
        let mut lines = vec![title_line(
            "findings",
            &list_hint("j/k move - enter details", offset, total, viewport),
        )];
        lines.push(divider_line(area.width));
        if self.report.findings.is_empty() {
            lines.push(note_line("ok", "No rule findings.", Palette::Green));
            lines.push(note_line(
                "--",
                "Score evidence can still explain non-critical gaps.",
                Palette::Amber,
            ));
        } else {
            for (index, finding) in self
                .report
                .findings
                .iter()
                .enumerate()
                .skip(offset)
                .take(viewport)
            {
                lines.push(finding_line(
                    index == self.selected_row,
                    finding,
                    area.width,
                ));
            }
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_evidence(&self, frame: &mut Frame<'_>, area: Rect) {
        self.render_evidence_rows(
            frame,
            area,
            "all score evidence",
            self.report.evidence.iter(),
        );
    }

    fn render_evidence_rows<'a>(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        title: &str,
        evidence: impl Iterator<Item = &'a Evidence>,
    ) {
        let rows = evidence.collect::<Vec<_>>();
        let viewport = list_viewport(area);
        let total = rows.len();
        let selected = self.selected_row.min(total.saturating_sub(1));
        let offset = scroll_offset(selected, total, viewport);
        let mut lines = vec![title_line(
            title,
            &list_hint("j/k move - r rescan", offset, total, viewport),
        )];
        lines.push(divider_line(area.width));
        for (index, item) in rows.into_iter().enumerate().skip(offset).take(viewport) {
            let selected = index == self.selected_row;
            let points = if item.points > 0 {
                format!("+{} pts", item.points)
            } else {
                "0 pts".to_string()
            };
            let palette = if item.points > 0 {
                Palette::Green
            } else {
                Palette::Amber
            };
            lines.push(row_line(
                selected,
                item.subject.as_str(),
                item.message.as_str(),
                points.as_str(),
                palette,
                area.width,
            ));
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_init_plan(&self, frame: &mut Frame<'_>, area: Rect) {
        let viewport = list_viewport(area);
        let total = self.init_plan.changes.len();
        let offset = scroll_offset(self.selected_row, total, viewport);
        let mut lines = vec![title_line(
            "init dry-run",
            &list_hint("preview only - no writes", offset, total, viewport),
        )];
        lines.push(divider_line(area.width));
        for (index, change) in self
            .init_plan
            .changes
            .iter()
            .enumerate()
            .skip(offset)
            .take(viewport)
        {
            match change {
                agentdoctor_engine::PlannedChange::CreateFile { path, .. } => {
                    lines.push(row_line(
                        index == self.selected_row,
                        "create",
                        &path.display().to_string(),
                        "dry-run",
                        Palette::Green,
                        area.width,
                    ));
                }
                agentdoctor_engine::PlannedChange::SkipExisting { path, reason } => {
                    lines.push(row_line(
                        index == self.selected_row,
                        "skip",
                        &format!("{} - {reason}", path.display()),
                        "exists",
                        Palette::Muted,
                        area.width,
                    ));
                }
            }
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_config(&self, frame: &mut Frame<'_>, area: Rect) {
        let rows = self.config_rows();
        let viewport = list_viewport(area);
        let offset = scroll_offset(self.selected_row, rows.len(), viewport);
        let save_state = if self.config_dirty {
            "unsaved"
        } else {
            "saved"
        };
        let mut lines = vec![title_line(
            "config",
            &list_hint(
                &format!("space toggle - enter edit/detail - s save - R reset   {save_state}"),
                offset,
                rows.len(),
                viewport,
            ),
        )];
        lines.push(divider_line(area.width));
        for (index, row) in rows.iter().enumerate().skip(offset).take(viewport) {
            let (subject, message, points, palette) = self.config_row_display(*row);
            lines.push(row_line(
                self.selected_row == index,
                &subject,
                &message,
                &points,
                palette,
                area.width,
            ));
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_footer(&self, frame: &mut Frame<'_>, area: Rect) {
        let status = if self.rescan_started.is_some() {
            format!("rescanning {}", indeterminate_bar(self.tick, 18))
        } else {
            self.status.clone()
        };
        let context_hint = match self.view {
            View::Config => " enter edit  space toggle  s save  R reset  ",
            View::InitPlan => " enter preview  a apply  A apply all  ",
            _ => " enter detail  ",
        };
        let line = Line::from(vec![
            Span::styled("tab", key_style()),
            Span::raw(" view  "),
            Span::styled("j/k", key_style()),
            Span::raw(" row  "),
            Span::styled("?", key_style()),
            Span::raw(" help  "),
            Span::styled("r", key_style()),
            Span::raw(" rescan  "),
            Span::styled("q", key_style()),
            Span::raw(" quit"),
            Span::styled(context_hint, Style::default().fg(Palette::Dim.color())),
            Span::styled("     ", Style::default()),
            Span::styled(status, Style::default().fg(Palette::Green.color())),
        ]);
        frame.render_widget(Paragraph::new(vec![divider_line(area.width), line]), area);
    }

    fn config_row_display(&self, row: ConfigRow) -> (String, String, String, Palette) {
        match row {
            ConfigRow::Agent(agent) => {
                let enabled = self.profile.selected_agents.contains(&agent);
                (
                    format!("agent:{}", agent.as_str()),
                    agent.display_name().to_string(),
                    toggle_label(enabled).to_string(),
                    if enabled {
                        Palette::Green
                    } else {
                        Palette::Muted
                    },
                )
            }
            ConfigRow::ScoreMinimum => {
                let value = self
                    .project_config
                    .as_ref()
                    .and_then(|config| config.score.minimum)
                    .map_or_else(|| "80".to_string(), |minimum| minimum.to_string());
                (
                    "score".to_string(),
                    "minimum readiness score".to_string(),
                    value,
                    Palette::Cyan,
                )
            }
            ConfigRow::Command(command) => {
                let value = self
                    .project_config
                    .as_ref()
                    .and_then(|config| command.get(&config.commands))
                    .cloned()
                    .unwrap_or_else(|| {
                        detected_command(&self.report, command)
                            .unwrap_or_else(|| "not set".to_string())
                    });
                (
                    format!("cmd:{}", command.label()),
                    value,
                    "edit".to_string(),
                    Palette::Cyan,
                )
            }
            ConfigRow::Rule(rule) => {
                let enabled = self.project_config.as_ref().map_or_else(
                    || rule.get(&RuleSettings::default()),
                    |config| rule.get(&config.rules),
                );
                (
                    format!("rule:{}", rule.label()),
                    "project rule".to_string(),
                    toggle_label(enabled).to_string(),
                    if enabled {
                        Palette::Green
                    } else {
                        Palette::Muted
                    },
                )
            }
            ConfigRow::PathsIgnore => (
                "paths".to_string(),
                self.project_config.as_ref().map_or_else(
                    || default_ignore_paths().join(", "),
                    |config| config.paths.ignore.join(", "),
                ),
                "ignore".to_string(),
                Palette::Muted,
            ),
            ConfigRow::ProjectFile => (
                "project".to_string(),
                project_config_path(&self.root).display().to_string(),
                if self.config_dirty { "dirty" } else { "file" }.to_string(),
                if self.config_dirty {
                    Palette::Amber
                } else {
                    Palette::Muted
                },
            ),
        }
    }

    fn render_rescan_popup(&self, frame: &mut Frame<'_>, area: Rect) {
        let popup = centered_rect(area, 56, 9);
        let block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(Palette::Shell.color()))
            .border_style(Style::default().fg(Palette::Green.color()))
            .title(Span::styled(
                " Rescanning ",
                Style::default()
                    .fg(Palette::Green.color())
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(popup);
        let bar = indeterminate_bar(
            self.tick,
            usize::from(inner.width.saturating_sub(8).max(10)),
        );
        let lines = vec![
            Line::raw(""),
            Line::styled(
                "Refreshing workspace scan",
                Style::default()
                    .fg(Palette::Text.color())
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled(bar, Style::default().fg(Palette::Green.color())),
            Line::styled(
                "Findings, score, evidence, and init plan are updating.",
                Style::default().fg(Palette::Muted.color()),
            ),
        ];

        frame.render_widget(Clear, popup);
        frame.render_widget(block, popup);
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }
}

pub fn run(
    root: PathBuf,
    profile: AgentProfile,
    project_config: Option<ProjectConfig>,
) -> anyhow::Result<ExitCode> {
    let mut session = TerminalSession::new()?;
    let mut app = App::new(root, profile, project_config)?;
    app.run(&mut session.terminal)?;
    Ok(ExitCode::SUCCESS)
}

fn scan(
    root: PathBuf,
    profile: AgentProfile,
    project_config: Option<ProjectConfig>,
) -> anyhow::Result<AuditReport> {
    scan_workspace(ScanInput {
        root,
        profile,
        project_config,
        options: ScanOptions::default(),
    })
    .context("failed to scan workspace")
}

fn init_plan(
    root: PathBuf,
    profile: AgentProfile,
    project_config: Option<ProjectConfig>,
) -> anyhow::Result<InitPlan> {
    generate_init_plan(InitInput {
        root,
        profile,
        project_config,
        options: ScanOptions::default(),
    })
    .context("failed to build init plan")
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn new() -> anyhow::Result<Self> {
        enable_raw_mode().context("failed to enable terminal raw mode")?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error).context("failed to enter alternate screen");
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = disable_raw_mode();
                Err(error).context("failed to initialize terminal")
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

#[derive(Debug, Clone, Copy)]
enum Palette {
    Background,
    Shell,
    Line,
    Text,
    Muted,
    Dim,
    Selected,
    Green,
    Cyan,
    Amber,
    Red,
    Violet,
}

impl Palette {
    const fn color(self) -> Color {
        match self {
            Self::Background => Color::Rgb(7, 10, 11),
            Self::Shell => Color::Rgb(13, 18, 19),
            Self::Line => Color::Rgb(45, 57, 58),
            Self::Text => Color::Rgb(220, 230, 223),
            Self::Muted => Color::Rgb(145, 160, 155),
            Self::Dim => Color::Rgb(96, 112, 108),
            Self::Selected => Color::Rgb(31, 45, 46),
            Self::Green => Color::Rgb(115, 226, 167),
            Self::Cyan => Color::Rgb(99, 203, 208),
            Self::Amber => Color::Rgb(227, 178, 97),
            Self::Red => Color::Rgb(240, 111, 111),
            Self::Violet => Color::Rgb(170, 157, 255),
        }
    }
}

fn render_too_small(frame: &mut Frame<'_>, area: Rect) {
    let message = Paragraph::new(vec![
        Line::styled(
            "AgentDoctor",
            Style::default()
                .fg(Palette::Green.color())
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::styled(
            format!("Terminal too small. Need at least {MIN_WIDTH}x{MIN_HEIGHT}."),
            Style::default().fg(Palette::Muted.color()),
        ),
        Line::styled(
            "Resize the terminal or run `agentdoctor scan`.",
            Style::default().fg(Palette::Dim.color()),
        ),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(message, area);
}

fn render_app_shell(frame: &mut Frame<'_>, area: Rect) -> Rect {
    frame.render_widget(
        Block::default().style(Style::default().bg(Palette::Background.color())),
        area,
    );

    let shell = centered_app_rect(area);
    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(Palette::Shell.color()))
        .border_style(Style::default().fg(Palette::Line.color()))
        .title(Span::styled(
            " AgentDoctor ",
            Style::default()
                .fg(Palette::Green.color())
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(shell);
    frame.render_widget(Clear, shell);
    frame.render_widget(block, shell);
    inner
}

fn centered_app_rect(area: Rect) -> Rect {
    let width = if area.width > APP_MAX_WIDTH + 4 {
        APP_MAX_WIDTH
    } else {
        area.width.saturating_sub(2).max(1)
    };
    let height = if area.height > APP_MAX_HEIGHT + 2 {
        APP_MAX_HEIGHT
    } else {
        area.height.saturating_sub(1).max(1)
    };

    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width).max(1);
    let height = height.min(area.height).max(1);

    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn render_overlay(frame: &mut Frame<'_>, area: Rect, overlay: &Overlay) {
    match overlay {
        Overlay::Help => render_text_popup(frame, area, "Help", help_lines(), 74, 18),
        Overlay::Detail { title, lines } => {
            render_text_popup(frame, area, title, lines.clone(), 86, 24);
        }
        Overlay::Confirm {
            title,
            message,
            action: _,
        } => render_text_popup(
            frame,
            area,
            title,
            vec![
                Line::styled(message.clone(), Style::default().fg(Palette::Text.color())),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("enter/y", key_style()),
                    Span::raw(" confirm   "),
                    Span::styled("esc/n", key_style()),
                    Span::raw(" cancel"),
                ]),
            ],
            72,
            8,
        ),
        Overlay::CommandInput { command, input } => render_text_popup(
            frame,
            area,
            &format!("edit command: {}", command.label()),
            vec![
                detail_line("value", input.as_str(), Palette::Text),
                Line::raw(""),
                Line::styled(
                    "Type a command. Leave empty to clear this project override.",
                    Style::default().fg(Palette::Muted.color()),
                ),
                Line::from(vec![
                    Span::styled("enter", key_style()),
                    Span::raw(" apply   "),
                    Span::styled("esc", key_style()),
                    Span::raw(" cancel"),
                ]),
            ],
            82,
            10,
        ),
    }
}

fn render_text_popup(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    lines: Vec<Line<'static>>,
    width: u16,
    height: u16,
) {
    let popup = centered_rect(area, width, height);
    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(Palette::Shell.color()))
        .border_style(Style::default().fg(Palette::Green.color()))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(Palette::Green.color())
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn help_lines() -> Vec<Line<'static>> {
    vec![
        heading("global"),
        help_line("tab", "switch views"),
        help_line("j/k", "move selection"),
        help_line("enter", "open detail, preview, or edit"),
        help_line("?", "open this help"),
        help_line("esc", "close modal"),
        help_line("r", "rescan repository"),
        help_line("q", "quit"),
        Line::raw(""),
        heading("config"),
        help_line("space", "toggle selected agent or rule"),
        help_line("+/-", "adjust score minimum when selected"),
        help_line("s", "save project config to .agentdoctor.toml"),
        help_line("R", "reset project config with confirmation"),
        Line::raw(""),
        heading("init dry-run"),
        help_line("a", "apply selected generated file"),
        help_line("A", "apply all generated files"),
    ]
}

fn help_line(key: &'static str, text: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<10}"), key_style()),
        Span::styled(text, Style::default().fg(Palette::Muted.color())),
    ])
}

fn sidebar_width(width: u16) -> u16 {
    if width >= 132 { 30 } else { 25 }
}

fn command_path_width(width: u16) -> usize {
    usize::from(width).saturating_sub(46).max(20)
}

fn split_right_rule(area: Rect) -> [Rect; 2] {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    [chunks[0], chunks[1]]
}

fn render_vertical_rule(frame: &mut Frame<'_>, area: Rect) {
    let lines = (0..area.height)
        .map(|_| Line::styled("|", Style::default().fg(Palette::Dim.color())))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

fn divider_line(width: u16) -> Line<'static> {
    Line::styled(
        "-".repeat(usize::from(width.max(1))),
        Style::default().fg(Palette::Dim.color()),
    )
}

fn list_viewport(area: Rect) -> usize {
    usize::from(area.height.saturating_sub(2))
}

fn scroll_offset(selected: usize, len: usize, viewport: usize) -> usize {
    if len == 0 || viewport == 0 {
        return 0;
    }

    let max_offset = len.saturating_sub(viewport);
    if selected >= viewport {
        selected + 1 - viewport
    } else {
        0
    }
    .min(max_offset)
}

fn list_hint(base: &str, offset: usize, len: usize, viewport: usize) -> String {
    if len == 0 {
        return base.to_string();
    }
    let start = offset + 1;
    let end = offset.saturating_add(viewport).min(len);
    format!("{base}   {start}-{end}/{len}")
}

fn detail_line(label: &str, value: &str, palette: Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<11}"),
            Style::default().fg(Palette::Dim.color()),
        ),
        Span::styled(value.to_string(), Style::default().fg(palette.color())),
    ])
}

fn evidence_palette(evidence: &Evidence) -> Palette {
    if evidence.points > 0 {
        Palette::Green
    } else {
        Palette::Amber
    }
}

fn ascii_bar(earned: u8, max: u8, width: u16) -> Line<'static> {
    let available = usize::from(width.saturating_sub(2).max(1));
    let filled = if max == 0 {
        0
    } else {
        usize::from(earned).saturating_mul(available) / usize::from(max)
    }
    .min(available);
    let empty = available.saturating_sub(filled);
    Line::from(vec![
        Span::styled("[", Style::default().fg(Palette::Dim.color())),
        Span::raw("=".repeat(filled)),
        Span::styled(" ".repeat(empty), Style::default().fg(Palette::Dim.color())),
        Span::styled("]", Style::default().fg(Palette::Dim.color())),
    ])
}

fn big_score_lines(score: u8) -> Vec<Line<'static>> {
    let text = score.to_string();
    (0..5)
        .map(|row| {
            let mut line = String::new();
            for (index, character) in text.chars().enumerate() {
                if index > 0 {
                    line.push(' ');
                }
                line.push_str(digit_row(character, row));
            }
            Line::styled(
                line,
                Style::default()
                    .fg(Palette::Green.color())
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect()
}

fn digit_row(character: char, row: usize) -> &'static str {
    let digit = match character {
        '0' => [" ### ", "#   #", "#   #", "#   #", " ### "],
        '1' => ["  #  ", " ##  ", "  #  ", "  #  ", " ### "],
        '2' => [" ### ", "#   #", "   # ", "  #  ", "#####"],
        '3' => ["#### ", "    #", " ### ", "    #", "#### "],
        '4' => ["#   #", "#   #", "#####", "    #", "    #"],
        '5' => ["#####", "#    ", "#### ", "    #", "#### "],
        '6' => [" ### ", "#    ", "#### ", "#   #", " ### "],
        '7' => ["#####", "    #", "   # ", "  #  ", "  #  "],
        '8' => [" ### ", "#   #", " ### ", "#   #", " ### "],
        '9' => [" ### ", "#   #", " ####", "    #", " ### "],
        _ => ["     ", "     ", "     ", "     ", "     "],
    };
    digit[row]
}

fn indeterminate_bar(tick: u64, width: usize) -> String {
    let width = width.max(10);
    let segment = (width / 4).max(3).min(width.saturating_sub(2));
    let travel = width.saturating_sub(segment);
    let cycle = travel.saturating_mul(2).max(1);
    let step = (tick as usize) % cycle;
    let offset = if step <= travel { step } else { cycle - step };

    let mut chars = vec![' '; width];
    for slot in chars.iter_mut().skip(offset).take(segment) {
        *slot = '=';
    }

    format!("[{}]", chars.into_iter().collect::<String>())
}

fn heading(value: &str) -> Line<'static> {
    Line::styled(
        value.to_ascii_uppercase(),
        Style::default().fg(Palette::Dim.color()),
    )
}

fn title_line(title: &str, hint: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            title.to_string(),
            Style::default().fg(Palette::Muted.color()),
        ),
        Span::styled(
            format!("   {hint}"),
            Style::default().fg(Palette::Dim.color()),
        ),
    ])
}

fn kv_line(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<9}"),
            Style::default().fg(Palette::Text.color()),
        ),
        Span::styled(value, Style::default().fg(Palette::Muted.color())),
    ])
}

fn note_line(marker: &'static str, text: &str, palette: Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{marker:<4}"), Style::default().fg(palette.color())),
        Span::styled(
            text.to_string(),
            Style::default().fg(Palette::Muted.color()),
        ),
    ])
}

fn row_line(
    selected: bool,
    subject: &str,
    message: &str,
    points: &str,
    palette: Palette,
    width: u16,
) -> Line<'static> {
    let prefix = if selected { "> " } else { "  " };
    let subject_width = row_subject_width(width);
    let message_width = usize::from(width)
        .saturating_sub(prefix.len() + subject_width + points.len() + 2)
        .max(12);
    let subject = truncate_end(subject, subject_width);
    Line::from(vec![
        Span::styled(prefix, row_span_style(selected, Palette::Green.color())),
        Span::styled(
            format!("{subject:<subject_width$}"),
            row_span_style(selected, Palette::Text.color()),
        ),
        Span::styled(" ", row_span_style(selected, Palette::Text.color())),
        Span::styled(
            format!("{:<message_width$} ", truncate_text(message, message_width)),
            row_span_style(
                selected,
                if selected {
                    Palette::Text.color()
                } else {
                    Palette::Muted.color()
                },
            ),
        ),
        Span::styled(
            points.to_string(),
            row_span_style(selected, palette.color()),
        ),
    ])
}

fn row_span_style(selected: bool, foreground: Color) -> Style {
    let style = Style::default().fg(foreground);
    if selected {
        style
            .bg(Palette::Selected.color())
            .add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

fn row_subject_width(width: u16) -> usize {
    (usize::from(width) / 3).clamp(18, 28)
}

fn finding_line(selected: bool, finding: &Finding, width: u16) -> Line<'static> {
    row_line(
        selected,
        finding.id.as_str(),
        finding.title.as_str(),
        severity_marker(finding.severity),
        severity_palette(finding.severity),
        width,
    )
}

fn key_style() -> Style {
    Style::default()
        .fg(Palette::Text.color())
        .bg(Color::Rgb(18, 24, 25))
}

fn command_evidence(report: &AuditReport) -> impl Iterator<Item = &Evidence> {
    report.evidence.iter().filter(|evidence| {
        matches!(
            evidence.subject.as_str(),
            "install-command"
                | "dev-command"
                | "build-command"
                | "test-command"
                | "typecheck-or-lint-command"
        )
    })
}

fn detected_command(report: &AuditReport, command: CommandOverride) -> Option<String> {
    match command {
        CommandOverride::Install => report.facts.commands.install.clone(),
        CommandOverride::Dev => report.facts.commands.dev.clone(),
        CommandOverride::Build => report.facts.commands.build.clone(),
        CommandOverride::Test => report.facts.commands.test.clone(),
        CommandOverride::Typecheck => report.facts.commands.typecheck.clone(),
        CommandOverride::Lint => report.facts.commands.lint.clone(),
        CommandOverride::Format => report.facts.commands.format.clone(),
    }
}

fn missed_evidence_count(report: &AuditReport) -> usize {
    report
        .evidence
        .iter()
        .filter(|evidence| evidence.points == 0)
        .count()
}

fn finding_strip(report: &AuditReport) -> Vec<Line<'static>> {
    let critical = finding_count(report, Severity::Critical);
    let warning = finding_count(report, Severity::Warning);
    let info = finding_count(report, Severity::Info);
    let suggestion = finding_count(report, Severity::Suggestion);
    vec![
        Line::from(vec![
            Span::styled(
                format!("{critical}"),
                Style::default().fg(Palette::Red.color()),
            ),
            Span::styled(" critical", Style::default().fg(Palette::Muted.color())),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{warning}"),
                Style::default().fg(Palette::Amber.color()),
            ),
            Span::styled(" warnings", Style::default().fg(Palette::Muted.color())),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{info}"),
                Style::default().fg(Palette::Cyan.color()),
            ),
            Span::styled(" info", Style::default().fg(Palette::Muted.color())),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{suggestion}"),
                Style::default().fg(Palette::Violet.color()),
            ),
            Span::styled(" suggestions", Style::default().fg(Palette::Muted.color())),
        ]),
    ]
}

fn finding_count(report: &AuditReport, severity: Severity) -> usize {
    report
        .findings
        .iter()
        .filter(|finding| finding.severity == severity)
        .count()
}

fn severity_marker(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "crit",
        Severity::Warning => "warn",
        Severity::Info => "info",
        Severity::Suggestion => "tip",
    }
}

fn severity_palette(severity: Severity) -> Palette {
    match severity {
        Severity::Critical => Palette::Red,
        Severity::Warning => Palette::Amber,
        Severity::Info => Palette::Cyan,
        Severity::Suggestion => Palette::Violet,
    }
}

fn stack_label(report: &AuditReport) -> String {
    let stack = report.facts.detected_stack_labels();
    if stack.is_empty() {
        "not detected".to_string()
    } else {
        stack.join(", ")
    }
}

fn agent_surface_label(report: &AuditReport) -> String {
    if report.facts.agent_files.is_empty() {
        "none".to_string()
    } else {
        report
            .facts
            .agent_files
            .iter()
            .map(|file| file.path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn truncate_path(path: &std::path::Path, max: usize) -> String {
    truncate_text(&path.display().to_string(), max)
}

fn default_project_config(report: &AuditReport, profile: &AgentProfile) -> ProjectConfig {
    ProjectConfig {
        version: 1,
        agents: agentdoctor_engine::ProjectAgents {
            enabled: profile.selected_agents.clone(),
        },
        score: ProjectScore { minimum: Some(80) },
        commands: ProjectCommands {
            install: report.facts.commands.install.clone(),
            dev: report.facts.commands.dev.clone(),
            build: report.facts.commands.build.clone(),
            test: report.facts.commands.test.clone(),
            typecheck: report.facts.commands.typecheck.clone(),
            lint: report.facts.commands.lint.clone(),
            format: report.facts.commands.format.clone(),
        },
        paths: ProjectPaths {
            ignore: default_ignore_paths(),
        },
        rules: RuleSettings::default(),
        ..ProjectConfig::default()
    }
}

fn default_ignore_paths() -> Vec<String> {
    ["node_modules", "target", "dist", ".next", "coverage"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn toggle_label(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

fn project_config_path(root: &Path) -> PathBuf {
    root.join(".agentdoctor.toml")
}

fn save_project_config_file(root: &Path, config: &ProjectConfig) -> anyhow::Result<PathBuf> {
    let path = project_config_path(root);
    let text = toml::to_string_pretty(config).context("failed to serialize project config")?;
    fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn load_project_config_file(root: &Path) -> anyhow::Result<Option<ProjectConfig>> {
    let path = project_config_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let config = toml::from_str(&text)
        .with_context(|| format!("failed to parse TOML in {}", path.display()))?;
    Ok(Some(config))
}

fn remove_project_config_file(path: &Path) -> anyhow::Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    Ok(true)
}

fn write_project_file(root: &Path, relative_path: &Path, content: &str) -> anyhow::Result<()> {
    validate_relative_path(relative_path)?;
    let path = root.join(relative_path);
    if path.exists() {
        anyhow::bail!("{} already exists", relative_path.display());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn validate_relative_path(path: &Path) -> anyhow::Result<()> {
    if path.as_os_str().is_empty() {
        anyhow::bail!("empty generated path");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("unsafe generated path {}", path.display());
            }
        }
    }
    Ok(())
}

fn truncate_text(value: &str, max: usize) -> String {
    let count = value.chars().count();
    if count <= max {
        return value.to_string();
    }
    let keep = max.saturating_sub(3);
    let tail = value
        .chars()
        .rev()
        .take(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("...{tail}")
}

fn truncate_end(value: &str, max: usize) -> String {
    let count = value.chars().count();
    if count <= max {
        return value.to_string();
    }
    let keep = max.saturating_sub(3);
    let head = value.chars().take(keep).collect::<String>();
    format!("{head}...")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use agentdoctor_engine::{
        AgentFile, AgentKind, AgentProfile, AuditReport, CiFacts, CommandFacts, EnvFacts, InitPlan,
        Score, ScoreCategory, WorkspaceFacts,
    };
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn render_scan_view_on_designed_canvas() {
        let backend = TestBackend::new(120, 38);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let app = sample_app();

        terminal.draw(|frame| app.render(frame)).expect("render");

        let rendered = rendered_text(terminal.backend().buffer());
        assert!(rendered.contains("score matters."));
    }

    #[test]
    fn render_rescan_state_shows_centered_popup() {
        let backend = TestBackend::new(120, 38);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut app = sample_app();
        app.tick = 3;
        app.rescan_started = Some(Instant::now());

        terminal.draw(|frame| app.render(frame)).expect("render");

        let rendered = rendered_text(terminal.backend().buffer());
        assert!(rendered.contains("Refreshing workspace scan"));
        assert!(rendered.contains("Findings, score, evidence, and init plan are updating."));
    }

    #[test]
    fn ascii_bar_uses_exact_available_width() {
        let line = ascii_bar(5, 10, 12);
        let rendered = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert_eq!(rendered.len(), 12);
        assert_eq!(rendered, "[=====     ]");
    }

    #[test]
    fn scroll_offset_keeps_selected_row_visible() {
        assert_eq!(scroll_offset(0, 20, 5), 0);
        assert_eq!(scroll_offset(4, 20, 5), 0);
        assert_eq!(scroll_offset(5, 20, 5), 1);
        assert_eq!(scroll_offset(19, 20, 5), 15);
    }

    #[test]
    fn centered_app_rect_uses_larger_terminal_space() {
        let rect = centered_app_rect(Rect::new(0, 0, 180, 52));

        assert_eq!(rect, Rect::new(10, 2, APP_MAX_WIDTH, APP_MAX_HEIGHT));
    }

    #[test]
    fn command_path_width_grows_with_available_space() {
        assert!(command_path_width(160) > command_path_width(120));
    }

    #[test]
    fn row_line_uses_available_width_before_truncating_subjects() {
        let rendered = line_text(row_line(
            false,
            "typecheck-or-lint-command",
            "Detected quality command: cargo clippy.",
            "+4 pts",
            Palette::Green,
            80,
        ));

        assert!(rendered.contains("typecheck-or-lint-command"));
        assert!(!rendered.contains("typecheck-or-lin...Detected"));
    }

    #[test]
    fn selected_row_line_highlights_every_span_across_full_width() {
        let width = 96;
        let line = row_line(
            true,
            "agent codex",
            "enabled",
            "agent",
            Palette::Green,
            width,
        );

        assert_eq!(line_text(line.clone()).len(), usize::from(width));
        assert!(line.spans.iter().all(|span| {
            span.style.bg == Some(Palette::Selected.color())
                && span.style.add_modifier.contains(Modifier::BOLD)
        }));
    }

    #[test]
    fn kv_line_keeps_gap_between_label_and_value() {
        let rendered = line_text(kv_line("selected", "codex".to_string()));

        assert!(rendered.contains("selected codex"));
    }

    #[test]
    fn help_key_opens_help_overlay() {
        let mut app = sample_app();

        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));

        assert!(matches!(app.overlay, Some(Overlay::Help)));
    }

    #[test]
    fn enter_opens_detail_for_selected_evidence() {
        let mut app = sample_app();
        app.view = View::Evidence;

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(matches!(
            app.overlay,
            Some(Overlay::Detail { ref title, .. }) if title == "install-command"
        ));
    }

    #[test]
    fn space_toggles_selected_config_agent_and_marks_dirty() {
        let mut app = sample_app();
        app.view = View::Config;
        app.selected_row = 1;

        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        assert_eq!(app.selected_row, 1);
        assert!(app.config_dirty);
        assert!(app.project_config.is_some());
    }

    #[test]
    fn score_minimum_adjustment_keeps_selected_config_row() {
        let mut app = sample_app();
        app.view = View::Config;
        app.selected_row = app
            .config_rows()
            .iter()
            .position(|row| *row == ConfigRow::ScoreMinimum)
            .expect("score row");

        app.handle_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE));

        assert_eq!(app.selected_config_row(), Some(ConfigRow::ScoreMinimum));
        assert!(app.config_dirty);
    }

    #[test]
    fn validate_relative_path_rejects_parent_components() {
        let result = validate_relative_path(Path::new("../AGENTS.md"));

        assert!(result.is_err());
    }

    fn sample_app() -> App {
        App {
            root: PathBuf::from("/tmp/agentdoctor"),
            profile: AgentProfile::default(),
            project_config: None,
            report: sample_report(),
            init_plan: InitPlan {
                changes: Vec::new(),
            },
            view: View::Scan,
            selected_row: 0,
            status: "scan completed - exit 0".to_string(),
            tick: 0,
            rescan_started: None,
            overlay: None,
            config_dirty: false,
        }
    }

    fn rendered_text(buffer: &ratatui::buffer::Buffer) -> String {
        buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    fn line_text(line: Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn sample_report() -> AuditReport {
        AuditReport {
            score: Score {
                total: 92,
                categories: vec![
                    category("Agent files", 25, 25),
                    category("Project-specific detail", 20, 20),
                    category("Commands", 12, 20),
                    category("Safety boundaries", 15, 15),
                    category("Repo hygiene", 10, 10),
                    category("Automation/CI", 10, 10),
                ],
            },
            facts: WorkspaceFacts {
                root: PathBuf::from("/tmp/agentdoctor"),
                files: Vec::new(),
                agent_files: vec![AgentFile {
                    path: PathBuf::from("AGENTS.md"),
                    kind: AgentKind::Generic,
                    size_bytes: 128,
                    content: None,
                }],
                customizations: Default::default(),
                package_managers: vec![agentdoctor_engine::PackageManager::Cargo],
                frameworks: Vec::new(),
                commands: CommandFacts {
                    build: Some("cargo build".to_string()),
                    test: Some("cargo test".to_string()),
                    lint: Some("cargo clippy".to_string()),
                    format: Some("cargo fmt".to_string()),
                    ..CommandFacts::default()
                },
                ci: CiFacts {
                    has_ci: true,
                    has_test: true,
                    has_build: true,
                    has_lint: true,
                    ..CiFacts::default()
                },
                env: EnvFacts::default(),
            },
            findings: Vec::new(),
            recommendations: Vec::new(),
            evidence: vec![
                Evidence::missed("install-command", "Command was not detected."),
                Evidence::missed("dev-command", "Command was not detected."),
                Evidence::awarded("build-command", 4, "Detected command: cargo build."),
                Evidence::awarded("test-command", 4, "Detected command: cargo test."),
                Evidence::awarded(
                    "typecheck-or-lint-command",
                    4,
                    "Detected quality command: cargo clippy.",
                ),
            ],
        }
    }

    fn category(name: &str, earned: u8, max: u8) -> ScoreCategory {
        ScoreCategory {
            name: name.to_string(),
            earned,
            max,
            evidence: Vec::new(),
        }
    }
}
