# LocalReel

**A private, scrollable streaming app for your own library.** Think of a
Douyin/Bilibili-style experience — open it, something plays, swipe to the next
episode — except the library is your disk, not the internet. One lightweight
binary with the web UI built in: no cloud service, no account, no ads.

Point LocalReel at folders on your machine; it scans them, generates metadata and
thumbnails with `ffmpeg`/`ffprobe`, and serves a browser UI on
<http://127.0.0.1:3001> for the feed, series and movie playback, resume, search
and admin. The product direction and 1.0 scope live in
[docs/product.md](docs/product.md).

This repository is the **product**: it owns the download/install entry point,
the composition of the components, and the documentation. The server, the UI
and the format crate are separate repositories, each independently buildable,
testable and releasable. See [docs/architecture.md](docs/architecture.md).

## What it does

- **Library** — add local folders as sources, scan them, and browse a paginated
  library with metadata and thumbnails produced by ffmpeg.
- **Reels** — a vertical short-video feed (`/watch`) over the same library, for
  watching episodes in order with progress carried over.
- **Resume** — playback position is stored per file on the server, so
  **Continue** picks up where you stopped; positions survive upgrades.
- **Search and filters** — search title, filename or path; filter by
  **Continue** or **Favorites**; archived media can be shown on demand.
- **Playback** — direct streaming with HTTP Range requests; H.264 files in
  MKV/MOV/TS are remuxed instead of re-encoded; other formats are transcoded to
  cached variants that can be cancelled.
- **Admin** — backup/restore, transcode/thumbnail cache GC, scan history and
  issues, source archive/remap, and a job list with cancellation.
- **Bilingual UI** — Simplified Chinese and English on every page, with a
  中文 / EN switch in the header. The first visit follows the browser language
  and the choice is remembered.
- **Local by design** — binds loopback only by default, single user, no
  authentication layer; `ffmpeg`/`ffprobe` are not bundled and are discovered
  next to the binary or on `PATH`.
- **Distribution** — Windows zip and per-user MSI; Linux tarball and a Docker
  image are built from source on those hosts.

## Download and install

Releases are published from the product repository:
**<https://github.com/LocalReel/LocalReel/releases>**

Each release carries:

| Artifact | Notes |
|---|---|
| `localreel-<version>-x64.msi` | Per-user Windows installer (WiX wizard) |
| `localreel-<version>-x64.zip` + `.sha256` | Portable Windows package |
| `SHA256SUMS.txt` | SHA256 for every artifact above |

### Windows MSI (recommended)

Run the MSI. It is **per-user** (no administrator rights) and the wizard lets
you **choose the install directory**; the default is
`%LOCALAPPDATA%\LocalReel`. Pick a folder you own — not `C:\Program Files`,
which needs administrator rights. It creates a Start Menu shortcut and does not
touch your `data\` folder or `localreel.toml`. Uninstalling removes the program
files but **keeps your library data and configuration**.

```powershell
msiexec /i localreel-<version>-x64.msi
```

Unattended installs can choose the directory up front:

```powershell
msiexec /i localreel-<version>-x64.msi INSTALLFOLDER="D:\Apps\LocalReel" /qn
```

### Windows zip (portable)

Extract the zip anywhere and either run `localreel.exe serve` in place, or use
the included installer to copy it to a stable location:

```powershell
# installs to %LOCALAPPDATA%\LocalReel and creates a Start Menu shortcut
.\install.ps1

