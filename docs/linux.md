# Linux packaging and install

Linux is a first-class target: the same binary runs as a desktop app on Ubuntu
and as the headless server of a NAS or a small always-on machine. It detects
that it was started by a service manager and stays on a fixed port without
opening a browser (see [ADR-011](../docs/architecture.md) and `--mode`).

Two packages are produced from the product repository:

| Artifact | For | Contains |
|---|---|---|
| `localreel-<version>-x86_64.tar.gz` | manual installs | binary, docs, `install.sh` / `uninstall.sh` |
| `localreel_<version>_amd64.deb` | Debian/Ubuntu servers | `/usr/bin/localreel`, systemd unit, docs |

## Build a tarball

From the product repository on Linux (components are resolved as siblings by default):

```bash
# the server embeds localreel-web/dist at compile time
./scripts/package-linux.sh --build-web
```

Output:

```text
dist/localreel-<version>-x86_64.tar.gz
dist/localreel-<version>-x86_64.tar.gz.sha256
```

Options:

```text
--version <v>        override the workspace version
--out <dir>          output directory
--skip-build         package target/release/localreel as-is
--ffmpeg-dir <dir>   copy ffmpeg/ffprobe into ffmpeg/bin
--allow-non-linux    structural test only (does not make the binary run)
```

Prerequisites: Rust stable, Node/npm if `--build-web`, and (optionally) ffmpeg.
The web UI is embedded in the binary; no separate `dist` is needed at runtime.

## Build a .deb

```bash
./scripts/package-deb.sh --build-web
sudo apt install ./dist/localreel_<version>_amd64.deb
```

The package creates a `localreel` system account, installs
`/lib/systemd/system/localreel.service`, and leaves the service **disabled**:
enable it once you have decided that this machine should serve the library.

```bash
sudo systemctl enable --now localreel
journalctl -u localreel -f
```

Data lives in `/var/lib/localreel`, logs in `/var/log/localreel` (and in the
journal). The service unit sets `LOCALREEL_MODE=server`, so it never tries to
open a browser.

## Install from the tarball

```bash
tar -xzf localreel-<version>-x86_64.tar.gz
cd localreel-<version>-x86_64

./install.sh                    # ~/.local/share/localreel + app menu entry
./install.sh --start-on-login   # also ~/.config/autostart/localreel.desktop
./install.sh --install-dir /opt/localreel --no-desktop
```

`install.sh` only writes the files it owns; the library and accounts live under
`~/.local/share/localreel` and `~/.config/localreel` either way.

## Run as a service

The core repository ships the units the package uses
(`localreel-core/packaging/`):

- `localreel.service` — hardened **system** unit, runs as `localreel`, data in
  `/var/lib/localreel`.
- `localreel.user.service` — **user** unit for a desktop machine, keeps XDG
  directories.

```bash
# system service
sudo cp localreel.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now localreel

# user service (desktop)
mkdir -p ~/.config/systemd/user
cp localreel.user.service ~/.config/systemd/user/localreel.service
systemctl --user daemon-reload
systemctl --user enable --now localreel
```

## Uninstall

```bash
./uninstall.sh --yes                 # keeps data and configuration
./uninstall.sh --remove-data --yes   # deletes everything
./uninstall.sh --stop-server --yes   # stop a process matching this install
```

The `.deb` removes the program and the unit; `/var/lib/localreel` and
`/var/log/localreel` are kept.

## Verify (when a Linux machine is available)

```bash
./localreel --version
./localreel doctor     # mode, data/config/log dirs, web: embedded, ffmpeg
./localreel serve --mode server &
curl -fsS http://127.0.0.1:3001/api/v1/health
curl -fsS http://127.0.0.1:3001/api/v1/auth/status   # setup_required before first run
```

Then open the UI, create the owner account, add a source, scan, and play one
file. Exposing the server beyond loopback is supported now that accounts exist,
but put it behind a reverse proxy: see [reverse-proxy.md](reverse-proxy.md).

See [operations.md](operations.md) for backup/upgrade and
[packaging.md](packaging.md) for ffmpeg discovery.
