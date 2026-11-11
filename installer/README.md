# Windows MSI

`localreel.wxs` is a WiX v5 source for a per-user MSI with a wizard that lets
the user choose the installation directory.

```
Scope            perUser (no administrator rights needed)
Default folder   %LOCALAPPDATA%\LocalReel
UI               WixUI_InstallDir  (Welcome -> License -> Destination Folder -> Install -> Finish)
Upgrade          MajorUpgrade, same UpgradeCode as 0.1.0/0.2.0
Override         msiexec /i <msi> INSTALLFOLDER="D:\Apps\LocalReel" /qn
```

## Prerequisites

```powershell
dotnet tool install --global wix --version 5.0.2
wix extension add -g WixToolset.UI.wixext/5.0.2
```

`scripts/build-msi.ps1` installs the UI extension automatically when it is
missing (same major/minor as the WiX tool).

## Build

```powershell
# from the product repository root, after scripts\compose.ps1 staged dist\
.\scripts\build-msi.ps1
# or explicitly, one edition at a time
.\scripts\build-msi.ps1 -Stage .\dist\localreel-0.5.6-x64 -Version 0.5.6 -Edition server
.\scripts\build-msi.ps1 -Stage .\dist\localreel-0.5.6-x64 -Version 0.5.6 -Edition desktop
```

Two editions come out of one stage. `-Edition` picks the product name
("LocalReel" or "LocalReel Desktop"), the upgrade code, and whether the shell is
installed; `auto` (the default) chooses desktop when the stage contains
`desktop\LocalReel.exe`. The two are separate products that declare each other in
the upgrade table, so installing one removes the other instead of leaving two
entries pointing at the same folder.

The script:

1. harvests the desktop payload into WiX components (`scripts/harvest-desktop.ps1`),
   or writes an empty group for the server edition;
2. builds the MSI with `wix build ... -ext WixToolset.UI.wixext`;
3. validates it with `msiexec /a` (the payload must contain `localreel.exe`);
4. opens the MSI's `Dialog` table and fails if the wizard pages are missing
   (a silent regression must not ship an installer without the directory page).

## Uninstall

```powershell
msiexec /x localreel-0.3.0-x64.msi
```

The untracked `data\` folder inside the install directory is intentionally kept
(uninstalling never deletes a library).

## Notes

- The package is per-user, so choose a folder you own (not `C:\Program Files`).
- The license page shows a short Apache-2.0 notice; the full text is installed
  as `LICENSE`.
- Artifacts are unsigned until a CA-issued code-signing certificate is
  available (see `docs/signing.md`).
