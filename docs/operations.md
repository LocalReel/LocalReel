# LocalReel operations manual

This is the practical runbook for a local, single-user LocalReel install. The
server binds loopback by default; it is not an internet-facing service.

## Layout

Default paths are relative to the working directory:

```text
localreel/
  localreel.exe
  localreel.toml            # optional; defaults + LOCALREEL_* env otherwise
  data/
    localreel.db            # SQLite (WAL)
    thumbnails/             # thumb_<uid>.jpg
    transcodes/             # transcode_<uid>_<preset>_....mp4
```

`LOCALREEL_DATA_DIR`, `LOCALREEL_CONFIG`, `LOCALREEL_HOST` and
`LOCALREEL_PORT` override the defaults. `localreel doctor` prints the resolved
paths.

## First run

```powershell
.\localreel.exe doctor
.\localreel.exe serve
# http://127.0.0.1:3001
```

To start automatically at logon on Windows, see
[service.md](service.md) (`install-task.ps1`). The installer's Start Menu
shortcut is manual start only.

Add a source through the UI/API or the Admin page:

```powershell
Invoke-RestMethod -Method Post http://127.0.0.1:3001/api/v1/sources `
  -ContentType application/json -Body '{"uri":"D:\\Videos"}'
Invoke-RestMethod -Method Post http://127.0.0.1:3001/api/v1/sources/1/scan
```

The scan job discovers files; probe/thumbnail jobs follow automatically.
`GET /api/v1/jobs` and `GET /api/v1/events` expose progress.

## Logs and health

- Logs go to **stderr** via `tracing`; there is no file rotation in v1.
  Redirect if running as a service:
  `.\localreel.exe serve 2>> localreel.log`
- `RUST_LOG=info` (default) / `RUST_LOG=debug` changes verbosity.
- `GET /api/v1/health`: status, version, DB, ffmpeg, media count.
- `GET /api/v1/admin/health`: paths, recent jobs, DB/ffmpeg, cache, watchers.
- `GET /api/v1/admin/cache`: transcode/thumbnail bytes and counts.
- `GET /api/v1/scan-runs`, `/scan-issues`: scan history and failures.

### Error details

Error bodies carry a stable `code` and a `message`. An optional `detail` field
holds diagnostics — a library path, an ffmpeg stderr tail — and is only
serialized for an **administrator** whose request comes from loopback, or for any
administrator when `RUST_LOG=debug`. A viewer never receives it, so a shared
NAS deployment does not leak the library layout.

The UI shows the detail behind a collapsed **Details** element. The job history
and the server log always keep the full text.

## Automatic rescans

Enabled local sources are watched; dropping a file into a library folder
triggers an incremental scan within a few seconds, with no button to press.

- Each source gets one recursive OS watcher
  (`ReadDirectoryChangesW` on Windows, `inotify` on Linux, `FSEvents` on macOS).
  Events are debounced for two seconds, so copying a season produces one scan,
  not one per file.
- The job is the same `scan_source` job the Admin button queues, with the same
  `scan:<id>` dedupe key: a manual scan and a filesystem change cannot run two
  competing scans.
- Hidden files, `~$` lock files, `.tmp`/`.part`/`.crdownload` partials and
  NAS metadata directories (`@eaDir`, `#recycle`) are ignored.

Configuration:

```toml
[scan]
watch = true        # default
```

`LOCALREEL_WATCH=0` disables watching without editing the config file.

Watching is best effort. `GET /api/v1/admin/health` reports
`"watchers": {"enabled": true, "active": 2, "skipped": []}`; the Admin page
shows the same as an **Auto watch** statistic. A source that cannot be watched
is listed under `skipped` (for example `webdav:https://nas/dav` for a plugin
source, or `local:D:\media (the platform refused to watch it: ...)`), the server
logs a warning, and everything else keeps working. Network shares that do not
forward filesystem events (SMB/NFS without inotify support) fall into this
group; use the manual scan for those, or enable watching on the machine that
hosts the files.


## ffmpeg

Media probing/thumbnails/transcodes need `ffmpeg` and `ffprobe`.
`doctor` shows the resolved paths. Discovery order is documented in
[packaging.md](packaging.md); in short:

