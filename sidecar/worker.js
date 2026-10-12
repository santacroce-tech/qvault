// Qvault QVAC sidecar — Bare + @qvac/inference 0.21.0 behind a JSON-lines
// protocol on stdin/stdout. See docs/qvac-integration.md §4.1.
//
// Invariants (do not relax without updating the design doc):
//   * stdio only: this process never opens a listening socket and never
//     exposes registry / discovery / download calls.
//   * Models load only from absolute local paths; any URL scheme
//     (registry://, pear://, http(s)://, …) is rejected.
//   * stdout carries protocol lines only. Everything else goes to stderr,
//     and nothing written to stderr contains document content.
//
// Usage: bare worker.js --cache-dir <abs app-private dir>

import process from 'bare-process'
globalThis.process = process

import fs from 'bare-fs'
import path from 'bare-path'

const SIDECAR_VERSION = '0.1.0'
const INFERENCE_VERSION = '0.21.0'
const PROTOCOL = 1

// ── stdout discipline ────────────────────────────────────────────────────
// Route the stdout-bound console methods to stderr so a stray log line from a
// dependency can never corrupt the protocol stream (console.warn/error already
// write to stderr in Bare and are left alone so uncaught errors still print).
// QVAC's own logger is additionally silenced via `loggerConsoleOutput: false`.
const toStderr = (level) => (...args) => {
  try {
    const msg = args.map((a) => (typeof a === 'string' ? a : safeStringify(a))).join(' ')
    process.stderr.write(`[${level}] ${msg.slice(0, 500)}\n`)
  } catch {}
}
function safeStringify (v) {
  try { return JSON.stringify(v) } catch { return String(v) }
}
console.log = toStderr('log')
console.info = toStderr('info')
console.debug = () => {}

function send (obj) {
  process.stdout.write(JSON.stringify(obj) + '\n')
}

function logMeta (msg) {
  // Metadata only — never pass document text, OCR output or prompts here.
  process.stderr.write(`[sidecar] ${msg}\n`)
}

// ── arguments / config ───────────────────────────────────────────────────
function argValue (name) {
  const i = process.argv.indexOf(name)
  return i >= 0 && i + 1 < process.argv.length ? process.argv[i + 1] : null
}

const cacheDir = argValue('--cache-dir')
if (!cacheDir || !path.isAbsolute(cacheDir)) {
  process.stderr.write('[sidecar] fatal: --cache-dir <absolute path> is required\n')
  process.exit(2)
}
fs.mkdirSync(cacheDir, { recursive: true })
const tmpDir = path.join(cacheDir, 'tmp')

const configPath = path.join(cacheDir, 'qvac.config.json')
fs.writeFileSync(configPath, JSON.stringify({
  cacheDirectory: cacheDir,
  loggerConsoleOutput: false,
  loggerLevel: 'error',
  // Belt and braces: even though we never pass remote sources, insist on
  // checksums and TLS if anything ever tried.
  requireHttpChecksum: true,
  requireSecureTransport: true
}))
process.env.QVAC_CONFIG_PATH = configPath

// Import QVAC only after the config path is set.
const qvac = await import('@qvac/inference')
const { ocrPlugin } = await import('@qvac/inference/ggml-ocr/plugin')
const { llmPlugin } = await import('@qvac/inference/llamacpp-completion/plugin')
const { embeddingsPlugin } = await import('@qvac/inference/llamacpp-embedding/plugin')
const sdk = qvac.plugins([ocrPlugin, llmPlugin, embeddingsPlugin])

// ── validation helpers ───────────────────────────────────────────────────
class ProtocolError extends Error {
  constructor (code, message) { super(message); this.code = code }
}

const SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//

function requireLocalPath (p, what) {
  if (typeof p !== 'string' || p.length === 0) {
    throw new ProtocolError('BAD_PARAMS', `${what} must be a non-empty string`)
  }
  if (SCHEME_RE.test(p) || p.startsWith('registry:') || p.startsWith('pear:')) {
    throw new ProtocolError('BAD_PARAMS', `${what}: remote/registry sources are not allowed`)
  }
  if (!path.isAbsolute(p)) {
    throw new ProtocolError('BAD_PARAMS', `${what} must be an absolute path`)
  }
  if (!fs.existsSync(p)) {
    throw new ProtocolError('BAD_PARAMS', `${what} does not exist`)
  }
  return p
}

