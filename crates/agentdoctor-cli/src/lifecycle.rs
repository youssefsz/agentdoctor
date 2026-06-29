use std::{
    env,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use agentdoctor_config::reset_global_config;
use anyhow::{Context, anyhow, bail};
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use inquire::Confirm;
use semver::Version;
use serde::Deserialize;
use tar::Archive;
use zip::ZipArchive;

use crate::UsageError;

const DEFAULT_REPO: &str = "youssefsz/agentdoctor";
const USER_AGENT: &str = concat!("agentdoctor/", env!("CARGO_PKG_VERSION"));

pub fn run_upgrade(repo: Option<String>, force: bool) -> anyhow::Result<ExitCode> {
    let repo = repo
        .or_else(|| env::var("AGENTDOCTOR_REPO").ok())
        .unwrap_or_else(|| DEFAULT_REPO.to_string());
    validate_repo(&repo)?;

    let target = release_target()?;
    let current = parse_version(env!("CARGO_PKG_VERSION"))?;
    let work_dir = create_temp_dir("agentdoctor-upgrade")?;
    let release = fetch_latest_release(&repo, &work_dir)?;
    let latest = parse_version(&release.tag_name)?;

    if latest <= current && !force {
        println!("AgentDoctor is already up to date ({current}).");
        let _ = fs::remove_dir_all(&work_dir);
        return Ok(ExitCode::SUCCESS);
    }

    let asset = select_asset(&release, &target).ok_or_else(|| {
        anyhow!(
            "could not find release asset for target {} in {}",
            target.triple,
            repo
        )
    })?;

    let archive_path = work_dir.join(&asset.name);
    download_asset(&asset.browser_download_url, &archive_path)?;
    let new_binary = extract_binary(&archive_path, &target, &work_dir)?;

    let spinner = spinner("Installing update...");
    self_replace::self_replace(&new_binary).context("failed to replace current executable")?;
    let _ = fs::remove_file(&new_binary);
    spinner.finish_with_message("Installed update.");
    let _ = fs::remove_dir_all(&work_dir);

    if force && latest <= current {
        println!(
            "AgentDoctor {current} reinstalled from {}.",
            release.tag_name
        );
    } else {
        println!("AgentDoctor upgraded from {current} to {latest}.");
    }
    Ok(ExitCode::SUCCESS)
}

pub fn run_uninstall(
    yes: bool,
    remove_config: bool,
    no_interactive: bool,
) -> anyhow::Result<ExitCode> {
    let exe = env::current_exe().context("failed to locate current executable")?;
    if !yes {
        if no_interactive {
            return Err(UsageError(
                "uninstall requires --yes when --no-interactive is set".to_string(),
            )
            .into());
        }
        let confirmed = Confirm::new(&format!("Uninstall AgentDoctor from {}?", exe.display()))
            .with_default(false)
            .prompt()
            .context("failed to read uninstall confirmation")?;
        if !confirmed {
            println!("Uninstall cancelled.");
            return Ok(ExitCode::SUCCESS);
        }
    }

    let spinner = spinner("Uninstalling AgentDoctor...");
    if remove_config {
        let _ = reset_global_config()?;
    }
    self_replace::self_delete().context("failed to remove current executable")?;
    spinner.finish_with_message("AgentDoctor uninstalled.");

    println!("Removed {}", exe.display());
    if remove_config {
        println!("Removed global AgentDoctor config when it existed.");
    } else {
        println!("Global config was kept. Use --remove-config to remove it during uninstall.");
    }
    println!("PATH entries are left unchanged because install directories can be shared.");
    if cfg!(windows) {
        println!("On Windows, the executable is removed after this process exits.");
    }

    Ok(ExitCode::SUCCESS)
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReleaseTarget {
    triple: &'static str,
    archive: ArchiveKind,
    binary: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    TarGz,
    Zip,
}

impl ArchiveKind {
    const fn extension(self) -> &'static str {
        match self {
            Self::TarGz => "tar.gz",
            Self::Zip => "zip",
        }
    }
}

fn fetch_latest_release(repo: &str, work_dir: &Path) -> anyhow::Result<GitHubRelease> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let json_path = work_dir.join("latest-release.json");
    fetch_url_to_file(&url, &json_path, "Checking latest release...")?;
    let text = fs::read_to_string(&json_path)
        .with_context(|| format!("failed to read {}", json_path.display()))?;
    let release = serde_json::from_str::<GitHubRelease>(&text)
        .map_err(|error| anyhow!("failed to parse latest release response: {error}"))?;
    eprintln!("Latest release: {}", release.tag_name);
    Ok(release)
}