1. explicit config/env paths,
2. `LOCALREEL_FFMPEG_DIR`,
3. next to the exe (`ffmpeg/bin`, `tools/ffmpeg/bin`, `bin`),
4. current directory,
5. `PATH`.

If ffmpeg is missing, the server still starts and the API/UI work; scans add
rows but metadata/playback preparation fails until it is installed. Run
`tools\verify-ffmpeg.ps1` to check an install.

### Hardware acceleration

```toml
[media]
hwaccel = "auto"   # default; or none, nvenc, qsv, vaapi, videotoolbox
```

`LOCALREEL_HWACCEL` overrides it. The encoder table (`ffmpeg -encoders`) is read
once at startup and the resolved choice is logged and shown by
`localreel doctor` as `hwaccel: nvenc (preferred: auto)`, and by
`GET /api/v1/admin/health` as `"hwaccel"`.

- `auto` picks the first encoder this build reports on this platform: NVENC,
  then QSV, then VAAPI on Linux; VideoToolbox on macOS. A requested encoder that
  is not installed falls back to the CPU rather than failing.
- Each encoder keeps its own quality flags — `-cq 23 -preset p5` for NVENC,
  `-global_quality 23` for QSV, `-qp 23` plus `format=nv12,hwupload` on the
  render node for VAAPI, `-q:v 65` for VideoToolbox. `-crf` and
  `-preset veryfast` belong to libx264 and are not passed.
- If ffmpeg fails with a device error (`No capable devices`, `Cannot load`,
  `Invalid device`, an impossible format conversion) the job retries once on the
  CPU and reports
  `transcode ready (hardware encoder failed, retried on CPU)`. A corrupt input is
  not retried — that would fail twice and hide the real reason.
- HLS packaging still encodes with libx264; only MP4 transcodes use the
  accelerator.

## Backup and restore

Application backup (portable, versioned JSON) is the primary state backup:

```powershell
# without stopping the server
.\localreel.exe backup-export .\backup.json

# one-command restore into a fresh data dir:
.\localreel.exe restore .\backup.json --dry-run
.\localreel.exe restore .\backup.json
# sources -> scan/probe/thumbnails -> state replay, in one command
```

HTTP equivalents:

- `GET /api/v1/backup` downloads JSON;
- `POST /api/v1/backup?dry_run=true|false` imports only (two-pass);
- `POST /api/v1/backup/restore` queues a `restore_backup` job from a JSON body
  (32 MiB limit);
- `POST /api/v1/backup/restore/upload` streams the body to
  `data/backup-uploads/<uuid>.json` and queues `restore_backup_file`
  (512 MiB limit, cleaned after the job). Admin uses this streaming route.
  The job deserializes directly from the file (no second raw byte buffer), but
  the typed state list still fits in memory.

Admin -> Backup now has both **Import** and **Restore**; Restore queues the job
and progress appears under Recent jobs. The job reports phases (`importing
sources` -> `probing media` per item -> `replaying states`) and can be canceled
with `POST /api/v1/jobs/{id}/cancel`. A canceled restore leaves whatever was
already imported/probed in place; it does not roll back.

The backup contains sources and per-media state (uid/path/flags/position). It
does **not** contain media files, thumbnails or transcodes. After importing into
a new machine, scan the sources before importing state; state is matched by
`uid` first, then by URI. Progress is never lowered, flags are OR-ed.

A verified restore drill on the real 12-file library (2026-11-01):

- streaming upload: a 34 MB JSON body was accepted (above the old 32 MiB
  limit), job succeeded, and the temp upload was removed;
- `restore` reported: 1 source, 12 discovered, 12 probed, 12 thumbnails,
  12 states imported, 0 missing;
- `/media?filter=continue` total=12, E08 position=306.44337;
- direct playback and Range 206 worked immediately (no second manual pass).

The manual/importer-only path still works: import sources, scan via server/API,
then import the backup again. Note: the CLI `scan` command only discovers rows;
`restore` runs probe/thumbnails itself, while server/API scan enqueues jobs.

Full DB backup: stop the server first, then copy `data/localreel.db` (and
`-wal`/`-shm` if present). Restoring an old DB is a rollback of both schema and
state; back up the current DB first.

