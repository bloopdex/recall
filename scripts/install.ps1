# Recall install script (Windows) - copies the release binary into a user
# bin directory. Deliberately minimal and explicit:
#   - never touches PATH, the registry, or shell profiles (it prints the
#     PATH guidance instead);
#   - never enables shell/git integrations - those are separate, explicit
#     commands (`recall shell install`, `recall git install`);
#   - never touches the database or the embedding model.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File install.ps1 -From <release-dir> [-BinDir <dir>]
#
#   -From:     directory containing recall.exe (and optionally SHA256SUMS);
#              defaults to this script's directory.
#   -BinDir:   install directory; defaults to %USERPROFILE%\.recall\bin.
#   -SkipShaCheck: skip checksum verification (not recommended).

param(
    [string]$From = "",
    [string]$BinDir = "$env:USERPROFILE\.recall\bin",
    [switch]$SkipShaCheck
)

$ErrorActionPreference = "Stop"

Write-Host "Recall - local-first engineering memory"
Write-Host "Installing the recall binary ..."
Write-Host ""

if ([string]::IsNullOrEmpty($From)) {
    $From = Split-Path -Parent $MyInvocation.MyCommand.Path
}
$Binary = Join-Path $From "recall.exe"
$Sums = Join-Path $From "SHA256SUMS"
$ShaVerified = $false

if (-not (Test-Path $Binary)) {
    Write-Error "recall.exe not found in '$From' - point -From at a release directory."
    exit 1
}

if ((Test-Path $Sums) -and (-not $SkipShaCheck)) {
    $Expected = $null
    Get-Content $Sums | ForEach-Object {
        if ($_ -match '^\s*([0-9a-fA-F]{64})\s+recall\.exe\s*$') {
            $Expected = $Matches[1].ToLowerInvariant()
        }
    }
    if ($null -eq $Expected) {
        Write-Error "SHA256SUMS found but contains no entry for recall.exe - refusing to install an unverifiable binary."
        exit 1
    }
    $Actual = (Get-FileHash -Algorithm SHA256 $Binary).Hash.ToLowerInvariant()
    if ($Actual -ne $Expected) {
        Write-Error "Checksum mismatch: expected $Expected, got $Actual - refusing to install."
        exit 1
    }
    Write-Host "Checksum verified: $Actual"
    $ShaVerified = $true
}

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Copy-Item -Force $Binary (Join-Path $BinDir "recall.exe")

Write-Host "Installed: $BinDir\recall.exe"
Write-Host ""
Write-Host "What changed:"
if ($ShaVerified) {
    Write-Host "  - recall.exe copied into $BinDir, checksum verified"
} else {
    Write-Host "  - recall.exe copied into $BinDir (checksum check skipped)"
}
Write-Host "  - nothing else: PATH, shell profiles, hooks, database, and model were NOT touched"
Write-Host ""
Write-Host "Verify the install:"
Write-Host "    & '$BinDir\recall.exe' version"
Write-Host ""
Write-Host "To use recall from anywhere, add it to your PATH:"
Write-Host "    [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path','User') + ';$BinDir', 'User')"
Write-Host "    (then open a new terminal)"
Write-Host ""
Write-Host "Next:"
Write-Host "    recall capture    # remember how you solved your first problem"
Write-Host ""
Write-Host "Optional, explicit integrations (never enabled automatically):"
Write-Host "    recall shell install   # prompt-hook failure capture"
Write-Host "    recall git install     # post-commit hook (per repository)"
Write-Host "    recall embeddings download  # one-time model download (the only network command)"
