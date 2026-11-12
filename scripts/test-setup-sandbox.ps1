param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [string]$PreviousInstaller,
    [string]$ExpectedVersion = '0.5.8',
    [switch]$ConfirmDisposableSandbox
)
$ErrorActionPreference = 'Stop'
# No bypass flag for ordinary host accounts. This is a safety interlock, not a
# security boundary against someone deliberately impersonating Sandbox.
if (-not $ConfirmDisposableSandbox -or $env:USERNAME -ne 'WDAGUtilityAccount' -or
    [Environment]::UserName -ne 'WDAGUtilityAccount' -or
    $env:USERPROFILE -ne 'C:\Users\WDAGUtilityAccount' -or
    -not (Test-Path -LiteralPath 'C:\Users\WDAGUtilityAccount')) {
    throw 'Run ONLY inside a disposable Windows Sandbox as WDAGUtilityAccount; host execution refused.'
}
$Installer = (Resolve-Path -LiteralPath $Installer).Path
if ($PreviousInstaller) { $PreviousInstaller = (Resolve-Path -LiteralPath $PreviousInstaller).Path }
$root = Join-Path 'C:\' ('lr-setup-verification-' + [guid]::NewGuid().ToString('N'))
$install = Join-Path $root 'install'
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{6E7B1C2A-9C41-4E3B-8A55-0B0A0F6C1D21}'
$shortcuts = @((Join-Path ([Environment]::GetFolderPath('Programs')) 'LocalReel.lnk'), (Join-Path ([Environment]::GetFolderPath('Desktop')) 'LocalReel.lnk'))
if (Test-Path $uninstallKey) { throw 'Existing installation registration; use a fresh Sandbox.' }
foreach ($link in $shortcuts) { if (Test-Path -LiteralPath $link) { throw 'Existing shortcut; use a fresh Sandbox.' } }
New-Item -ItemType Directory -Path $root | Out-Null
function Invoke-Setup([string]$Exe, [switch]$Uninstall) {
    $args = @('/S', '/nolaunch', "/D=$install")
    if ($Uninstall) { $args += '/uninstall' }
    $process = Start-Process -FilePath $Exe -ArgumentList $args -PassThru
    if (-not $process.WaitForExit(120000)) {
        $process.Kill()
        throw 'Installer timed out; discard this Sandbox after inspecting evidence.'
    }
    if ($process.ExitCode -ne 0) { throw "Installer failed: $($process.ExitCode)" }
}
$first = if ($PreviousInstaller) { $PreviousInstaller } else { $Installer }
Invoke-Setup $first
New-Item -ItemType Directory -Path (Join-Path $install 'library') | Out-Null
Set-Content (Join-Path $install 'library/sentinel.db') 'KEEP-DATABASE'
Set-Content (Join-Path $install 'library/sentinel.mp4') 'KEEP-MEDIA'
Set-Content (Join-Path $install 'localreel.toml') "[data]`ndir='library'"
$sentinels = @('library/sentinel.db', 'library/sentinel.mp4', 'localreel.toml')
$before = @{}
foreach ($relative in $sentinels) { $before[$relative] = (Get-FileHash (Join-Path $install $relative)).Hash }
Invoke-Setup $Installer
$manifest = Get-Content (Join-Path $install 'install.json') -Raw | ConvertFrom-Json
if ($manifest.version -ne $ExpectedVersion -or $manifest.file_manifest_version -ne 2 -or ($manifest.files -match '^library[/\\]')) { throw 'Wrong version or polluted payload manifest' }
if ((Get-ItemProperty $uninstallKey).DisplayVersion -ne $ExpectedVersion) { throw 'Wrong registered version' }
foreach ($link in $shortcuts) { if (-not (Test-Path -LiteralPath $link)) { throw "Missing shortcut: $link" } }
# A shortcut that opens a browser instead of the desktop window looks fine in
# every other check, so compare the target itself.
$shellExe = Join-Path $install 'desktop\LocalReel.exe'
$expectedTarget = if (Test-Path -LiteralPath $shellExe) {
    $shellExe
} else {
    Join-Path $install 'localreel-desktop.exe'
}
$wscript = New-Object -ComObject WScript.Shell
foreach ($link in $shortcuts) {
    $actual = $wscript.CreateShortcut($link).TargetPath
    if ($actual -ne $expectedTarget) {
        throw "Shortcut $link points at $actual, expected $expectedTarget"
    }
}
Invoke-Setup $Installer -Uninstall
foreach ($relative in $sentinels) {
    if ((Get-FileHash (Join-Path $install $relative)).Hash -ne $before[$relative]) { throw "User file changed: $relative" }
}
if (Test-Path (Join-Path $install 'localreel.exe')) { throw 'Program remains after uninstall' }
if (Test-Path $uninstallKey) { throw 'Uninstall registration remains' }
foreach ($link in $shortcuts) { if (Test-Path -LiteralPath $link) { throw "Shortcut remains: $link" } }
Write-Host "PASS: install -> custom library -> upgrade -> ordinary uninstall ($ExpectedVersion)."
Write-Host "Evidence retained at $root; close Sandbox to discard its state. No server was launched."
