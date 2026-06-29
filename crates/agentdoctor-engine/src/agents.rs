use std::path::PathBuf;

use crate::{AgentFileSpec, AgentKind, AgentProfile, AgentSpec};

const AGENTS_FILE: &[AgentFileSpec] = &[AgentFileSpec {
    path: "AGENTS.md",
    required: true,
}];

const CLAUDE_FILES: &[AgentFileSpec] = &[AgentFileSpec {
    path: "CLAUDE.md",
    required: true,
}];

const CURSOR_FILES: &[AgentFileSpec] = &[AgentFileSpec {
    path: ".cursor/rules/project.mdc",
    required: true,
}];

const COPILOT_FILES: &[AgentFileSpec] = &[AgentFileSpec {
    path: ".github/copilot-instructions.md",
    required: true,
}];

const GEMINI_FILES: &[AgentFileSpec] = &[AgentFileSpec {
    path: "GEMINI.md",
    required: true,
}];

const SPECS: &[AgentSpec] = &[
    AgentSpec {
        kind: AgentKind::Codex,
        display_name: "Codex",
        description: "Reads repository instructions from AGENTS.md.",
        files: AGENTS_FILE,
    },
    AgentSpec {
        kind: AgentKind::Claude,
        display_name: "Claude Code",
        description: "Uses CLAUDE.md as repository memory.",
        files: CLAUDE_FILES,
    },
    AgentSpec {
        kind: AgentKind::Cursor,
        display_name: "Cursor",
        description: "Uses project rules from .cursor/rules.",
        files: CURSOR_FILES,
    },
    AgentSpec {
        kind: AgentKind::Copilot,
        display_name: "GitHub Copilot",
        description: "Uses repository custom instructions.",
        files: COPILOT_FILES,
    },
    AgentSpec {
        kind: AgentKind::Gemini,
        display_name: "Gemini CLI",
        description: "Uses GEMINI.md as project context.",
        files: GEMINI_FILES,
    },
    AgentSpec {
        kind: AgentKind::Generic,
        display_name: "Generic agent",
        description: "Reads repository instructions from AGENTS.md.",
        files: AGENTS_FILE,
    },
];

pub fn agent_specs() -> &'static [AgentSpec] {
    SPECS
}

pub fn required_agent_paths(profile: &AgentProfile) -> Vec<(AgentKind, PathBuf)> {
    let mut paths = Vec::new();
    for selected in &profile.selected_agents {
        if let Some(spec) = SPECS.iter().find(|spec| spec.kind == *selected) {
            for file in spec.files.iter().filter(|file| file.required) {
                let path = PathBuf::from(file.path);
                if !paths.iter().any(|(_, existing)| existing == &path) {
                    paths.push((*selected, path));
                }
            }
        }
    }
    paths
}
