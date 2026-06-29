#requires -Version 5.1

[CmdletBinding()]
param(
    [string]$Repo = $env:AGENTDOCTOR_REPO,
    [string]$InstallDir = $env:AGENTDOCTOR_INSTALL_DIR
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($Repo)) {
    $Repo = "youssefsz/agentdoctor"
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Join-Path $env:USERPROFILE ".local\bin"
}

function Add-UserPath {
    param([Parameter(Mandatory = $true)][string]$PathToAdd)

    $resolved = [System.IO.Path]::GetFullPath($PathToAdd)
    $current = [Environment]::GetEnvironmentVariable("Path", "User")
    $entries = if ([string]::IsNullOrWhiteSpace($current)) {
        @()
    }
    else {
        $current.Split(";")
    }

    $normalized = $resolved.TrimEnd("\").ToLowerInvariant()
    $exists = $entries | Where-Object { $_.TrimEnd("\").ToLowerInvariant() -eq $normalized }
    if (-not $exists) {
        $next = if ([string]::IsNullOrWhiteSpace($current)) {
            $resolved
        }
        else {
            "$current;$resolved"
        }
        [Environment]::SetEnvironmentVariable("Path", $next, "User")
        $env:Path = "$resolved;$env:Path"
    }
}

function Get-Target {
    $arch = $env:PROCESSOR_ARCHITEW6432
    if ([string]::IsNullOrWhiteSpace($arch)) {
        $arch = $env:PROCESSOR_ARCHITECTURE
    }

    switch ($arch) {
        "AMD64" { return "x86_64-pc-windows-gnu" }
        default {
            throw "Unsupported Windows architecture: $arch"
        }
    }
}

$script:InstallStep = 0
$script:InstallStepCount = 6

function Show-InstallStep {
    param([Parameter(Mandatory = $true)][string]$Status)

    $script:InstallStep += 1
    $percent = [Math]::Min(100, [Math]::Round(($script:InstallStep / $script:InstallStepCount) * 100))
    Write-Progress -Activity "Installing AgentDoctor" -Status $Status -PercentComplete $percent
    Write-Host ("[{0}/{1}] {2}" -f $script:InstallStep, $script:InstallStepCount, $Status)
}

$target = Get-Target
$apiUrl = "https://api.github.com/repos/$Repo/releases/latest"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("agentdoctor-install-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

try {
    Show-InstallStep "Checking latest release for $target..."
    $release = Invoke-RestMethod -Uri $apiUrl -Headers @{ "User-Agent" = "agentdoctor-install" }

    Show-InstallStep "Selecting release asset..."
    $asset = $release.assets | Where-Object {
        $_.name -like "agentdoctor-*$target.zip"
    } | Select-Object -First 1

    if ($null -eq $asset) {
        throw "Could not find a release asset for $target in $Repo."
    }

    $archive = Join-Path $tmp $asset.name
    Show-InstallStep "Downloading $($asset.name)..."
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archive

    Show-InstallStep "Extracting release archive..."
    Expand-Archive -LiteralPath $archive -DestinationPath $tmp -Force

    $binary = Get-ChildItem -Path $tmp -Recurse -Filter agentdoctor.exe | Select-Object -First 1
    if ($null -eq $binary) {
        throw "Release archive did not contain agentdoctor.exe."
    }

    $destination = Join-Path $InstallDir "agentdoctor.exe"
    Show-InstallStep "Installing agentdoctor.exe..."
    Copy-Item -LiteralPath $binary.FullName -Destination $destination -Force

    Show-InstallStep "Configuring PATH and verifying install..."
    Add-UserPath $InstallDir
    $installedVersion = & $destination --version
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($installedVersion)) {
        throw "Installed binary did not run: $destination"
    }

    Write-Host "Installed $installedVersion"
    Write-Host "AgentDoctor installed to $destination"
    Write-Host "Open a new terminal if the agentdoctor command is not visible yet."
}
finally {
    Write-Progress -Activity "Installing AgentDoctor" -Completed
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
