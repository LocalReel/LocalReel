# Runs an external tool with the error preference narrowed.
#
# Native commands (cargo, npm, wix, the packaged binary) write progress and
# warnings to stderr. With `$ErrorActionPreference = 'Stop'` at the script
# level, PowerShell turns those lines into terminating errors as soon as the
# caller captures or redirects the script's output - CI logs, `*> file`, or
# `2>&1 | Tee-Object` all abort a perfectly healthy build. Scripts therefore
# keep 'Stop' for cmdlets and send every external tool through Invoke-Native,
# which relaxes the preference for the duration of that one call and decides on
# the process exit code instead.
#
# Dot-source it:
#
#     . (Join-Path $PSScriptRoot 'native.ps1')
#
# and call it with -Capture when the output is needed:
#
#     $json = Invoke-Native -Label 'cargo metadata' -Capture { & $cargo metadata }
#
# Without -Capture the tool's output goes to the host as it is produced, so
# compilers still stream their progress.
#
# Every local here is prefixed. A scriptblock passed by a caller resolves
# variables through the live scope chain, so a plain `$output` in this function
# would shadow a caller's `$output` parameter inside its own arguments.

function Invoke-Native {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][scriptblock]$Command,
        [switch]$Capture
    )

    $nativePreviousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $nativeResult = $null
    try {
        if ($Capture) {
            $nativeResult = & $Command
        } else {
            # Merge stderr here rather than letting the caller's own redirection
            # do it: that is what turns a compiler's progress lines into error
            # records (and, under Stop, into a terminating error).
            & $Command 2>&1 | ForEach-Object { Write-Host $_ }
        }
    } finally {
        $ErrorActionPreference = $nativePreviousPreference
    }

    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed (exit $LASTEXITCODE)"
    }

    if ($Capture) { return $nativeResult }
}
