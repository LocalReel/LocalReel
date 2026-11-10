param(
    [string]$Stage,
    [string]$Version,
    [string]$Output,
    # `desktop` also installs the Electron shell; `server` does not. Defaults to
    # whichever the stage contains, so an old compose invocation keeps working.
    [ValidateSet('auto', 'server', 'desktop')][string]$Edition = 'auto',
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
. (Join-Path $PSScriptRoot 'native.ps1')

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

# WiX 3 sets $env:WIX to its install *directory*, and the v3 executables are
# candle.exe/light.exe, not `wix`. Only accept a real executable so a machine
# with WiX 3 installed still builds through the .NET tool.
function Resolve-Wix {
    $candidates = @()
    if ($env:WIX) {
        $candidates += $env:WIX
        $candidates += (Join-Path $env:WIX 'wix.exe')
    }
    $candidates += (Join-Path $env:USERPROFILE '.dotnet\tools\wix.exe')
    $command = Get-Command wix -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command) { $candidates += $command.Source }
    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf) -and $candidate -match '\.exe$') {
            return $candidate
        }
    }
    throw 'wix.exe not found; install with: dotnet tool install --global wix --version 5.0.2'
}

$wix = Resolve-Wix

$extension = 'WixToolset.UI.wixext'
# `wix extension list` writes its table to stderr; with the preference at Stop
# and the caller capturing output, that used to abort the script.
$installed = Invoke-Native -Label 'wix extension list' -Capture {
    & $wix extension list -g
}
if ($installed -notmatch [regex]::Escape($extension)) {
    $wixVersion = (Invoke-Native -Label 'wix --version' -Capture { & $wix --version }) -replace '\+.*$', ''
    Write-Host "installing $extension/$wixVersion ..."
    Invoke-Native -Label "wix extension add $extension" {
        & $wix extension add -g "$extension/$wixVersion"
    }
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

# Desktop shell payload. When the staged product carries one, the shortcuts
# point at it and every file is harvested; otherwise the installer keeps the
# console-less launcher and the component group stays empty, so a product-only
# build still works.
$desktopDir = Join-Path $Stage 'desktop'
$hasDesktop = Test-Path -LiteralPath (Join-Path $desktopDir 'LocalReel.exe')
if ($Edition -eq 'auto') {
    $Edition = if ($hasDesktop) { 'desktop' } else { 'server' }
}
if ($Edition -eq 'desktop' -and -not $hasDesktop) {
    throw "the desktop edition needs the shell in $desktopDir; run 'npm run pack' in desktop\ and re-compose"
}

# The two editions are separate products that share an install folder, so each
# names the other for the upgrade table and the two can replace one another.
$serverUpgradeCode = '58A5C330-4BBB-475F-B6D2-58C0699B7551'
$desktopUpgradeCode = '8F1C5B52-6C3E-4A0D-9A64-0C7B6D2E9F31'
if ($Edition -eq 'desktop') {
    $upgradeCode = $desktopUpgradeCode
    $otherUpgradeCode = $serverUpgradeCode
    $productName = 'LocalReel Desktop'
} else {
    $upgradeCode = $serverUpgradeCode
    $otherUpgradeCode = $desktopUpgradeCode
    $productName = 'LocalReel'
}
if (-not $Output) {
    $suffix = if ($Edition -eq 'desktop') { '-desktop' } else { '' }
    $Output = Join-Path $root "dist\localreel-$Version-x64$suffix.msi"
}

$desktopWxs = Join-Path ([System.IO.Path]::GetTempPath()) ("localreel-desktop-" + [guid]::NewGuid().ToString('N') + ".wxs")
if ($Edition -eq 'desktop') {
    & (Join-Path $PSScriptRoot 'harvest-desktop.ps1') -Root $desktopDir -Out $desktopWxs
    $launcherExe = 'desktop\LocalReel.exe'
    Write-Host "edition: desktop ($productName), shipping $((Get-ChildItem -LiteralPath $desktopDir -Recurse -File).Count) shell files"
} else {
    $empty = @'
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Fragment>
    <ComponentGroup Id="DesktopComponents" />
  </Fragment>
</Wix>
'@
    [System.IO.File]::WriteAllText($desktopWxs, $empty, (New-Object System.Text.UTF8Encoding($false)))
    $launcherExe = 'localreel-desktop.exe'
    Write-Host "edition: server ($productName), shortcuts stay on the console-less launcher"
}

try {
    Invoke-Native -Label 'wix build' {
        & $wix build (Join-Path $root 'installer\localreel.wxs') $ffmpegWxs $desktopWxs `
            -arch x64 `
            -ext $extension `
            -d "SourceDir=$Stage" `
            -d "Version=$Version" `
            -d "ProductName=$productName" `
            -d "UpgradeCode=$upgradeCode" `
            -d "OtherUpgradeCode=$otherUpgradeCode" `
            -d "LicenseRtf=$licenseRtf" `
            -d "IconFile=$icon" `
            -d "LauncherExe=$launcherExe" `
            -o $Output
    }
} finally {
    Remove-Item -LiteralPath $ffmpegWxs -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $desktopWxs -Force -ErrorAction SilentlyContinue
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