const MODEL_TYPES = {
  ocr: 'ggml-ocr',
  llm: 'llamacpp-completion',
  embeddings: 'llamacpp-embedding'
}

// Per-type allow-list of config keys. Anything that could reach the network
// (e.g. llama.cpp 'rpc-servers') is deliberately absent.
const CONFIG_KEYS = {
  ocr: {
    langList: 'any', detectorModelSrc: 'path', pipelineType: 'any', magRatio: 'any',
    canvasSize: 'any', contrastRetry: 'any', lowConfidenceThreshold: 'any',
    recognizerBatchSize: 'any', nThreads: 'any', backendDevice: 'any',
    defaultRotationAngles: 'any'
  },
  llm: {
    ctx_size: 'any', temp: 'any', top_p: 'any', top_k: 'any', seed: 'any',
    gpu_layers: 'any', lora: 'path', device: 'any', predict: 'any',
    repeat_penalty: 'any', threads: 'any', 'batch-size': 'any', 'flash-attn': 'any'
  },
  embeddings: { device: 'any', gpu_layers: 'any', ctx_size: 'any', 'batch-size': 'any', threads: 'any' }
}

function sanitizeConfig (type, config) {
  if (config == null) return {}
  if (typeof config !== 'object' || Array.isArray(config)) {
    throw new ProtocolError('BAD_PARAMS', 'config must be an object')
  }
  const allowed = CONFIG_KEYS[type]
  const out = {}
  for (const [k, v] of Object.entries(config)) {
    const kind = allowed[k]
    if (!kind) throw new ProtocolError('BAD_PARAMS', `config key not allowed: ${k}`)
    if (kind === 'path') out[k] = requireLocalPath(v, `config.${k}`)
    else {
      if (typeof v === 'string' && SCHEME_RE.test(v)) {
        throw new ProtocolError('BAD_PARAMS', `config.${k}: URLs are not allowed`)
      }
      out[k] = v
    }
  }
  return out
}

function requireString (v, what) {
  if (typeof v !== 'string' || v.length === 0) throw new ProtocolError('BAD_PARAMS', `${what} must be a non-empty string`)
  return v
}

function wipeTmp () {
  try { fs.rmSync(tmpDir, { recursive: true, force: true }) } catch {}
}

// ── request bookkeeping ──────────────────────────────────────────────────
// id -> { cancelled, qvacRequestId?, modelId?, kind }
const inflight = new Map()

function finish (id, payload) {
  const entry = inflight.get(id)
  if (!entry) return // already answered (e.g. cancelled)
  inflight.delete(id)
  send({ id, ...payload })
}

function errorPayload (e) {
  if (e instanceof ProtocolError) return { error: { code: e.code, message: e.message } }
  const name = e && e.constructor && e.constructor.name
  if (name === 'InferenceCancelledError') return { error: { code: 'CANCELLED', message: 'cancelled' } }
  if (name === 'ModelNotLoadedError') return { error: { code: 'MODEL_NOT_LOADED', message: 'model not loaded' } }
  const msg = (e && e.message) ? String(e.message).slice(0, 300) : 'internal error'
  return { error: { code: 'INTERNAL', message: msg } }
}

