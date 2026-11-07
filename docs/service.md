# Run LocalReel automatically (Windows)

The installer creates Start Menu and desktop shortcuts that start the app with
a tray icon, and does not register anything that runs by itself. Three supported
options for starting it automatically:

- `localreel autostart enable` — per-user Run key, desktop persona with the
  tray, no administrator rights (documented in [install.md](install.md));
- a Task Scheduler entry, headless server persona (Option A below);
- a Startup-folder shortcut (Option B below).

## Option A — Task Scheduler (recommended)

Register a scheduled task that runs `localreel.exe serve` (the task has no
console, so the server detects the `server` persona automatically: fixed port,
no browser):

```powershell
# from the install directory
.\install-task.ps1
```

Or from the repository against a custom install:

```powershell
.\scripts\install-task.ps1 -InstallDir 'D:\Apps\LocalReel'
```

Defaults:

- Task name `LocalReel Server`
- Trigger: **at logon** of the current user (no admin required)
- Action: `<install>\localreel.exe serve`, working directory `<install>`
- Restart up to 3 times at 1-minute intervals; no execution time limit
- If the task is already registered, it is replaced

Start it immediately:

```powershell
.\install-task.ps1 -StartNow
```

Inspect / stop / remove:

```powershell
Get-ScheduledTask -TaskName 'LocalReel Server' | Get-ScheduledTaskInfo
Stop-ScheduledTask -TaskName 'LocalReel Server'
.\install-task.ps1 -Uninstall
.\install-task.ps1 -Uninstall -StopServer   # also stop a matching process
```

### Boot instead of logon

```powershell
# elevated PowerShell required
.\install-task.ps1 -AtStartup
```

`-AtStartup` registers the task as `SYSTEM` at boot. Use an install directory
whose data/config are writable by SYSTEM; a per-user `%LOCALAPPDATA%` install is
usually fine, but verify with a reboot. Prefer the default logon task for a
personal desktop.

## Option B — Startup folder shortcut

The installer can place a shortcut in the Startup folder:

```powershell
.\install.ps1 -StartOnLogin
```

This starts a console window at logon; closing the window stops the server.
The Task Scheduler option is quieter and supports restart-on-failure.

## Logs

Every run appends to `%LOCALAPPDATA%\LocalReel\logs\localreel.log` and rotates
it to `localreel.log.1` at 8 MiB, whether it was started by a shortcut, the
Task Scheduler or a terminal. Set `LOCALREEL_LOG_DIR` to move it, and `RUST_LOG`
(e.g. `RUST_LOG=debug`) to change verbosity.

For health, use `GET /api/v1/health` and `GET /api/v1/admin/health`.

## Linux

The `.deb` installs a hardened system unit (`/lib/systemd/system/localreel.service`)
and the tarball ships both the system and user units in
`localreel-core/packaging/`. See [linux.md](linux.md). There is no Task
Scheduler equivalent.
