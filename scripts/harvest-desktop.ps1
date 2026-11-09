param(
    [Parameter(Mandatory = $true)][string]$Root,
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$GroupId = 'DesktopComponents'
)

# Emit a WiX fragment for a staged directory tree.
#
# WiX 5 has no `heat`; before v6 the payload of a directory has to be described
# file by file. This walks a staged folder (the unpacked Electron shell, in
# practice) and writes the Directory + Component tree that installing it needs.
# Every file gets its own component, which keeps the authoring simple and lets
# Windows heal an individual file when it is replaced.
#
# Usage:  pwsh -File scripts\harvest-desktop.ps1 -Root dist\localreel-0.5.6-x64\desktop -Out out.wxs

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Root)) {
    throw "nothing to harvest at $Root"
}

$resolvedRoot = (Resolve-Path -LiteralPath $Root).Path.TrimEnd('\')
$leaf = Split-Path -Leaf $resolvedRoot

function ConvertTo-IdPart {
    param([string]$Value)
    $clean = $Value -replace '[\\/]', '_'
    $clean = $clean -replace '[^A-Za-z0-9_]', '_'
    if ($clean -match '^\d') { $clean = "n$clean" }
    return $clean
}

function Get-DirectoryId {
    param([string]$Relative)
    if ([string]::IsNullOrEmpty($Relative)) { return 'DesktopRootDir' }
    return 'DD_' + (ConvertTo-IdPart $Relative)
}

# A map from parent relative path to its children, so the tree can be emitted
# depth-first instead of in flat path order (which would nest siblings).
$children = @{}
foreach ($directory in Get-ChildItem -LiteralPath $resolvedRoot -Directory -Recurse | Sort-Object FullName) {
    $relative = $directory.FullName.Substring($resolvedRoot.Length + 1)
    $parent = Split-Path -Parent $relative
    if ($null -eq $parent) { $parent = '' }
    if (-not $children.ContainsKey($parent)) { $children[$parent] = @() }
    $children[$parent] += [pscustomobject]@{ Relative = $relative; Name = $directory.Name }
}

$builder = New-Object System.Text.StringBuilder
[void]$builder.AppendLine('<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">')
[void]$builder.AppendLine('  <Fragment>')

function Write-DirectoryBranch {
    param([string]$Relative, [string]$Name, [int]$Depth)
    $indent = '    ' + ('  ' * $Depth)
    [void]$builder.AppendLine("$indent<Directory Id=""$(Get-DirectoryId $Relative)"" Name=""$Name"">")
    foreach ($child in ($children[$Relative] | Sort-Object Name)) {
        Write-DirectoryBranch -Relative $child.Relative -Name $child.Name -Depth ($Depth + 1)
    }
    [void]$builder.AppendLine("$indent</Directory>")
}

[void]$builder.AppendLine('    <DirectoryRef Id="INSTALLFOLDER">')
Write-DirectoryBranch -Relative '' -Name $leaf -Depth 1
[void]$builder.AppendLine('    </DirectoryRef>')

$files = Get-ChildItem -LiteralPath $resolvedRoot -File -Recurse | Sort-Object FullName
$index = 0
[void]$builder.AppendLine("    <ComponentGroup Id=""$GroupId"">")
foreach ($file in $files) {
    $relative = $file.FullName.Substring($resolvedRoot.Length + 1)
    $parent = Split-Path -Parent $relative
    $directoryId = Get-DirectoryId $parent
    $index += 1
    $componentId = "DesktopFile$index"
    $fileId = "DesktopFile${index}Payload"
    $source = "`$(var.SourceDir)\$leaf\$relative"
    [void]$builder.AppendLine("      <Component Id=""$componentId"" Directory=""$directoryId"" Guid=""*"">")
    [void]$builder.AppendLine("        <File Id=""$fileId"" Source=""$source"" />")
    [void]$builder.AppendLine('      </Component>')
}
[void]$builder.AppendLine('    </ComponentGroup>')
[void]$builder.AppendLine('  </Fragment>')
[void]$builder.AppendLine('</Wix>')

[System.IO.File]::WriteAllText($Out, $builder.ToString(), (New-Object System.Text.UTF8Encoding($false)))
Write-Host "harvested $($files.Count) files in $($children.Count + 1) directories into $Out"
