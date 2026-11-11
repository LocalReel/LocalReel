'use strict'

function isSameOrigin(target, base) {
  try {
    const parsed = new URL(target)
    return /^https?:$/.test(parsed.protocol) && parsed.origin === new URL(base).origin
  } catch {
    return false
  }
}

module.exports = { isSameOrigin }
