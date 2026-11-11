'use strict'

/**
 * The desktop shell.
 *
 * It is a window onto the server that already exists on this machine — not a
 * second implementation of the app. Three rules follow from that:
 *
 * 1. Load http://127.0.0.1:<port>, never file://. The UI's relative API paths,
 *    its cookie session and its CSP all assume it was served by the server.
 * 2. Never stop a server this shell did not start; someone's NAS or another
 *    window may be using it.
 * 3. Before an install replaces files on disk, the window has to be gone. The
 *    server shuts itself down for that (see the update runtime in core); the
 *    shell simply follows it out instead of lingering on a dead page.
 */

const { app, BrowserWindow, dialog, shell } = require('electron')
const path = require('node:path')
const { writeFile } = require('node:fs/promises')

const { ensureServer, stopServer } = require('./server')
const { isSameOrigin } = require('./navigation')

let serverChild = null
let stopping = false

const smokeArg = process.argv.find(arg => arg.startsWith('--smoke='))
const smokeFile = smokeArg ? smokeArg.slice('--smoke='.length) : null

function log(message) {
  process.stdout.write(`[shell] ${message}\n`)
}

function createWindow(url) {
  const window = new BrowserWindow({
    width: 1280,
    height: 820,
    minWidth: 960,
    minHeight: 600,
    backgroundColor: '#0b0b0c',
    show: false,
    autoHideMenuBar: true,
    title: 'LocalReel',
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  })

  window.once('ready-to-show', () => window.show())

  // A link in the UI must not turn the shell into a browser: no navigation
  // away from the library, no new Electron windows.
  window.webContents.setWindowOpenHandler(({ url: target }) => {
    if (/^https?:/i.test(target)) void shell.openExternal(target)
    return { action: 'deny' }
  })
  window.webContents.on('will-navigate', (event, target) => {
    if (!isSameOrigin(target, url)) {
      event.preventDefault()
      if (/^https?:/i.test(target)) void shell.openExternal(target)
    }
  })

  void window.loadURL(url)
  return window
}

async function boot() {
  const result = await ensureServer({
    appPath: app.getAppPath(),
    resourcesPath: process.resourcesPath,
    log,
  })
  serverChild = result.spawned ? result.child : null

  if (!result.health) {
    dialog.showErrorBox(
      'LocalReel could not start',
      `No server answered at ${result.url}.\n\n` +
        'Start LocalReel once from the Start menu, or set LOCALREEL_SERVER to a server binary.',
    )
    app.quit()
    return
  }

  log(`server ${result.health.version} is ${result.health.status} at ${result.url}`)
  const window = createWindow(result.url)

  // The server going away is either a crash or an install taking the files. In
  // both cases a window pointing at nothing is worse than no window, and during
  // an install staying open would hold up the replacement.
  if (serverChild) {
    serverChild.once('exit', code => {
      log(`server exited (${code}); closing the window`)
      if (!stopping) app.quit()
    })
  }

  if (smokeFile) {
    window.webContents.once('did-finish-load', async () => {
      // Give the app one paint so the capture shows the interface, not a
      // blank canvas.
      await new Promise(resolve => setTimeout(resolve, 2500))
      try {
        const image = await window.webContents.capturePage()
        await writeFile(smokeFile, image.toPNG())
        log(`smoke screenshot written to ${smokeFile}`)
      } catch (error) {
        log(`smoke capture failed: ${error}`)
        process.exitCode = 1
      } finally {
        app.quit()
      }
    })
  }

  return window
}

if (!app.requestSingleInstanceLock()) {
  app.quit()
} else {
  app.on('second-instance', () => {
    const [window] = BrowserWindow.getAllWindows()
    if (window) {
      if (window.isMinimized()) window.restore()
      window.focus()
    }
  })

  app.whenReady().then(boot)

  app.on('window-all-closed', () => app.quit())

  // Quit is not instant when this shell owns the server: the process has to be
  // gone before the installer can replace the binary, so the first quit is
  // postponed until the child exits.
  app.on('before-quit', event => {
    if (stopping || !serverChild) return
    event.preventDefault()
    stopping = true
    log('stopping the server before quitting')
    void stopServer(serverChild).then(stopped => {
      log(stopped ? 'server stopped' : 'server did not stop in time')
      app.quit()
    })
  })
}
