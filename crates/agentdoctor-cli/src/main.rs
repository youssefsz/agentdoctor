#![forbid(unsafe_code)]

mod lifecycle;
use std::{error::Error, fmt, io::IsTerminal, path::PathBuf, process::ExitCode, str::FromStr};

use agentdoctor_config::{
    ConfigError, GlobalConfig, load_global_config, load_project_config, reset_global_config,
    resolve_profile, save_global_config,
};
use agentdoctor_engine::{
    AgentKind, AgentProfile, InitInput, PlannedChange, ProjectConfig, ScanInput, ScanOptions,
    agent_specs, generate_init_plan, scan_workspace,
};
use agentdoctor_report::{render_json, render_pretty};
use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use inquire::MultiSelect;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(exit_code_for(&error))
        }
    }
}

fn run() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        None => run_default(cli.no_interactive),
        Some(command) => run_command(command, cli.no_interactive),
    }
}

fn run_command(command: Commands, no_interactive: bool) -> anyhow::Result<ExitCode> {
    match command {
        Commands::Scan {
            path,
            format,
            agents,
            no_progress: _,
        } => run_scan(path, format, agents, no_interactive),
        Commands::Init {
            path,
            agents,
            dry_run,
        } => run_init(path, agents, dry_run, no_interactive),
        Commands::Config { command } => run_config(command),
        Commands::Upgrade { repo, force } => lifecycle::run_upgrade(repo, force),
        Commands::Uninstall { yes, remove_config } => {
            lifecycle::run_uninstall(yes, remove_config, no_interactive)
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "agentdoctor")]
#[command(version, about = "Audit repositories for AI coding agent readiness.")]
struct Cli {
    #[arg(long, global = true)]
    no_interactive: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "Scan a repository for AI coding agent readiness.")]
    Scan {
        /// Repository path to scan. Defaults to the current directory.
        path: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Pretty)]
        format: OutputFormat,
        /// Comma-separated selected agents: codex, claude, cursor, copilot, gemini, generic.
        #[arg(long)]
        agents: Option<String>,
        /// Disable progress output.
        #[arg(long)]
        no_progress: bool,
    },
    #[command(about = "Preview generated setup files without writing them.")]
    Init {
        /// Repository path. Defaults to the current directory.
        path: Option<PathBuf>,
        /// Comma-separated selected agents: codex, claude, cursor, copilot, gemini, generic.
        #[arg(long)]
        agents: Option<String>,
        /// Print the init plan without writing files. Required in v0.1.
        #[arg(long)]
        dry_run: bool,
    },
    #[command(about = "Manage AgentDoctor global configuration.")]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    #[command(about = "Upgrade AgentDoctor from the latest GitHub Release.")]
    Upgrade {
        /// Release repository in owner/name form.
        #[arg(long)]
        repo: Option<String>,
        /// Reinstall even when the current version is already latest.
        #[arg(long)]
        force: bool,
    },
    #[command(about = "Uninstall the current AgentDoctor executable.")]
    Uninstall {
        /// Skip the confirmation prompt. Required with --no-interactive.
        #[arg(long)]
        yes: bool,
        /// Also remove global AgentDoctor config.
        #[arg(long)]
        remove_config: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    #[command(about = "Show global config as TOML.")]
    Show,
    #[command(about = "Show or set selected global agents.")]
    Agents {
        /// Comma-separated selected agents to save globally.
        #[arg(long)]
        set: Option<String>,
    },
    #[command(about = "Remove global config.")]
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Pretty,
    Json,
}

fn run_default(no_interactive: bool) -> anyhow::Result<ExitCode> {
    if should_launch_tui(no_interactive) {
        let root = std::env::current_dir().context("failed to read current directory")?;
        let project_config = load_project_config(&root)?;
        let global_config = load_global_config()?;
        let profile = resolve_profile(None, project_config.as_ref(), global_config.as_ref());
        return agentdoctor_tui::run(root, profile, project_config);
    }

    run_scan(None, OutputFormat::Pretty, None, no_interactive)
}

