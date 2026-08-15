# Phase 4 performance baselines (run from the repository root).
#
# Measures:
#   1. PowerShell prompt-hook overhead (200 prompt invocations, with vs
#      without the recall snippet)
#   2. `recall capture --from-shell` end-to-end (spawned release binary,
#      snapshot env vars set, git context present)
#   3. `git commit` overhead with the recall post-commit hook installed
#      (non-interactive skip path)
#
# Results are written to docs/development/benchmarks.md by hand after a
# run - this script only prints the numbers.

$ErrorActionPreference = "Continue"
$repoRoot = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $repoRoot "codebase\recall\target\release\recall.exe"
if (-not (Test-Path $bin)) {
    throw "release binary not found - run: cargo build --release"
}

# ---- 1. Prompt-hook overhead ----
$snippetFile = Join-Path $env:TEMP "recall-bench-snippet.ps1"
$snippetText = & $bin shell init --shell powershell 2>$null | Out-String
Set-Content -Path $snippetFile -Value $snippetText -Encoding utf8

$plain = @"
function prompt { "PS> " }
"@
$plainFile = Join-Path $env:TEMP "recall-bench-plain.ps1"
Set-Content -Path $plainFile -Value $plain -Encoding utf8

function Measure-Prompt($file) {
    $script = @"
. '$file'
1..200 | ForEach-Object { & prompt } | Out-Null
"@
    $t = Measure-Command { powershell -NoProfile -Command $script }
    return $t.TotalMilliseconds
}

$plainMs = Measure-Prompt $plainFile
$hookedMs = Measure-Prompt $snippetFile
$perPromptPlain = [math]::Round($plainMs / 200, 3)
$perPromptHooked = [math]::Round($hookedMs / 200, 3)
$overhead = [math]::Round($perPromptHooked - $perPromptPlain, 3)
Write-Output "prompt-hook: plain $perPromptPlain ms/invocation, hooked $perPromptHooked ms/invocation, overhead $overhead ms"

# ---- 2. capture --from-shell end-to-end ----
$work = Join-Path $env:TEMP "recall-bench-work"
if (Test-Path $work) { Remove-Item -Recurse -Force $work }
New-Item -ItemType Directory -Force $work | Out-Null
$db = Join-Path $work "bench.db"
$env:RECALL_LAST_COMMAND = "cargo test --release"
$env:RECALL_LAST_EXIT_CODE = "101"
$env:RECALL_LAST_CWD = $work
$env:RECALL_MODEL_DIR = Join-Path $work "no-model-dir"
$env:RECALL_DB_PATH = $db

$t = Measure-Command {
    1..20 | ForEach-Object {
        $null = & $bin capture --from-shell --solution "fixed by rerunning with retries" --force 2>$null
    }
}
$perCapture = [math]::Round($t.TotalMilliseconds / 20, 1)
Write-Output "capture --from-shell: $perCapture ms/capture (spawned binary, 20 runs)"

# ---- 3. git commit with the hook installed ----
$gitRepo = Join-Path $work "git-repo"
git init -b main $gitRepo 2>$null | Out-Null
git -C $gitRepo config user.email bench@example.com
git -C $gitRepo config user.name bench
$null = & $bin --db $db git install 2>$null
Set-Location $gitRepo
"x" | Out-File -FilePath file.txt -Encoding ascii
git add file.txt

# Warm-up commit.
git commit -m "warmup" 2>$null | Out-Null

$t = Measure-Command {
    1..10 | ForEach-Object {
        "y$_" | Out-File -FilePath file.txt -Encoding ascii
        git add file.txt
        git commit -m "fix: bench commit $_" 2>$null | Out-Null
    }
}
$perCommit = [math]::Round($t.TotalMilliseconds / 10, 1)
Write-Output "git commit with recall hook: $perCommit ms/commit (10 commits, non-interactive skip path)"

$null = & $bin --db $db git uninstall 2>$null

$t = Measure-Command {
    1..10 | ForEach-Object {
        "z$_" | Out-File -FilePath file.txt -Encoding ascii
        git add file.txt
        git commit -m "chore: bench commit $_" 2>$null | Out-Null
    }
}
$perCommitPlain = [math]::Round($t.TotalMilliseconds / 10, 1)
Write-Output "git commit without hook: $perCommitPlain ms/commit (10 commits)"
$hookOverhead = [math]::Round($perCommit - $perCommitPlain, 1)
Write-Output "hook overhead: $hookOverhead ms/commit"

Remove-Item Env:RECALL_LAST_COMMAND, Env:RECALL_LAST_EXIT_CODE, Env:RECALL_LAST_CWD, Env:RECALL_MODEL_DIR, Env:RECALL_DB_PATH -ErrorAction SilentlyContinue
