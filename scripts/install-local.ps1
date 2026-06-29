#requires -Version 5.1

[CmdletBinding()]
param(
    [switch]$NoPathUpdate,
    [switch]$SkipVerify,
    [switch]$UseCurrentToolchain
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Write-Step {
    param([Parameter(Mandatory = $true)][string]$Message)
    Write-Host "==> $Message"
}

function Get-CommandPath {
    param([Parameter(Mandatory = $true)][string]$Name)
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        return $null
    }
    return $command.Source
}

function Split-PathList {
    param([AllowNull()][string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) {
        return @()
    }
    return $Value.Split(";") | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
}

function Normalize-PathEntry {
    param([Parameter(Mandatory = $true)][string]$Path)
    try {
        return [System.IO.Path]::GetFullPath($Path).TrimEnd("\").ToLowerInvariant()
    }
    catch {
        return $Path.TrimEnd("\").ToLowerInvariant()
    }
}

function Add-PathEntry {
    param([Parameter(Mandatory = $true)][string]$Entry)

    $resolved = [System.IO.Path]::GetFullPath($Entry)
    if (-not (Test-Path -LiteralPath $resolved)) {
        New-Item -ItemType Directory -Force -Path $resolved | Out-Null
    }

    $currentEntries = Split-PathList $env:Path
    $normalized = Normalize-PathEntry $resolved
    $hasCurrent = $currentEntries | Where-Object { (Normalize-PathEntry $_) -eq $normalized }
    if (-not $hasCurrent) {
        $env:Path = "$resolved;$env:Path"
    }

    if ($NoPathUpdate) {
        return
    }

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $userEntries = Split-PathList $userPath
    $hasUser = $userEntries | Where-Object { (Normalize-PathEntry $_) -eq $normalized }
    if (-not $hasUser) {
        $newUserPath = if ([string]::IsNullOrWhiteSpace($userPath)) {
            $resolved
        }
        else {
            "$userPath;$resolved"
        }
        [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")
        Write-Step "Added $resolved to your user PATH"
    }
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code $LASTEXITCODE`: $FilePath $($Arguments -join ' ')"
    }
}

function Invoke-Cargo {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)

    if ($script:RustupPath -and $script:InstallToolchain) {
        Invoke-Checked $script:RustupPath (@("run", $script:InstallToolchain, "cargo") + $Arguments)
        return
    }

    $cargoPath = Get-CommandPath "cargo"
    if (-not $cargoPath) {
        throw "cargo was not found on PATH. Install Rust from https://rustup.rs/ and rerun this script."
    }
    Invoke-Checked $cargoPath $Arguments
}

function Ensure-WindowsGnuToolchain {
    if ($UseCurrentToolchain -or -not $script:RunningOnWindows) {
        return
    }

    $script:RustupPath = Get-CommandPath "rustup"
    if (-not $script:RustupPath) {
        $commonRustup = Join-Path $env:USERPROFILE ".cargo\bin\rustup.exe"
        if (Test-Path -LiteralPath $commonRustup) {
            $script:RustupPath = $commonRustup
        }
    }

    if (-not $script:RustupPath) {
        return
    }

    $script:InstallToolchain = "stable-x86_64-pc-windows-gnu"
    Write-Step "Ensuring Rust GNU toolchain is installed"
    Invoke-Checked $script:RustupPath @("toolchain", "install", $script:InstallToolchain)

    if (-not (Get-CommandPath "dlltool")) {
        $scoopPath = Get-CommandPath "scoop"
        if ($scoopPath) {
            Write-Step "Ensuring MinGW is installed"
            Invoke-Checked $scoopPath @("install", "mingw")
            Add-PathEntry (Join-Path $env:USERPROFILE "scoop\apps\mingw\current\bin")
        }
        else {
            throw "The GNU Rust toolchain needs MinGW binutils, but dlltool.exe was not found. Install Scoop and run `scoop install mingw`, or rerun this script with -UseCurrentToolchain after configuring MSVC Build Tools."
        }
    }
}

$script:RustupPath = $null
$script:InstallToolchain = $null
$platform = if ($PSVersionTable.ContainsKey("Platform")) {
    $PSVersionTable.Platform
}
else {
    "Win32NT"
}
$script:RunningOnWindows = $platform -eq "Win32NT"
$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$CliCrate = Join-Path $RepoRoot "crates\agentdoctor-cli"
$CliManifest = Join-Path $CliCrate "Cargo.toml"

if (-not (Test-Path -LiteralPath $CliManifest)) {
    throw "Could not find AgentDoctor CLI manifest at $CliManifest. Run this script from the repository checkout."
}

$CargoHome = if ($env:CARGO_HOME) {
    $env:CARGO_HOME
}
else {
    Join-Path $env:USERPROFILE ".cargo"
}
$CargoBin = Join-Path $CargoHome "bin"
Add-PathEntry $CargoBin

Ensure-WindowsGnuToolchain

Write-Step "Installing agentdoctor CLI"
Invoke-Cargo @("install", "--path", $CliCrate, "--locked", "--force")

$AgentDoctorExe = if ($script:RunningOnWindows) {
    Join-Path $CargoBin "agentdoctor.exe"
}
else {
    Join-Path $CargoBin "agentdoctor"
}

if (-not (Test-Path -LiteralPath $AgentDoctorExe)) {
    $resolved = Get-CommandPath "agentdoctor"
    if ($resolved) {
        $AgentDoctorExe = $resolved
    }
    else {
        throw "agentdoctor was installed, but the executable was not found in $CargoBin."
    }
}

if (-not $SkipVerify) {
    Write-Step "Verifying installed CLI"
    Invoke-Checked $AgentDoctorExe @("--version")
    $json = & $AgentDoctorExe "scan" (Join-Path $RepoRoot "fixtures\rust-cli") "--format" "json" "--no-interactive"
    if ($LASTEXITCODE -ne 0) {
        throw "Installed agentdoctor failed verification scan."
    }
    $json | ConvertFrom-Json | Out-Null
}

Write-Host ""
Write-Host "AgentDoctor installed successfully."
Write-Host "Executable: $AgentDoctorExe"
Write-Host ""
Write-Host "Try:"
Write-Host "  agentdoctor scan --no-interactive"
Write-Host "  agentdoctor init fixtures/empty-repo --dry-run --agents codex,claude --no-interactive"
if (-not $NoPathUpdate) {
    Write-Host ""
    Write-Host "Open a new terminal if the agentdoctor command is not visible in existing shells."
}
