$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ('localreel-uninstall-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$oldLocal = $env:LOCALAPPDATA
$oldApp = $env:APPDATA
try {
    $env:LOCALAPPDATA = Join-Path $root 'local'
    $env:APPDATA = Join-Path $root 'roaming'
    $links = Join-Path $root 'shortcuts'
    New-Item -ItemType Directory -Path $links | Out-Null
    foreach ($mode in 'missing','legacy','v2') {
        $install = Join-Path $root $mode
        New-Item -ItemType Directory -Path (Join-Path $install 'library') | Out-Null
        Set-Content (Join-Path $install 'library/media.mp4') 'media'
        Set-Content (Join-Path $install 'library/localreel.db') 'database'
        Set-Content (Join-Path $install 'localreel.toml') "[data]`ndir='library'"
        Set-Content (Join-Path $install 'localreel.exe') 'fake-program-never-executed'
        if ($mode -ne 'missing') {
            $manifest = @{ files = @('localreel.exe', 'library/media.mp4', 'library/localreel.db') }
            if ($mode -eq 'v2') { $manifest.file_manifest_version = 2; $manifest.files = @('localreel.exe','../outside.txt') }
            $manifest | ConvertTo-Json | Set-Content (Join-Path $install 'install.json')
        }
        Set-Content (Join-Path $root 'outside.txt') 'outside'
        & "$PSScriptRoot/uninstall.ps1" -InstallDir $install -Force -StartMenuDir $links -DesktopDir $links -StartupDir $links
        foreach ($kept in 'library/media.mp4','library/localreel.db','localreel.toml') {
            if (-not (Test-Path (Join-Path $install $kept))) { throw "$mode deleted $kept" }
        }
        if (Test-Path (Join-Path $install 'localreel.exe')) { throw "$mode did not remove program" }
        if (-not (Test-Path (Join-Path $root 'outside.txt'))) { throw 'path traversal' }
        Write-Host "PASS $mode preserves custom library and config"
    }
} finally {
    $env:LOCALAPPDATA = $oldLocal
    $env:APPDATA = $oldApp
    # This exact directory was created above exclusively for this test.
    if ((Split-Path -Leaf $root) -like 'localreel-uninstall-test-*') { Remove-Item -LiteralPath $root -Recurse -Force }
}
