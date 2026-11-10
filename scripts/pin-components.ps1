param(
    [string]$Version
)

# Pin every component to the commit that is checked out right now.
#
# A release tag must build exactly one set of sources. Before this script, a tag
# built whatever `main` happened to be in each component repository, so the same
# tag could produce different binaries on different days. The release workflow
# refuses to build a tag whose components are not pinned, and this is the step
# that pins them.
#
# Usage:  pwsh -File scripts\pin-components.ps1            # keep the current version
#         pwsh -File scripts\pin-components.ps1 -Version 0.5.7

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $root 'components.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json

if ($Version) {
    $manifest | Add-Member -NotePropertyName version -NotePropertyValue $Version -Force
}

foreach ($name in @('core', 'web')) {
    $component = $manifest.components.$name
    $path = if ([System.IO.Path]::IsPathRooted($component.path)) {
        $component.path
    } else {
        Join-Path $root $component.path
    }
    $path = [System.IO.Path]::GetFullPath($path)
    if (-not (Test-Path -LiteralPath (Join-Path $path '.git'))) {
        throw "$name is not a git checkout: $path"
    }

    $commit = (& git -C $path rev-parse HEAD).Trim()
    $dirty = (& git -C $path status --porcelain) -join ''
    if ($dirty) {
        Write-Warning "$name has uncommitted changes; the pinned commit is what is in its working tree's HEAD, not the edits"
    }

    # ConvertFrom-Json returns PSCustomObject, which rejects assignment to a
    # property it has not seen yet; Add-Member creates it.
    $component | Add-Member -NotePropertyName commit -NotePropertyValue $commit -Force
    if ($Version) {
        $component | Add-Member -NotePropertyName version -NotePropertyValue $Version -Force
    }
    Write-Host ("{0,-5} {1}  {2}" -f $name, $commit.Substring(0, 7), $component.version)
}

# Written by hand rather than ConvertTo-Json: PowerShell's serialiser indents
# every nesting level onto one line, which turns a one-field change into a diff
# nobody can read. The shape is fixed and small, so spelling it out is honest.
$json = @"
{
  "product": "$($manifest.product)",
  "version": "$($manifest.version)",
  "description": "$($manifest.description)",
  "components": {
    "core": {
      "repository": "$($manifest.components.core.repository)",
      "path": "$($manifest.components.core.path)",
      "version": "$($manifest.components.core.version)",
      "provides": "$($manifest.components.core.provides)",
      "commit": "$($manifest.components.core.commit)"
    },
    "web": {
      "repository": "$($manifest.components.web.repository)",
      "path": "$($manifest.components.web.path)",
      "version": "$($manifest.components.web.version)",
      "provides": "$($manifest.components.web.provides)",
      "commit": "$($manifest.components.web.commit)"
    }
  }
}
"@
[System.IO.File]::WriteAllText($manifestPath, $json, (New-Object System.Text.UTF8Encoding($false)))
Write-Host "pinned $manifestPath"
