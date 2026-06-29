use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use ignore::WalkBuilder;
use serde_json::Value;

use crate::{
    AgentCustomizationFacts, AgentFile, AgentKind, CiFacts, CommandFacts, EngineError, EnvFacts,
    FileKind, Framework, McpConfigFile, PackageManager, ScanOptions, SkillFile, SkillRoot,
    WorkspaceFacts, WorkspaceFile, path_key,
};

pub fn discover_workspace(
    root: &Path,
    options: &ScanOptions,
) -> Result<WorkspaceFacts, EngineError> {
    if !root.exists() {
        return Err(EngineError::MissingRoot(root.to_path_buf()));
    }
    if !root.is_dir() {
        return Err(EngineError::RootNotDirectory(root.to_path_buf()));
    }

    let mut files = Vec::new();
    let mut agent_files = Vec::new();
    let mut file_keys = BTreeSet::new();

    let mut walker = WalkBuilder::new(root);
    walker.hidden(false).git_ignore(true).git_exclude(true);

    for entry in walker.build() {
        let entry = entry.map_err(|source| EngineError::Walk {
            path: root.to_path_buf(),
            source,
        })?;
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }

        let path = entry.path();
        let rel = relative_path(root, path);
        if should_skip_path(&rel, options.include_hidden) {
            continue;
        }

        let metadata = fs::metadata(path).map_err(|source| EngineError::Metadata {
            path: path.to_path_buf(),
            source,
        })?;
        let size_bytes = metadata.len();
        let kind = classify_file(&rel);
        let workspace_file = WorkspaceFile {
            path: rel.clone(),
            kind,
            size_bytes,
        };
        file_keys.insert(path_key(&rel));

        if let Some(agent_kind) = detect_agent_file(&rel) {
            let content = read_text_limited(path, size_bytes, options.max_file_size_bytes).ok();
            agent_files.push(AgentFile {
                path: rel.clone(),
                kind: agent_kind,
                size_bytes,
                content,
            });
        }

        files.push(workspace_file);
    }

    let package_managers = detect_package_managers(&file_keys);
    let frameworks = detect_frameworks(root, &file_keys, options.max_file_size_bytes)?;
    let commands = detect_commands(
        root,
        &file_keys,
        &package_managers,
        options.max_file_size_bytes,
    )?;
    let ci = detect_ci(root, &file_keys, options.max_file_size_bytes)?;
    let env = detect_env(root, &files, options.max_file_size_bytes)?;
    let customizations = detect_agent_customizations(root, &files, options.max_file_size_bytes)?;

    Ok(WorkspaceFacts {
        root: root.to_path_buf(),
        files,
        agent_files,
        customizations,
        package_managers,
        frameworks,
        commands,
        ci,
        env,
    })
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
}

fn should_skip_path(path: &Path, include_hidden: bool) -> bool {
    let key = path_key(path);
    let skipped_dirs = [
        ".git/",
        "node_modules/",
        "target/",
        ".next/",
        "dist/",
        "coverage/",
    ];
    if skipped_dirs
        .iter()
        .any(|prefix| key == prefix.trim_end_matches('/') || key.starts_with(prefix))
    {
        return true;
    }

    if include_hidden {
        return false;
    }

    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        value.starts_with('.')
            && value != ".github"
            && value != ".agents"
            && value != ".claude"
            && value != ".codex"
            && value != ".cursor"
            && value != ".gemini"
            && value != ".mcp.json"
            && value != ".vscode"
            && value != ".env"
            && !value.starts_with(".env.")
            && value != ".env.example"
            && value != ".gitlab-ci.yml"
    })
}

fn classify_file(path: &Path) -> FileKind {
    let key = path_key(path);
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("md" | "mdc") => FileKind::Markdown,
        Some("json") => FileKind::Json,
        Some("toml") => FileKind::Toml,
        Some("yml" | "yaml") => FileKind::Yaml,
        Some("rs" | "js" | "jsx" | "ts" | "tsx" | "py" | "go" | "java" | "kt" | "swift") => {
            FileKind::Source
        }
        _ if matches!(
            key.as_str(),
            "dockerfile" | "compose.yml" | "docker-compose.yml" | "go.mod" | "requirements.txt"
        ) =>
        {
            FileKind::Config
        }
        _ => FileKind::Other,
    }
}