# custom install directory
.\install.ps1 -InstallDir 'D:\Apps\LocalReel'
```

Start LocalReel from the Start Menu (or `localreel.exe serve`) and open
<http://127.0.0.1:3001>. Data lives in `data\` next to the binary and settings
in `localreel.toml`; upgrades and uninstalls keep both. To start automatically
at logon, use the included `install-task.ps1` (see [docs/service.md](docs/service.md)).

> **Unsigned builds.** The MSI and the executables are **not code-signed yet**,
> so Windows SmartScreen will show "Windows protected your PC" for both. Verify
> the SHA256 of what you downloaded against `SHA256SUMS.txt` before running it.
> Code signing is a public v1.0 blocker — see
> [docs/release/v1.0-readiness.md](https://github.com/LocalReel/LocalReel/blob/main/docs/release/v1.0-readiness.md).

### 简体中文快速说明

Windows 用户从 [Releases](https://github.com/LocalReel/LocalReel/releases)
下载最新的 `localreel-<版本>-x64.msi` 或 `-x64.zip`。运行 MSI 时，安装向导会
让你**选择安装位置**（默认 `%LOCALAPPDATA%\LocalReel`，无需管理员权限）；如果
下载的是 zip，解压后执行 `.\install.ps1` 会装到同一默认位置，也可以用
`.\install.ps1 -InstallDir 'D:\Apps\LocalReel'` 指定目录。安装完成后从开始菜单
启动 LocalReel（或在该目录运行 `.\localreel.exe serve`），浏览器打开
<http://127.0.0.1:3001>，在界面里添加媒体目录即可开始扫描。Library、Reels 和
Admin 页面都支持中文 / English 切换。

当前产物尚未代码签名，SmartScreen 会提示"未知发布者"，请先用 `SHA256SUMS.txt`
校验下载文件的 SHA256。卸载时 `data\` 与 `localreel.toml` 默认保留，升级不会
丢失播放进度。

### Linux

The Linux tarball is built from source on a Linux host with
`./scripts/package-linux.sh --build-web` (from this repository); extract it and
run `./install.sh` for a per-user install in `~/.local/share/localreel`. Linux
is a secondary target verified under WSL2; see [docs/linux.md](docs/linux.md).

### Docker

The image runs the same binary with `ffmpeg` inside and a `/data` volume; the
Compose file mounts your media directory read-only. See
[docs/docker.md](docs/docker.md).

## Build from source

Building the product takes three steps; the full walkthrough is in
[docs/building.md](docs/building.md).

1. **Get the four repositories as siblings** — `LocalReel/` (this one),
   `localreel-core/`, `localreel-web/` and `localreel-format/`.
2. **Develop** — the server runs from `localreel-core` with
   `.\scripts\dev.ps1` (port 3101, data in `.dev-data/`, isolated from any
   installed release on 3001); the UI runs from `localreel-web` with
   `npm run dev` or builds with `npm run build`.
3. **Compose artifacts** — from this repository, `.\scripts\compose.ps1`
   builds the UI, embeds it into the server binary, and writes
   `dist/localreel-<version>-x64.zip` (add `-BuildMsi` for the MSI), then
   `.\scripts\checksums.ps1` writes `dist/SHA256SUMS.txt`.

## Repository structure

LocalReel is a **multi-repository product**. Each component is its own git
repository with its own build, tests and tag; the product repository only pins
versions and composes them.

| Repository | Role | Produces |
|---|---|---|
| **LocalReel** (this repo) | Product shell: documentation, download entry point, installers, version pins | `localreel-<version>-x64.zip`, `.msi`, `SHA256SUMS.txt` |
| [`localreel-core`](https://github.com/LocalReel/localreel-core) | Rust server: kernel + media domain + HTTP API + `localreel` binary (web UI embedded at build time) | `localreel` executable, cargo tests |
| [`localreel-web`](https://github.com/LocalReel/localreel-web) | React + TypeScript + Vite UI: Library, Reels, Admin; zh/en | static `dist/` |
| [`localreel-format`](https://github.com/LocalReel/localreel-format) | Rust crate: versioned backup/artifact formats, no I/O | crate + unit tests |

The components are deliberately separable:

- `localreel-core` builds and tests with `cargo` alone; the web UI is a
  **build-time input** it embeds, not a crate dependency.
- `localreel-web` builds to static files with `npm run build`; any static build
  that speaks `/api/v1` can be composed with the server.
- `localreel-format` is a leaf crate with no I/O and no async runtime, so it can
  be tested in isolation.
- Only this repository produces installers, checksums and release notes.
  `components.json` pins exactly which component versions make up the product.

## Status and roadmap

**Current release: 0.4.0.** Windows is the primary target; Linux and Docker were
verified under WSL2, not on bare-metal desktops of every kind. Known limitations
(single-user, loopback-only, no HLS output, HEVC/AV1 sources need a full
re-encode) are recorded in the release notes and the readiness report.

A public **v1.0** is blocked by two things:

1. **Code signing** — artifacts are unsigned, so SmartScreen warns and there is
   no trusted publisher name yet.
2. **A published release pipeline** — component repositories are tagged and the
   product can compose a release locally, but no CI job builds, signs and
   publishes artifacts automatically.

Details, evidence and the remaining checklist:
[docs/release/v1.0-readiness.md](https://github.com/LocalReel/LocalReel/blob/main/docs/release/v1.0-readiness.md).
Release notes: [0.4.0](https://github.com/LocalReel/LocalReel/blob/main/docs/release/v0.4.0.md); older releases live in [docs/release/](https://github.com/LocalReel/LocalReel/tree/main/docs/release).

## Documentation

- [Install guide](docs/install.md) · [Windows service](docs/service.md) ·
  [Linux](docs/linux.md) · [Docker](docs/docker.md)
- [Architecture](docs/architecture.md) · [Building from source](docs/building.md) ·
  [Packaging](docs/packaging.md)
- [Signing](docs/signing.md) · [Release checklist](docs/release-checklist.md) ·
  [v1.0 readiness](https://github.com/LocalReel/LocalReel/blob/main/docs/release/v1.0-readiness.md)
  · [Operations](docs/operations.md)

## License

Apache-2.0 — see [LICENSE](LICENSE). Copyright 2026 LocalReel contributors.
Each component repository carries its own copy of the license.

`ffmpeg` is **not** distributed with LocalReel. If you install your own
`ffmpeg` build (or bundle it with `compose.ps1 -FfmpegDir`), its own license
(GPL or LGPL depending on the build) applies.
