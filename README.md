# LocalReel

**A private, scrollable streaming app for your own library.** Open it and
something plays; swipe to the next episode; the library is your own disk, not
the internet. No cloud service, no account with us, no ads.

Point LocalReel at folders on your machine; it scans them, generates metadata and
thumbnails with `ffmpeg`/`ffprobe`, and serves the UI on
<http://127.0.0.1:3001>.

This repository is the **product**: it owns the download/install entry point, the
composition of the components, and the documentation. The server, the UI, the
desktop shell and the format crate are separate repositories, each independently
buildable, testable and releasable. See [docs/architecture.md](docs/architecture.md).

## Two ways to run the same library

| | Server edition | Desktop edition |
|---|---|---|
| What you get | The server and its web UI | The same server **plus** a desktop window |
| How you open it | A browser at `127.0.0.1:3001` | A normal application window (Electron) |
| Windows installer | ~6 MB | ~124 MB |
| Best for | A NAS, a spare machine, phones and tablets on the same network | A laptop or a desktop where the app *is* the way in |

Both install into the same folder and share the same data. They are separate
products to Windows (so Apps & Features shows which one you have), and each one
replaces the other when installed — you will not end up with two entries pointing
at one directory. In-app updates follow the edition that is installed: a server
installation is never handed a 300 MB desktop package, and a desktop
installation keeps its shell when it updates.

## What it does

- **Library** — add local folders as sources, scan them, and browse a paginated
  library with metadata and thumbnails produced by ffmpeg.
- **Reels** — a vertical feed over the same library, ranked by the server:
  continue-watching first, then unwatched episodes in series order, then movies,
  then finished items.
- **Series and episodes** — series, season and episode numbers are derived from
  the folder layout and release-style file names; the player autoplays the next
  episode.
- **Resume** — playback position is stored on the server for the signed-in
  account, so **Continue** picks up where you stopped; positions survive upgrades
  and stay separate for each account.
- **Search and filters** — search title, filename or path; filter by
  **Continue** or **Favorites**; archived media can be shown on demand.
- **Accounts** — the first run asks for the owner account; the owner creates a
  viewer account per person, and everyone gets their own progress, likes and
  favourites. The console lists the browsers that are signed in and can end any
  of them, including ones from an old phone.
- **Playback** — direct streaming with HTTP Range requests; H.264 files in
  MKV/MOV/TS are remuxed instead of re-encoded; other formats are transcoded to
  cached variants that can be cancelled.
- **Mobile-first player** — swipe navigation, double-tap like, long-press on the
  right side for 2× (release restores 1×), draggable progress bar, fullscreen for
  landscape video, and a PWA manifest.
- **Share links** — one item, one random token, a page that works without an
  account (`/s/<token>`). Links can expire, be revoked, allow or refuse
  downloads, and are rate-limited separately from the rest of the server.
- **Phones on the same Wi-Fi** — off by default. The console offers to turn it on
  and shows the address to type; nothing changes until you accept.
- **Plugins** — a first-class module in the rail: install a signed package, see
  what it asks for, grant capabilities per version, read its logs, and see the
  icon its manifest declares.
- **Admin** — folders, accounts, sessions, jobs, scan history and issues, cache
  GC, backup/restore, an append-only audit of administrator actions, and the
  running server's configuration.
- **Bilingual UI** — English and Simplified Chinese on every page. The first
  visit follows the browser language; a small globe button switches language and
  the choice is remembered.
- **Local by design** — binds loopback only by default and requires a signed-in
  account; remote access is an explicit switch, and even then a request must come
  from the same origin.
- **Distribution** — Windows zip, per-user MSI and per-user setup in both
  editions; .deb with the systemd unit, AppImage and tarball for Linux (plus a
  desktop AppImage); macOS .dmg (plus a desktop .dmg); Docker image; and an
  experimental Synology .spk.

## Download and install

Releases are published from this repository:
**<https://github.com/LocalReel/LocalReel/releases>**

Each release carries:

| Artifact | Size (approx.) | Notes |
|---|---|---|
| `localreel-<version>-x64-desktop-setup.exe` | 150 MB | **Desktop edition**, one click, no wizard |
| `localreel-<version>-x64-desktop.msi` | 124 MB | Desktop edition as an MSI (policy deployment) |
| `localreel-<version>-x64-setup.exe` | 7 MB | Server edition, one click |
| `localreel-<version>-x64.msi` | 6 MB | Server edition as an MSI (WiX wizard) |
| `localreel-<version>-x64.zip` | 6 MB | Server edition, portable |
| `SHA256SUMS.txt` | — | SHA256 for every artifact |

Linux and macOS releases add the tarball, `.deb`, `AppImage`, `.dmg` and the
desktop shell's own AppImage/dmg, all built on the matching runners.

### Which Windows download?