fn detect_agent_file(path: &Path) -> Option<AgentKind> {
    let key = path_key(path);
    match key.as_str() {
        "agents.md" => Some(AgentKind::Generic),
        "claude.md" => Some(AgentKind::Claude),
        "gemini.md" => Some(AgentKind::Gemini),
        ".github/copilot-instructions.md" => Some(AgentKind::Copilot),
        _ if key.starts_with(".cursor/rules/")
            && (key.ends_with(".mdc") || key.ends_with(".md")) =>
        {
            Some(AgentKind::Cursor)
        }
        _ => None,
    }
}

fn detect_agent_customizations(
    root: &Path,
    files: &[WorkspaceFile],
    max_file_size_bytes: u64,
) -> Result<AgentCustomizationFacts, EngineError> {
    let mut facts = AgentCustomizationFacts::default();

    for file in files {
        let key = path_key(&file.path);

        if let Some(root_kind) = skill_root_for_key(&key) {
            let text = read_optional_text(root, &file.path.to_string_lossy(), max_file_size_bytes)?
                .unwrap_or_default();
            let metadata = parse_skill_metadata(&text);
            let effective_name = metadata
                .name
                .clone()
                .or_else(|| skill_folder_name(&file.path))
                .unwrap_or_else(|| "unknown".to_string());
            facts.skills.push(SkillFile {
                path: file.path.clone(),
                root: root_kind,
                declared_name: metadata.name,
                effective_name,
                description: metadata.description,
                allowed_tools: metadata.allowed_tools,
                size_bytes: file.size_bytes,
            });
            continue;
        }

        if is_instruction_file_key(&key) {
            facts.instruction_files.push(file.path.clone());
            continue;
        }

        if is_prompt_file_key(&key) {
            facts.prompt_files.push(file.path.clone());
            continue;
        }

        if is_custom_agent_file_key(&key) {
            facts.custom_agent_files.push(file.path.clone());
            continue;
        }

        if is_command_file_key(&key) {
            facts.command_files.push(file.path.clone());
            continue;
        }

        if is_hook_file_key(&key) {
            facts.hook_files.push(file.path.clone());
            continue;
        }

        if is_local_settings_key(&key) {
            facts.local_settings.push(file.path.clone());
            continue;
        }

        if is_mcp_config_key(&key) {
            let text = read_optional_text(root, &file.path.to_string_lossy(), max_file_size_bytes)?
                .unwrap_or_default();
            facts.mcp_configs.push(McpConfigFile {
                path: file.path.clone(),
                size_bytes: file.size_bytes,
                has_secret_like_literal: has_secret_like_literal(&text),
            });
        }
    }

    facts.skills.sort_by_key(|skill| path_key(&skill.path));
    facts.instruction_files.sort_by_key(|path| path_key(path));
    facts.prompt_files.sort_by_key(|path| path_key(path));
    facts.custom_agent_files.sort_by_key(|path| path_key(path));
    facts.command_files.sort_by_key(|path| path_key(path));
    facts.hook_files.sort_by_key(|path| path_key(path));
    facts
        .mcp_configs
        .sort_by_key(|config| path_key(&config.path));
    facts.local_settings.sort_by_key(|path| path_key(path));

    Ok(facts)
}

fn skill_root_for_key(key: &str) -> Option<SkillRoot> {
    if !key.ends_with("/skill.md") {
        return None;
    }
    [
        (".agents/skills/", SkillRoot::Codex),
        (".claude/skills/", SkillRoot::Claude),
        (".github/skills/", SkillRoot::Copilot),
        (".cursor/skills/", SkillRoot::Cursor),
        (".codex/skills/", SkillRoot::LegacyCodex),
    ]
    .into_iter()
    .find_map(|(marker, root)| has_path_marker(key, marker).then_some(root))
}

fn is_instruction_file_key(key: &str) -> bool {
    has_path_marker(key, ".github/instructions/") && key.ends_with(".instructions.md")
}

fn is_prompt_file_key(key: &str) -> bool {
    has_path_marker(key, ".github/prompts/") && key.ends_with(".prompt.md")
}

fn is_custom_agent_file_key(key: &str) -> bool {
    (has_path_marker(key, ".github/agents/") && key.ends_with(".md"))
        || (has_path_marker(key, ".claude/agents/") && key.ends_with(".md"))
        || (has_path_marker(key, ".codex/agents/") && key.ends_with(".toml"))
}

