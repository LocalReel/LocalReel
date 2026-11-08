param(
    [string]$InstallDir,
    [switch]$RemoveData,
    [switch]$StopServer,
    [switch]$Force,
    [string]$StartMenuDir = [Environment]::GetFolderPath('Programs'),
    [string]$DesktopDir = [Environment]::GetFolderPath('Desktop'),
    [string]$StartupDir = [Environment]::GetFolderPath('Startup')
)

$ErrorActionPreference = 'Stop'

if (-not $InstallDir) {
    if (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'localreel.exe')) {
        $InstallDir = $PSScriptRoot
    } else {
        $InstallDir = Join-Path (Join-Path $env:LOCALAPPDATA 'Programs') 'LocalReel'
    }
}

if (-not (Test-Path -LiteralPath $InstallDir)) {
    Write-Host "Install directory not found: $InstallDir"
    exit 0
}

$manifestPath = Join-Path $InstallDir 'install.json'
$manifest = $null
if (Test-Path -LiteralPath $manifestPath) {
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
}

$exe = Join-Path $InstallDir 'localreel.exe'
$running = @(Get-Process localreel -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and ($_.Path -ieq $exe) })
if ($running.Count -gt 0) {
    if (-not $StopServer) {
        throw "LocalReel is running from $exe; stop it or pass -StopServer"
    }
    $running | ForEach-Object { Stop-Process -Id $_.Id -Force }
    Start-Sleep -Milliseconds 500
}

if (-not $Force) {
    $detail = if ($RemoveData) { 'This will delete data and progress.' } else { 'Data and localreel.toml will be kept.' }
    $answer = Read-Host "Uninstall LocalReel from $InstallDir? $detail [y/N]"
    if ($answer -notmatch '^[yY]') {
        Write-Host 'Canceled.'
        exit 0
    }
}

$shortcuts = @()
if ($manifest -and $manifest.shortcuts) {
    $shortcuts = @($manifest.shortcuts)
}
$defaults = @(
    (Join-Path $StartMenuDir 'LocalReel.lnk'),
    (Join-Path $DesktopDir 'LocalReel.lnk'),
    (Join-Path $StartupDir 'LocalReel.lnk')
)
foreach ($path in ($shortcuts + $defaults)) {
    if ($path -and (Test-Path -LiteralPath $path)) {
        Remove-Item -LiteralPath $path -Force
        Write-Host "Removed shortcut: $path"
    }
}

# The program lives under %LOCALAPPDATA%\Programs\LocalReel; the library, the
# database and the caches live in %LOCALAPPDATA%\LocalReel (or wherever
# LOCALREEL_DATA_DIR points). Removing one must never take the other with it.
$dataDir = if ($env:LOCALREEL_DATA_DIR) {
    $env:LOCALREEL_DATA_DIR
} else {
    Join-Path $env:LOCALAPPDATA 'LocalReel'
}

if ($RemoveData) {
    Remove-Item -LiteralPath $InstallDir -Recurse -Force
    Write-Host "Removed program files from $InstallDir."
    if ((Test-Path -LiteralPath $dataDir) -and ($dataDir -ne $InstallDir)) {
        Remove-Item -LiteralPath $dataDir -Recurse -Force
        Write-Host "Removed data: $dataDir"
    }
} else {
    # A config file written by an older per-directory install is worth keeping:
    # move it where the program looks for it now before the folder goes away.
    $stale = Join-Path $InstallDir 'localreel.toml'
    $configDir = Join-Path $env:APPDATA 'LocalReel'
    if (Test-Path -LiteralPath $stale) {
        New-Item -ItemType Directory -Force -Path $configDir | Out-Null
        Copy-Item -LiteralPath $stale -Destination (Join-Path $configDir 'localreel.toml') -Force
        Write-Host "Kept config: $(Join-Path $configDir 'localreel.toml')"
    }
    # A relative [data] dir resolves against the program's directory, so the
    # library can sit inside the folder being removed. Move it aside first and
    # put it back afterwards; deleting the program must never delete media.
    $insideData = Join-Path $InstallDir 'data'
    $stash = $null
    if (Test-Path -LiteralPath $insideData) {
        $stash = Join-Path ([System.IO.Path]::GetTempPath()) ('localreel-data-' + [guid]::NewGuid().ToString('N') + '.keep')
        Move-Item -LiteralPath $insideData -Destination $stash
    }
    Remove-Item -LiteralPath $InstallDir -Recurse -Force
    Write-Host "Removed program files from $InstallDir."
    if ($stash) {
        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
        Move-Item -LiteralPath $stash -Destination $insideData
        Write-Host "Kept library inside the install folder: $insideData"
    }
    if (Test-Path -LiteralPath $dataDir) {
        Write-Host "Kept data: $dataDir"
    }
}