- **Just want the app on this PC** → `...-x64-desktop-setup.exe`. It installs the
  server and the window, creates a Start Menu entry, and starting it is the whole
  workflow.
- **Running it on a NAS or a spare machine, watching from a browser** →
  `...-x64-setup.exe` (or the MSI). 6 MB instead of 150 MB.

Both are per-user by default and let you choose the install directory; the
default is `%LOCALAPPDATA%\LocalReel`. Pick a folder you own — not
`C:\Program Files`, which needs administrator rights. Neither touches your
`data\` folder or `localreel.toml`, so uninstalling removes the program and keeps
the library.

```powershell
msiexec /i localreel-<version>-x64-desktop.msi
msiexec /i localreel-<version>-x64-desktop.msi INSTALLFOLDER="D:\Apps\LocalReel" /qn
```

> **Unsigned builds.** The installers and the executables are **not code-signed**,
> so Windows SmartScreen shows "Windows protected your PC". Verify the SHA256 of
> what you downloaded against `SHA256SUMS.txt` before running it. The update
> manifest is signed separately, so in-app updates are still verified.

### Linux and macOS

The Linux tarball is built from source on a Linux host with
`./scripts/package-linux.sh --build-web`; extract it and run `./install.sh` for a
per-user install in `~/.local/share/localreel`. See [docs/linux.md](docs/linux.md).
macOS ships a `.app` in a `.dmg`; see [docs/macos.md](docs/macos.md). On both, the
desktop shell expects the server on `PATH` (or set `LOCALREEL_SERVER`), because
the server is installed by the package manager, not bundled beside the window.

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
   3001); the UI runs from `localreel-web` with `npm run dev`.
3. **Package the desktop shell** — from `desktop/`, `npm install` then
   `npm run pack`; the directory lands in `desktop/out/win-unpacked`. Without it,
   `compose.ps1` builds the server edition only.
4. **Compose artifacts** — from this repository, `.\scripts\compose.ps1` builds
   the UI, embeds it into the server binary, stages the product and writes the
   zip and both setup executables; add `-BuildMsi` for both MSIs. Then
   `.\scripts\checksums.ps1` writes `dist/SHA256SUMS.txt`.

## Releases

`components.json` records a version **and a commit** for every component. A
release tag must name both; the workflow refuses to build a tag whose components
are pinned to nothing, because the same tag must not produce different binaries on
different days.

```powershell
# 1. pin the components you are about to ship
pwsh -File scripts\pin-components.ps1 -Version 0.5.7
git commit -am "release: pin 0.5.7"
# 2. tag and push — the tag builds, signs and publishes
git tag v0.5.7 && git push origin v0.5.7
```

A `workflow_dispatch` run builds the same pipeline without publishing; it is the
way to test changes to the release workflow itself. A nightly
`.github/workflows/smoke.yml` builds the components' `main` every day and asserts
the package, so drift is noticed within a day rather than at the next tag.

## Repository structure

LocalReel is a **multi-repository product**. Each component is its own git
repository with its own build, tests and tag; the product repository pins
versions and commits and composes them.

| Repository | Role | Produces |
|---|---|---|
| **LocalReel** (this repo) | Product shell: documentation, download entry point, installers, version pins | zips, MSIs, setup executables, `SHA256SUMS.txt` |
| [`localreel-core`](https://github.com/LocalReel/localreel-core) | Rust server: kernel + media domain + HTTP API + `localreel` binary (web UI embedded at build time); owns `localreel-format`, `localreel-plugin` and `localreel-update` | `localreel` executable, cargo tests |
| [`localreel-web`](https://github.com/LocalReel/localreel-web) | React + TypeScript + Vite UI: Library, Reels, Plugins, Console; English/Simplified Chinese | static `dist/` |
| `desktop/` (in this repo) | Electron shell: one window onto the server, with its own packaging | `LocalReel.exe` + app bundle |

The components are deliberately separable:

- `localreel-core` builds and tests with `cargo` alone; the web UI is a
  build-time input it embeds, not a crate dependency.
- `localreel-web` builds to static files with `npm run build`; any static build
  that speaks `/api/v1` can be composed with the server.
- `crates/localreel-update` verifies the signed update manifest and hands the
  installer over; `crates/localreel-format` has no I/O and no async runtime.
- Only this repository produces installers, checksums and release notes.

## Documentation

- [Install guide](docs/install.md) · [Windows service](docs/service.md) ·
  [Linux](docs/linux.md) · [macOS](docs/macos.md) · [Docker](docs/docker.md) ·
  [Reverse proxy](docs/reverse-proxy.md)
- [Architecture](docs/architecture.md) · [Building from source](docs/building.md)
  · [Packaging](docs/packaging.md) · [Updating](docs/updating.md)
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
不需要云服务、不需要注册账号、没有广告。

把本地目录添加为来源后，LocalReel 会用 `ffmpeg`/`ffprobe` 扫描并生成元数据与
缩略图，然后在 <http://127.0.0.1:3001> 提供界面。

### 两个版本，同一个媒体库

| | 服务器版 | 桌面版 |
|---|---|---|
| 你得到什么 | 服务器 + 浏览器界面 | 同样的服务器 **加上** 一个桌面窗口 |
| 怎么打开 | 浏览器访问 `127.0.0.1:3001` | 一个普通的应用程序窗口（Electron） |
| Windows 安装包 | 约 6 MB | 约 124 MB |
| 适合谁 | NAS、闲置机器、局域网里的手机和平板 | 希望"打开就是应用"的笔记本或台式机 |

两个版本装进同一个目录、共用同一份数据。它们在 Windows 里是两个独立产品（所以
「应用和功能」里能看出装的是哪个），但互相声明了对方：装任何一个都会替换另一个，
不会出现两条记录指向同一个文件夹。**应用内更新跟随已安装的版本**——服务器版永远
不会被塞进一个 300 MB 的桌面包，桌面版升级后也仍然带着窗口。

### 功能

- **媒体库**：分页浏览，支持搜索、继续观看与收藏筛选，可显示已归档内容。
- **短视频流**：竖向 feed，由服务端排序（续播优先 → 未看剧集 → 未看电影 →
  已看完）；自动连播下一集。
- **剧集与集数**：从目录结构与发布式文件名推导系列、季、集数。
- **续播**：播放进度按账户保存在服务端，升级不丢，不同账户互不影响。
- **账户**：首次运行创建所有者账户；所有者可为每个人创建观看者账户。控制台能
  列出当前登录的浏览器，并能把任何一台（包括旧手机）登出。
- **播放**：支持 HTTP Range 直连；MKV/MOV/TS 中的 H.264 用流拷贝封装；其余格式
  转码为可取消的缓存副本。
- **移动端播放器**：上下滑切集、双击点赞、右侧长按 2 倍速、可拖动进度条、横屏
  视频全屏观看、支持添加到主屏幕（PWA）。
- **分享链接**：一条媒体一个随机 token，`/s/<token>` 免登录可看；可设过期、可
  撤销、可单独关闭下载，并且有独立于全站的限速。
- **同一 Wi-Fi 上的手机**：默认关闭。控制台会主动提示开启并给出要输入的地址，
  在你点同意之前不会有任何变化。
- **插件**：左侧一级模块。安装签名包、查看它申请的能力、按版本授权、读日志，
  并显示清单里声明的图标。
- **管理页**：媒体目录、账户、会话、任务、扫描历史与问题、缓存回收、备份与恢复、
  只追加的管理员审计，以及当前运行配置。
- **双语界面**：每页支持英文与简体中文，首次访问跟随浏览器语言，右上角地球按钮
  切换并记住选择。
- **本地优先**：默认只监听回环地址且需要登录；局域网访问是显式开关，打开后请求
  仍须同源。
- **分发**：Windows 提供 zip、MSI 与 setup，服务器版和桌面版各一套；Linux 提供
  deb、AppImage、tar.gz（另有桌面版 AppImage）；macOS 提供 dmg（另有桌面版
  dmg）；以及 Docker 镜像与实验性的群晖 spk。

### 安装

从 [Releases](https://github.com/LocalReel/LocalReel/releases) 下载：

- **只想在这台电脑上用** → `...-x64-desktop-setup.exe`（服务器 + 窗口，一次装好）
- **放在 NAS 或闲置机器上，用浏览器看** → `...-x64-setup.exe` 或对应 MSI（6 MB）

安装向导会让你选择安装位置（默认 `%LOCALAPPDATA%\LocalReel`，无需管理员权限）。
数据存放在 `%LOCALAPPDATA%\LocalReel`（数据库、缩略图、转码缓存）与
`%APPDATA%\LocalReel`（配置），与安装目录分离：升级、卸载、移动程序都不会影响
媒体库与观看进度。

产物未签名，SmartScreen 会提示，请先用 `SHA256SUMS.txt` 校验；更新清单本身是
单独签名的，所以应用内更新仍然经过校验。

### 发版

`components.json` 同时记录版本**与 commit**。打 tag 前必须先把组件锁定：

```powershell
pwsh -File scripts\pin-components.ps1 -Version 0.5.7
git commit -am "release: pin 0.5.7"
git tag v0.5.7 && git push origin v0.5.7
```

`workflow_dispatch` 会跑同样的流水线但不发布，用来验证工作流本身的改动；每天还有
一次 `smoke.yml` 用各组件的 `main` 构建一遍，确保漂移一天内被发现。

本仓库是产品仓库：负责下载入口、组合组件与文档。服务器与界面分别在
`localreel-core`、`localreel-web` 两个仓库，均可独立构建与测试；共享的
备份/产物格式定义在 `localreel-core/crates/localreel-format`，更新协议在
`crates/localreel-update`。