fn is_command_file_key(key: &str) -> bool {
    (has_path_marker(key, ".claude/commands/") && key.ends_with(".md"))
        || (has_path_marker(key, ".gemini/commands/")
            && (key.ends_with(".toml") || key.ends_with(".md")))
}

fn is_hook_file_key(key: &str) -> bool {
    has_path_marker(key, ".codex/hooks/")
        || has_path_marker(key, ".claude/hooks/")
        || key.ends_with(".claude/settings.json")
}

fn is_local_settings_key(key: &str) -> bool {
    key.ends_with(".claude/settings.local.json") || key.ends_with(".codex/config.local.toml")
}

fn is_mcp_config_key(key: &str) -> bool {
    key == ".mcp.json"
        || key.ends_with("/.mcp.json")
        || key.ends_with(".vscode/mcp.json")
        || key.ends_with(".cursor/mcp.json")
        || key.ends_with(".claude/mcp.json")
        || key.ends_with(".codex/config.toml")
}

fn has_path_marker(key: &str, marker: &str) -> bool {
    key.starts_with(marker) || key.contains(&format!("/{marker}"))
}

fn skill_folder_name(path: &Path) -> Option<String> {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.eq_ignore_ascii_case("skills"))
        .map(ToString::to_string)
}

#[derive(Default)]
struct SkillMetadata {
    name: Option<String>,
    description: Option<String>,
    allowed_tools: Vec<String>,
}

fn parse_skill_metadata(text: &str) -> SkillMetadata {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return SkillMetadata::default();
    }

    let mut metadata = SkillMetadata::default();
    let mut collecting_allowed_tools = false;
    let mut collecting_description = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }

        if collecting_description {
            if line.starts_with(' ') || line.starts_with('\t') || trimmed.is_empty() {
                append_metadata_text(&mut metadata.description, trimmed);
                continue;
            }
            collecting_description = false;
        }

        if collecting_allowed_tools && trimmed.starts_with('-') {
            if let Some(tool) = trim_metadata_value(trimmed.trim_start_matches('-')) {
                metadata.allowed_tools.push(tool);
            }
            continue;
        }

        collecting_allowed_tools = false;

        let Some((raw_key, raw_value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = raw_key.trim().to_ascii_lowercase();
        match key.as_str() {
            "name" => metadata.name = trim_metadata_value(raw_value),
            "description" => {
                if matches!(raw_value.trim(), ">" | "|") {
                    collecting_description = true;
                    metadata.description = Some(String::new());
                } else {
                    metadata.description = trim_metadata_value(raw_value);
                }
            }
            "allowed-tools" | "allowed_tools" | "tools" => {
                let tools = parse_inline_tools(raw_value);
                collecting_allowed_tools = tools.is_empty() && raw_value.trim().is_empty();
                metadata.allowed_tools.extend(tools);
            }
            _ => {}
        }
    }

    metadata
}

fn parse_inline_tools(value: &str) -> Vec<String> {
    let value = value.trim();
    if value.is_empty() {
        return Vec::new();
    }

    let value = value.trim_start_matches('[').trim_end_matches(']').trim();
    if value.contains(',') {
        value.split(',').filter_map(trim_metadata_value).collect()
    } else {
        value
            .split_whitespace()
            .filter_map(trim_metadata_value)
            .collect()
    }
}

