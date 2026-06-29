#![forbid(unsafe_code)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use agentdoctor_engine::{AgentKind, AgentProfile, ProjectConfig};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const CONFIG_FILE_NAME: &str = "config.toml";
const CONFIG_HOME_ENV: &str = "AGENTDOCTOR_CONFIG_HOME";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to locate platform config directory")]
    MissingConfigDir,
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to remove {path}: {source}")]
    Remove {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse TOML in {path}: {source}")]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to serialize config: {0}")]
    SerializeToml(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalConfig {
    pub version: u32,
    pub onboarding_completed: bool,
    pub selected_agents: Vec<AgentKind>,
    pub default_output: String,
    pub progress: String,
    pub color: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            version: 1,
            onboarding_completed: false,
            selected_agents: vec![AgentKind::Generic],
            default_output: "pretty".to_string(),
            progress: "auto".to_string(),
            color: "auto".to_string(),
        }
    }
}

impl GlobalConfig {
    pub fn completed_with_agents(selected_agents: Vec<AgentKind>) -> Self {
        Self {
            onboarding_completed: true,
            selected_agents: AgentProfile::new(selected_agents).selected_agents,
            ..Self::default()
        }
    }
}

pub fn global_config_path() -> Result<PathBuf, ConfigError> {
    if let Some(path) = std::env::var_os(CONFIG_HOME_ENV) {
        return Ok(config_path_for_home(Path::new(&path)));
    }
    let project_dirs =
        ProjectDirs::from("", "", "agentdoctor").ok_or(ConfigError::MissingConfigDir)?;
    Ok(project_dirs.config_dir().join(CONFIG_FILE_NAME))
}

pub fn config_path_for_home(home: &Path) -> PathBuf {
    home.join(CONFIG_FILE_NAME)
}

pub fn load_global_config() -> Result<Option<GlobalConfig>, ConfigError> {
    let path = global_config_path()?;
    load_global_config_from_path(&path)
}

pub fn load_global_config_from_home(home: &Path) -> Result<Option<GlobalConfig>, ConfigError> {
    load_global_config_from_path(&config_path_for_home(home))
}

fn load_global_config_from_path(path: &Path) -> Result<Option<GlobalConfig>, ConfigError> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let config = toml::from_str(&text).map_err(|source| ConfigError::ParseToml {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some(config))
}

pub fn save_global_config(config: &GlobalConfig) -> Result<PathBuf, ConfigError> {
    let path = global_config_path()?;
    save_global_config_to_path(config, &path)
}

pub fn save_global_config_to_home(
    config: &GlobalConfig,
    home: &Path,
) -> Result<PathBuf, ConfigError> {
    save_global_config_to_path(config, &config_path_for_home(home))
}

fn save_global_config_to_path(config: &GlobalConfig, path: &Path) -> Result<PathBuf, ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let text = toml::to_string_pretty(config)?;
    fs::write(path, text).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(path.to_path_buf())
}

pub fn reset_global_config() -> Result<bool, ConfigError> {
    let path = global_config_path()?;
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(&path).map_err(|source| ConfigError::Remove {
        path: path.clone(),
        source,
    })?;
    Ok(true)
}

pub fn load_project_config(root: &Path) -> Result<Option<ProjectConfig>, ConfigError> {
    let path = root.join(".agentdoctor.toml");
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
        path: path.clone(),
        source,
    })?;
    let config = toml::from_str(&text).map_err(|source| ConfigError::ParseToml {
        path: path.clone(),
        source,
    })?;
    Ok(Some(config))
}

pub fn resolve_profile(
    cli_agents: Option<Vec<AgentKind>>,
    project_config: Option<&ProjectConfig>,
    global_config: Option<&GlobalConfig>,
) -> AgentProfile {
    if let Some(agents) = cli_agents {
        return AgentProfile::new(agents);
    }
    if let Some(agents) = project_config.and_then(ProjectConfig::selected_agents) {
        return AgentProfile::new(agents);
    }
    if let Some(config) = global_config {
        return AgentProfile::new(config.selected_agents.clone());
    }
    AgentProfile::default()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn config_round_trips_through_explicit_test_home() {
        let dir = tempdir().expect("tempdir");
        let config = GlobalConfig::completed_with_agents(vec![AgentKind::Codex, AgentKind::Claude]);

        let path = save_global_config_to_home(&config, dir.path()).expect("save");
        let loaded = load_global_config_from_home(dir.path())
            .expect("load")
            .expect("config exists");

        assert_eq!(path, dir.path().join(CONFIG_FILE_NAME));
        assert_eq!(
            loaded.selected_agents,
            vec![AgentKind::Codex, AgentKind::Claude]
        );
    }

    #[test]
    fn resolve_profile_precedence_is_cli_project_global_default() {
        let global = GlobalConfig::completed_with_agents(vec![AgentKind::Claude]);
        let project = ProjectConfig {
            agents: agentdoctor_engine::ProjectAgents {
                enabled: vec![AgentKind::Cursor],
            },
            ..ProjectConfig::default()
        };

        assert_eq!(
            resolve_profile(Some(vec![AgentKind::Codex]), Some(&project), Some(&global))
                .selected_agents,
            vec![AgentKind::Codex]
        );
        assert_eq!(
            resolve_profile(None, Some(&project), Some(&global)).selected_agents,
            vec![AgentKind::Cursor]
        );
        assert_eq!(
            resolve_profile(None, None, Some(&global)).selected_agents,
            vec![AgentKind::Claude]
        );
        assert_eq!(
            resolve_profile(None, None, None).selected_agents,
            vec![AgentKind::Generic]
        );
    }

    #[test]
    fn load_project_config_reads_agentdoctor_toml() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join(".agentdoctor.toml"),
            "version = 1\n[agents]\nenabled = [\"codex\"]\n",
        )
        .expect("write project config");

        let config = load_project_config(dir.path())
            .expect("load")
            .expect("config");

        assert_eq!(config.selected_agents(), Some(vec![AgentKind::Codex]));
    }
}
