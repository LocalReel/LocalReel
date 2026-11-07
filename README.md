# LocalReel

**A private, scrollable streaming app for your own library.** Open it and
something plays; swipe to the next episode; the library is your own disk, not
the internet. One lightweight binary with the web UI built in: no cloud service,
no account, no ads.

Point LocalReel at folders on your machine; it scans them, generates metadata and
thumbnails with `ffmpeg`/`ffprobe`, and serves a browser UI on
<http://127.0.0.1:3001>.

This repository is the **product**: it owns the download/install entry point, the
composition of the components, and the documentation. The server, the UI and the
format crate are separate repositories, each independently buildable, testable
and releasable. See [docs/architecture.md](docs/architecture.md).

## What it does

- **Library** — add local folders as sources, scan them, and browse a paginated
  library with metadata and thumbnails produced by ffmpeg.
- **Reels** — a vertical feed (`/watch`) over the same library, ranked by the
  server: continue-watching first, then unwatched episodes in series order, then
  movies, then finished items.
- **Series and episodes** — series, season and episode numbers are derived from
  the folder layout and release-style file names; the player autoplays the next
  episode.
- **Resume** — playback position is stored on the server for the signed-in
  account, so **Continue** picks up where you stopped; positions survive
  upgrades and stay separate for each account.
- **Search and filters** — search title, filename or path; filter by
  **Continue** or **Favorites**; archived media can be shown on demand.
- **Accounts** — the first run asks for the owner account; the owner creates a
  viewer account per person, and everyone gets their own progress, likes and
  favourites. The owner can disable accounts and reset passwords from the
  admin page.
- **Playback** — direct streaming with HTTP Range requests; H.264 files in
  MKV/MOV/TS are remuxed instead of re-encoded; other formats are transcoded to
  cached variants that can be cancelled.
- **Mobile-first player** — swipe navigation, double-tap like, long-press on the
  right side for 2× (release restores 1×), draggable progress bar, fullscreen for
  landscape video, and a PWA manifest.
- **Admin** — add folders, create and manage accounts, backup/restore,
  transcode/thumbnail cache GC, scan history and issues, source archive/remap,
  and a job list with cancellation.
- **Bilingual UI** — English and Simplified Chinese on every page. The first
  visit follows the browser language; a small globe button switches language and
  the choice is remembered.
- **Local by design** — binds loopback only by default and requires a signed-in
  account; `ffmpeg`/`ffprobe` are not bundled and are discovered next to the
  binary or on `PATH`.
- **Distribution** — Windows zip and per-user MSI; a Linux tarball and a Docker
  image are built from source on those hosts.

## Download and install

Releases are published from this repository:
**<https://github.com/LocalReel/LocalReel/releases>**

Each release carries:

| Artifact | Notes |
|---|---|
| `localreel-<version>-x64.msi` | Per-user Windows installer (WiX wizard) |
| `localreel-<version>-x64.zip` + `.sha256` | Portable Windows package |
| `SHA256SUMS.txt` | SHA256 for every artifact above |

### Windows MSI (recommended)

Run the MSI. It is **per-user** (no administrator rights) and the wizard lets you
**choose the install directory**; the default is `%LOCALAPPDATA%\LocalReel`. Pick
a folder you own — not `C:\Program Files`, which needs administrator rights. It
creates a Start Menu shortcut and does not touch your `data\` folder or
`localreel.toml`. Uninstalling removes the program files but keeps your library
data and configuration.

```powershell
msiexec /i localreel-<version>-x64.msi
```

Unattended installs can choose the directory up front:

```powershell
msiexec /i localreel-<version>-x64.msi INSTALLFOLDER="D:\Apps\LocalReel" /qn
```

### Windows zip (portable)

Extract the zip anywhere and either run `localreel.exe serve` in place, or use the
included installer to copy it to a stable location:

```powershell
# installs to %LOCALAPPDATA%\LocalReel and creates a Start Menu shortcut
.\install.ps1