fn download_asset(url: &str, destination: &Path) -> anyhow::Result<()> {
    fetch_url_to_file(url, destination, "Downloading release asset...")
}

fn extract_binary(
    archive_path: &Path,
    target: &ReleaseTarget,
    work_dir: &Path,
) -> anyhow::Result<PathBuf> {
    let spinner = spinner("Extracting release archive...");
    let destination = work_dir.join(target.binary);
    if destination.exists() {
        fs::remove_file(&destination).with_context(|| {
            format!(
                "failed to remove existing temporary binary {}",
                destination.display()
            )
        })?;
    }

    match target.archive {
        ArchiveKind::Zip => extract_zip_binary(archive_path, target.binary, &destination)?,
        ArchiveKind::TarGz => extract_tar_binary(archive_path, target.binary, &destination)?,
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o755)).with_context(
            || {
                format!(
                    "failed to mark extracted binary executable: {}",
                    destination.display()
                )
            },
        )?;
    }

    spinner.finish_with_message("Extracted release archive.");
    Ok(destination)
}

fn extract_zip_binary(
    archive_path: &Path,
    binary_name: &str,
    destination: &Path,
) -> anyhow::Result<()> {
    let archive = File::open(archive_path)
        .with_context(|| format!("failed to open {}", archive_path.display()))?;
    let mut archive = ZipArchive::new(archive).context("failed to read zip archive")?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .context("failed to read zip archive entry")?;
        if !entry.is_file() || archive_leaf_name(entry.name()) != Some(binary_name) {
            continue;
        }
        let mut output = File::create(destination)
            .with_context(|| format!("failed to create {}", destination.display()))?;
        io::copy(&mut entry, &mut output).context("failed to extract binary from zip archive")?;
        return Ok(());
    }

    bail!("release archive did not contain {binary_name}");
}

fn extract_tar_binary(
    archive_path: &Path,
    binary_name: &str,
    destination: &Path,
) -> anyhow::Result<()> {
    let archive = File::open(archive_path)
        .with_context(|| format!("failed to open {}", archive_path.display()))?;
    let decoder = GzDecoder::new(archive);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries().context("failed to read tar archive")? {
        let mut entry = entry.context("failed to read tar archive entry")?;
        let path = entry.path().context("failed to read tar entry path")?;
        if path.file_name().and_then(|name| name.to_str()) != Some(binary_name) {
            continue;
        }
        entry
            .unpack(destination)
            .with_context(|| format!("failed to extract {}", destination.display()))?;
        return Ok(());
    }

    bail!("release archive did not contain {binary_name}");
}

fn select_asset<'a>(release: &'a GitHubRelease, target: &ReleaseTarget) -> Option<&'a GitHubAsset> {
    let suffix = format!("{}.", target.triple) + target.archive.extension();
    release
        .assets
        .iter()
        .find(|asset| asset.name.starts_with("agentdoctor-") && asset.name.ends_with(&suffix))
}

fn release_target() -> anyhow::Result<ReleaseTarget> {
    release_target_for(env::consts::OS, env::consts::ARCH)
}

fn release_target_for(os: &str, arch: &str) -> anyhow::Result<ReleaseTarget> {
    match (os, arch) {
        ("windows", "x86_64") => Ok(ReleaseTarget {
            triple: "x86_64-pc-windows-gnu",
            archive: ArchiveKind::Zip,
            binary: "agentdoctor.exe",
        }),
        ("linux", "x86_64") => Ok(ReleaseTarget {
            triple: "x86_64-unknown-linux-gnu",
            archive: ArchiveKind::TarGz,
            binary: "agentdoctor",
        }),
        ("linux", "aarch64") => Ok(ReleaseTarget {
            triple: "aarch64-unknown-linux-gnu",
            archive: ArchiveKind::TarGz,
            binary: "agentdoctor",
        }),
        ("macos", "x86_64") => Ok(ReleaseTarget {
            triple: "x86_64-apple-darwin",
            archive: ArchiveKind::TarGz,
            binary: "agentdoctor",
        }),
        ("macos", "aarch64") => Ok(ReleaseTarget {
            triple: "aarch64-apple-darwin",
            archive: ArchiveKind::TarGz,
            binary: "agentdoctor",
        }),
        _ => bail!("unsupported platform for upgrade: {os}/{arch}"),
    }
}

fn parse_version(value: &str) -> anyhow::Result<Version> {
    Version::parse(value.trim().trim_start_matches('v'))
        .with_context(|| format!("failed to parse version '{value}'"))
}

fn validate_repo(repo: &str) -> anyhow::Result<()> {
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err(UsageError("repo must use the form owner/name".to_string()).into());
    }
    Ok(())
}

