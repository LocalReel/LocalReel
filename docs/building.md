# Building LocalReel from source

This guide takes you from four checkouts to the artifacts users download:
`dist/localreel-<version>-x64.zip`, `dist/localreel-<version>-x64.msi` and
`dist/SHA256SUMS.txt`.

The product repository only **composes** the components. Each component can be
built and tested on its own, and packaging always happens here — never inside
`localreel-core`. See [architecture.md](architecture.md) for why.

## Prerequisites

| Tool | Version | Needed for |
|---|---|---|
| Rust (`cargo`, `rustfmt`, `clippy`) | **1.85+** (`rust-version` in the workspace) | `localreel-core` (including the format crate) |
| Node.js + npm | **22** (same as `localreel-web` CI) | building/testing the UI |
| PowerShell | **5.1+** (Windows PowerShell) | `scripts/*.ps1`, `localreel-core/scripts/*.ps1` |
| WiX Toolset CLI | **5.0.2** + `WixToolset.UI.wixext` | MSI only |
| .NET SDK | 6+ (for `dotnet tool`) | installing WiX |
| Git | any recent version | cloning the repositories |
| ffmpeg / ffprobe | optional | runtime scan/thumbnail/transcode; optionally bundled into the package |

Install the Rust components if needed:

```powershell
rustup component add rustfmt clippy
```

Install WiX 5 and the UI extension (MSI wizard, including the install-directory
page):

```powershell
dotnet tool install --global wix --version 5.0.2
wix extension add -g WixToolset.UI.wixext/5.0.2
```

If `wix` is not on `PATH` after `dotnet tool install`, add
`%USERPROFILE%\.dotnet\tools` to `PATH`. `scripts/build-msi.ps1` locates
`wix.exe` in that directory too, and adds the UI extension matching the
installed WiX version automatically if it is missing.

