# Architecture

LocalReel is a **product composed from independently versioned components**.
This repository is the product shell; the server, the web UI and the format
crate are separate repositories with their own builds, tests and tags. This
document describes the component boundaries and the contracts that let the
product be assembled without the components knowing about each other's
packaging.

The guiding rule: **the product repository contains no product code.** It pins
component versions, embeds the UI into the server binary, stages installers and
publishes documentation. Business logic lives in `localreel-core`.

## Shape

```text
LocalReel/ (this repository - product shell)
├── components.json          version pins: core 0.3.0, web 0.3.0, format 0.1.0
├── scripts/compose.ps1      builds web + core, stages the zip (optionally MSI)
├── scripts/build-msi.ps1    WiX 5 build of installer/localreel.wxs
├── scripts/checksums.ps1    SHA256SUMS.txt for the staged artifacts
├── installer/               per-user MSI source (WixUI_InstallDir wizard)
└── docs/                    install, building, architecture, release notes
                │
                │ composes (build time)
                ▼
   localreel-web ──dist/──▶ localreel-core ──▶ localreel.exe ──▶ zip / MSI
   (React + TS)            (Rust workspace,          (UI embedded)
                            kernel/media/api/server)
                │
                └── localreel-format (independent versioned-schema crate)
```

The dependency direction is one-way — the web UI talks to the server's HTTP
API, and inside the server each crate only depends on the layer below it:

```text
web  ->  api  ->  media  ->  kernel
```

`web -> api` is an **HTTP contract** (`/api/v1`), not a compile-time crate
dependency: the UI is static files, embedded into the binary as bytes.

## Component responsibilities and boundaries

### `localreel-core` — Rust server and `localreel` binary

A Cargo workspace with four crates and one binary:

| Crate | Responsibility | Must not |
|---|---|---|
| `localreel-kernel` | Generic capabilities: config, SQLite database, persistent job queue, events (SSE), storage/VFS, typed ffmpeg process handling (`media-ops`) | Know anything about videos as a domain |
| `localreel-media` | The media domain: sources, scan, metadata, playback state, search, playback policy, transcode variants, backup/restore | Write HTTP handlers or spawn `ffmpeg` directly |
| `localreel-api` | HTTP `/api/v1`, SSE events, static/embedded assets, loopback guard and security headers | Contain domain logic |
| `localreel-server` | The `localreel` binary: wiring, migrations, CLI (`serve`, `migrate`, `doctor`, `scan`, `import`) | Leak into the crates above |

The kernel/media split is what keeps the system a *modular monolith*: media
bytes, playback sessions, transcode processes and scans never go through an
extension runtime, and long-running work always goes through the persistent,
cancellable, SSE-observable job queue (`sys_jobs`).

**Builds and tests independently:**