fn fetch_url_to_file(url: &str, destination: &Path, message: &str) -> anyhow::Result<()> {
    let progress = spinner(message);
    let output = download_command(url, destination)
        .with_context(|| format!("failed to start download for {url}"))?
        .output()
        .with_context(|| format!("failed to run download command for {url}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        progress.finish_and_clear();
        bail!(
            "download command failed for {url}: {}",
            stderr.trim().if_empty("no error output")
        );
    }
    progress.finish_with_message(format!("Saved {}", destination.display()));
    Ok(())
}

fn download_command(url: &str, destination: &Path) -> anyhow::Result<Command> {
    #[cfg(windows)]
    {
        let shell = if command_exists("powershell.exe") {
            "powershell.exe"
        } else if command_exists("pwsh.exe") {
            "pwsh.exe"
        } else {
            bail!("PowerShell was not found; cannot download release files");
        };
        let mut command = Command::new(shell);
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &format!(
                "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; Invoke-WebRequest -Uri '{}' -OutFile '{}' -Headers @{{ 'User-Agent' = '{}' }}",
                escape_powershell_single_quoted(url),
                escape_powershell_single_quoted(&destination.to_string_lossy()),
                escape_powershell_single_quoted(USER_AGENT)
            ),
        ]);
        command.stdout(Stdio::null()).stderr(Stdio::piped());
        Ok(command)
    }

    #[cfg(not(windows))]
    {
        if !command_exists("curl") {
            bail!("curl was not found; cannot download release files");
        }
        let mut command = Command::new("curl");
        command.args(["-fsSL", "-A", USER_AGENT, "-o"]);
        command.arg(destination);
        command.arg(url);
        command.stdout(Stdio::null()).stderr(Stdio::piped());
        Ok(command)
    }
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg(if cfg!(windows) { "/?" } else { "--version" })
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

#[cfg(windows)]
fn escape_powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

fn create_temp_dir(prefix: &str) -> anyhow::Result<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = env::temp_dir().join(format!("{prefix}-{}-{millis}", std::process::id()));
    fs::create_dir_all(&path)
        .with_context(|| format!("failed to create temporary directory {}", path.display()))?;
    Ok(path)
}

fn spinner(message: impl Into<String>) -> ProgressBar {
    let progress = ProgressBar::new_spinner();
    progress.set_style(
        ProgressStyle::with_template("{spinner:.green} {wide_msg}")
            .expect("spinner template should be valid")
            .tick_strings(&["-", "\\", "|", "/"]),
    );
    progress.set_message(message.into());
    progress.enable_steady_tick(Duration::from_millis(80));
    progress
}

fn archive_leaf_name(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
}

trait EmptyString {
    fn if_empty<'a>(&'a self, fallback: &'a str) -> &'a str;
}

impl EmptyString for str {
    fn if_empty<'a>(&'a self, fallback: &'a str) -> &'a str {
        if self.is_empty() { fallback } else { self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_tagged_versions() {
        assert_eq!(parse_version("0.1.0").expect("plain").to_string(), "0.1.0");
        assert_eq!(
            parse_version("v0.2.3").expect("tagged").to_string(),
            "0.2.3"
        );
    }

    #[test]
    fn maps_supported_release_targets() {
        assert_eq!(
            release_target_for("windows", "x86_64")
                .expect("windows")
                .triple,
            "x86_64-pc-windows-gnu"
        );
        assert_eq!(
            release_target_for("linux", "aarch64")
                .expect("linux arm")
                .triple,
            "aarch64-unknown-linux-gnu"
        );
        assert!(release_target_for("windows", "aarch64").is_err());
    }

    #[test]
    fn selects_matching_release_asset() {
        let release = GitHubRelease {
            tag_name: "v0.1.0".to_string(),
            assets: vec![
                GitHubAsset {
                    name: "agentdoctor-v0.1.0-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                    browser_download_url: "https://example.com/linux".to_string(),
                },
                GitHubAsset {
                    name: "agentdoctor-v0.1.0-x86_64-pc-windows-gnu.zip".to_string(),
                    browser_download_url: "https://example.com/windows".to_string(),
                },
            ],
        };
        let target = ReleaseTarget {
            triple: "x86_64-pc-windows-gnu",
            archive: ArchiveKind::Zip,
            binary: "agentdoctor.exe",
        };

        let asset = select_asset(&release, &target).expect("asset");

        assert_eq!(asset.browser_download_url, "https://example.com/windows");
    }

    #[test]
    fn validates_repo_owner_name_shape() {
        assert!(validate_repo("youssefsz/agentdoctor").is_ok());
        assert!(validate_repo("agentdoctor").is_err());
        assert!(validate_repo("a/b/c").is_err());
    }
}