`ffmpeg` is not required to build. Point `-FfmpegDir` at a directory containing
`ffmpeg.exe`/`ffprobe.exe` (or their `bin\`) to bundle a copy into the package;
`scripts/fetch-ffmpeg.ps1` (shipped as `tools/fetch-ffmpeg.ps1` in the package)
can download and verify a build (see [packaging.md](packaging.md) for the
discovery order).

## Repository layout

Clone the repositories as **siblings**:

```text
<workspace>/
  LocalReel/          the product (this repository): docs, installers, compose
  localreel-core/     Rust: kernel + media + API + format crate + localreel binary
  localreel-web/      React + TypeScript + Vite UI
```

`components.json` declares `../localreel-core` and `../localreel-web`;
`scripts/compose.ps1` resolves those relative to this
repository. Non-sibling layouts work too:

```powershell
.\scripts\compose.ps1 -ComponentsDir 'D:\checkouts'
.\scripts\compose.ps1 -CoreDir 'D:\checkouts\core' -WebDir 'D:\checkouts\web'
```

The core's default embed path is also the sibling layout (`localreel-web` next
to `localreel-core`); compose overrides that with `LOCALREEL_WEB_DIR` so the
components can live anywhere.

For a **release build**, check out the component versions named in
`components.json` (their tags), not arbitrary `main` commits — compose refuses
to package mismatched versions.

## Development

Development runs are isolated from any installed release: the dev server uses
port **3101** and `.dev-data/`, while the installed product uses port **3001**,
its install directory (default `%LOCALAPPDATA%\LocalReel`) and `data\` inside
it. They must never share a database or a port.

| | Development | Installed release |
|---|---|---|
| Start with | `localreel-core\scripts\dev.ps1` | Start Menu shortcut / `localreel.exe serve` |
| Binary | `localreel-core\target\debug\localreel.exe` | `<install dir>\localreel.exe` |
| Port | 3101 | 3001 |
| Data | `localreel-core\.dev-data\` | `<install dir>\data\` |
| Config | env vars set by `dev.ps1` | `<install dir>\localreel.toml` |
| Updated by | `git pull` + rebuild | a GitHub Release artifact |

### Server

```powershell
cd localreel-core
.\scripts\dev.ps1
# http://127.0.0.1:3101, data in .dev-data/
```

`dev.ps1` options:

| Option | Default | Effect |
|---|---|---|
| `-Port` | `3101` | Port to bind (`LOCALREEL_PORT`) |
| `-DataDir` | `<repo>\.dev-data` | Data directory (`LOCALREEL_DATA_DIR`) |
| `-Profile` | `debug` | Build profile (`debug` or `release`) |
| `-SkipBuild` | off | Reuse the existing binary |
| `-FfmpegDir` | unset | Pass `LOCALREEL_FFMPEG_DIR` for this run |

Rules: never point the dev server at an installed release's data directory, and
never install a release into a checkout.

### Web UI

```powershell
cd localreel-web
npm install
npm test              # resume + i18n unit tests
npm run build         # tsc && vite build -> dist/
npm run test:browser  # headless Chrome against the mock API (needs Chrome/Chromium)
```

`npm run dev` starts Vite on <http://127.0.0.1:5173>. Note that
`localreel-web/vite.config.ts` currently proxies `/api`, `/thumbnails` and
`/transcodes` to `http://127.0.0.1:3001`, while `scripts/dev.ps1` defaults to
3101. To develop the UI against the dev server, either start it with
`.\scripts\dev.ps1 -Port 3001` (only when no installed release is running) or
point the proxy at 3101 in your checkout.

To try a built UI without rebuilding the server binary, build `dist/` and run
`localreel.exe serve` with `LOCALREEL_WEB_DIR` set to the web checkout (or its
`dist`): the server serves the UI from disk when its binary has no embedded
assets. `localreel.exe doctor` reports `web: embedded`, `web: disk (<dir>)` or
`web: missing`.

### Full loop with the UI embedded

```powershell
cd localreel-web
npm run build

cd ..\localreel-core
.\scripts\dev.ps1
```

When `..\localreel-web\dist` exists, the server build embeds the current UI, so
the page served on 3101 is the real UI. Rebuild the UI and re-run `dev.ps1`
(without `-SkipBuild`) after UI changes.

## Quality gates

Before pushing or tagging, run the component checks. From `localreel-core`:

```powershell
.\scripts\release-check.ps1
```

This runs `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, then `npm test` and `npm run build` in the sibling
`localreel-web` (use `-SkipRust` / `-SkipWeb` to narrow it; it expects the
sibling layout).

The individual commands, if you prefer to run them yourself:

```powershell
cd localreel-core
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd ..\localreel-web
npm test
npm run build
```

The web browser smoke test (`npm run test:browser`) runs against a mock API and
fixture and needs Chrome/Chromium on `PATH` or `CHROME_PATH` set. Both
`localreel-core` and `localreel-web` run their checks in CI (Windows + Ubuntu
for Rust, Node 22 for the web).

## Packaging (Windows)

`scripts/compose.ps1` builds the UI, builds the server with the UI embedded,
stages the portable layout, writes the zip, and — with `-BuildMsi` — the MSI.
It then regenerates the checksum manifest.

```powershell
cd LocalReel
.\scripts\compose.ps1
```

Options (see the file header in `scripts/compose.ps1`):

| Option | Default | Effect |
|---|---|---|
| `-ComponentsDir` | parent of this repo | Directory containing the component checkouts |
| `-CoreDir` / `-WebDir` | from `components.json` | Explicit component directories |
| `-OutputDir` | `dist` | Output directory (relative to this repo or absolute) |
| `-Profile` | `release` | `release` or `debug` server build |
| `-SkipBuild` | off | Reuse the existing `target\<profile>\localreel.exe` |
| `-SkipWeb` | off | Do not rebuild `localreel-web/dist` |
| `-BuildMsi` | off | Also build the MSI via `scripts/build-msi.ps1` |
| `-FfmpegDir` | unset | Bundle `ffmpeg.exe`/`ffprobe.exe` into `ffmpeg\bin` |
| `-Force` | off | Package even when component versions disagree with `components.json` |

Version gate: the server version comes from `cargo metadata`, the UI version
from `localreel-web/package.json`; both must agree with each other and with
their pins in `components.json`. `-Force` is for local pre-release experiments
only.

Typical invocations:

```powershell
# zip only, rebuilding UI and server
.\scripts\compose.ps1

# zip + MSI
.\scripts\compose.ps1 -BuildMsi

# bundle ffmpeg (directory that contains ffmpeg.exe/ffprobe.exe, or its bin\)
.\scripts\compose.ps1 -BuildMsi -FfmpegDir '.tools\ffmpeg'

# repackage an existing build without rebuilding anything
.\scripts\compose.ps1 -SkipWeb -SkipBuild -BuildMsi
```

Output, for version `0.3.0` on x64:

```text
dist/localreel-0.3.0-x64/            staged package (exe, docs, scripts, tools)
dist/localreel-0.3.0-x64.zip         portable package
dist/localreel-0.3.0-x64.zip.sha256  SHA256 of the zip
dist/localreel-0.3.0-x64.msi         with -BuildMsi
dist/SHA256SUMS.txt                  all artifacts (written by checksums.ps1)
```

The stage contains `localreel.exe` (UI embedded), `localreel.toml.example`,
`README.md`, `README.txt`, `LICENSE`, `install.ps1`, `uninstall.ps1`,
`install-task.ps1`, `docs/` (product docs plus the core operations manual),
`tools/` (ffmpeg fetch/verify helpers) and, only with `-FfmpegDir`,
`ffmpeg\bin\ffmpeg.exe`/`ffprobe.exe`.

The MSI is built from that same stage. `installer/localreel.wxs` is a **per-user**
WiX 5 package whose wizard (`WixUI_InstallDir`) lets the user choose the install
directory — default `%LOCALAPPDATA%\LocalReel` — with a Start Menu shortcut, a
license page and major-upgrade handling. `data\`/`localreel.toml` are not MSI
components, so they survive uninstall.

Standalone MSI build (without recomposing):

```powershell
.\scripts\build-msi.ps1
# newest dist\localreel-*-x64 stage, version inferred from its name

.\scripts\build-msi.ps1 -Stage dist\localreel-0.3.0-x64 -Version 0.3.0 `
  -Output D:\release\localreel-0.3.0-x64.msi
```

`build-msi.ps1` options: `-Stage`, `-Version`, `-Output`, `-SkipValidation`. By
default it validates the result with an administrative extract
(`msiexec /a`, proving the payload contains `localreel.exe`) and by checking
that the MSI's dialog table actually contains the wizard pages. Requires WiX 5
plus the UI extension; see [Prerequisites](#prerequisites).

Unattended installs can override the directory:

```powershell
msiexec /i localreel-0.3.0-x64.msi INSTALLFOLDER="D:\Apps\LocalReel" /qn
```

## Checksums

```powershell
.\scripts\checksums.ps1
# dist\SHA256SUMS.txt over every .zip/.msi in dist\

.\scripts\checksums.ps1 -Dist D:\release
```

Compose runs this automatically at the end; run it again after adding or
replacing any artifact. Only `.zip` and `.msi` files are hashed, sorted by name.

## Release flow

1. **Bump versions.** Update the Rust workspace version
   (`localreel-core/Cargo.toml`, which also covers the format crate),
   `localreel-web/package.json` (and `package-lock.json`), and the pins in
   `components.json`.
2. **Verify.** Run the quality gates on each component; green `cargo
   fmt`/`clippy`/`test`, `npm test`/`npm run build`, and a clean working tree.
3. **Tag.** Tag each component repository `v<version>` and tag this repository
   `v<version>`.
4. **Compose.** From this repository: `.\scripts\compose.ps1 -BuildMsi`
   (add `-FfmpegDir` if the release bundles ffmpeg).
5. **Checksums.** `.\scripts\checksums.ps1` — record the values; publish
   `SHA256SUMS.txt` with the artifacts.
6. **Verify on a clean profile.** Install the MSI or extract the zip, check
   `localreel.exe doctor` (`web: embedded`, database ok, ffmpeg resolved), scan
   a small source, play a file, export a backup, then uninstall and confirm
   `data\`/`localreel.toml` survive. The full list is in
   [release-checklist.md](release-checklist.md).
7. **Sign (when a certificate exists).** Sign the `localreel.exe` in the stage
   and the MSI, then re-create the zip from the signed stage; never sign the zip
   itself. See [signing.md](signing.md). The current artifacts are unsigned.
8. **Publish.** Create the GitHub Release on
   <https://github.com/LocalReel/LocalReel/releases> for the product tag,
   upload the zip, MSI and `SHA256SUMS.txt`, and attach the release notes from
   `docs/release/`. Note the unsigned status.

## Other targets

- **Docker** — the image is built from the `localreel-core` checkout with the
  web checkout passed as a named build context:
  `docker build --build-context web=../localreel-web -t localreel:dev .`
  (from `localreel-core`). See [docker.md](docker.md).
- **Linux** — the tarball is built on a Linux host from this repository:
  `./scripts/package-linux.sh --build-web` (options: `--core <dir>`,
  `--web <dir>`, `--ffmpeg-dir <dir>`); install with the bundled `install.sh`.
  See [linux.md](linux.md). Linux is verified under WSL2 and remains a
  secondary target.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `component versions disagree: core X vs web Y` | Bump both components together, or use `-Force` for a local experiment |
| `components.json pins <component> <v> but the checkout is <v>` | Check out the pinned tag in that component repo, or update the pin |
| `web dist is missing at ...` | Run without `-SkipWeb`, or `npm run build` in `localreel-web` |
| `binary not found: ... (run without -SkipBuild)` | Build the server first or drop `-SkipBuild` |
| `wix.exe not found` | `dotnet tool install --global wix --version 5.0.2`; add `%USERPROFILE%\.dotnet\tools` to `PATH` |
| `no staged product found; run scripts\compose.ps1 first` | Compose first, or pass `-Stage` to `build-msi.ps1` |
| MSI UI check fails (too few dialogs) | The UI extension is missing or the MSI is stale; rebuild with `build-msi.ps1` |
| Vite dev server cannot reach the API | Its proxy targets 3001 while `dev.ps1` defaults to 3101 — see [Web UI](#web-ui) |