fn trim_metadata_value(value: &str) -> Option<String> {
    let value = value.trim().trim_matches('"').trim_matches('\'').trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn append_metadata_text(slot: &mut Option<String>, value: &str) {
    if value.is_empty() {
        return;
    }
    let existing = slot.get_or_insert_with(String::new);
    if !existing.is_empty() {
        existing.push(' ');
    }
    existing.push_str(value);
}

fn has_secret_like_literal(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if !contains_any(
        &lower,
        &[
            "api_key",
            "apikey",
            "access_token",
            "auth_token",
            "authorization",
            "bearer ",
            "client_secret",
            "password",
            "secret",
            "token",
        ],
    ) {
        return false;
    }

    lower.lines().any(|line| {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with("//") {
            return false;
        }
        let has_assignment = line.contains(':') || line.contains('=');
        let looks_secret_key = contains_any(
            line,
            &[
                "api_key",
                "apikey",
                "access_token",
                "auth_token",
                "authorization",
                "client_secret",
                "password",
                "secret",
                "token",
            ],
        );
        let uses_literal = line.contains('"') || line.contains('\'');
        let uses_env_reference = line.contains("${")
            || line.contains("$env")
            || line.contains("env:")
            || line.contains("process.env");
        has_assignment && looks_secret_key && uses_literal && !uses_env_reference
    })
}

fn detect_package_managers(file_keys: &BTreeSet<String>) -> Vec<PackageManager> {
    let mut managers = Vec::new();
    push_if(
        file_keys.contains("bun.lock"),
        PackageManager::Bun,
        &mut managers,
    );
    push_if(
        file_keys.contains("pnpm-lock.yaml"),
        PackageManager::Pnpm,
        &mut managers,
    );
    push_if(
        file_keys.contains("package-lock.json"),
        PackageManager::Npm,
        &mut managers,
    );
    push_if(
        file_keys.contains("yarn.lock"),
        PackageManager::Yarn,
        &mut managers,
    );
    push_if(
        file_keys.contains("deno.json"),
        PackageManager::Deno,
        &mut managers,
    );
    if managers.is_empty() && file_keys.contains("package.json") {
        managers.push(PackageManager::Npm);
    }
    push_if(
        file_keys.contains("cargo.toml"),
        PackageManager::Cargo,
        &mut managers,
    );
    push_if(
        file_keys.contains("uv.lock"),
        PackageManager::Uv,
        &mut managers,
    );
    push_if(
        file_keys.contains("poetry.lock"),
        PackageManager::Poetry,
        &mut managers,
    );
    if file_keys.contains("requirements.txt") || file_keys.contains("pyproject.toml") {
        push_if(
            !managers.contains(&PackageManager::Poetry),
            PackageManager::Pip,
            &mut managers,
        );
    }
    push_if(
        file_keys.contains("go.mod"),
        PackageManager::Go,
        &mut managers,
    );
    managers
}

fn push_if<T: PartialEq>(condition: bool, value: T, values: &mut Vec<T>) {
    if condition && !values.contains(&value) {
        values.push(value);
    }
}

fn detect_frameworks(
    root: &Path,
    file_keys: &BTreeSet<String>,
    max_file_size_bytes: u64,
) -> Result<Vec<Framework>, EngineError> {
    let mut frameworks = Vec::new();
    push_if(
        file_keys.iter().any(|key| key.starts_with("next.config.")),
        Framework::NextJs,
        &mut frameworks,
    );
    push_if(
        file_keys.iter().any(|key| key.starts_with("astro.config.")),
        Framework::Astro,
        &mut frameworks,
    );
    push_if(
        file_keys.iter().any(|key| key.starts_with("vite.config.")),
        Framework::Vite,
        &mut frameworks,
    );
    push_if(
        file_keys.contains("dockerfile")
            || file_keys.contains("docker-compose.yml")
            || file_keys.contains("compose.yml"),
        Framework::Docker,
        &mut frameworks,
    );
    push_if(
        file_keys.contains("prisma/schema.prisma"),
        Framework::Prisma,
        &mut frameworks,
    );
    push_if(
        file_keys
            .iter()
            .any(|key| key.starts_with("drizzle.config.")),
        Framework::Drizzle,
        &mut frameworks,
    );
    push_if(
        file_keys
            .iter()
            .any(|key| key == "supabase" || key.starts_with("supabase/")),
        Framework::Supabase,
        &mut frameworks,
    );
    push_if(
        file_keys.contains("diesel.toml"),
        Framework::Diesel,
        &mut frameworks,
    );

    if let Some(package_json) = read_json(root, "package.json", max_file_size_bytes)? {
        let deps = dependency_names(&package_json);
        push_if(deps.contains("next"), Framework::NextJs, &mut frameworks);
        push_if(deps.contains("astro"), Framework::Astro, &mut frameworks);
        push_if(deps.contains("vite"), Framework::Vite, &mut frameworks);
        push_if(deps.contains("elysia"), Framework::Elysia, &mut frameworks);
        push_if(deps.contains("prisma"), Framework::Prisma, &mut frameworks);
        push_if(
            deps.contains("drizzle-orm"),
            Framework::Drizzle,
            &mut frameworks,
        );
    }

    if let Some(pyproject) = read_optional_text(root, "pyproject.toml", max_file_size_bytes)? {
        if pyproject.to_ascii_lowercase().contains("fastapi") {
            push_if(true, Framework::FastApi, &mut frameworks);
        }
    }
    if let Some(requirements) = read_optional_text(root, "requirements.txt", max_file_size_bytes)? {
        if requirements.to_ascii_lowercase().contains("fastapi") {
            push_if(true, Framework::FastApi, &mut frameworks);
        }
    }

    Ok(frameworks)
}

fn detect_commands(
    root: &Path,
    file_keys: &BTreeSet<String>,
    package_managers: &[PackageManager],
    max_file_size_bytes: u64,
) -> Result<CommandFacts, EngineError> {
    let mut commands = CommandFacts::default();

    if let Some(package_json) = read_json(root, "package.json", max_file_size_bytes)? {
        let manager = preferred_js_manager(package_managers);
        let runner = js_runner(manager);
        commands.install = Some(js_install_command(manager).to_string());
        if has_script(&package_json, "dev") {
            commands.dev = Some(format!("{runner} run dev"));
        }
        if has_script(&package_json, "build") {
            commands.build = Some(format!("{runner} run build"));
        }
        if has_script(&package_json, "test") {
            commands.test = Some(format!("{runner} run test"));
        }
        if has_script(&package_json, "typecheck") {
            commands.typecheck = Some(format!("{runner} run typecheck"));
        }
        if has_script(&package_json, "lint") {
            commands.lint = Some(format!("{runner} run lint"));
        }
        if has_script(&package_json, "format") {
            commands.format = Some(format!("{runner} run format"));
        }
    }

    if file_keys.contains("cargo.toml") {
        commands
            .build
            .get_or_insert_with(|| "cargo build".to_string());
        commands
            .test
            .get_or_insert_with(|| "cargo test".to_string());
        commands
            .lint
            .get_or_insert_with(|| "cargo clippy".to_string());
        commands
            .format
            .get_or_insert_with(|| "cargo fmt".to_string());
    }

    if file_keys.contains("go.mod") {
        commands
            .build
            .get_or_insert_with(|| "go build ./...".to_string());
        commands
            .test
            .get_or_insert_with(|| "go test ./...".to_string());
    }

    if file_keys.contains("pyproject.toml") || file_keys.contains("requirements.txt") {
        let pyproject = read_optional_text(root, "pyproject.toml", max_file_size_bytes)?
            .unwrap_or_default()
            .to_ascii_lowercase();
        let requirements = read_optional_text(root, "requirements.txt", max_file_size_bytes)?
            .unwrap_or_default()
            .to_ascii_lowercase();
        let python_text = format!("{pyproject}\n{requirements}");
        if python_text.contains("pytest") {
            commands.test.get_or_insert_with(|| "pytest".to_string());
        }
        if python_text.contains("ruff") {
            commands
                .lint
                .get_or_insert_with(|| "ruff check .".to_string());
            commands
                .format
                .get_or_insert_with(|| "ruff format .".to_string());
        }
        if python_text.contains("mypy") {
            commands
                .typecheck
                .get_or_insert_with(|| "mypy .".to_string());
        }
    }

    Ok(commands)
}

fn preferred_js_manager(package_managers: &[PackageManager]) -> PackageManager {
    [
        PackageManager::Bun,
        PackageManager::Pnpm,
        PackageManager::Yarn,
        PackageManager::Npm,
        PackageManager::Deno,
    ]
    .into_iter()
    .find(|manager| package_managers.contains(manager))
    .unwrap_or(PackageManager::Npm)
}

fn js_runner(manager: PackageManager) -> &'static str {
    match manager {
        PackageManager::Bun => "bun",
        PackageManager::Pnpm => "pnpm",
        PackageManager::Yarn => "yarn",
        PackageManager::Deno => "deno task",
        _ => "npm",
    }
}

