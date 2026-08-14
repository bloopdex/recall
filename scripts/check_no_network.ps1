# Zero-network guard: fail if any networking/async crate appears in the
# dependency tree. Run from the repository root.
Set-Location "$PSScriptRoot\..\codebase\recall"
$banned = @("reqwest","hyper","tokio","ureq","isahc","curl","openssl","rustls","native-tls","surf","attohttpc","minreq","websocket","async-std","smol")
$tree = cargo tree 2>&1 | Out-String
$bad = $banned | Where-Object { $tree -match " $_ v" -or $tree -match "^$_ v" }
if ($bad) {
    Write-Error "Banned networking dependencies found in tree: $($bad -join ', ')"
    exit 1
}
Write-Output "OK: no networking dependencies in the tree."