// ── methods ──────────────────────────────────────────────────────────────
const methods = {
  async hello () {
    return {
      protocol: PROTOCOL,
      sidecar: SIDECAR_VERSION,
      inference: INFERENCE_VERSION,
      platform: `${process.platform}-${process.arch}`,
      plugins: ['ocr', 'llm', 'embeddings']
    }
  },

  async loadModel (params, ctx) {
    const type = params.type
    if (!MODEL_TYPES[type]) throw new ProtocolError('BAD_PARAMS', 'type must be one of ocr|llm|embeddings')
    const modelSrc = requireLocalPath(params.path, 'path')
    const modelConfig = sanitizeConfig(type, params.config)
    if (type === 'ocr' && !modelConfig.detectorModelSrc) {
      throw new ProtocolError('BAD_PARAMS', 'ocr requires config.detectorModelSrc')
    }
    if (type === 'ocr' && !modelConfig.langList) modelConfig.langList = ['en']
    const started = Date.now()
    const p = sdk.loadModel({ modelSrc, modelType: MODEL_TYPES[type], modelConfig })
    ctx.entry.qvacRequestId = p.requestId
    const modelId = await p
    logMeta(`loaded ${type} model in ${Date.now() - started} ms`)
    return { modelId }
  },

  async unloadModel (params) {
    const modelId = requireString(params.modelId, 'modelId')
    // clearStorage:false — never let QVAC delete files it did not download.
    await sdk.unloadModel({ modelId, clearStorage: false })
    return { ok: true }
  },

  async ocr (params, ctx) {
    const modelId = requireString(params.modelId, 'modelId')
    const image = requireLocalPath(params.image, 'image')
    ctx.entry.modelId = modelId
    ctx.entry.kind = 'ocr'
    try {
      const run = sdk.ocr({ modelId, image, options: { paragraph: !!(params.options && params.options.paragraph) } })
      const blocks = await run.blocks
      const stats = await run.stats
      return { blocks, stats: stats || null }
    } finally {
      wipeTmp()
    }
  },

  async complete (params, ctx) {
    const modelId = requireString(params.modelId, 'modelId')
    const messages = params.messages
    if (!Array.isArray(messages) || messages.length === 0) {
      throw new ProtocolError('BAD_PARAMS', 'messages must be a non-empty array')
    }
    const history = messages.map((m) => {
      if (!m || !['system', 'user', 'assistant'].includes(m.role) || typeof m.content !== 'string') {
        throw new ProtocolError('BAD_PARAMS', 'each message needs role (system|user|assistant) and string content')
      }
      return { role: m.role, content: m.content }
    })
    const req = { modelId, history, stream: true, requestId: `qvault-${ctx.id}` }
    if (params.responseFormat != null) {
      const rf = params.responseFormat
      if (!rf || !['text', 'json_object', 'json_schema'].includes(rf.type)) {
        throw new ProtocolError('BAD_PARAMS', 'responseFormat.type must be text|json_object|json_schema')
      }
      req.responseFormat = rf
    }
    if (params.generationParams != null) {
      const gp = params.generationParams
      const allowed = ['temp', 'top_p', 'top_k', 'predict', 'seed', 'repeat_penalty']
      const clean = {}
      for (const [k, v] of Object.entries(gp)) {
        if (!allowed.includes(k)) throw new ProtocolError('BAD_PARAMS', `generationParams key not allowed: ${k}`)
        clean[k] = v
      }
      req.generationParams = clean
    }
    ctx.entry.modelId = modelId
    ctx.entry.kind = 'completion'
    ctx.entry.qvacRequestId = req.requestId
    const run = sdk.completion(req)
    const wantText = !!params.stream
    let tokens = 0
    let text = ''
    for await (const ev of run.events) {
      if (ctx.entry.cancelled) break
      if (ev && ev.type === 'contentDelta') {
        tokens += 1
        text += ev.text
        // Progress carries a counter; text only when the caller asked to stream.
        send({ event: 'progress', id: ctx.id, data: wantText ? { tokens, text: ev.text } : { tokens } })
      }
    }
    const final = await run.final
    return {
      text: (final && typeof final.contentText === 'string') ? final.contentText : text,
      stopReason: final ? final.stopReason || null : null,
      stats: final ? final.stats || null : null
    }
  },

  async embed (params, ctx) {
    const modelId = requireString(params.modelId, 'modelId')
    const text = params.text
    if (!(typeof text === 'string' || (Array.isArray(text) && text.every((t) => typeof t === 'string')))) {
      throw new ProtocolError('BAD_PARAMS', 'text must be a string or string[]')
    }
    ctx.entry.modelId = modelId
    ctx.entry.kind = 'embeddings'
    const p = sdk.embed({ modelId, text })
    ctx.entry.qvacRequestId = p.requestId
    const out = await p
    return { embedding: out.embedding }
  },

  async assessFit (params) {
    const type = params.type
    if (!MODEL_TYPES[type]) throw new ProtocolError('BAD_PARAMS', 'type must be one of ocr|llm|embeddings')
    // Local files only: assessModelFit fetches a registry description for
    // sources that are not on disk, so requireLocalPath (exists check) is
    // what keeps this call offline.
    const modelSrc = requireLocalPath(params.path, 'path')
    const modelConfig = sanitizeConfig(type, params.config)
    const res = await sdk.assessModelFit({
      models: [{ modelSrc, modelType: MODEL_TYPES[type], modelConfig }],
      execution: 'sequential'
    })
    return {
      verdict: res.verdict,
      evidence: res.evidence || null,
      reasons: Array.isArray(res.reasons) ? res.reasons.slice(0, 5) : []
    }
  },

  async cancel (params) {
    const target = params.target
    const entry = inflight.get(target)
    if (!entry) return { cancelled: false }
    entry.cancelled = true
    try {
      if (entry.qvacRequestId) await sdk.cancel({ requestId: entry.qvacRequestId })
      else if (entry.modelId && entry.kind) await sdk.cancel({ modelId: entry.modelId, kind: entry.kind })
    } catch (e) {
      logMeta(`cancel of ${target} raised ${e && e.constructor && e.constructor.name}`)
    }
    // Answer the cancelled request now; a late native result is dropped.
    finish(target, { error: { code: 'CANCELLED', message: 'cancelled' } })
    if (entry.kind === 'ocr') wipeTmp()
    return { cancelled: true }
  },

  async shutdown () {
    for (const [id, entry] of inflight) {
      if (entry.method === 'shutdown') continue
      entry.cancelled = true
      finish(id, { error: { code: 'CANCELLED', message: 'shutting down' } })
    }
    try { await qvac.close() } catch {}
    wipeTmp()
    return { bye: true }
  }
}

