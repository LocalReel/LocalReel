param(
    # Directory that contains the component checkouts. Defaults to the parent of
    # this repository, which is the usual "sibling repositories" layout.
    [string]$ComponentsDir,
    [string]$CoreDir,
    [string]$WebDir,
    [string]$OutputDir = 'dist',
    [ValidateSet('release', 'debug')]
    [string]$Profile = 'release',
    [switch]$SkipBuild,
    [switch]$SkipWeb,
    [switch]$BuildMsi,
    [string]$FfmpegDir,
    # Package even when component versions disagree with components.json.
    [switch]$Force
)

# Compose the LocalReel product from its components.
#
# This repository is the only place that produces distributable artifacts:
#   components.json pins which component versions belong to the product,
#   scripts/compose.ps1 builds the web UI, builds the server binary with the UI
#   embedded, stages the documentation/scripts, and writes the zip (optionally
#   the MSI via scripts/build-msi.ps1).
#
#   .\scripts\compose.ps1                      # zip only, for the current tree
#   .\scripts\compose.ps1 -BuildMsi            # zip + MSI
#   .\scripts\compose.ps1 -ComponentsDir ..\..\checkouts -SkipWeb

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

function Resolve-Tool {
    param([string]$Name, [string[]]$Fallbacks)
    foreach ($fallback in $Fallbacks) {
        if ($fallback -and (Test-Path -LiteralPath $fallback)) { return $fallback }
    }
    $command = Get-Command $Name -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandType -ne 'ExternalScript' } |
        Select-Object -First 1
    if ($command) { return $command.Source }
    $any = Get-Command $Name -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($any) { return $any.Source }
    throw "could not find '$Name'; install it or add it to PATH"
}

