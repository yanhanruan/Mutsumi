import { createHash, createPublicKey, timingSafeEqual, verify } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { pathToFileURL } from 'node:url'

const ED25519_SPKI_PREFIX = Buffer.from('302a300506032b6570032100', 'hex')
const PREHASHED_ALGORITHM = Buffer.from('ED')
const PUBLIC_KEY_ALGORITHMS = new Set(['Ed', 'ED'])

function decodeBase64(value, label) {
  if (
    typeof value !== 'string' ||
    value.length === 0 ||
    value.length % 4 === 1 ||
    !/^[A-Za-z0-9+/]+={0,2}$/.test(value)
  ) {
    throw new Error(`${label} is not valid base64`)
  }

  const decoded = Buffer.from(value, 'base64')
  if (decoded.toString('base64').replace(/=+$/, '') !== value.replace(/=+$/, '')) {
    throw new Error(`${label} is not canonical base64`)
  }
  return decoded
}

function normalizedLines(value, expectedCount, label) {
  if (typeof value !== 'string' || value.includes('\r')) {
    throw new Error(`${label} must use UTF-8 LF text`)
  }
  const withoutFinalNewline = value.endsWith('\n') ? value.slice(0, -1) : value
  const lines = withoutFinalNewline.split('\n')
  if (lines.length !== expectedCount || lines.some((line) => line.length === 0)) {
    throw new Error(`${label} has an unexpected Minisign structure`)
  }
  return lines
}

function parseEncodedPublicKey(encodedPublicKey) {
  const publicKeyText = decodeBase64(encodedPublicKey, 'updater public key').toString('utf8')
  const [comment, encodedKey] = normalizedLines(publicKeyText, 2, 'updater public key')
  if (!comment.startsWith('untrusted comment: minisign public key')) {
    throw new Error('updater public key has an unexpected comment')
  }

  const keyPacket = decodeBase64(encodedKey, 'Minisign public key packet')
  if (keyPacket.length !== 42) {
    throw new Error('Minisign public key packet must be 42 bytes')
  }
  const algorithm = keyPacket.subarray(0, 2).toString('ascii')
  if (!PUBLIC_KEY_ALGORITHMS.has(algorithm)) {
    throw new Error('updater public key uses an unsupported algorithm')
  }
  return {
    keyId: keyPacket.subarray(2, 10),
    key: createPublicKey({
      key: Buffer.concat([ED25519_SPKI_PREFIX, keyPacket.subarray(10)]),
      format: 'der',
      type: 'spki',
    }),
  }
}

function parseSignature(signatureText) {
  const [encodedEnvelope] = normalizedLines(signatureText, 1, 'encoded updater signature')
  const decodedSignature = decodeBase64(
    encodedEnvelope,
    'encoded updater signature',
  ).toString('utf8')
  const [untrustedComment, encodedSignature, trustedComment, encodedGlobalSignature] =
    normalizedLines(decodedSignature, 4, 'updater signature')
  if (!untrustedComment.startsWith('untrusted comment:')) {
    throw new Error('updater signature has an unexpected untrusted comment')
  }
  if (!trustedComment.startsWith('trusted comment: ')) {
    throw new Error('updater signature has an unexpected trusted comment')
  }

  const signaturePacket = decodeBase64(encodedSignature, 'Minisign signature packet')
  const globalSignature = decodeBase64(encodedGlobalSignature, 'Minisign global signature')
  if (signaturePacket.length !== 74 || globalSignature.length !== 64) {
    throw new Error('updater signature has an unexpected packet length')
  }
  if (!timingSafeEqual(signaturePacket.subarray(0, 2), PREHASHED_ALGORITHM)) {
    throw new Error('updater signature must use the prehashed Minisign algorithm')
  }
  return {
    keyId: signaturePacket.subarray(2, 10),
    signature: signaturePacket.subarray(10),
    trustedComment: trustedComment.slice('trusted comment: '.length),
    globalSignature,
  }
}

export function verifyUpdaterSignature({ encodedPublicKey, content, signatureText }) {
  const publicKey = parseEncodedPublicKey(encodedPublicKey)
  const signature = parseSignature(signatureText)
  if (!timingSafeEqual(publicKey.keyId, signature.keyId)) {
    throw new Error('updater signature was produced by a different private key')
  }

  const digest = createHash('blake2b512').update(content).digest()
  if (!verify(null, digest, publicKey.key, signature.signature)) {
    throw new Error('updater signature does not verify against the committed public key')
  }

  const globalPayload = Buffer.concat([
    signature.signature,
    Buffer.from(signature.trustedComment, 'utf8'),
  ])
  if (!verify(null, globalPayload, publicKey.key, signature.globalSignature)) {
    throw new Error('updater signature trusted comment did not verify')
  }
}

function option(args, name) {
  const index = args.indexOf(name)
  if (index === -1 || index === args.length - 1 || args[index + 1].startsWith('--')) {
    throw new Error(`Missing required option: ${name}`)
  }
  return args[index + 1]
}

export function main(args = process.argv.slice(2)) {
  if (args.length !== 6) {
    throw new Error(
      'Usage: node scripts/verify-updater-signature.mjs --config <tauri.conf.json> --file <probe> --signature <probe.sig>',
    )
  }
  const configPath = option(args, '--config')
  const filePath = option(args, '--file')
  const signaturePath = option(args, '--signature')
  const config = JSON.parse(readFileSync(configPath, 'utf8'))
  const encodedPublicKey = config?.plugins?.updater?.pubkey

  verifyUpdaterSignature({
    encodedPublicKey,
    content: readFileSync(filePath),
    signatureText: readFileSync(signaturePath, 'utf8'),
  })
  console.log('Updater private key matches the committed public key.')
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main()
  } catch (error) {
    console.error(`::error::${error instanceof Error ? error.message : String(error)}`)
    process.exitCode = 1
  }
}