fn js_install_command(manager: PackageManager) -> &'static str {
    match manager {
        PackageManager::Bun => "bun install",
        PackageManager::Pnpm => "pnpm install",
        PackageManager::Yarn => "yarn install",
        PackageManager::Deno => "deno install",
        _ => "npm install",
    }
}

fn has_script(package_json: &Value, name: &str) -> bool {
    package_json
        .get("scripts")
        .and_then(Value::as_object)
        .is_some_and(|scripts| scripts.contains_key(name))
}

fn dependency_names(package_json: &Value) -> BTreeSet<String> {
    ["dependencies", "devDependencies", "peerDependencies"]
        .into_iter()
        .filter_map(|section| package_json.get(section).and_then(Value::as_object))
        .flat_map(|dependencies| dependencies.keys().map(|key| key.to_ascii_lowercase()))
        .collect()
}

fn detect_ci(
    root: &Path,
    file_keys: &BTreeSet<String>,
    max_file_size_bytes: u64,
) -> Result<CiFacts, EngineError> {
    let workflows: Vec<PathBuf> = file_keys
        .iter()
        .filter(|key| {
            (key.starts_with(".github/workflows/")
                && (key.ends_with(".yml") || key.ends_with(".yaml")))
                || *key == ".gitlab-ci.yml"
        })
        .map(PathBuf::from)
        .collect();

    let mut facts = CiFacts {
        has_ci: !workflows.is_empty(),
        workflows,
        ..CiFacts::default()
    };

    for workflow in &facts.workflows {
        let Some(text) =
            read_optional_text(root, &workflow.to_string_lossy(), max_file_size_bytes)?
        else {
            continue;
        };
        let lower = text.to_ascii_lowercase();
        facts.has_test |= contains_any(
            &lower,
            &[
                "cargo test",
                "npm test",
                "npm run test",
                "pnpm test",
                "pnpm run test",
                "bun test",
                "bun run test",
                "pytest",
                "go test",
            ],
        );
        facts.has_build |= contains_any(
            &lower,
            &[
                "cargo build",
                "npm run build",
                "pnpm run build",
                "bun run build",
                "go build",
            ],
        );
        facts.has_lint |= contains_any(
            &lower,
            &[
                "cargo clippy",
                "npm run lint",
                "pnpm run lint",
                "bun run lint",
                "ruff check",
                "eslint",
            ],
        );
    }

    Ok(facts)
}

