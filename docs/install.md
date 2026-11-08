# Install LocalReel

LocalReel is a single binary with the web UI embedded. It binds
`127.0.0.1:3001` by default and is meant for one person on one machine.

Get the artifacts from the
[releases page](https://github.com/LocalReel/LocalReel/releases). The artifacts
are **unsigned**, so Windows SmartScreen may ask for confirmation
("More info" 鈫?"Run anyway").

## Requirements

- Windows 10/11 x64 (primary target). Linux tarballs are built from source.
- `ffmpeg` and `ffprobe`, either on `PATH`, in `ffmpeg\bin` next to the
  executable, or via `LOCALREEL_FFMPEG_DIR`. `tools\fetch-ffmpeg.ps1` can
  download a verified build for you.

### If you do not want the full ffmpeg

A full build is around 100 MB. Probing only needs `ffprobe`, so three profiles
are supported:

```powershell
# scan and catalogue only (one binary, no encoding)
.\tools\fetch-ffmpeg.ps1 -Url <url> -Sha256 <hash> -Profile probe

# ffmpeg + ffprobe from a smaller "essentials" build
.\tools\fetch-ffmpeg.ps1 -Url <url> -Sha256 <hash> -Profile mini

# the complete build (default)
.\tools\fetch-ffmpeg.ps1 -Url <url> -Sha256 <hash>
```

`-Profile probe` keeps only `ffprobe.exe`. LocalReel then scans, catalogues and
serves direct-playable files, but thumbnails, transcoding and HLS packaging stay
off. `localreel doctor` reports the level:

```text
  ffmpeg:    C:\...\ffmpeg.exe (missing)
  ffprobe:   C:\...\ffprobe.exe
```

`full` / `probe-only` / `missing`, and the Admin page shows the same status.
Adding `ffmpeg.exe` next to `ffprobe.exe` later needs no reconfiguration.

Upstream ships one archive, so `-Profile probe` does not shrink the download — it shrinks what is installed and removes the encoder dependency.

## Option A - per-user setup (recommended)

1. Download `localreel-<version>-x64-setup.exe`.
2. Double-click it. It installs for you alone: no administrator prompt, no
   machine-wide changes, nothing written outside your profile.
3. The installer puts the program in `%LOCALAPPDATA%\Programs\LocalReel`, adds a
   Start Menu shortcut and a desktop shortcut named *LocalReel*, and registers
   the program in **Apps & Features**.

The setup program follows the Windows display language: Chinese and English
today. Pass `/lang=zh` or `/lang=en` to override, `/S` for an unattended
install, or `/D=<folder>` to pick another folder.

Uninstalling from Apps & Features removes the program and the shortcuts but
keeps the library. Add `/removedata` to remove the library as well.

## Option B - MSI (all users)

The MSI installs for every account on the machine, so it asks for administrator
rights. Use it when several Windows accounts should share one installation.

1. Download `localreel-<version>-x64.msi`.
2. Double-click it and pick a folder (default `%LOCALAPPDATA%\Programs\LocalReel`).
3. Unattended installs can pass the folder explicitly:

```powershell
msiexec /i localreel-0.5.0-x64.msi INSTALLFOLDER="D:\Apps\LocalReel" /qn
```

Windows Installer cannot keep this package per-user when an administrator runs
it: it registers machine-wide. For a strictly per-user install use Option A.

### "Could not set file security for file 'X:\Config.Msi\....rbf'"

Windows Installer keeps its rollback files in a `Config.Msi` folder at the root
of every volume it writes to. If that folder already exists and its permissions
belong to another account — usually left behind by an earlier elevated install —
the per-user installer cannot change them, and the wizard stops with `Error: 5`.

Pick one:

- install to the default `%LOCALAPPDATA%\Programs\LocalReel`, or to a folder on a drive you
  already install software to;
- use the portable zip on that drive instead: it never touches `Config.Msi`;
- or remove the stale folder once from an **administrator** prompt and retry —
  replace `D:` with the drive the error names:

  ```powershell
  takeown /f D:\Config.Msi /a /r /d y
  icacls D:\Config.Msi /reset /t /c /l
  rmdir /s /q D:\Config.Msi
  ```

The message names the volume that needs this and is not specific to LocalReel.

## Option C - portable zip

```powershell
# extract the zip, then from inside it:
.\install.ps1 -InstallDir 'D:\Apps\LocalReel'
# other switches: -DesktopShortcut, -StartOnLogin, -NoShortcuts
```

Without `-InstallDir` it installs to `%LOCALAPPDATA%\Programs\LocalReel` and writes an
`install.json` manifest. You can also run the package in place
(`.\localreel.exe doctor`, `.\localreel.exe serve`) without installing.

## First run

1. Start LocalReel from the Start Menu, the desktop shortcut, or tick **Launch
   LocalReel** on the last wizard page. The app starts with a **tray icon**, the
   browser follows at <http://127.0.0.1:3001>, and the console window hides
   itself. Right-click the tray icon to **open the UI** or **quit**.
2. Create the **owner account**: a username and a password of at least eight
   characters. This account manages the household.
3. Open **Admin 鈫?Add a folder**, paste a path (`D:\Media`, `/volume1/media`, a
   mounted share) and press **Add and scan**. Scanning is a background job:
   metadata and thumbnails arrive progressively and the progress is visible in
   Admin.
4. Playback is direct for MP4 (H.264/AAC). H.264 files in MKV/MOV/TS/FLV/AVI
   are remuxed into MP4 in seconds; other codecs are re-encoded (slow).

**Stopping it.** Right-click the tray icon 鈫?**Quit**, or open **Admin 鈫?Server 鈫?Shut down server** (the path that works without a tray, e.g. on
Linux and macOS). When you start the server from a terminal, `--tray` hides
nothing: the console stays, because the console belongs to your terminal, and
`Ctrl+C` still stops it.

```powershell
.\localreel.exe serve              # console only, Ctrl+C to stop
.\localreel.exe serve --tray       # tray icon; hides the console when it owns one
.\localreel.exe serve --no-open    # do not open the browser
```

Starting LocalReel again while it runs just reopens the UI. If another program
holds the port, the server moves to the next free one and prints the address it
used.

## Accounts

The owner can create one viewer account per person under **Admin 鈫?Accounts**.
The library is shared; progress, likes and favourites are per account.
Disabling an account ends its sessions immediately, and resetting a password
signs that account out everywhere.

## Where things live

| What | Location |
|---|---|
| Program | the folder chosen during install (default `%LOCALAPPDATA%\Programs\LocalReel`) |
| Data (database, thumbnails, transcodes, HLS cache) | `%LOCALAPPDATA%\LocalReel` |
| Config | `%APPDATA%\LocalReel\localreel.toml` (optional) |
| Logs | `%LOCALAPPDATA%\LocalReel\logs\localreel.log` |

Program and data are deliberately separate: upgrading, uninstalling or moving
the program never touches the library or the accounts.

A relative `[data] dir` in the config file is resolved against **that file's
directory**, never the working directory the server was started from, so a
double-click, a shortcut and a terminal all use the same library. Prefer an
absolute path when the library lives on another drive.

Override with `LOCALREEL_DATA_DIR`, `LOCALREEL_LOG_DIR`, `LOCALREEL_CONFIG`,
`LOCALREEL_PORT`, `LOCALREEL_HOST` when you need a different layout.

## Upgrade and uninstall

- **per-user setup**: install a newer `*-setup.exe` over the old one; it replaces
  the program and keeps the library. Apps & Features removes it, or run
  `localreel-setup.exe /uninstall` from the install folder. Add `/removedata` to
  remove the library too.
- **MSI**: installing a newer version upgrades in place (data is untouched).
  `msiexec /x localreel-<version>-x64.msi` uninstalls the program; the untracked
  `data\` folder is kept.
- **zip**: run `.\uninstall.ps1` (keeps `data\` and `localreel.toml`), or
  `.\uninstall.ps1 -RemoveData` for a full removal.

## Start at login (optional)

The built-in command needs no administrator rights and no script:

```powershell
.\localreel.exe autostart enable    # HKCU\Software\Microsoft\Windows\CurrentVersion\Run
.\localreel.exe autostart status
.\localreel.exe autostart disable
```

It registers `"<install>\localreel.exe" serve --mode desktop --tray --no-open`,
so at login the tray icon appears and no browser is taken over. A console window
flashes for a moment before the tray hides it; a scheduled task avoids the flash
but needs administrator rights, which is why this is the default.

For the headless alternative, `install-task.ps1` registers a Task Scheduler
entry that runs the server persona at logon (no console at all):

```powershell
.\install-task.ps1            # runs `localreel.exe serve` at logon
.\install-task.ps1 -StartNow  # register and start immediately
.\install-task.ps1 -Uninstall -StopServer
```

See [service.md](service.md).

## Verify a download

Every release ships `SHA256SUMS.txt`:

```powershell
Get-FileHash .\localreel-0.5.0-x64.zip -Algorithm SHA256
```

## Other platforms

- Linux: [linux.md](linux.md)
- Docker: [docker.md](docker.md) (the image recipe lives in the core component)
- Operations and troubleshooting: [operations.md](operations.md)