# custom install directory
.\install.ps1 -InstallDir 'D:\Apps\LocalReel'
```

Start LocalReel from the Start Menu (or `localreel.exe serve`) and open
<http://127.0.0.1:3001>. Data lives in `data\` next to the binary and settings in
`localreel.toml`; upgrades and uninstalls keep both. To start automatically at
logon, use the included `install-task.ps1` (see [docs/service.md](docs/service.md)).

> **Unsigned builds.** The MSI and the executables are **not code-signed**, so
> Windows SmartScreen shows "Windows protected your PC". Verify the SHA256 of
> what you downloaded against `SHA256SUMS.txt` before running it.

### Linux

The Linux tarball is built from source on a Linux host with
`./scripts/package-linux.sh --build-web` (from this repository); extract it and
run `./install.sh` for a per-user install in `~/.local/share/localreel`. See
[docs/linux.md](docs/linux.md).

### Docker

The image runs the same binary with `ffmpeg` inside and a `/data` volume; the
Compose file mounts your media directory read-only. See
[docs/docker.md](docs/docker.md).

## Build from source

The full walkthrough is in [docs/building.md](docs/building.md).

1. **Get the repositories as siblings** — `LocalReel/` (this one),
   `localreel-core/` and `localreel-web/`.
2. **Develop** — the server runs from `localreel-core` with `.\scripts\dev.ps1`
   (port 3101, data in `.dev-data/`, isolated from any installed release on
   3001); the UI runs from `localreel-web` with `npm run dev` or builds with
   `npm run build`.
3. **Compose artifacts** — from this repository, `.\scripts\compose.ps1` builds
   the UI, embeds it into the server binary, and writes
   `dist/localreel-<version>-x64.zip` (add `-BuildMsi` for the MSI), then
   `.\scripts\checksums.ps1` writes `dist/SHA256SUMS.txt`.

## Repository structure

LocalReel is a **multi-repository product**. Each component is its own git
repository with its own build, tests and tag; the product repository pins
versions and composes them.

| Repository | Role | Produces |
|---|---|---|
| **LocalReel** (this repo) | Product shell: documentation, download entry point, installers, version pins | `localreel-<version>-x64.zip`, `.msi`, `SHA256SUMS.txt` |
| [`localreel-core`](https://github.com/LocalReel/localreel-core) | Rust server: kernel + media domain + HTTP API + `localreel` binary (web UI embedded at build time); owns the `localreel-format` crate | `localreel` executable, cargo tests |
| [`localreel-web`](https://github.com/LocalReel/localreel-web) | React + TypeScript + Vite UI: Library, Reels, Admin; English/Simplified Chinese | static `dist/` |

The components are deliberately separable:

- `localreel-core` builds and tests with `cargo` alone; the web UI is a
  build-time input it embeds, not a crate dependency.
- `localreel-web` builds to static files with `npm run build`; any static build
  that speaks `/api/v1` can be composed with the server.
- `crates/localreel-format` (inside `localreel-core`) is a leaf crate with no I/O
  and no async runtime, so the shared backup/artifact schema is tested in
  isolation.
- Only this repository produces installers, checksums and release notes.
  `components.json` pins exactly which component versions make up the product.

## Documentation

- [Install guide](docs/install.md) · [Windows service](docs/service.md) ·
  [Linux](docs/linux.md) · [Docker](docs/docker.md) ·
  [Reverse proxy](docs/reverse-proxy.md)
- [Architecture](docs/architecture.md) · [Building from source](docs/building.md)
  · [Packaging](docs/packaging.md)
- [Signing](docs/signing.md) · [Operations](docs/operations.md)

## License

Apache-2.0 — see [LICENSE](LICENSE). Copyright 2026 LocalReel contributors. Each
component repository carries its own copy of the license.

Releases normally do not include FFmpeg. A build can bundle it with
`compose.ps1 -FfmpegDir`; the package then contains `ffmpeg\NOTICE.txt` and the
license that applies to the bundled build (GPL or LGPL). If you install your own
`ffmpeg`, its own license applies.

---

## 简体中文

**把你自己的硬盘变成可以像刷短视频一样刷、像流媒体网站一样追剧的私有播放器。**
一个轻量程序内置 Web 界面，不需要云服务、账号或广告。

把本地目录添加为来源后，LocalReel 会用 `ffmpeg`/`ffprobe` 扫描并生成元数据与
缩略图，然后在 <http://127.0.0.1:3001> 提供界面。

- **媒体库**：分页浏览，支持搜索、继续观看与收藏筛选，可显示已归档内容。
- **短视频流**：竖向 feed，由服务端排序（续播优先 → 未看剧集 → 未看电影 →
  已看完）；自动连播下一集。
- **剧集与集数**：从目录结构与发布式文件名推导系列、季、集数。
- **续播**：播放进度按账户保存在服务端，升级不丢，不同账户互不影响。
- **账户**：首次运行创建所有者账户；所有者可为每个人创建观看者账户，各自拥有
  独立的进度、喜欢与收藏，也可在管理页停用账户或重置密码。
- **播放**：支持 HTTP Range 直连；MKV/MOV/TS 中的 H.264 用流拷贝封装；其余格式
  转码为可取消的缓存副本。
- **移动端播放器**：上下滑切集、双击点赞、右侧长按 2 倍速（松手恢复）、可拖动
  进度条、横屏视频全屏观看、支持添加到主屏幕（PWA）。
- **管理页**：添加媒体目录、账户管理、扫描历史与问题、任务取消、缓存回收、
  备份与恢复。
- **双语界面**：每页支持英文与简体中文，首次访问跟随浏览器语言，右上角小地球
  按钮切换并记住选择。
- **本地优先**：默认只监听回环地址，且需要登录账户；`ffmpeg` 默认不随包分发，
  可用 `compose.ps1 -FfmpegDir` 打入指定版本（随包附带其许可证与 NOTICE）。

安装：从 [Releases](https://github.com/LocalReel/LocalReel/releases) 下载 MSI
或 zip。MSI 向导会让你选择安装位置（默认 `%LOCALAPPDATA%\LocalReel`，无需管理员
权限），装完勾选「Launch LocalReel」即可直接进入界面；zip 解压后运行
`.\install.ps1`，也可用 `-InstallDir` 指定目录。从开始菜单启动时会自动打开浏览器
（<http://127.0.0.1:3001>）。

数据存放在 `%LOCALAPPDATA%\LocalReel`（数据库、缩略图、转码缓存）与
`%APPDATA%\LocalReel`（配置），与安装目录分离：升级、卸载、移动程序都不会影响
媒体库与观看进度。

产物未签名，SmartScreen 会提示，请先用 `SHA256SUMS.txt` 校验；卸载只移除程序文件，
媒体库与账户数据默认保留。

本仓库是产品仓库：负责下载入口、组合组件与文档。服务器与界面分别在
`localreel-core`、`localreel-web` 两个仓库，均可独立构建与测试；共享的
备份/产物格式定义在 `localreel-core/crates/localreel-format`。
