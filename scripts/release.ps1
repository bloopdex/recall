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
# Publication is deliberately NOT part of this script. `dist/` is
# gitignored: the bundle is generated locally (verification, dogfooding)
# and by CI (publication). Pushing a `vX.Y.Z` tag triggers the release
# workflow (.github/workflows/release.yml), which validates the tag
# against Cargo.toml, rebuilds this bundle, verifies its checksums, and
# creates the GitHub Release (docs/release/RELEASE-CHECKLIST.md).
# Generated artifacts are never committed.
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
Copy-Item (Join-Path $Root "scripts\uninstall.ps1") (Join-Path $Bundle "uninstall.ps1")
Copy-Item (Join-Path $Root "scripts\path.ps1") (Join-Path $Bundle "path.ps1")
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
Write-Host "  - tag:  git tag -a v$Version -m 'Release v$Version'"
Write-Host "  - push: git push origin main; git push origin v$Version"
Write-Host "    (the tag push triggers the GitHub Actions release workflow -"
Write-Host "     dist/ stays uncommitted)"
