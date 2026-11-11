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

if ((Get-Item -LiteralPath $InstallDir -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw 'Refusing to uninstall through a linked installation directory.'
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
    # Ordinary uninstall only removes owned payload files, never arbitrary
    # directories (which may contain custom relative data paths).
    $root = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\', '/')
    $owned = @('localreel.exe', 'localreel-desktop.exe', 'localreel-setup.exe')
    if ($manifest -and $manifest.file_manifest_version -eq 2 -and $manifest.files) {
        $owned = @($manifest.files)
    }
    foreach ($relative in $owned) {
        if ([IO.Path]::IsPathRooted($relative) -or $relative -match '(^|[\\/])\.\.([\\/]|$)' -or $relative -match '^(?i:localreel\.toml|data([\\/]|$))') { continue }
        $path = [IO.Path]::GetFullPath((Join-Path $root $relative))
        if (-not $path.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { continue }
        $cursor = $path
        $linked = $false
        while ($cursor -and $cursor -ne $root) {
            if (Test-Path -LiteralPath $cursor) {
                if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { $linked = $true; break }
            }
            $cursor = Split-Path -Parent $cursor
        }
        if (-not $linked -and (Test-Path -LiteralPath $path -PathType Leaf)) { Remove-Item -LiteralPath $path -Force }
    }
    if (Test-Path -LiteralPath $manifestPath) { Remove-Item -LiteralPath $manifestPath -Force }
    Write-Host "Removed owned program files; preserved other files in $InstallDir."
}