## Streaming and validators

`GET /api/v1/media/{id}/stream` (and `HEAD`) serves stored bytes with
`Accept-Ranges: bytes`, a weak `ETag` (`W/"<size>-<mtime>"`) and
`Last-Modified`.

- `If-None-Match` and `If-Modified-Since` revalidate and return `304`.
- `Range` resumes or seeks; single ranges are capped at 8 MiB per response.
- `If-Range` accepts the ETag or the `Last-Modified` date. A stale validator
  makes the server ignore `Range` and return the full file with `200`.
- Unsatisfiable ranges return `416` with `Content-Range: bytes */<size>`.

## HLS playback

Source media can also be packaged as an HLS VOD playlist and served to clients
that play HLS natively (Safari, VLC, mobile players). The web UI keeps using
direct play and MP4 transcodes.

```bash
# First call: 202 with a job id while ffmpeg packages the playlist
curl -X POST "http://127.0.0.1:3001/api/v1/media/42/playback?format=hls"
# {"mode":"preparing","job_id":"...","message":"hls packaging started"}

# Once the job succeeds: 200 with a ready URL
curl -X POST "http://127.0.0.1:3001/api/v1/media/42/playback?format=hls"
# {"mode":"ready","url":"/hls/<media-uid>/index.m3u8"}
```

In the web player, the **HLS** button switches playback to this mode; browsers
without native HLS support load `hls.js` on demand.

The playlist and segments are plain files under `data/hls` (`media.hls_dir`),
served with `application/vnd.apple.mpegurl` and `video/mp2t`. Segments are four
seconds by default and use H.264/AAC. The cache GC job removes HLS packages that
have not been used for `cache_max_age_hours`.

## Multiple audio tracks

A file may carry several audio tracks (a Japanese and an English dub, a
commentary, a stereo and a 5.1 mix). They are discovered by the `probe_media`
job and stored in `media_tracks`; a re-probe replaces them wholesale, so a track
that left the container leaves the menu too.

```bash
# what the player offers
curl "http://127.0.0.1:3001/api/v1/media/42/tracks"
# [{"id":7,"kind":"audio","language":"ja","channels":6,"is_default":true,...}]

# play with a specific track
curl -X POST "http://127.0.0.1:3001/api/v1/media/42/playback?audio=8"
```

Behaviour:

- Track languages are normalized (`chi`/`zho`/`zh-CN` → `zh`, `zh-TW` →
  `zh-Hant`, `eng` → `en`) so the UI can compare them.
- When the container marks no default, the first audio track is the default.
- Choosing the default track leaves direct play alone: the browser plays the file
  as it is. Choosing any other track answers `202 preparing` and starts a
  remux/transcode, because a browser cannot switch embedded audio on a file it
  is playing. The player shows the same "preparing" state as any other transcode.
- The audio choice is part of the transcode cache key
  (`h264_aac_a2`), so two viewers watching different languages get two files and
  neither overwrites the other.
- HLS packaging keeps the file's default audio track; switch to a track and the
  player falls back to MP4 playback instead of HLS.

## Transcode budget

Every encode — an MP4 transcode or an HLS package — draws from one process-wide
budget. Without it, "two people press play at once" means two ffmpeg processes
fighting over one NAS CPU.

```toml
[media]
max_concurrent_transcodes = 2   # default
idle_session_secs = 900         # default: 15 minutes
```

`LOCALREEL_MAX_CONCURRENT_TRANSCODES` and `LOCALREEL_IDLE_SESSION_SECS`
override them. A value of `1` serialises all encoding; `0` is rejected.

Behaviour:

- Jobs that cannot get a slot stay **queued** with
  `message: "waiting for a transcode slot"`; they never occupy a worker thread.
  A saturated budget does not block scans, probes or thumbnails — the worker
  skips the saturated kinds and takes other work first.
- `GET /api/v1/admin/health` reports
  `"transcodes": {"running": 1, "queued": 3, "limit": 2, "sessions": 1}` and the
  Admin page shows the same as a **Transcode slots** statistic. The job list
  shows what is waiting.
