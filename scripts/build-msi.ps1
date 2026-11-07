param(
    [string]$Stage,
    [string]$Version,
    [string]$Output,
    [switch]$SkipValidation
)

# Build the per-user MSI from a staged product directory.
#
# The installer ships the WixUI_InstallDir wizard, so the user picks the
# install directory (default %LOCALAPPDATA%\LocalReel). Unattended installs can
# override it:  msiexec /i localreel-x.y.z-x64.msi INSTALLFOLDER="D:\Apps\LocalReel" /qn
#
# Requires WiX 5 plus the UI extension:
#   dotnet tool install --global wix --version 5.0.2
#   wix extension add -g WixToolset.UI.wixext/5.0.2

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

if (-not $Stage) {
    $candidate = Get-ChildItem -LiteralPath (Join-Path $root 'dist') -Directory -Filter 'localreel-*-x64' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $candidate) {
        throw 'no staged product found; run scripts\compose.ps1 first or pass -Stage'
    }
    $Stage = $candidate.FullName
}
$exe = Join-Path $Stage 'localreel.exe'
if (-not (Test-Path -LiteralPath $exe)) {
    throw "missing $exe; compose the product first"
}

if (-not $Version) {
    $Version = if ((Split-Path -Leaf $Stage) -match '^localreel-(.+)-x64$') {
        $Matches[1]
    } else {
        throw 'could not infer the version from the stage directory; pass -Version'
    }
}

$wix = if ($env:WIX) {
    $env:WIX
} elseif (Get-Command wix -ErrorAction SilentlyContinue) {
    (Get-Command wix).Source
} elseif (Test-Path (Join-Path $env:USERPROFILE '.dotnet\tools\wix.exe')) {
    Join-Path $env:USERPROFILE '.dotnet\tools\wix.exe'
} else {
    throw 'wix.exe not found; install with: dotnet tool install --global wix --version 5.0.2'
}

$extension = 'WixToolset.UI.wixext'
$installed = & $wix extension list -g 2>&1
if ($LASTEXITCODE -ne 0) { throw 'wix extension list failed' }
if ($installed -notmatch [regex]::Escape($extension)) {
    $wixVersion = (& $wix --version) -replace '\+.*$', ''
    Write-Host "installing $extension/$wixVersion ..."
    & $wix extension add -g "$extension/$wixVersion"
    if ($LASTEXITCODE -ne 0) { throw "could not install $extension" }
}

if (-not $Output) {
    $Output = Join-Path $root "dist\localreel-$Version-x64.msi"
}
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Output) | Out-Null
if (Test-Path -LiteralPath $Output) { Remove-Item -LiteralPath $Output -Force }

$licenseRtf = Join-Path $root 'installer\license.rtf'
$icon = Join-Path $root 'installer\localreel.ico'
if (-not (Test-Path -LiteralPath $icon)) {
    throw "missing $icon"
}

# Optional ffmpeg payload: generate components only for files that were staged,
# so a package without bundled ffmpeg still builds an empty group.
$ffmpegWxs = Join-Path ([System.IO.Path]::GetTempPath()) ("localreel-ffmpeg-" + [guid]::NewGuid().ToString('N') + ".wxs")
$builder = New-Object System.Text.StringBuilder
[void]$builder.AppendLine('<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">')
[void]$builder.AppendLine('  <Fragment>')
[void]$builder.AppendLine('    <ComponentGroup Id="FfmpegComponents">')
$ffmpegFiles = @(
    @{ Id = 'FfmpegExe'; Directory = 'FfmpegBinFolder'; Path = 'ffmpeg\bin\ffmpeg.exe' },
    @{ Id = 'FfprobeExe'; Directory = 'FfmpegBinFolder'; Path = 'ffmpeg\bin\ffprobe.exe' },
    @{ Id = 'FfmpegNotice'; Directory = 'FfmpegFolder'; Path = 'ffmpeg\NOTICE.txt' },
    @{ Id = 'FfmpegLicense'; Directory = 'FfmpegFolder'; Path = 'ffmpeg\LICENSE' },
    @{ Id = 'FfmpegReadme'; Directory = 'FfmpegFolder'; Path = 'ffmpeg\README.txt' }
)
foreach ($file in $ffmpegFiles) {
    if (Test-Path -LiteralPath (Join-Path $Stage $file.Path)) {
        [void]$builder.AppendLine(
            ('      <Component Id="{0}" Directory="{1}" Guid="*"><File Id="{0}File" Source="$(var.SourceDir)\{2}" /></Component>' -f $file.Id, $file.Directory, $file.Path)
        )
    }
}
[void]$builder.AppendLine('    </ComponentGroup>')
[void]$builder.AppendLine('  </Fragment>')
[void]$builder.AppendLine('</Wix>')
[System.IO.File]::WriteAllText($ffmpegWxs, $builder.ToString(), (New-Object System.Text.UTF8Encoding($false)))

try {
    & $wix build (Join-Path $root 'installer\localreel.wxs') $ffmpegWxs `
        -arch x64 `
        -ext $extension `
        -d "SourceDir=$Stage" `
        -d "Version=$Version" `
        -d "LicenseRtf=$licenseRtf" `
        -d "IconFile=$icon" `
        -o $Output
    if ($LASTEXITCODE -ne 0) { throw "wix build failed with exit $LASTEXITCODE" }
} finally {
    Remove-Item -LiteralPath $ffmpegWxs -Force -ErrorAction SilentlyContinue
}

$info = Get-Item -LiteralPath $Output
Write-Host "MSI: $($info.FullName) ($([math]::Round($info.Length / 1MB, 2)) MiB)"

if (-not $SkipValidation) {
    # Administrative extract proves the payload is complete.
    $extract = Join-Path ([System.IO.Path]::GetTempPath()) ("localreel-msi-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Path $extract | Out-Null
    try {
        $arguments = @('/a', "`"$Output`"", '/qn', "TARGETDIR=`"$extract`"")
        $process = Start-Process -FilePath 'msiexec.exe' -ArgumentList $arguments -Wait -PassThru
        if ($process.ExitCode -ne 0) {
            throw "msiexec administrative extract failed with $($process.ExitCode)"
        }
        $extracted = Get-ChildItem -LiteralPath $extract -Recurse -Filter 'localreel.exe' -File |
            Select-Object -First 1
        if (-not $extracted) {
            throw 'administrative extract did not produce localreel.exe'
        }
        Write-Host "extract check: $($extracted.FullName)"
    } finally {
        Remove-Item -LiteralPath $extract -Recurse -Force -ErrorAction SilentlyContinue
    }

    # The directory-choosing UI must actually be in the package.
    try {
        $installer = New-Object -ComObject WindowsInstaller.Installer
        $database = $installer.OpenDatabase($Output, 0)
        $view = $database.OpenView('SELECT `Dialog` FROM `Dialog`')
        $view.Execute()
        $dialogs = 0
        while ($view.Fetch()) { $dialogs++ }
        $view.Close()
        if ($dialogs -lt 5) {
            throw "MSI contains only $dialogs dialogs; expected the WixUI_InstallDir wizard"
        }
        Write-Host "UI check: $dialogs dialogs, including the install-directory page"
    } catch {
        throw "MSI UI check failed: $($_.Exception.Message)"
    }
}
