# Packaging the product

This repository is the only place that produces distributable artifacts. The
components (`localreel-core`, `localreel-web`, `localreel-format`) never build
installers or release zips; they are consumed here.

## Composition contract

`components.json` pins which component versions belong to the product:

```json
{ "core": { "path": "../localreel-core", "version": "0.3.0" },
  "web":  { "path": "../localreel-web",  "version": "0.3.0" } }
```

`scripts/compose.ps1` resolves the checkouts (by `path`, by sibling directory,
or via `-CoreDir`/`-WebDir`/`-ComponentsDir`), verifies the versions, builds the
web UI, builds the server with the UI embedded (the core crate reads it from
`LOCALREEL_WEB_DIR`), stages the docs and scripts, and writes the zip.

```powershell
.\scripts\compose.ps1                    # zip only
.\scripts\compose.ps1 -BuildMsi          # zip + MSI
.\scripts\compose.ps1 -SkipWeb -BuildMsi # reuse an existing web build
.\scripts\compose.ps1 -ComponentsDir D:\checkouts -Force
```

Outputs land in `dist/`:

- `localreel-<version>-x64.zip` + `.sha256`
- `localreel-<version>-x64/` (the stage)
- `localreel-<version>-x64.msi` (with `-BuildMsi`)
- `SHA256SUMS.txt` (both artifacts)

## What the package contains

```text
localreel.exe              server + embedded web UI
localreel.toml.example     configuration template
README.md / README.txt     product readme + quick start
LICENSE                    Apache-2.0
install.ps1
uninstall.ps1
install-task.ps1
docs/install.md            product docs
docs/packaging.md
docs/release-checklist.md
docs/operations.md         (from the core component)
docs/linux.md, service.md, signing.md, docker.md
tools/fetch-ffmpeg.ps1
tools/verify-ffmpeg.ps1
ffmpeg/bin/...             only with -FfmpegDir
```

## MSI

Prerequisites (once per machine):

```powershell
dotnet tool install --global wix --version 5.0.2
wix extension add -g WixToolset.UI.wixext/5.0.2
```

`scripts/build-msi.ps1` builds `installer/localreel.wxs` against the staged
directory. The package is **per-user** and uses `WixUI_InstallDir`, so the
wizard lets the user choose the install folder; the default is
`%LOCALAPPDATA%\LocalReel`. Unattended installs override it with
`INSTALLFOLDER`:

```powershell
msiexec /i dist\localreel-0.3.0-x64.msi INSTALLFOLDER="D:\Apps\LocalReel" /qn
```

The script also validates the package:

1. `msiexec /a` administrative extract produces `localreel.exe`;
2. the MSI's `Dialog` table contains the wizard pages (install-directory page
   included), so a silent regression cannot ship an installer without the
   directory prompt.

## Linux tarball

From a Linux host, in this repository:

```bash
./scripts/package-linux.sh --build-web
# optional: --core <dir> --web <dir> --ffmpeg-dir <dir> --allow-non-linux
```

## Checksums

`compose.ps1` calls `scripts/checksums.ps1` at the end; run it manually after
re-signing or repacking:

```powershell
.\scripts\checksums.ps1
```

It writes `dist/SHA256SUMS.txt` for every `.zip`/`.msi` in `dist/`. Record the
values for the current release in
[docs/release/v1.0-readiness.md](release/v1.0-readiness.md).

## Signing

See [signing.md](signing.md). Sign the exe in the stage and the MSI, then
re-create the zip so the archive contains the signed exe.

## Where the components live

| Concern | Repository |
|---|---|
| server code, tests, dev server | `localreel-core` |
| UI source and its build | `localreel-web` |
| data formats | `localreel-format` |
| composition, installers, docs, releases | this repository |
