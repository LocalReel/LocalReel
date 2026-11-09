# LocalReel desktop shell

An Electron window onto the LocalReel server that is already running on this
machine. It is a shell, not a second copy of the app: the window loads
`http://127.0.0.1:<port>`, so the UI keeps its cookie session, its relative API
paths and its CSP exactly as a browser would see them.

## Running it

```powershell
npm install
npm start
```

The shell looks for a server in this order:

1. `LOCALREEL_SERVER` — an explicit binary.
2. `<resources>/server/localreel[.exe]` — a packaged app.
3. `<next to the app>/server/localreel[.exe]`, then `<next to the app>/localreel[.exe]`.
4. `localreel` on `PATH`.

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

## Updates

There is no `electron-updater` here on purpose. The server signs and verifies its
own `update.json` (ed25519), downloads the installer and hands over to
`localreel-setup.exe /S /wait /launch`. The shell's only job in that sequence is
to get out of the way: when the server exits — because it is installing — the
window closes too, and `before-quit` waits for the child process to be gone
before letting the app exit.

## Known environment trap

If `ELECTRON_RUN_AS_NODE=1` is set in the environment (some tools set it
globally), `electron .` runs the file with plain Node and fails with
`Cannot read properties of undefined (reading 'requestSingleInstanceLock')`.
Clear the variable before launching.
