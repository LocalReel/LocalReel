# macOS

LocalReel runs as a normal `.app`. The bundle starts the server, opens the
browser at <http://127.0.0.1:3001>, and keeps running in the background — there
is no window, so it is stopped from the web UI (**Admin → Server → Shut down
server**) or with `launchctl` when it was started as a LaunchAgent.

The build is **ad-hoc signed**, not notarised. Notarisation needs an Apple
Developer account; until then the first start of a downloaded copy needs one
right-click → **Open**, or:

```bash
xattr -dr com.apple.quarantine /Applications/LocalReel.app
```

## Build

```bash
# on a Mac with Xcode command line tools, Rust and Node
./scripts/package-macos.sh --build-web
```

Output:

```text
dist/LocalReel.app
dist/localreel-<version>-<arm64|x86_64>.dmg
```

The script:

1. builds the server with the web UI embedded (or reuses
   `<core>/target/release/localreel` with `--skip-build`);
2. writes the bundle (`Contents/MacOS/localreel`, `Info.plist`,
   `PkgInfo`, `AppIcon.icns` built from the shipped 256px PNG);
3. ad-hoc signs it (`codesign -s -`);
4. packs the app, the LaunchAgent template and an `/Applications` link into a
   compressed `.dmg`.

The architecture in the file name is the one the build ran on: an Intel Mac
produces `x86_64`, Apple silicon produces `arm64`. Build once per architecture
(or on the target machine) for now.

## Install

1. Open the `.dmg`, drag **LocalReel** to *Applications*.
2. First start: right-click → **Open** (the ad-hoc signature is unknown to
   Gatekeeper).
3. The browser opens on the setup page; create the owner account.

Data and logs follow the platform rules from
[ADR-012](../../localreel-core/docs/adr/012-state-locations.md):
`~/Library/Application Support/LocalReel` for data, `~/Library/Preferences`
for configuration, and `<data>/logs/localreel.log` for the rolling log.

## Start at login (optional)

The `.dmg` contains `com.localreel.server.plist`. Install it as a LaunchAgent
so the server starts with your session and never opens a browser:

```bash
mkdir -p ~/Library/LaunchAgents
cp /Volumes/LocalReel*/com.localreel.server.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.localreel.server.plist
```

Stop or remove it:

```bash
launchctl unload ~/Library/LaunchAgents/com.localreel.server.plist
rm ~/Library/LaunchAgents/com.localreel.server.plist
```

The agent runs `LocalReel.app/Contents/MacOS/localreel serve --mode server
--no-open`, so it keeps the fixed port and the strict failure behaviour of the
server persona.

## Uninstall

Quit the app (Admin → Server → Shut down server, or `launchctl unload`), then
delete `/Applications/LocalReel.app` and, if you want to remove the library
too, `~/Library/Application Support/LocalReel`.

## What is not done yet

- Notarised builds and a universal (arm64 + x86_64) binary.
- A menu-bar icon: the Windows build has one, macOS still relies on the admin
  page for quitting.
