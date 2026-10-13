import assert from 'node:assert/strict'
import test from 'node:test'

import { verifyUpdaterSignature } from './verify-updater-signature.mjs'

const encodedPublicKey =
  'dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQ5QUJFQzVDODdCRUE5RjUKUldUMXFiNkhYT3lyU1NJRGlEWVlULzFkODNQTUZEZy9LM0hCRU4xcG1pa1AycWpaVGtzWFNuanIK'
const committedPublicKey =
  'dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDFFRTJCMEQ5Rjc4Q0NFODAKUldTQXpvejMyYkRpSGk0TlAxTmtjNENiMFltSkpzOStyd3B4bkVDb3dKYXJRSmI2TE91VE5vdlIK'
const content = Buffer.from('mutsumi release readiness probe\n')
const signatureText =
  'dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUMXFiNkhYT3lyU2JKMG5ROHZobVBQTDJMbGc2YXdmdEdMeXV4aDVKMUxvMXZKYWh2eEx5YmNQOGF2M1hJS3grK3p6K2ZqTlVJUHlFa0kzR3NGaGdFQmZwZktrYXYreFFRPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzg3OTg0NDA3CWZpbGU6cHJvYmUudHh0CkorNHpCY0s5VlM2SjE3dXFwcWF5b005cDlRUUhyZWZ6L3JObHVuR0tHZEtmaTQwaTNnU1R0U0k0YytId0owbTR6aFJaK1AwdDdnVHdnU0o0cVhveEF3PT0K'

function rewriteSignature(rewrite) {
  const decoded = Buffer.from(signatureText, 'base64').toString('utf8')
  return Buffer.from(rewrite(decoded), 'utf8').toString('base64')
}

test('verifies a Tauri prehashed signature against its encoded Minisign public key', () => {
  assert.doesNotThrow(() =>
    verifyUpdaterSignature({ encodedPublicKey, content, signatureText }),
  )
})

test('rejects a probe signed for different content', () => {
  assert.throws(
    () =>
      verifyUpdaterSignature({
        encodedPublicKey,
        content: Buffer.from('tampered readiness probe\n'),
        signatureText,
      }),
    /does not verify/,
  )
})

test('rejects a probe signed by a private key that does not match the committed key', () => {
  assert.throws(
    () =>
      verifyUpdaterSignature({
        encodedPublicKey: committedPublicKey,
        content,
        signatureText,
      }),
    /different private key/,
  )
})

test('rejects a signature whose trusted comment was changed', () => {
  assert.throws(
    () =>
      verifyUpdaterSignature({
        encodedPublicKey,
        content,
        signatureText: rewriteSignature((value) =>
          value.replace('file:probe.txt', 'file:other.txt'),
        ),
      }),
    /trusted comment did not verify/,
  )
})

test('rejects malformed encoded updater public keys', () => {
  assert.throws(
    () => verifyUpdaterSignature({ encodedPublicKey: 'not-base64', content, signatureText }),
    /not valid base64/,
  )
})
