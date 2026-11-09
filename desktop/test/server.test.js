'use strict'

/**
 * The parts of the shell that can be checked without a display.
 *
 * Electron is a window library; the decisions that actually break a desktop app
 * are not in it. These tests cover which server the window points at, whether
 * the shell may stop it, and the ordering that lets an installer replace files.
 */

const assert = require('node:assert/strict')
const { test } = require('node:test')
const { EventEmitter } = require('node:events')

const {
  DEFAULT_PORT,
  baseUrl,
  resolvePort,
  serverCandidates,
  probe,
  waitForHealth,
  ensureServer,
  stopServer,
} = require('../src/server')

function fakeFetch(responses) {
  let index = 0
  return async () => {
    const response = responses[Math.min(index, responses.length - 1)]
    index += 1
    if (response instanceof Error) throw response
    if (response === 'nope') return { ok: false, json: async () => ({}) }
    return { ok: true, json: async () => response }
  }
}

function fakeChild() {
  const child = new EventEmitter()
  child.exitCode = null
  child.stdout = new EventEmitter()
  child.stderr = new EventEmitter()
  child.killed = false
  child.kill = () => {
    child.killed = true
    child.exitCode = 0
    setImmediate(() => child.emit('exit', 0))
    return true
  }
  return child
}

test('the window points at loopback, not at a file', () => {
  assert.equal(baseUrl(3101), 'http://127.0.0.1:3101')
  assert.match(baseUrl(1), /^http:\/\/127\.0\.0\.1:/)
})

test('the port falls back to the default instead of trusting the environment', () => {
  assert.equal(resolvePort({}), DEFAULT_PORT)
  assert.equal(resolvePort({ LOCALREEL_PORT: '4100' }), 4100)
  assert.equal(resolvePort({ LOCALREEL_PORT: 'nonsense' }), DEFAULT_PORT)
  assert.equal(resolvePort({ LOCALREEL_PORT: '0' }), DEFAULT_PORT)
  assert.equal(resolvePort({ LOCALREEL_PORT: '70000' }), DEFAULT_PORT)
})

test('an explicit server path wins over the bundled ones', () => {
  const candidates = serverCandidates({
    env: { LOCALREEL_SERVER: 'D:/build/localreel.exe' },
    appPath: 'C:/Program Files/LocalReel/desktop/main.js',
    resourcesPath: 'C:/Program Files/LocalReel/desktop/resources',
  })
  assert.equal(candidates[0], 'D:/build/localreel.exe')
  assert.ok(candidates.some(candidate => candidate.includes('server')))
})

test('the installer layout is searched, where the server sits above the shell', () => {
  const path = require('node:path')
  const install = 'C:/Users/x/AppData/Local/Programs/LocalReel'
  const candidates = serverCandidates({ env: {}, appPath: path.join(install, 'desktop', 'main.js') })
  assert.ok(
    candidates.includes(path.join(install, 'localreel.exe')),
    'the server next to the install directory must be a candidate',
  )
})

test('a probe that times out is a miss, not an exception', async () => {
  const fetchImpl = fakeFetch([new Error('ECONNREFUSED')])
  assert.equal(await probe(fetchImpl, 'http://127.0.0.1:1'), null)
})

test('waiting tolerates "not yet" and returns the first real answer', async () => {
  const fetchImpl = fakeFetch([new Error('not yet'), 'nope', { status: 'ok' }])
  const health = await waitForHealth(fetchImpl, 'http://127.0.0.1:3101', {
    attempts: 5,
    intervalMs: 1,
  })
  assert.equal(health.status, 'ok')
})

test('waiting gives up when the server never answers', async () => {
  const fetchImpl = fakeFetch([new Error('no')])
  const health = await waitForHealth(fetchImpl, 'http://127.0.0.1:3101', {
    attempts: 3,
    intervalMs: 1,
  })
  assert.equal(health, null)
})

test('an already running server is adopted, not started again', async () => {
  let spawned = false
  const result = await ensureServer({
    fetchImpl: fakeFetch([{ status: 'ok', version: '0.5.6' }]),
    spawnImpl: () => {
      spawned = true
      return fakeChild()
    },
    attempts: 1,
    intervalMs: 1,
  })
  assert.equal(spawned, false)
  assert.equal(result.spawned, false)
  assert.equal(result.child, null, 'the shell must not own a server it did not start')
  assert.equal(result.health.version, '0.5.6')
})

test('a server the shell started is tracked so it can be stopped', async () => {
  const child = fakeChild()
  const result = await ensureServer({
    fetchImpl: fakeFetch([new Error('no'), { status: 'ok' }]),
    spawnImpl: () => child,
    attempts: 5,
    intervalMs: 1,
  })
  assert.equal(result.spawned, true)
  assert.equal(result.child, child)
})

test('stopping resolves only after the process is gone', async () => {
  const child = fakeChild()
  const stopped = await stopServer(child)
  assert.equal(stopped, true)
  assert.equal(child.killed, true)
})

test('a server that ignores the request is killed, and that is reported', async () => {
  const child = new EventEmitter()
  child.exitCode = null
  child.stdout = new EventEmitter()
  child.stderr = new EventEmitter()
  child.kill = signal => {
    child.signals = (child.signals || []).concat(signal || 'SIGTERM')
    return true
  }
  const stopped = await stopServer(child, { graceMs: 10 })
  assert.equal(stopped, false)
  // Asked politely first, then forced: the installer cannot wait forever.
  assert.deepEqual(child.signals, ['SIGTERM', 'SIGKILL'])
})

test('stopping nothing is a success, not an error', async () => {
  assert.equal(await stopServer(null), true)
  const already = fakeChild()
  already.exitCode = 0
  assert.equal(await stopServer(already), true)
})
