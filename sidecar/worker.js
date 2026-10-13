// Qvault QVAC sidecar — Bare + @qvac/inference 0.21.0 (+ @qvac/onnx 0.15.1
// for token classification) behind a JSON-lines protocol on stdin/stdout.
// See docs/qvac-integration.md §4.1 and §15.
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

const SIDECAR_VERSION = '0.2.0'
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

// Second inference engine (docs/qvac-integration.md §15): QVAC's generic
// ONNX Runtime addon, for token-classification models (OpenAI Privacy
// Filter). Loaded defensively: if this platform has no working prebuild the
// worker still starts, and `hello` reports `onnx: false` so the app falls
// back to the LLM / Layers 1–2.
const ONNX_VERSION = '0.15.1'
let onnx = null
let onnxError = null
try {
  const mod = await import('@qvac/onnx')
  onnx = mod.default || mod
  // ORT's own logger writes to stdout; keep it to fatal errors only so it
  // cannot interleave with protocol lines (Rust ignores non-JSON lines).
  onnx.configureEnvironment({ loggingLevel: 'fatal', loggingId: 'qvault' })
} catch (e) {
  onnx = null
  onnxError = (e && e.message) ? String(e.message).slice(0, 200) : 'failed to load'
  process.stderr.write(`[sidecar] ONNX addon unavailable: ${onnxError}\n`)
}

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
  // Keys of @qvac/inference 0.21.0's embedConfigBaseSchema (camelCase; the
  // snake_case keys listed here before Phase 4 were silently ignored).
  embeddings: {
    device: 'any', gpuLayers: 'any', batchSize: 'any', pooling: 'any',
    attention: 'any', embdNormalize: 'any', flashAttention: 'any'
  }
}

// Bounds for one `embed` request (entity names are short; this keeps a
// misbehaving caller from queueing unbounded work in the native addon).
const EMBED_MAX_ITEMS = 256
const EMBED_MAX_CHARS = 2000

// Token classification (ONNX). One request is one window of token ids, as
// produced by the app's tokenizer; the bound matches the model's 4,096-token
// window (opf's CPU default), and ids must fit the model's vocabulary.
const CLASSIFY_MAX_TOKENS = 4096
const CLASSIFY_MAX_ID = 262143
// Config keys for token-classifier sessions. The execution provider is not
// configurable: always CPU (measured: CoreML is slower for this model).
const CLASSIFIER_CONFIG_KEYS = { threads: 'int' }
// modelId -> { handle, labels }
const onnxSessions = new Map()
let nextOnnxId = 1

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

// ── token classifier (ONNX) ───────────────────────────────────────────────
// @qvac/onnx's C++ logger prints a few INFO/DEBUG lines to stdout (not
// configurable at run time) while a session is created. Rust skips non-JSON
// lines, but a native print must never land inside a protocol line that is
// still being written, so pending stdout writes are drained first; the native
// calls are synchronous, so no protocol write can start during them.
function drainStdout () {
  return new Promise((resolve) => process.stdout.write('', resolve))
}

async function loadClassifier (params) {
  if (!onnx) throw new ProtocolError('UNAVAILABLE', 'the ONNX engine is not available on this platform')
  const modelPath = requireLocalPath(params.path, 'path')
  if (!modelPath.toLowerCase().endsWith('.onnx')) {
    throw new ProtocolError('BAD_PARAMS', 'path must name an .onnx file')
  }
  const config = params.config == null ? {} : params.config
  if (typeof config !== 'object' || Array.isArray(config)) throw new ProtocolError('BAD_PARAMS', 'config must be an object')
  for (const [k, v] of Object.entries(config)) {
    if (!CLASSIFIER_CONFIG_KEYS[k]) throw new ProtocolError('BAD_PARAMS', `config key not allowed: ${k}`)
    if (!Number.isInteger(v) || v < 0 || v > 256) throw new ProtocolError('BAD_PARAMS', `config.${k} must be a small integer`)
  }
  await drainStdout()
  const started = Date.now()
  // ONNX Runtime reads the external weights file (`<name>.onnx_data`) from
  // the model's own directory; nothing else is opened.
  const handle = onnx.createSession(modelPath, {
    provider: 'cpu',
    optimization: 'all',
    intraOpThreads: config.threads || 0
  })
  const inputs = onnx.getInputInfo(handle).map((i) => i.name)
  const outputs = onnx.getOutputInfo(handle)
  if (!inputs.includes('input_ids') || !inputs.includes('attention_mask') || outputs.length === 0) {
    onnx.destroySession(handle)
    throw new ProtocolError('BAD_PARAMS', 'not a token-classification model (expects input_ids, attention_mask -> logits)')
  }
  const modelId = `onnx-${nextOnnxId++}`
  onnxSessions.set(modelId, { handle })
  logMeta(`loaded token-classifier model in ${Date.now() - started} ms`)
  return { modelId }
}

