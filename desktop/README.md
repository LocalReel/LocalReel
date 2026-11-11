# LocalReel desktop shell

An Electron window onto the LocalReel server that is already running on this
machine. It is a shell, not a second copy of the app: the window loads
`http://127.0.0.1:<port>`, so the UI keeps its cookie session, its relative API
paths and its CSP exactly as a browser would see them.

## Running it

```powershell
npm ci
# Electron 44.9.1 has no postinstall hook: explicitly fetch its runtime.
node node_modules/electron/install.js
npm start
```

A successful `npm ci` or `npm rebuild electron` does not prove that the runtime
exists: Electron 44.9.1 downloads it on explicit installation or first import.
The explicit command above uses the Electron cache and verifies its checksums;
the Windows x64 download is approximately 151 MiB before extraction. To verify
without opening a window or starting the LocalReel server:

```powershell
Get-Content node_modules/electron/dist/version # expected: 44.9.1
$previous = $env:ELECTRON_RUN_AS_NODE
try {
    $env:ELECTRON_RUN_AS_NODE = '1'
    & ./node_modules/electron/dist/electron.exe -p 'process.versions.electron'
} finally {
    $env:ELECTRON_RUN_AS_NODE = $previous
}
```

Do not use `require('electron')` as a read-only availability check: it can start
a download when the runtime is absent. Installing the npm package with
`--ignore-scripts` alone is not evidence that an existing packaged application
contains the patched runtime; rebuild and verify the packaged executable too.

The shell looks for a server in this order:

1. `LOCALREEL_SERVER` — an explicit binary.
2. `<resources>/server/localreel[.exe]` — a packaged app.
3. `<next to the app>/server/localreel[.exe]`, then `<next to the app>/localreel[.exe]`.
4. `../localreel[.exe]` — the installer layout, where the shell lives in
   `desktop\` and the server sits one level up.
5. `localreel` on `PATH`.

If one already answers `/api/v1/health` on the port, the shell attaches to it and
leaves it running; only a server the shell started is stopped when the window
closes.

| Variable | Default | Meaning |
| --- | --- | --- |
| `LOCALREEL_SERVER` | — | Server binary to start when none is running |
| `LOCALREEL_PORT` | `3101` | Port to probe and pass to a started server |

## Tests

```powershell
npm test
```

`node --test` covers the parts that break desktop apps without a display: which
server the window points at, when the shell may stop it, and the ordering that
lets an installer replace files.

## Packaging

```powershell
# 1. put the server where the shell will look for it
copy ..\dist\localreel.exe resources\server\localreel.exe
# 2. build an unpacked app directory (the WiX installer harvests this)
npm run pack
```

`npm run pack` writes `out\win-unpacked\` — `LocalReel.exe` plus
`resources\app.asar` and, when the binary was copied in first, the server under
`resources\server`. In the release pipeline the binary is *not* copied in: the
installer places the server one level above this directory, which is why the
lookup above includes `../localreel.exe`.

The WiX installer is what users run: `scripts/compose.ps1` stages this directory
into the product, `scripts/build-msi.ps1 -Edition desktop` harvests every file
into components, and the shortcuts point at `desktop\LocalReel.exe`. `npm run
dist` additionally produces a `.dmg` (macOS) and an `.AppImage` (Linux) on those
platforms; the release workflow runs it for both.

## Updates

There is no `electron-updater` here on purpose. The server signs and verifies its
own `update.json` (ed25519), downloads the installer and hands over to
`localreel-setup.exe /S /wait /launch`. The shell's only job in that sequence is
to get out of the way: when the server exits — because it is installing — the
window closes too, and `before-quit` waits for the child process to be gone
before letting the app exit.

Which installer it downloads depends on the edition: `localreel-update` looks for
`setup-desktop` when `desktop/LocalReel.exe` exists beside the server, and for
`setup` when it does not. A server installation therefore never grows a shell it
did not ask for, and a desktop installation keeps its window across updates.

## Known environment trap

If `ELECTRON_RUN_AS_NODE=1` is set in the environment (some tools set it
globally), `electron .` runs the file with plain Node and fails with
`Cannot read properties of undefined (reading 'requestSingleInstanceLock')`.
Clear the variable before launching.
