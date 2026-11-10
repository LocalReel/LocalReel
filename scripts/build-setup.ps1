param(
    # Staged product directory (contains localreel.exe), e.g. dist/localreel-0.5.0-x64
    [string]$Stage,
    # Output path; defaults to dist/localreel-<version>-x64-setup.exe
    [string]$Output,
    [string]$Version,
    # Names to leave out of the embedded payload. The server edition uses this
    # to skip `desktop`, so the thin installer does not carry 300 MB of Chromium.
    [string[]]$Exclude = @(),
    [switch]$SkipBuild
)

# Build the per-user setup executable.
#
# Windows Installer promotes a per-user MSI to a machine-wide install when an
# administrator runs it, which puts the files in one user's profile and the
# registration in HKLM - the wrong half of each. This installer does the per-user
# work itself: it embeds the staged payload, extracts it to
# %LOCALAPPDATA%\Programs\LocalReel, and registers the program for the current
# user only. See ADR-016.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root 'dist'
. (Join-Path $PSScriptRoot 'native.ps1')

if (-not $Stage) {
    $candidate = Get-ChildItem -Path $dist -Directory -Filter 'localreel-*' -ErrorAction SilentlyContinue |
        Where-Object { Test-Path (Join-Path $_.FullName 'localreel.exe') } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $candidate) { throw 'no staged product found; run compose.ps1 first' }
    $Stage = $candidate.FullName
}
if (-not (Test-Path (Join-Path $Stage 'localreel.exe'))) {
    throw "localreel.exe is missing from $Stage"
}
if (-not $Version) {
    $name = Split-Path -Leaf $Stage
    if ($name -match '^localreel-(.+)-x64$') {
        $Version = $matches[1]
    } else {
        $Version = (Get-Content (Join-Path $root 'components.json') -Raw | ConvertFrom-Json).version
    }
}
if (-not $Output) {
    $Output = Join-Path $dist ("localreel-$Version-x64-setup.exe")
}

$payload = Join-Path ([System.IO.Path]::GetTempPath()) ("localreel-payload-" + [guid]::NewGuid() + '.zip')
try {
    Write-Host "staging payload from $Stage ..."
    if ($Exclude.Count -gt 0) {
        # Compress-Archive has no exclude switch, so stage a filtered copy.
        $filtered = Join-Path ([System.IO.Path]::GetTempPath()) ("localreel-payload-src-" + [guid]::NewGuid())
        New-Item -ItemType Directory -Path $filtered | Out-Null
        try {
            Get-ChildItem -LiteralPath $Stage | Where-Object { $Exclude -notcontains $_.Name } | ForEach-Object {
                Copy-Item -LiteralPath $_.FullName -Destination $filtered -Recurse
            }
            Compress-Archive -Path (Join-Path $filtered '*') -DestinationPath $payload -CompressionLevel Optimal -Force
        } finally {
            Remove-Item -LiteralPath $filtered -Recurse -Force -ErrorAction SilentlyContinue
        }
    } else {
        Compress-Archive -Path (Join-Path $Stage '*') -DestinationPath $payload -CompressionLevel Optimal -Force
    }
    if (-not (Test-Path $payload)) { throw 'payload archive was not created' }
    Write-Host ("payload: {0:N1} MiB" -f ((Get-Item $payload).Length / 1MB))

    if (-not $SkipBuild) {
        $env:LOCALREEL_PAYLOAD = $payload
        $env:LOCALREEL_VERSION = $Version
        $env:LOCALREEL_BUILD_STAMP = (Get-Date).ToUniversalTime().ToString('yyyyMMddHHmmss')
        Push-Location $root
        try {
            Invoke-Native -Label 'cargo build (setup)' {
                & cargo build --release --manifest-path (Join-Path $root 'installer/setup/Cargo.toml')
            }
        } finally {
            Pop-Location
        }
    }

    $built = Join-Path $root 'installer/setup/target/release/localreel-setup.exe'
    if (-not (Test-Path $built)) { throw "installer binary not found at $built" }
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Output) | Out-Null
    Copy-Item -LiteralPath $built -Destination $Output -Force
    Write-Host ("setup: {0} ({1:N1} MiB)" -f $Output, ((Get-Item $Output).Length / 1MB))
} finally {
    Remove-Item -LiteralPath $payload -Force -ErrorAction SilentlyContinue
}
