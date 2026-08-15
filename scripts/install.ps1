# Recall install script (Windows) - copies the release binary into a user
# bin directory and adds that directory to the USER PATH (opt-out with
# -SkipPath). Deliberately minimal and explicit:
#   - adds ONLY the bin directory to the USER PATH (never the SYSTEM
#     PATH), appending without touching existing entries, and never
#     duplicating an entry that is already there;
#   - never touches shell profiles or startup files;
#   - never enables shell/git integrations - those are separate, explicit
#     commands (`recall shell install`, `recall git install`);
#   - never touches the database or the embedding model.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File install.ps1 -From <release-dir> [-BinDir <dir>] [-SkipPath] [-SkipShaCheck]
#
#   -From:     directory containing recall.exe (and optionally SHA256SUMS);
#              defaults to this script's directory.
#   -BinDir:   install directory; defaults to %USERPROFILE%\.recall\bin.
#   -SkipPath: do NOT add the bin directory to the user PATH.
#   -SkipShaCheck: skip checksum verification (not recommended).

param(
    [string]$From = "",
    [string]$BinDir = "$env:USERPROFILE\.recall\bin",
    [switch]$SkipPath,
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

# ---- User PATH (opt-out with -SkipPath) ----
. (Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) "path.ps1")

if ($SkipPath) {
    Write-Host "PATH: skipped (-SkipPath) - recall runs from $BinDir\recall.exe."
} else {
    $Current = [Environment]::GetEnvironmentVariable("Path", "User")
    $Updated = Add-UserPathEntry -Entry $BinDir
    if ($Updated -ne $Current) {
        [Environment]::SetEnvironmentVariable("Path", $Updated, "User")
        Write-Host "PATH: added $BinDir to your USER path (SYSTEM path untouched)."
    } else {
        Write-Host "PATH: $BinDir is already in your user path (no duplicate added)."
    }
    $InSession = $false
    foreach ($Part in $env:PATH -split ";") {
        if ([Environment]::ExpandEnvironmentVariables($Part).TrimEnd("\") -ieq [Environment]::ExpandEnvironmentVariables($BinDir).TrimEnd("\")) {
            $InSession = $true
            break
        }
    }
    if ($InSession) {
        Write-Host "      This terminal can already run recall."
    } else {
        Write-Host "      Open a NEW terminal for the change to take effect."
    }
}

Write-Host ""
Write-Host "What changed:"
if ($ShaVerified) {
    Write-Host "  - recall.exe copied into $BinDir, checksum verified"
} else {
    Write-Host "  - recall.exe copied into $BinDir (checksum check skipped)"
}
if ($SkipPath) {
    Write-Host "  - user PATH: unchanged (requested with -SkipPath)"
} elseif ($Updated -ne $Current) {
    Write-Host "  - user PATH: $BinDir appended"
} else {
    Write-Host "  - user PATH: already contained $BinDir"
}
Write-Host "  - nothing else: SYSTEM PATH, shell profiles, hooks, database, and model were NOT touched"
Write-Host ""
Write-Host "Verify the install:"
Write-Host "    & '$BinDir\recall.exe' version"
if (-not $SkipPath) {
    Write-Host "    recall version    # in a new terminal"
}
Write-Host ""
Write-Host "Next:"
Write-Host "    recall capture    # remember how you solved your first problem"
Write-Host ""
Write-Host "Optional, explicit integrations (never enabled automatically):"
Write-Host "    recall shell install   # prompt-hook failure capture"
Write-Host "    recall git install     # post-commit hook (per repository)"
Write-Host "    recall embeddings download  # one-time model download (the only network command)"
