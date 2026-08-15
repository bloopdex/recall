# Recall uninstall script (Windows) - removes the binary and the Recall
# bin directory from the USER PATH. Deliberately minimal and explicit:
#   - removes ONLY the bin directory entry from the USER PATH (never the
#     SYSTEM PATH), preserving every other entry byte-for-byte;
#   - deletes only recall.exe (and the bin directory when it is empty);
#   - never touches shell/git integrations - remove those explicitly with
#     `recall shell uninstall` and `recall git uninstall` BEFORE deleting
#     the binary if you want them gone;
#   - never touches the database or the embedding model - your memories
#     are untouched and stay exactly where they are.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File uninstall.ps1 [-BinDir <dir>]
#
#   -BinDir: install directory; defaults to %USERPROFILE%\.recall\bin.

param(
    [string]$BinDir = "$env:USERPROFILE\.recall\bin"
)

$ErrorActionPreference = "Stop"

Write-Host "Recall uninstall (Windows)"
Write-Host ""

. (Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) "path.ps1")

# ---- User PATH ----
$Current = [Environment]::GetEnvironmentVariable("Path", "User")
$Updated = Remove-UserPathEntry -Entry $BinDir
if ($Updated -ne $Current) {
    [Environment]::SetEnvironmentVariable("Path", $Updated, "User")
    Write-Host "PATH: removed $BinDir from your user path (all other entries preserved)."
} else {
    Write-Host "PATH: $BinDir was not in your user path."
}

# ---- Binary ----
$Binary = Join-Path $BinDir "recall.exe"
if (Test-Path $Binary) {
    Remove-Item -Force $Binary
    Write-Host "Removed: $Binary"
} else {
    Write-Host "Binary: not present at $Binary."
}

# Remove the bin directory itself only when it is empty - a directory
# the user put files into is never touched.
$Remaining = @(Get-ChildItem -Force $BinDir -ErrorAction SilentlyContinue)
if ($Remaining.Count -eq 0) {
    Remove-Item -Force $BinDir
    Write-Host "Removed: $BinDir (was empty)."
}

Write-Host ""
Write-Host "Not touched (deliberately):"
Write-Host "  - your memories: the database and the model are exactly where they were;"
Write-Host "  - shell/git integrations: remove them explicitly first with"
Write-Host "      recall shell uninstall"
Write-Host "      recall git uninstall   (in each repository)"
Write-Host "  - the SYSTEM PATH and your shell profiles."
Write-Host ""
Write-Host "Recall is uninstalled. Your memories were not deleted."
