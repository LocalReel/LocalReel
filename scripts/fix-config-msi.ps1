param(
    # Volume to repair, e.g. "D:" or "D". Defaults to every fixed volume.
    [string[]]$Drive,
    # Report what would be done without changing anything.
    [switch]$WhatIf
)

# Repair a Config.Msi folder that a previous install left owned by another
# account.
#
# Windows Installer creates <volume>\Config.Msi for its rollback scripts. If an
# earlier elevated install left that folder behind with an ACL that does not
# include the current user, the next MSI fails with
#   Could not set file security for file 'D:\Config.Msi\....rbf'. Error: 5
# The per-user setup.exe never touches Config.Msi; this script exists for the
# MSI (all-users) path.
#
# Run it from an elevated PowerShell:
#   .\scripts\fix-config-msi.ps1 -Drive D:
#   .\scripts\fix-config-msi.ps1            # every fixed volume
#   .\scripts\fix-config-msi.ps1 -WhatIf

$ErrorActionPreference = 'Stop'

$principal = New-Object Security.Principal.WindowsPrincipal(
    [Security.Principal.WindowsIdentity]::GetCurrent()
)
$isAdmin = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    throw 'Run this from an elevated (administrator) PowerShell.'
}

if (-not $Drive -or $Drive.Count -eq 0) {
    $Drive = Get-Volume -FileSystemLabel $null -ErrorAction SilentlyContinue |
        Where-Object { $_.DriveType -eq 'Fixed' -and $_.DriveLetter } |
        ForEach-Object { "$($_.DriveLetter):" }
    if (-not $Drive) { $Drive = @('C:') }
}

$failed = 0
foreach ($letter in $Drive) {
    $root = $letter.TrimEnd(':', '\') + ':\'
    $path = Join-Path $root 'Config.Msi'
    if (-not (Test-Path -LiteralPath $path)) {
        Write-Host "$path : not present"
        continue
    }

    # A folder that accepts a probe file is healthy; leave it alone.
    $probe = Join-Path $path ("probe-" + [guid]::NewGuid().ToString('N') + '.tmp')
    try {
        Set-Content -LiteralPath $probe -Value 'probe' -ErrorAction Stop
        Remove-Item -LiteralPath $probe -Force
        Write-Host "$path : healthy, nothing to do"
        continue
    } catch {
        Write-Host "$path : not writable ($($_.Exception.Message)); repairing"
    }

    if ($WhatIf) {
        Write-Host "would take ownership, reset the ACL and remove $path"
        continue
    }

    # Any install in progress holds these files; refuse instead of corrupting it.
    $inUse = Get-Process msiexec -ErrorAction SilentlyContinue
    if ($inUse) {
        Write-Warning 'msiexec is running; close the installer wizard before repairing.'
        $failed++
        continue
    }

    & takeown.exe /f $path /a /r /d y | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "takeown failed for $path (exit $LASTEXITCODE)"
        $failed++
        continue
    }
    & icacls.exe $path /reset /t /c | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "icacls failed for $path (exit $LASTEXITCODE)"
        $failed++
        continue
    }
    Remove-Item -LiteralPath $path -Recurse -Force
    Write-Host "$path : removed; Windows Installer will recreate it on the next install"
}

if ($failed -gt 0) {
    throw "$failed volume(s) could not be repaired."
}
Write-Host 'Config.Msi repair finished.'
