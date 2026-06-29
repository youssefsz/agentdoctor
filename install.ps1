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

$target = Get-Target
$apiUrl = "https://api.github.com/repos/$Repo/releases/latest"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("agentdoctor-install-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

try {
    Write-Host "Fetching latest AgentDoctor release for $target..."
    $release = Invoke-RestMethod -Uri $apiUrl -Headers @{ "User-Agent" = "agentdoctor-install" }
    $asset = $release.assets | Where-Object {
        $_.name -like "agentdoctor-*$target.zip"
    } | Select-Object -First 1

    if ($null -eq $asset) {
        throw "Could not find a release asset for $target in $Repo."
    }

    $archive = Join-Path $tmp $asset.name
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archive
    Expand-Archive -LiteralPath $archive -DestinationPath $tmp -Force

    $binary = Get-ChildItem -Path $tmp -Recurse -Filter agentdoctor.exe | Select-Object -First 1
    if ($null -eq $binary) {
        throw "Release archive did not contain agentdoctor.exe."
    }

    $destination = Join-Path $InstallDir "agentdoctor.exe"
    Copy-Item -LiteralPath $binary.FullName -Destination $destination -Force
    Add-UserPath $InstallDir

    Write-Host "AgentDoctor installed to $destination"
    Write-Host "Open a new terminal if the agentdoctor command is not visible yet."
}
finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