fn detect_env(
    root: &Path,
    files: &[WorkspaceFile],
    max_file_size_bytes: u64,
) -> Result<EnvFacts, EngineError> {
    let mut facts = EnvFacts::default();
    for file in files {
        let key = path_key(&file.path);
        if key == ".env.example" || key.ends_with("/.env.example") {
            facts.has_env_example = true;
            facts.uses_env = true;
            continue;
        }
        if is_secret_env_file(&key) {
            facts.uses_env = true;
            continue;
        }
        if !matches!(
            file.kind,
            FileKind::Source | FileKind::Json | FileKind::Toml | FileKind::Config
        ) {
            continue;
        }
        let Some(text) =
            read_optional_text(root, &file.path.to_string_lossy(), max_file_size_bytes)?
        else {
            continue;
        };
        let lower = text.to_ascii_lowercase();
        if contains_any(
            &lower,
            &[
                "process.env",
                "import.meta.env",
                "std::env::",
                "os.environ",
                "getenv(",
                "dotenv",
                "database_url",
            ],
        ) {
            facts.uses_env = true;
        }
    }
    Ok(facts)
}

fn is_secret_env_file(key: &str) -> bool {
    key == ".env" || key.ends_with("/.env") || key.starts_with(".env.")
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn read_json(
    root: &Path,
    relative: &str,
    max_file_size_bytes: u64,
) -> Result<Option<Value>, EngineError> {
    let Some(text) = read_optional_text(root, relative, max_file_size_bytes)? else {
        return Ok(None);
    };
    Ok(serde_json::from_str(&text).ok())
}

fn read_optional_text(
    root: &Path,
    relative: &str,
    max_file_size_bytes: u64,
) -> Result<Option<String>, EngineError> {
    let path = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::metadata(&path).map_err(|source| EngineError::Metadata {
        path: path.clone(),
        source,
    })?;
    match read_text_limited(&path, metadata.len(), max_file_size_bytes) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => Ok(None),
        Err(source) => Err(EngineError::Read { path, source }),
    }
}