const ALLOWED = new Set(Object.keys(methods))

async function handle (req) {
  const id = req.id
  if (typeof id !== 'number' || !Number.isSafeInteger(id)) {
    logMeta('dropping request without numeric id')
    return
  }
  if (typeof req.method !== 'string' || !ALLOWED.has(req.method)) {
    send({ id, error: { code: 'METHOD_NOT_ALLOWED', message: 'method not allowed' } })
    return
  }
  const params = (req.params && typeof req.params === 'object') ? req.params : {}
  const entry = { cancelled: false, method: req.method }
  inflight.set(id, entry)
  try {
    const result = await methods[req.method](params, { id, entry })
    if (entry.cancelled) return finish(id, { error: { code: 'CANCELLED', message: 'cancelled' } })
    finish(id, { result })
  } catch (e) {
    if (entry.cancelled) return finish(id, { error: { code: 'CANCELLED', message: 'cancelled' } })
    finish(id, errorPayload(e))
  }
  if (req.method === 'shutdown') {
    process.stdout.write('', () => process.exit(0))
  }
}

// ── stdin reader ─────────────────────────────────────────────────────────
let buf = ''
process.stdin.on('data', (chunk) => {
  buf += chunk.toString()
  let nl
  while ((nl = buf.indexOf('\n')) >= 0) {
    const line = buf.slice(0, nl).trim()
    buf = buf.slice(nl + 1)
    if (!line) continue
    let req
    try { req = JSON.parse(line) } catch {
      logMeta('ignoring non-JSON input line')
      continue
    }
    handle(req)
  }
})
process.stdin.on('end', async () => {
  // Parent went away: release native resources and exit.
  try { await qvac.close() } catch {}
  wipeTmp()
  process.exit(0)
})

wipeTmp()
send({ ready: true, version: `qvac-sidecar ${SIDECAR_VERSION} / @qvac/inference ${INFERENCE_VERSION}`, protocol: PROTOCOL })
