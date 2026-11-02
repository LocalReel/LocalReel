# Install LocalReel

LocalReel is a single binary with the web UI embedded. It binds
`127.0.0.1:3001` by default and is meant for one person on one machine.

Get the artifacts from the
[releases page](https://github.com/LocalReel/LocalReel/releases). The artifacts
are **unsigned**, so Windows SmartScreen may ask for confirmation
("More info" → "Run anyway").

## Requirements

- Windows 10/11 x64 (primary target). Linux tarballs are built from source.
- `ffmpeg` and `ffprobe`, either on `PATH`, in `ffmpeg\bin` next to the
  executable, or via `LOCALREEL_FFMPEG_DIR`. `tools\fetch-ffmpeg.ps1` can
  download a verified build for you.

## Option A — MSI (recommended)

1. Download `localreel-<version>-x64.msi`.
2. Double-click it. The wizard asks where to install:
   - default: `%LOCALAPPDATA%\LocalReel` (per-user, no administrator rights);
   - or **Browse…** to pick any folder you own, e.g. `D:\Apps\LocalReel`.
3. Finish the wizard. A Start Menu shortcut named *LocalReel* is created; it
   starts the server (`localreel.exe serve`) in the install folder.

Unattended installs can pass the folder explicitly:

```powershell
msiexec /i localreel-0.3.0-x64.msi INSTALLFOLDER="D:\Apps\LocalReel" /qn
```

Because the package is per-user, do not pick a location that needs
administrator rights (such as `C:\Program Files`).

## Option B — portable zip

```powershell
# extract the zip, then from inside it:
.\install.ps1 -InstallDir 'D:\Apps\LocalReel'
# other switches: -DesktopShortcut, -StartOnLogin, -NoShortcuts
```

Without `-InstallDir` it installs to `%LOCALAPPDATA%\LocalReel` and writes an
`install.json` manifest. You can also run the package in place
(`.\localreel.exe doctor`, `.\localreel.exe serve`) without installing.

## First run

1. Start LocalReel (Start Menu, or `localreel.exe serve` in the install folder).
2. Open <http://127.0.0.1:3001>.
3. Open **Admin → Sources**, add your media folder, and run a scan. Scanning is
   a background job: metadata and thumbnails arrive progressively and the
   progress is visible in Admin.
4. Playback is direct for MP4 (H.264/AAC). H.264 files in MKV/MOV/TS/FLV/AVI
   are remuxed into MP4 in seconds; other codecs are re-encoded (slow).

## Where things live

| What | Location |
|---|---|
| Program | the folder chosen during install |
| Data (database, thumbnails, transcodes) | `data\` inside that folder |
| Config | `localreel.toml` inside that folder (optional; env overrides win) |
| Logs | the terminal / scheduled-task output |

Override with `LOCALREEL_DATA_DIR`, `LOCALREEL_CONFIG`, `LOCALREEL_PORT`,
`LOCALREEL_HOST` when you need a different layout.

## Upgrade and uninstall

- **MSI**: installing a newer version upgrades in place (data is untouched).
  `msiexec /x localreel-<version>-x64.msi` uninstalls the program; the untracked
  `data\` folder is kept.
- **zip**: run `.\uninstall.ps1` (keeps `data\` and `localreel.toml`), or
  `.\uninstall.ps1 -RemoveData` for a full removal.

## Start at login (optional)

```powershell
.\install-task.ps1            # runs `localreel.exe serve` at logon
.\install-task.ps1 -StartNow  # register and start immediately
.\install-task.ps1 -Uninstall -StopServer
```

See [service.md](service.md).

## Verify a download

Every release ships `SHA256SUMS.txt`:

```powershell
Get-FileHash .\localreel-0.3.0-x64.zip -Algorithm SHA256
```

## Other platforms

- Linux: [linux.md](linux.md)
- Docker: [docker.md](docker.md) (the image recipe lives in the core component)
- Operations and troubleshooting: [operations.md](operations.md)