- A playback session is refreshed by every `/playback`, `/stream` and `/hls/…`
  request. After `idle_session_secs` with no request the session is dropped and
  the encode it was waiting on is cancelled. Finished cache files are **not**
  deleted here — that is the cache GC's job.
- On Linux, if free memory is below `max(200 MB, 5 %)` a new encode is refused
  with `503` and `{"error":{"code":"busy",…}}` instead of pushing the machine
  into swap. Other platforms skip the check.

## Upgrade

1. Export a backup and copy `data/localreel.db` while stopped.
2. Replace `localreel.exe` (and `ffmpeg/bin` if bundled).
3. Run `.\localreel.exe migrate`; migrations are forward-only and applied on
   `serve` too.
4. Run `.\localreel.exe doctor` and check `/api/v1/health`.
5. Open the UI and spot-check playback.

There are no down migrations. Roll back by restoring the DB and the old binary.

## Maintenance

- Cache GC runs at startup and every `jobs.cache_gc_interval_secs` (default 24h,
  0 disables). It removes transcodes by age/size and keeps anything used in the
  last hour. Admin → Cache can run it manually.
- Archived sources keep their media/state; they are hidden from the default
  source list, cannot be scanned, and their media is hidden from Library/Reels
  unless the `Archived` filter is enabled.
- If a source directory moves, use Admin → Sources → Remap (or
  `POST /api/v1/sources/{id}/remap` / `localreel.exe remap-source`). Preview is
  dry-run; only files present under the new root are rewritten.

## CLI reference

| Command | Purpose |
|---|---|
| `serve` | Run HTTP server (default) |
| `migrate` | Apply DB migrations and exit |
| `doctor` | Config/DB/ffmpeg/web checks |
| `scan <source-id>` | Synchronous discovery for one source |
| `remap-source <id> <uri> [--dry-run]` | Rewrite a moved local root |
| `import <legacy.db> [--dry-run]` | Import a copy of a v1 database |
| `state-import <legacy.db> [--source-id N] [--dry-run]` | Map v1 states by episode |
| `backup-export <out.json>` | Export v2 source/state backup |
| `backup-import <file> [--dry-run]` | Merge v2 backup (sources/states only) |
| `restore <file> [--dry-run]` | Import + scan/probe/thumbnail + replay states |

## Troubleshooting

| Symptom | Check / fix |
|---|---|
| UI is API-only placeholder | Build `localreel-web/dist` before `cargo build`, or use the packaged release whose web UI is embedded |
| `ffmpeg: false` in health | Run `doctor`; install ffmpeg or set `LOCALREEL_FFMPEG_DIR` |
| Scan finds 0 files | `GET /scan-runs`; verify source URI/dir exists and is not archived |
| Media shows missing | File moved/deleted; restore it and rescan, or remap the source root |
| Playback stuck on `preparing` | Check `GET /jobs` for failed `transcode`; check ffmpeg and disk space |
| Port already in use | Set `LOCALREEL_PORT` or `server.port` |
| `source is archived` on scan | Restore it in Admin or `PATCH /sources/{id} {"archived":false}` |
| Cache keeps growing | Check `jobs.cache_gc_interval_secs`, run Admin → Run GC, inspect `/admin/cache` |
| DB locked | Ensure only one server process; check `-wal`/`-shm`; do not run two writers |
| A file dropped into a folder is not detected | `GET /admin/health` → `watchers.skipped`. SMB/NFS shares often forward no events: scan manually, or watch on the machine that hosts the files |
| An error shows only "media operation failed" | The detail is admin-only. Sign in as the owner from the server machine, or set `RUST_LOG=debug`, then open **Details** under the message or read the job's `error` |

## Security

- Default host is `127.0.0.1`; the API has a loopback Host/Origin guard.
- Do not set `allow_remote`/`LOCALREEL_ALLOW_REMOTE=1` without an external
  authentication layer (v1 has no users/tokens).
- The server sets `Content-Security-Policy` (`default-src 'self'`), `nosniff`,
  `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer` and a restrictive
  `Permissions-Policy` on responses. The UI has no inline scripts/styles.
- The server has filesystem access to configured sources; keep the data dir and
  config private.
