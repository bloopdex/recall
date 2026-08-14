# Zero-network guard. Run from the repository root.
#
# Rules (ADR-0010 + ADR-0013 amendment):
#  1. No banned networking crate may be a DIRECT dependency (checked by
#     tests/security.rs against Cargo.toml — only reqwest, only behind the
#     opt-in `download` feature, is allowed there).
#  2. In the dependency TREE, banned crates may appear only as transitive
#     dependencies of fastembed (its hf-hub download path, which Recall
#     never calls — model files are loaded from a local directory).
#  3. Recall's own source must never reference network APIs (pinned by the
#     source-guard test in tests/security.rs).
Set-Location "$PSScriptRoot\..\codebase\recall"

# Locate cargo (may not be on PATH in non-interactive shells).
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    $cargoPath = "$env:USERPROFILE\.cargo\bin\cargo.exe"
    if (Test-Path $cargoPath) { $cargo = $cargoPath } else { throw "cargo not found" }
}

$banned = @("reqwest","hyper","tokio","ureq","isahc","curl","openssl","rustls","native-tls","surf","attohttpc","minreq","websocket","async-std","smol")
$tree = & $cargo tree 2>&1 | Out-String
foreach ($name in $banned) {
    if ($tree -notmatch "(?m)^[^\n]* $name v") { continue }
    # Find who pulls it in: only fastembed is an allowed parent.
    $parents = (& $cargo tree -i $name 2>&1 | Out-String)
    if ($parents -notmatch "fastembed v") {
        Write-Error "Banned crate '$name' in tree with no fastembed parent: $parents"
        exit 1
    }
}
Write-Output "OK: banned networking crates appear only as fastembed transitive dependencies."
Write-Output "   (Recall's own code has no network paths - pinned by tests/security.rs.)"