function destroyOnnxSessions () {
  for (const [, s] of onnxSessions) {
    try { onnx.destroySession(s.handle) } catch {}
  }
  onnxSessions.clear()
}

// ── methods ──────────────────────────────────────────────────────────────
const methods = {
  async hello () {
    return {
      protocol: PROTOCOL,
      sidecar: SIDECAR_VERSION,
      inference: INFERENCE_VERSION,
      platform: `${process.platform}-${process.arch}`,
      plugins: onnx ? ['ocr', 'llm', 'embeddings', 'token-classifier'] : ['ocr', 'llm', 'embeddings'],
      onnx: onnx ? ONNX_VERSION : null,
      onnxError: onnx ? null : onnxError
    }
  },

  async loadModel (params, ctx) {
    const type = params.type
    if (type === 'token-classifier') return loadClassifier(params)
    if (!MODEL_TYPES[type]) throw new ProtocolError('BAD_PARAMS', 'type must be one of ocr|llm|embeddings|token-classifier')
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
    if (onnxSessions.has(modelId)) {
      const sess = onnxSessions.get(modelId)
      onnxSessions.delete(modelId)
      onnx.destroySession(sess.handle)
      return { ok: true }
    }
    // clearStorage:false — never let QVAC delete files it did not download.
    await sdk.unloadModel({ modelId, clearStorage: false })
    return { ok: true }
  },

  // Run a token classifier on one window of token ids. Returns the raw
  // logits, row-major [tokens, labels], as base64 little-endian float32 (exact
  // and ~4x smaller than JSON numbers). Decoding happens in the app.
  async classify (params, ctx) {
    if (!onnx) throw new ProtocolError('UNAVAILABLE', 'the ONNX engine is not available on this platform')
    const modelId = requireString(params.modelId, 'modelId')
    const sess = onnxSessions.get(modelId)
    if (!sess) throw new ProtocolError('MODEL_NOT_LOADED', 'model not loaded')
    const ids = params.inputIds
    if (!Array.isArray(ids) || ids.length === 0 || ids.length > CLASSIFY_MAX_TOKENS) {
      throw new ProtocolError('BAD_PARAMS', `inputIds must hold 1..${CLASSIFY_MAX_TOKENS} token ids`)
    }
    const n = ids.length
    const x = new BigInt64Array(n)
    for (let i = 0; i < n; i++) {
      const v = ids[i]
      if (!Number.isInteger(v) || v < 0 || v > CLASSIFY_MAX_ID) {
        throw new ProtocolError('BAD_PARAMS', 'inputIds must be non-negative integers within the vocabulary')
      }
      x[i] = BigInt(v)
    }
    ctx.entry.kind = 'classify'
    await drainStdout()
    const mask = new BigInt64Array(n).fill(1n)
    // Synchronous native call (~0.2 s per page); a cancel arriving meanwhile
    // is handled after it returns and drops the result.
    const out = onnx.run(sess.handle, [
      { name: 'input_ids', shape: [1, n], type: 'int64', data: x },
      { name: 'attention_mask', shape: [1, n], type: 'int64', data: mask }
    ])
    const logits = out.find((o) => o.name === 'logits') || out[0]
    if (!logits || logits.type !== 'float32' || !logits.data || logits.data.length % n !== 0) {
      throw new ProtocolError('INTERNAL', 'unexpected classifier output')
    }
    const labels = logits.data.length / n
    const data = logits.data
    const b64 = Buffer.from(data.buffer, data.byteOffset, data.byteLength).toString('base64')
    return { shape: [n, labels], logits: b64 }
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
      const allowed = ['temp', 'top_p', 'top_k', 'predict', 'seed', 'repeat_penalty', 'reasoning_budget']
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
    const items = Array.isArray(text) ? text : [text]
    if (items.length === 0 || items.length > EMBED_MAX_ITEMS) {
      throw new ProtocolError('BAD_PARAMS', `text must hold 1..${EMBED_MAX_ITEMS} strings`)
    }
    if (items.some((t) => t.length === 0 || t.length > EMBED_MAX_CHARS)) {
      throw new ProtocolError('BAD_PARAMS', `each text must be 1..${EMBED_MAX_CHARS} characters`)
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
    destroyOnnxSessions()
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
  destroyOnnxSessions()
  try { await qvac.close() } catch {}
  wipeTmp()
  process.exit(0)
})

wipeTmp()
send({ ready: true, version: `qvac-sidecar ${SIDECAR_VERSION} / @qvac/inference ${INFERENCE_VERSION}` + (onnx ? ` / @qvac/onnx ${ONNX_VERSION}` : ''), protocol: PROTOCOL })