fn read_text_limited(
    path: &Path,
    size_bytes: u64,
    max_file_size_bytes: u64,
) -> Result<String, std::io::Error> {
    if size_bytes > max_file_size_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file exceeds maximum readable size",
        ));
    }
    let bytes = fs::read(path)?;
    String::from_utf8(bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn detects_rust_commands_without_reading_env_values() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .expect("write Cargo.toml");
        fs::write(dir.path().join(".env"), "DATABASE_URL=secret").expect("write env");

        let facts = discover_workspace(dir.path(), &ScanOptions::default()).expect("scan");

        assert_eq!(facts.commands.test.as_deref(), Some("cargo test"));
        assert!(facts.env.uses_env);
        assert!(!facts.env.has_env_example);
        assert!(
            facts
                .files
                .iter()
                .any(|file| path_key(&file.path) == ".env")
        );
    }

    #[test]
    fn includes_agent_files_from_hidden_tool_dirs() {
        let dir = tempdir().expect("tempdir");
        let cursor_dir = dir.path().join(".cursor").join("rules");
        fs::create_dir_all(&cursor_dir).expect("create cursor");
        fs::write(cursor_dir.join("project.mdc"), "Read AGENTS.md").expect("write cursor rule");

        let facts = discover_workspace(dir.path(), &ScanOptions::default()).expect("scan");

        assert!(facts.has_any_agent_file(".cursor/rules/project.mdc"));
    }

    #[test]
    fn detects_modern_agent_customization_files() {
        let dir = tempdir().expect("tempdir");
        let skill_dir = dir.path().join(".agents").join("skills").join("release");
        let instruction_dir = dir.path().join(".github").join("instructions");
        let prompt_dir = dir.path().join(".github").join("prompts");
        let command_dir = dir.path().join(".claude").join("commands");
        fs::create_dir_all(&skill_dir).expect("create skill");
        fs::create_dir_all(&instruction_dir).expect("create instructions");
        fs::create_dir_all(&prompt_dir).expect("create prompts");
        fs::create_dir_all(&command_dir).expect("create commands");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: release\n// ignored by parser\ndescription: >\n  Prepare release notes\n  and release checks.\nallowed-tools: Bash(cargo:*) Read Grep\n---\n",
        )
        .expect("write skill");
        fs::write(
            instruction_dir.join("project.instructions.md"),
            "Use workspace conventions.",
        )
        .expect("write instructions");
        fs::write(prompt_dir.join("review.prompt.md"), "Review this change.")
            .expect("write prompt");
        fs::write(command_dir.join("triage.md"), "Triage issue.").expect("write command");
        fs::write(
            dir.path().join(".mcp.json"),
            r#"{"env":{"API_KEY":"${API_KEY}"}}"#,
        )
        .expect("write mcp");

        let facts = discover_workspace(dir.path(), &ScanOptions::default()).expect("scan");

        assert_eq!(facts.customizations.skills.len(), 1);
        assert_eq!(facts.customizations.skills[0].root, SkillRoot::Codex);
        assert_eq!(
            facts.customizations.skills[0].declared_name.as_deref(),
            Some("release")
        );
        assert_eq!(
            facts.customizations.skills[0].description.as_deref(),
            Some("Prepare release notes and release checks.")
        );
        assert_eq!(
            facts.customizations.skills[0].allowed_tools,
            vec!["Bash(cargo:*)", "Read", "Grep"]
        );
        assert!(!facts.customizations.skills[0].preapproves_shell());
        assert_eq!(facts.customizations.instruction_files.len(), 1);
        assert_eq!(facts.customizations.prompt_files.len(), 1);
        assert_eq!(facts.customizations.command_files.len(), 1);
        assert_eq!(facts.customizations.mcp_configs.len(), 1);
        assert!(!facts.customizations.mcp_configs[0].has_secret_like_literal);
    }

    #[test]
    fn detects_package_scripts_and_frameworks() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("package.json"),
            r#"{
              "scripts": {"dev": "next dev", "build": "next build", "test": "vitest"},
              "dependencies": {"next": "latest"}
            }"#,
        )
        .expect("write package");

        let facts = discover_workspace(dir.path(), &ScanOptions::default()).expect("scan");

        assert!(facts.frameworks.contains(&Framework::NextJs));
        assert_eq!(facts.commands.build.as_deref(), Some("npm run build"));
        assert_eq!(facts.commands.test.as_deref(), Some("npm run test"));
    }
}
