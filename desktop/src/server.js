'use strict'

/**
 * Finding, starting and stopping the LocalReel server.
 *
 * The shell deliberately does not own the library: the server is a separate
 * process with its own data directory, so the same installation can be reached
 * from the window, from a browser, and from a phone on the network. This module
 * only decides which server the window should talk to and whether the shell is
 * responsible for stopping it.
 */

const { spawn } = require('node:child_process')
const path = require('node:path')

const DEFAULT_PORT = 3101
const HEALTH_PATH = '/api/v1/health'

/** Where the window should point. Relative API calls in the UI resolve here. */
function baseUrl(port) {
  return `http://127.0.0.1:${port}`
}

/** The port to try, from the environment or the product default. */
function resolvePort(env = process.env) {
  const raw = env.LOCALREEL_PORT
  if (!raw) return DEFAULT_PORT
  const port = Number.parseInt(raw, 10)
  if (!Number.isInteger(port) || port <= 0 || port > 65535) return DEFAULT_PORT
  return port
}

/**
 * Candidate server binaries, in order of preference.
 *
 * A packaged app keeps the server next to itself; a development checkout has it
 * in the Rust target directory. An explicit LOCALREEL_SERVER wins so an
 * operator can point the window at a build of their own.
 */
function serverCandidates({ env = process.env, appPath, resourcesPath } = {}) {
  const exe = process.platform === 'win32' ? 'localreel.exe' : 'localreel'
  const candidates = []
  if (env.LOCALREEL_SERVER) candidates.push(env.LOCALREEL_SERVER)
  if (resourcesPath) candidates.push(path.join(resourcesPath, 'server', exe))
  if (appPath) {
    candidates.push(path.join(path.dirname(appPath), 'server', exe))
    candidates.push(path.join(path.dirname(appPath), exe))
    // A packaged app has appPath = <install>\desktop\resources\app.asar, so the
    // server one level above `desktop\` is three levels up from the file.
    candidates.push(path.join(path.dirname(appPath), '..', '..', exe))
    // A development checkout keeps the shell in `desktop\` and the server in
    // the repository root.
    candidates.push(path.join(path.dirname(appPath), '..', exe))
  }
  candidates.push(exe)
  return candidates
}

/** One non-blocking probe. Resolves to the parsed health body, or null. */
async function probe(fetchImpl, url, timeoutMs = 1500) {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)
  try {
    const response = await fetchImpl(`${url}${HEALTH_PATH}`, { signal: controller.signal })
    if (!response.ok) return null
    return await response.json()
  } catch {
    return null
  } finally {
    clearTimeout(timer)
  }
}

/**
 * Poll until the server answers or the deadline passes.
 *
 * A freshly started server spends a moment on migrations, so "not yet" is the
 * normal answer for the first second and must not be treated as a failure.
 */
async function waitForHealth(fetchImpl, url, { attempts = 60, intervalMs = 250 } = {}) {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const health = await probe(fetchImpl, url)
    if (health && health.status) return health
    await new Promise(resolve => setTimeout(resolve, intervalMs))
  }
  return null
}

/**
 * Start the server and wait for it.
 *
 * Resolves `{ child, spawned: true, health }` when this shell started it, and
 * `{ child: null, spawned: false, health }` when one was already running — the
 * difference matters at quit time, because the shell must not stop a server it
 * did not start.
 */
async function ensureServer({
  fetchImpl = fetch,
  env = process.env,
  appPath,
  resourcesPath,
  spawnImpl = spawn,
  attempts = 60,
  intervalMs = 250,
  log = () => {},
} = {}) {
  const port = resolvePort(env)
  const url = baseUrl(port)

  const existing = await probe(fetchImpl, url)
  if (existing) {
    log(`attached to an existing server on ${url}`)
    return { child: null, spawned: false, url, health: existing }
  }

  const candidates = serverCandidates({ env, appPath, resourcesPath })
  for (const binary of [...new Set(candidates)]) {
    log(`starting ${binary}`)
    let child
    try {
      child = spawnImpl(binary, ['serve'], {
        env: { ...env, LOCALREEL_PORT: String(port) },
        stdio: ['ignore', 'pipe', 'pipe'],
        windowsHide: true,
      })
    } catch (error) {
      log(`could not start ${binary}: ${error.message}`)
      continue
    }
    let failed = false
    // Keep the error listener for the child's full lifetime: errors are not
    // necessarily accompanied by exit (ENOENT, EACCES in particular).
    child.on('error', error => {
      failed = true
      log(`could not start ${binary}: ${error.message}`)
    })
    child.once('exit', () => { failed = true })
    child.stdout?.on('data', chunk => log(String(chunk).trimEnd()))
    child.stderr?.on('data', chunk => log(String(chunk).trimEnd()))
    for (let attempt = 0; attempt < attempts && !failed; attempt += 1) {
      const health = await probe(fetchImpl, url)
      if (health?.status && !failed) return { child, spawned: true, url, health }
      await new Promise(resolve => setTimeout(resolve, intervalMs))
    }
    // Do not start another candidate while this one is still running. Let the
    // caller report the timeout and stop the owned process normally.
    if (!failed) return { child, spawned: true, url, health: null }
  }
  return { child: null, spawned: false, url, health: null }
}

/**
 * Stop a server this shell started, and resolve only once it is gone.
 *
 * ->quit is not enough: the installer replaces files on disk, so "we asked" is
 * not "it finished". The promise settles when the process exits or the grace
 * period runs out.
 */
function stopServer(child, { graceMs = 8000, kill = true } = {}) {
  if (!child || child.exitCode !== null) return Promise.resolve(true)
  return new Promise(resolve => {
    let settled = false
    const finish = stopped => {
      if (settled) return
      settled = true
      clearTimeout(timer)
      resolve(stopped)
    }
    const timer = setTimeout(() => {
      if (kill && child.exitCode === null) child.kill('SIGKILL')
      finish(false)
    }, graceMs)
    child.once('exit', () => finish(true))
    try {
      child.kill()
    } catch {
      finish(true)
    }
  })
}

module.exports = {
  DEFAULT_PORT,
  baseUrl,
  resolvePort,
  serverCandidates,
  probe,
  waitForHealth,
  ensureServer,
  stopServer,
}