```powershell
cd localreel-core
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`.\scripts\release-check.ps1` wraps the above and also runs the web checks.
Development uses `.\scripts\dev.ps1` (port 3101, data in `.dev-data/`), which is
kept strictly apart from an installed release (port 3001).

### `localreel-web` — React + TypeScript + Vite UI

Pages: Library (`/`), Reels (`/watch`), Admin (`/admin`). The UI talks to the
server only through `/api/v1` over relative same-origin URLs, so the same build
works behind the embedded server, a reverse proxy or a Docker container.

Owns the bilingual presentation (Simplified Chinese / English): two flat
dictionaries with compile-time key parity and a runtime test, a 中文 / EN switch
on every page, the language persisted in `localStorage` (`localreel.lang`) and
defaulted from `navigator.language`.

**Builds and tests independently:**

```powershell
cd localreel-web
npm test            # resume + i18n unit tests
npm run build       # tsc && vite build -> dist/
npm run test:browser  # headless Chrome + mock API + fixture (needs Chrome/Chromium)
```

Deliverable: a plain static `dist/` directory. No server code, no knowledge of
Rust, no assumption that it is embedded.

### `localreel-format` — versioned data formats (Rust crate)

A leaf crate that defines the versioned shapes LocalReel reads and writes, and
nothing else:

- `Backup` — portable sources + playback state export (`version = 2`);
- `ArtifactKind` / `artifact_file_name` — deterministic, filesystem-safe names
  for generated thumbnails, transcodes and HLS packages;
- `FormatError` — validation errors that callers map to their own error types.

Boundary: **no I/O and no async runtime**. It does not open files, touch the
network or spawn processes; reading and writing is the consumer's job. The
format is versioned from day one, and unknown/newer versions must be rejected
rather than reinterpreted.

**Builds and tests independently:**

```powershell
cd localreel-format
cargo test
```

### `LocalReel` — the product repository

Owns only composition and product output:

- `components.json` — which component versions make up this product;
- `scripts/compose.ps1` — build web, embed it into the server build, stage the
  portable layout, write the zip, optionally invoke the MSI build;
- `scripts/build-msi.ps1` + `installer/localreel.wxs` — per-user MSI with the
  `WixUI_InstallDir` wizard;
- `scripts/checksums.ps1` — `SHA256SUMS.txt` for the release artifacts;
- `install.ps1` / `uninstall.ps1` / `install-task.ps1` and the Linux scripts —
  install lifecycle users run from the extracted package;
- `docs/` and GitHub Releases — documentation, release notes, downloads.

It contains no Rust, TypeScript or business logic, and it is the **only** place
that produces installers and checksums.

## Composition contracts

### 1. UI embedding: `LOCALREEL_WEB_DIR`

The only coupling between the web UI and the server is a directory of static
files.

- `localreel-api/build.rs` walks the UI's `dist` directory and generates a
  `FILES` table of `include_bytes!` entries. The default is the sibling layout
  (`localreel-web/dist` next to `localreel-core`).
- Setting `LOCALREEL_WEB_DIR` at **build time** redirects it: the variable may
  point at a web checkout or directly at its `dist` directory, so a component
  checkout can live anywhere.
- If no `dist` exists, the binary still builds and serves the API plus a
  placeholder page; `localreel doctor` then reports `web: missing (API-only
  placeholder)`.
- At **runtime**, `LOCALREEL_WEB_DIR` (or `[server] web_dir` in
  `localreel.toml`) lets an *unembedded* binary serve the UI from disk. Embedded
  assets always win when present. `localreel doctor` reports
  `web: embedded`, `web: disk (<dir>)` or `web: missing`.

`scripts/compose.ps1` sets `LOCALREEL_WEB_DIR` to the resolved web checkout
before building the server, which is how a product build pins a specific UI to
a specific binary.

### 2. Version pinning: `components.json`

```json
{
  "product": "LocalReel",
  "version": "0.3.0",
  "components": {
    "core":   { "path": "../localreel-core",   "version": "0.3.0", "...": "..." },
    "web":    { "path": "../localreel-web",    "version": "0.3.0", "...": "..." },
    "format": { "path": "../localreel-format", "version": "0.1.0", "...": "..." }
  }
}
```

Each entry declares the GitHub repository, the local search path, the version
that belongs to this product, and what the component provides. `compose.ps1`
enforces the pin:

- the server version is read from `cargo metadata` (`localreel-server`), the UI
  version from `localreel-web/package.json`;
- core and web versions must agree;
- the core and web checkouts must match their pins in `components.json`,
  otherwise composition fails;
- `-Force` bypasses the gate for local pre-release experiments only.

That gives one authoritative answer to "what is version X of the product?": a
product tag plus a `components.json` whose pins point at component tags. The
component repositories stay independently releasable.

### 3. Artifact ownership

Only this repository produces what a user downloads. `compose.ps1` stages a
directory (and then the zip) containing:

```text
localreel-<version>-x64/
  localreel.exe            # server with the UI embedded
  localreel.toml.example
  README.md, README.txt
  LICENSE
  install.ps1, uninstall.ps1, install-task.ps1
  docs/                    # product docs + core operations manual
  tools/                   # ffmpeg fetch/verify helpers
  ffmpeg/bin/...           # only with -FfmpegDir
```

`build-msi.ps1` turns that stage into the MSI (with an administrative-extract
validation and a check that the directory-choosing wizard is actually present);
`checksums.ps1` hashes the `.zip`/`.msi` into `dist/SHA256SUMS.txt`. The core
repository produces a binary and tests, never an installer.

## Why `localreel-core` no longer builds installers

Packaging used to live in `localreel-core` (`package.ps1`, `build-msi.ps1`,
checksums, install scripts). It moved to this repository deliberately:

1. **Separation of concerns.** The core knows how to build one binary. Installer
   layout, Start Menu shortcuts, default install directories, ffmpeg bundling
   and which documents ship are product decisions, not server internals.
2. **Component replaceability.** The UI is a build-time input, not a crate
   dependency; the product can compose any UI build with the same server. If
   packaging lived in the core, every UI or layout change would force a core
   change.
3. **One release authority.** There is exactly one place that says "this is
   version 0.3.0 of LocalReel" and one place that produces the zip and MSI from
   it. Otherwise a zip built from one checkout and an MSI built from another
   can silently drift.
4. **Independent release cadence.** Components tag and test on their own
   schedule; the product pins them and publishes. A packaging fix does not
   require a server release, and a server release does not have to wait for
   installer docs to move.
5. **Docs live with downloads.** Install, upgrade and release notes are
   versioned next to the artifacts users actually download.

The trade-off is explicit: a product release needs all component checkouts
available. `compose.ps1` fails fast with a clear message when a component is
missing or is at the wrong version, and `-ComponentsDir` / `-CoreDir` /
`-WebDir` cover non-sibling layouts.

## Versioning and compatibility

- **Product version** — this repository's tag (`v0.3.0`) plus the pins in
  `components.json`.
- **Component versions** — each component repository tags its own releases
  (`localreel-core`/`localreel-web` `v0.3.0`, `localreel-format` `v0.1.0`).
- **API contract** — `/api/v1` between the UI and the server; the UI and the
  server are versioned together in a product release.
- **Backup format** — strictly versioned in `localreel-format`; an unknown
  version is rejected, never reinterpreted. Database migrations are
  forward-only; the supported rollback is restoring a backup.
- **Platforms** — Windows is the primary target (per-user MSI and portable
  zip); Linux and Docker are verified under WSL2 and remain secondary.

## What is deliberately *not* here

- No plugin/WASM runtime in v1; extension seams are traits inside the kernel.
- No remote/NAS/S3 media backends; sources are local paths.
- No authentication layer: the server binds loopback only and exists for a
  single local user.
- No cloud sync, account or telemetry service.
- No automated publish pipeline: releases are composed locally and uploaded by
  hand. Artifacts are unsigned.