$cargo = if ($env:CARGO) { $env:CARGO } else { Resolve-Tool 'cargo' @((Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe')) }
$npm = if ($env:NPM) { $env:NPM } else { Resolve-Tool 'npm' @('C:\Program Files\nodejs\npm.cmd') }

# --- locate components -------------------------------------------------------
if (-not $ComponentsDir) { $ComponentsDir = Split-Path -Parent $root }
$manifestPath = Join-Path $root 'components.json'
if (-not (Test-Path -LiteralPath $manifestPath)) { throw "missing components.json in $root" }
$manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json

function Resolve-Component([string]$name, [string]$explicit) {
    if ($explicit) { return (Resolve-Path -LiteralPath $explicit).Path }
    $entry = $manifest.components.$name
    if (-not $entry) { throw "components.json does not describe a '$name' component" }
    if ($entry.path) {
        $candidate = if ([System.IO.Path]::IsPathRooted($entry.path)) { $entry.path } else { Join-Path $root $entry.path }
        if (Test-Path -LiteralPath $candidate) { return (Resolve-Path -LiteralPath $candidate).Path }
    }
    $sibling = Join-Path $ComponentsDir $name
    if (Test-Path -LiteralPath $sibling) { return (Resolve-Path -LiteralPath $sibling).Path }
    throw "component '$name' not found (looked at '$($entry.path)' and '$sibling'); pass -ComponentsDir or -$($name.Substring(0,1).ToUpper()+$name.Substring(1))Dir"
}

$core = Resolve-Component 'core' $CoreDir
$web = Resolve-Component 'web' $WebDir
Write-Host "core: $core"
Write-Host "web:  $web"

# --- version check -----------------------------------------------------------
$cargoMetadata = & $cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $core 'Cargo.toml') | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
$serverPackage = $cargoMetadata.packages | Where-Object { $_.name -eq 'localreel-server' } | Select-Object -First 1
if (-not $serverPackage) { throw 'could not determine localreel-server version' }
$version = $serverPackage.version

$webVersion = (Get-Content -LiteralPath (Join-Path $web 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json).version
if ($webVersion -ne $version -and -not $Force) {
    throw "component versions disagree: core $version vs web $webVersion (pass -Force to package anyway)"
}
foreach ($name in @('core', 'web')) {
    $pinned = $manifest.components.$name.version
    $actual = if ($name -eq 'core') { $version } else { $webVersion }
    if ($pinned -and $pinned -ne $actual -and -not $Force) {
        throw "components.json pins $name $pinned but the checkout is $actual (update components.json or pass -Force)"
    }
}
Write-Host "product version: $version"

# --- build web ---------------------------------------------------------------
$webDist = Join-Path $web 'dist'
if (-not $SkipWeb) {
    $nodeDir = Split-Path -Parent $npm
    if ($nodeDir) { $env:Path = "$nodeDir;$env:Path" }
    Write-Host 'building web UI...'
    Push-Location -LiteralPath $web
    try {
        & $npm run build
        if ($LASTEXITCODE -ne 0) { throw 'web build failed' }
    } finally {
        Pop-Location
    }
}
if (-not (Test-Path -LiteralPath (Join-Path $webDist 'index.html'))) {
    throw "web dist is missing at $webDist; run without -SkipWeb or build it in the web component"
}

# --- build core (with the web dist embedded) ---------------------------------
$exeName = if ($env:OS -eq 'Windows_NT') { 'localreel.exe' } else { 'localreel' }
$binary = Join-Path $core "target\$Profile\$exeName"
if (-not $SkipBuild) {
    Write-Host "building localreel-server ($Profile)..."
    # Components stay separable: the core crate reads its UI from anywhere.
    $env:LOCALREEL_WEB_DIR = $web
    $buildArgs = @('build', '-p', 'localreel-server', '--manifest-path', (Join-Path $core 'Cargo.toml'))
    if ($Profile -eq 'release') { $buildArgs += @('--profile', 'release') }
    & $cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
}
if (-not (Test-Path -LiteralPath $binary)) {
    throw "binary not found: $binary (run without -SkipBuild)"
}

# --- stage -------------------------------------------------------------------
$arch = if ($env:PROCESSOR_ARCHITECTURE -match 'ARM64') { 'arm64' } else { 'x64' }
$stageName = "localreel-$version-$arch"
$outDir = if ([System.IO.Path]::IsPathRooted($OutputDir)) { $OutputDir } else { Join-Path $root $OutputDir }
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$stage = Join-Path $outDir $stageName
if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Path $stage | Out-Null

Copy-Item -LiteralPath $binary -Destination (Join-Path $stage $exeName)

# The GUI launcher is what the shortcuts and the login entry run: it starts the
# server without a console window. It is Windows-only.
if ($exeName -eq 'localreel.exe') {
    $launcher = Join-Path $core "target\$Profile\localreel-desktop.exe"
    if (-not (Test-Path -LiteralPath $launcher)) {
        throw "launcher not found: $launcher (cargo build should produce it)"
    }
    Copy-Item -LiteralPath $launcher -Destination (Join-Path $stage 'localreel-desktop.exe')
}
Copy-Item -LiteralPath (Join-Path $core 'localreel.toml.example') -Destination $stage
Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $stage
Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $stage
foreach ($script in @('install.ps1', 'uninstall.ps1', 'install-task.ps1')) {
    Copy-Item -LiteralPath (Join-Path $root "scripts\$script") -Destination $stage
}

# Documentation shipped next to the binary. Release notes and the readiness
# report stay on GitHub (their hashes change per build, so shipping them would
# immediately drift); the packaged README links to them by URL.
New-Item -ItemType Directory -Force -Path (Join-Path $stage 'docs') | Out-Null
foreach ($doc in @('install.md', 'operations.md', 'packaging.md', 'architecture.md', 'building.md', 'linux.md', 'macos.md', 'docker.md', 'service.md', 'signing.md', 'reverse-proxy.md')) {
    $path = Join-Path $root "docs\$doc"
    if (Test-Path -LiteralPath $path) { Copy-Item -LiteralPath $path -Destination (Join-Path $stage 'docs') }
}

New-Item -ItemType Directory -Force -Path (Join-Path $stage 'tools') | Out-Null
Copy-Item -LiteralPath (Join-Path $root 'scripts\fetch-ffmpeg.ps1') -Destination (Join-Path $stage 'tools')
Copy-Item -LiteralPath (Join-Path $root 'scripts\verify-ffmpeg.ps1') -Destination (Join-Path $stage 'tools')

if ($FfmpegDir) {
    # `-Profile probe` in fetch-ffmpeg.ps1 installs only ffprobe; a probe-only
    # package is a supported layout, so accept a directory without ffmpeg.exe.
    $source = if (Test-Path (Join-Path $FfmpegDir 'ffprobe.exe')) {
        $FfmpegDir
    } elseif (Test-Path (Join-Path $FfmpegDir 'bin\ffprobe.exe')) {
        Join-Path $FfmpegDir 'bin'
    } else {
        throw "ffprobe.exe not found in $FfmpegDir"
    }
    $ffmpegRoot = Join-Path $stage 'ffmpeg'
    $target = Join-Path $ffmpegRoot 'bin'
    New-Item -ItemType Directory -Force -Path $target | Out-Null
    $hasFfmpeg = Test-Path (Join-Path $source 'ffmpeg.exe')
    if ($hasFfmpeg) {
        Copy-Item -LiteralPath (Join-Path $source 'ffmpeg.exe') -Destination $target
    }
    Copy-Item -LiteralPath (Join-Path $source 'ffprobe.exe') -Destination $target
    if (-not $hasFfmpeg) {
        Write-Host 'ffprobe only: the package can scan and catalogue, but cannot transcode.'
    }

    # FFmpeg is a separate GPL/LGPL work: ship its license and a notice next to
    # the binaries instead of only the executables.
    $distribution = Split-Path -Parent $source
    foreach ($name in @('LICENSE', 'LICENSE.txt', 'COPYING', 'COPYING.GPLv3', 'README.txt')) {
        $candidate = Join-Path $distribution $name
        if (Test-Path -LiteralPath $candidate) {
            Copy-Item -LiteralPath $candidate -Destination $ffmpegRoot
        }
    }
    $notice = @"
FFmpeg is bundled with this package as a separate program.

FFmpeg is licensed under the GNU General Public License (GPL) version 3, or the
GNU Lesser General Public License (LGPL) version 2.1 or later, depending on the
build. The license file shipped next to this notice is the one that applies.

LocalReel is licensed under Apache-2.0. The two programs are aggregated in this
package; they are not combined into one work.

FFmpeg source: https://ffmpeg.org/download.html
"@
    Set-Content -LiteralPath (Join-Path $ffmpegRoot 'NOTICE.txt') -Value $notice -Encoding UTF8
}

$quickStart = @"
LocalReel $version ($arch)

Quick start (Windows):
  1. Optional one-click install: .\install.ps1 [-InstallDir <path>]
     (default: %LOCALAPPDATA%\LocalReel, Start Menu shortcut included)
     Uninstall later with .\uninstall.ps1 (keeps data unless -RemoveData).
  2. Or run in place: copy localreel.toml.example to localreel.toml,
     edit [data], then .\localreel.exe doctor && .\localreel.exe serve.
  3. Optional: start at logon via Task Scheduler: .\install-task.ps1
  4. Optional: put ffmpeg.exe/ffprobe.exe in ffmpeg\bin, or run tools\fetch-ffmpeg.ps1.
  5. Open http://127.0.0.1:3001

The web UI is embedded in localreel.exe; no separate dist directory is needed.
"@
Set-Content -LiteralPath (Join-Path $stage 'README.txt') -Value $quickStart -Encoding ASCII

$versionOutput = & (Join-Path $stage $exeName) --version
if ($LASTEXITCODE -ne 0) { throw 'packaged binary failed --version' }
Write-Host "packaged binary: $versionOutput"

# --- zip ---------------------------------------------------------------------
$zip = Join-Path $outDir "$stageName.zip"
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
Compress-Archive -LiteralPath $stage -DestinationPath $zip -CompressionLevel Optimal
$zipInfo = Get-Item -LiteralPath $zip
$hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath "$zip.sha256" -Value "$hash  $($zipInfo.Name)" -Encoding ASCII
Write-Host "package: $($zipInfo.FullName) ($([math]::Round($zipInfo.Length / 1MB, 1)) MiB)"
Write-Host "sha256:  $hash"

# --- optional MSI ------------------------------------------------------------
if ($BuildMsi) {
    Write-Host 'building MSI...'
    & (Join-Path $PSScriptRoot 'build-msi.ps1') -Stage $stage -Version $version
    if ($LASTEXITCODE -ne 0) { throw 'MSI build failed' }
}

# --- checksums ---------------------------------------------------------------
& (Join-Path $PSScriptRoot 'checksums.ps1') -Dist $outDir