fn should_launch_tui(no_interactive: bool) -> bool {
    !no_interactive && !is_ci() && std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn run_scan(
    path: Option<PathBuf>,
    format: OutputFormat,
    agents: Option<String>,
    no_interactive: bool,
) -> anyhow::Result<ExitCode> {
    let root = path.unwrap_or(std::env::current_dir().context("failed to read current directory")?);
    let project_config = load_project_config(&root)?;
    let cli_agents = parse_agent_list(agents)?;
    let global_config = load_or_onboard_global_config(
        cli_agents.is_none(),
        project_config.as_ref(),
        no_interactive,
        format,
    )?;
    let profile = resolve_profile(cli_agents, project_config.as_ref(), global_config.as_ref());
    let report = scan_workspace(ScanInput {
        root,
        profile,
        project_config: project_config.clone(),
        options: ScanOptions::default(),
    })?;

    match format {
        OutputFormat::Pretty => {
            println!("{}", render_pretty(&report));
        }
        OutputFormat::Json => {
            println!("{}", render_json(&report)?);
        }
    }

    if let Some(minimum) = project_config
        .as_ref()
        .and_then(|config| config.score.minimum)
        .filter(|minimum| report.score.total < *minimum)
    {
        eprintln!(
            "score {} is below configured minimum {minimum}",
            report.score.total
        );
        return Ok(ExitCode::from(1));
    }

    Ok(ExitCode::SUCCESS)
}

fn run_init(
    path: Option<PathBuf>,
    agents: Option<String>,
    dry_run: bool,
    no_interactive: bool,
) -> anyhow::Result<ExitCode> {
    if !dry_run {
        return Err(
            UsageError("v0.1 supports only `agentdoctor init --dry-run`.".to_string()).into(),
        );
    }

    let root = path.unwrap_or(std::env::current_dir().context("failed to read current directory")?);
    let project_config = load_project_config(&root)?;
    let cli_agents = parse_agent_list(agents)?;
    let global_config = load_or_onboard_global_config(
        cli_agents.is_none(),
        project_config.as_ref(),
        no_interactive,
        OutputFormat::Pretty,
    )?;
    let profile = resolve_profile(cli_agents, project_config.as_ref(), global_config.as_ref());
    let plan = generate_init_plan(InitInput {
        root,
        profile,
        project_config,
        options: ScanOptions::default(),
    })?;

    println!("AgentDoctor init dry run\n");
    for change in plan.changes {
        match change {
            PlannedChange::CreateFile { path, content } => {
                println!("Create {}", path.display());
                println!("--- {} ---", path.display());
                println!("{content}");
            }
            PlannedChange::SkipExisting { path, reason } => {
                println!("Skip {}: {reason}", path.display());
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn run_config(command: ConfigCommand) -> anyhow::Result<ExitCode> {
    match command {
        ConfigCommand::Show => {
            if let Some(config) = load_global_config()? {
                println!("{}", toml::to_string_pretty(&config)?);
            } else {
                println!("No global config found.");
            }
        }
        ConfigCommand::Agents { set } => {
            if let Some(value) = set {
                let agents = parse_agent_list(Some(value))?
                    .ok_or_else(|| UsageError("agent list cannot be empty".to_string()))?;
                let config = GlobalConfig::completed_with_agents(agents);
                let path = save_global_config(&config)?;
                println!("Saved selected agents to {}", path.display());
            } else if let Some(config) = load_global_config()? {
                println!("{}", format_agents(&config.selected_agents));
            } else {
                println!("{}", AgentKind::Generic.as_str());
            }
        }
        ConfigCommand::Reset => {
            if reset_global_config()? {
                println!("Global config reset.");
            } else {
                println!("No global config found.");
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn load_or_onboard_global_config(
    may_onboard: bool,
    project_config: Option<&ProjectConfig>,
    no_interactive: bool,
    format: OutputFormat,
) -> anyhow::Result<Option<GlobalConfig>> {
    let global_config = load_global_config()?;
    if global_config.is_some()
        || !may_onboard
        || project_config
            .and_then(ProjectConfig::selected_agents)
            .is_some()
        || !should_prompt(no_interactive, format)
    {
        return Ok(global_config);
    }

    let selected = prompt_for_agents()?;
    let config = GlobalConfig::completed_with_agents(selected);
    save_global_config(&config)?;
    Ok(Some(config))
}

fn should_prompt(no_interactive: bool, format: OutputFormat) -> bool {
    !no_interactive
        && format != OutputFormat::Json
        && !is_ci()
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
}

fn is_ci() -> bool {
    std::env::var("CI")
        .map(|value| {
            let lower = value.to_ascii_lowercase();
            lower == "1" || lower == "true"
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
struct PromptAgent {
    kind: AgentKind,
    label: String,
}

impl fmt::Display for PromptAgent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

fn prompt_for_agents() -> anyhow::Result<Vec<AgentKind>> {
    let options = agent_specs()
        .iter()
        .map(|spec| PromptAgent {
            kind: spec.kind,
            label: format!("{} / {}", spec.display_name, spec.files[0].path),
        })
        .collect::<Vec<_>>();

    let selected = MultiSelect::new("Which AI coding agents do you use?", options)
        .with_default(&[5])
        .prompt()
        .context("failed to read onboarding selection")?;
    let agents = selected.into_iter().map(|agent| agent.kind).collect();
    Ok(AgentProfile::new(agents).selected_agents)
}

fn parse_agent_list(value: Option<String>) -> anyhow::Result<Option<Vec<AgentKind>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let mut agents = Vec::new();
    for raw in value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let agent = AgentKind::from_str(raw).map_err(UsageError)?;
        if !agents.contains(&agent) {
            agents.push(agent);
        }
    }
    if agents.is_empty() {
        return Err(UsageError("agent list cannot be empty".to_string()).into());
    }
    Ok(Some(agents))
}

fn format_agents(agents: &[AgentKind]) -> String {
    agents
        .iter()
        .map(|agent| agent.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn exit_code_for(error: &anyhow::Error) -> u8 {
    if error.downcast_ref::<UsageError>().is_some() {
        2
    } else if error.downcast_ref::<ConfigError>().is_some()
        || error
            .downcast_ref::<agentdoctor_engine::EngineError>()
            .is_some()
    {
        3
    } else {
        4
    }
}

#[derive(Debug)]
struct UsageError(String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for UsageError {}
