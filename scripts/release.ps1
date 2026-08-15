# Recall release bundle script (Windows host).
#
# Builds the release binary and assembles the distributable bundle:
#   dist/recall-<version>-windows-x86_64/
#       recall.exe
#       SHA256SUMS
#       install.ps1 / install.sh
#       CHANGELOG.md
#       LICENSE
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\release.ps1 -Version 1.0.0
#
# Publication (attaching the bundle to a GitHub Release) is deliberately
# NOT part of this script. `dist/` is gitignored: the bundle is generated
# locally and attached to the GitHub Release for the tag — generated
# artifacts are never committed (docs/release/RELEASE-CHECKLIST.md).
# Publication is currently pending a hosting decision; until then the
# generated bundle directory IS the release artifact.
#
# For Linux/macOS, build on the target OS: `cargo build --release` then
# copy `target/release/recall` + `scripts/install.sh` into the bundle and
# run `sha256sum recall > SHA256SUMS`.

param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)

# Verify the manifest version matches the release version (version
# consistency is a release-checklist item).
$Manifest = Get-Content (Join-Path $Root "codebase\recall\Cargo.toml") -Raw
if ($Manifest -notmatch "version = `"$([regex]::Escape($Version))`"") {
    Write-Error "Cargo.toml version does not match -Version $Version - bump it first."
    exit 1
}

Push-Location (Join-Path $Root "codebase\recall")
cargo build --release
if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
Pop-Location

$Bundle = Join-Path $Root "dist\recall-$Version-windows-x86_64"
New-Item -ItemType Directory -Force -Path $Bundle | Out-Null
Copy-Item (Join-Path $Root "codebase\recall\target\release\recall.exe") (Join-Path $Bundle "recall.exe")
Copy-Item (Join-Path $Root "scripts\install.ps1") (Join-Path $Bundle "install.ps1")
Copy-Item (Join-Path $Root "scripts\install.sh") (Join-Path $Bundle "install.sh")
Copy-Item (Join-Path $Root "CHANGELOG.md") (Join-Path $Bundle "CHANGELOG.md")
Copy-Item (Join-Path $Root "LICENSE") (Join-Path $Bundle "LICENSE")

$Hash = (Get-FileHash -Algorithm SHA256 (Join-Path $Bundle "recall.exe")).Hash.ToLowerInvariant()
"$Hash  recall.exe" | Out-File -Encoding ascii (Join-Path $Bundle "SHA256SUMS")

# Smoke test the bundle binary.
$Reported = & (Join-Path $Bundle "recall.exe") --version
if ($LASTEXITCODE -ne 0 -or $Reported -notmatch $Version) {
    Write-Error "Bundle smoke test failed: '$Reported'"
    exit 1
}

Write-Host ""
Write-Host "Release bundle: $Bundle"
Write-Host "  recall.exe  ($Reported)"
Write-Host "  SHA256SUMS  $Hash"
Write-Host ""
Write-Host "Next steps (docs/release/RELEASE-CHECKLIST.md):"
Write-Host "  - run the full validation suite"
Write-Host "  - test install.ps1 against this bundle"
Write-Host "  - tag: git tag v$Version"
Write-Host "  - create the GitHub Release and attach these bundle files"
Write-Host "    (publication is pending a hosting decision - dist/ stays uncommitted)"
