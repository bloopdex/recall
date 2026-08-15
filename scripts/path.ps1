# User-PATH helpers for the Recall installer/uninstaller.
#
# All functions take an optional -UserPath value: the installer passes
# nothing and works against the real user PATH; the test suite passes
# explicit strings so it never has to mutate a developer's environment.
#
# Rules (deliberate):
#   - only the USER PATH is ever considered; the SYSTEM PATH is never read
#     for modification and never written;
#   - entries compare case-insensitively and with trailing backslashes
#     ignored, after expanding environment-variable tokens, so the same
#     directory is recognized no matter how it was written;
#   - adding an entry only ever APPENDS it; existing entries are preserved
#     byte-for-byte, in order, including empty entries.

function Test-UserPathEntry {
    param(
        [Parameter(Mandatory = $true)][string]$Entry,
        [string]$UserPath
    )
    if (-not $PSBoundParameters.ContainsKey("UserPath")) {
        $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    }
    if ([string]::IsNullOrEmpty($UserPath)) {
        return $false
    }
    $Target = [Environment]::ExpandEnvironmentVariables($Entry).TrimEnd("\")
    foreach ($Part in $UserPath -split ";") {
        if ([string]::IsNullOrEmpty($Part)) {
            continue
        }
        $Expanded = [Environment]::ExpandEnvironmentVariables($Part).TrimEnd("\")
        if ($Expanded -ieq $Target) {
            return $true
        }
    }
    return $false
}

function Add-UserPathEntry {
    param(
        [Parameter(Mandatory = $true)][string]$Entry,
        [string]$UserPath
    )
    # Returns the new PATH value WITHOUT writing it; the caller applies
    # it with SetEnvironmentVariable. Idempotent: an entry already
    # present returns the value unchanged (no duplicates, ever).
    if (-not $PSBoundParameters.ContainsKey("UserPath")) {
        $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    }
    if (Test-UserPathEntry -Entry $Entry -UserPath $UserPath) {
        return $UserPath
    }
    if ([string]::IsNullOrEmpty($UserPath)) {
        return $Entry
    }
    # Pure append: every existing byte of the value is preserved, exactly
    # as written (including any trailing/empty entries).
    return $UserPath + ";" + $Entry
}

function Remove-UserPathEntry {
    param(
        [Parameter(Mandatory = $true)][string]$Entry,
        [string]$UserPath
    )
    # Returns the new PATH value WITHOUT writing it; the caller applies
    # it with SetEnvironmentVariable. Entries not matching the target
    # directory are preserved byte-for-byte, in order - including empty
    # entries, which some tools interpret as the current directory.
    if (-not $PSBoundParameters.ContainsKey("UserPath")) {
        $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    }
    if ([string]::IsNullOrEmpty($UserPath)) {
        return $UserPath
    }
    $Target = [Environment]::ExpandEnvironmentVariables($Entry).TrimEnd("\")
    $Kept = @()
    foreach ($Part in $UserPath -split ";") {
        $Expanded = [Environment]::ExpandEnvironmentVariables($Part).TrimEnd("\")
        if ($Expanded -ine $Target) {
            $Kept += $Part
        }
    }
    return ($Kept -join ";")
}
