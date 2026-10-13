// Protocol smoke test for worker.js. Runs on Node, spawns the sidecar the
// same way Rust does (piped stdio) and exercises the allow-list.
//
//   node test/protocol-test.mjs
//
// Optional env (model-dependent checks are skipped when unset):
//   QVAC_OCR_DETECTOR, QVAC_OCR_RECOGNIZER  absolute .gguf paths
//   QVAC_LLM                                absolute .gguf path
//   QVAC_SIDECAR_BIN                        bare binary (default: node_modules)
//   QVAC_SIDECAR_WORKER                     worker.js (default: ../worker.js)
import { spawn, execFileSync } from 'node:child_process'
import { mkdtempSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import assert from 'node:assert/strict'
import { createInterface } from 'node:readline'

const here = dirname(fileURLToPath(import.meta.url))
const root = join(here, '..')
const bare = process.env.QVAC_SIDECAR_BIN ||
  join(root, 'node_modules', `bare-runtime-${process.platform}-${process.arch}`, 'bin', process.platform === 'win32' ? 'bare.exe' : 'bare')
const worker = process.env.QVAC_SIDECAR_WORKER || join(root, 'worker.js')
const cacheDir = mkdtempSync(join(tmpdir(), 'qvac-sidecar-test-'))

const child = spawn(bare, [worker, '--cache-dir', cacheDir], { stdio: ['pipe', 'pipe', 'pipe'] })
child.stderr.on('data', (d) => process.stderr.write(`  [stderr] ${d}`))

const pending = new Map()
const events = []
let readyResolve
const ready = new Promise((resolve) => { readyResolve = resolve })
let nonJson = 0
createInterface({ input: child.stdout }).on('line', (line) => {
  let msg
  try { msg = JSON.parse(line) } catch { nonJson++; return }
  if (msg.ready) return readyResolve(msg)
  if (msg.event) return events.push(msg)
  const p = pending.get(msg.id)
  if (p) { pending.delete(msg.id); p(msg) }
})

let nextId = 1
function call (method, params = {}) {
  const id = nextId++
  child.stdin.write(JSON.stringify({ id, method, params }) + '\n')
  return { id, reply: new Promise((resolve) => pending.set(id, resolve)) }
}
const rpc = (m, p) => call(m, p).reply

function inetSockets () {
  try {
    return execFileSync('lsof', ['-nP', '-a', '-p', String(child.pid), '-i'], { encoding: 'utf8' }).trim()
  } catch { return '' } // lsof exits 1 when nothing matches
}

const t0 = Date.now()
const readyMsg = await ready
console.log(`ready in ${Date.now() - t0} ms:`, readyMsg.version)
assert.equal(readyMsg.protocol, 1)

const hello = await rpc('hello')
assert.equal(hello.result.inference, '0.21.0')
console.log('hello ok', hello.result.platform)

// Allow-list and path validation.
assert.equal((await rpc('downloadAsset', { assetSrc: 'registry://x' })).error.code, 'METHOD_NOT_ALLOWED')
assert.equal((await rpc('modelRegistryList')).error.code, 'METHOD_NOT_ALLOWED')
assert.equal((await rpc('loadModel', { path: 'registry://s3/x.gguf', type: 'llm' })).error.code, 'BAD_PARAMS')
assert.equal((await rpc('loadModel', { path: 'https://huggingface.co/x.gguf', type: 'llm' })).error.code, 'BAD_PARAMS')
assert.equal((await rpc('loadModel', { path: 'relative/model.gguf', type: 'llm' })).error.code, 'BAD_PARAMS')
assert.equal((await rpc('loadModel', { path: '/nope.gguf', type: 'llm', config: { 'rpc-servers': '1.2.3.4:50052' } })).error.code, 'BAD_PARAMS')
assert.equal((await rpc('assessFit', { path: 'pear://abc/model.gguf', type: 'llm' })).error.code, 'BAD_PARAMS')
console.log('allow-list / path validation ok')

const det = process.env.QVAC_OCR_DETECTOR
const rec = process.env.QVAC_OCR_RECOGNIZER
if (det && rec && existsSync(det) && existsSync(rec)) {
  const img = process.env.QVAC_OCR_IMAGE || join(here, 'fixtures', 'ocr-sample.png')
  let t = Date.now()
  const load = await rpc('loadModel', { path: rec, type: 'ocr', config: { detectorModelSrc: det, langList: ['en'] } })
  assert.ok(load.result, JSON.stringify(load.error))
  console.log(`ocr load ${Date.now() - t} ms`)
  t = Date.now()
  const out = await rpc('ocr', { modelId: load.result.modelId, image: img })
  assert.ok(out.result, JSON.stringify(out.error))
  const text = out.result.blocks.map((b) => b.text).join(' ')
  console.log(`ocr ${Date.now() - t} ms, ${out.result.blocks.length} blocks`)
  assert.match(text, /Jane/i)
  assert.ok(!existsSync(join(cacheDir, 'tmp')), 'tmp wiped after ocr')
  await rpc('unloadModel', { modelId: load.result.modelId })
} else {
  console.log('skip OCR (set QVAC_OCR_DETECTOR / QVAC_OCR_RECOGNIZER)')
}

const llm = process.env.QVAC_LLM
if (llm && existsSync(llm)) {
  const fit = await rpc('assessFit', { path: llm, type: 'llm' })
  assert.ok(fit.result, JSON.stringify(fit.error))
  console.log('fit:', fit.result.verdict, fit.result.evidence)
  let t = Date.now()
  const load = await rpc('loadModel', { path: llm, type: 'llm', config: { ctx_size: 2048, temp: 0, predict: 256 } })
  assert.ok(load.result, JSON.stringify(load.error))
  console.log(`llm load ${Date.now() - t} ms`)
  const schema = {
    type: 'object',
    properties: {
      entities: {
        type: 'array',
        items: {
          type: 'object',
          properties: { text: { type: 'string' }, category: { type: 'string', enum: ['PersonName', 'Email', 'Address'] } },
          required: ['text', 'category'],
          additionalProperties: false
        }
      }
    },
    required: ['entities'],
    additionalProperties: false
  }
  t = Date.now()
  const out = await rpc('complete', {
    modelId: load.result.modelId,
    messages: [
      { role: 'system', content: 'Extract personal data. Reply with JSON only.' },
      { role: 'user', content: 'Text: Please contact Maria Gonzalez at maria.g@example.org.' }
    ],
    responseFormat: { type: 'json_schema', json_schema: { name: 'pii', schema, strict: true } },
    generationParams: { temp: 0, predict: 200 }
  })
  assert.ok(out.result, JSON.stringify(out.error))
  const parsed = JSON.parse(out.result.text)
  console.log(`complete ${Date.now() - t} ms ->`, JSON.stringify(parsed))
  assert.ok(Array.isArray(parsed.entities))

  // Cancel a long generation.
  const long = call('complete', {
    modelId: load.result.modelId,
    messages: [{ role: 'user', content: 'Count from 1 to 500, one number per line.' }],
    generationParams: { predict: 2000 }
  })
  await new Promise((resolve) => setTimeout(resolve, 300))
  const c = await rpc('cancel', { target: long.id })
  const lr = await long.reply
  assert.equal(lr.error && lr.error.code, 'CANCELLED')
  console.log('cancel ok', c.result)
  await rpc('unloadModel', { modelId: load.result.modelId })
} else {
  console.log('skip LLM (set QVAC_LLM)')
}

const embedModel = process.env.QVAC_EMBED
if (embedModel && existsSync(embedModel)) {
  let t = Date.now()
  const load = await rpc('loadModel', { path: embedModel, type: 'embeddings', config: { gpuLayers: 99 } })
  assert.ok(load.result, JSON.stringify(load.error))
  console.log(`embed load ${Date.now() - t} ms`)
  assert.equal((await rpc('embed', { modelId: load.result.modelId, text: [] })).error.code, 'BAD_PARAMS')
  assert.equal((await rpc('embed', { modelId: load.result.modelId, text: ['x'.repeat(2001)] })).error.code, 'BAD_PARAMS')
  t = Date.now()
  const out = await rpc('embed', { modelId: load.result.modelId, text: ['Maria Santos', 'Santos, Maria', 'Acme Holdings LLC'] })
  assert.ok(out.result, JSON.stringify(out.error))
  const [a, b, c] = out.result.embedding
  const dot = (x, y) => x.reduce((s, v, i) => s + v * y[i], 0)
  console.log(`embed ${Date.now() - t} ms, dim ${a.length}; cos(name, reordered) ${dot(a, b).toFixed(3)} cos(name, company) ${dot(a, c).toFixed(3)}`)
  assert.ok(Math.abs(dot(a, a) - 1) < 1e-3, 'vectors are L2-normalised')
  assert.ok(dot(a, b) > dot(a, c))
  await rpc('unloadModel', { modelId: load.result.modelId })
} else {
  console.log('skip embeddings (set QVAC_EMBED)')
}

const sockets = inetSockets()
assert.equal(sockets, '', `sidecar holds inet sockets:\n${sockets}`)
console.log('no inet sockets')

const bye = await rpc('shutdown')
assert.ok(bye.result && bye.result.bye)
const code = await new Promise((resolve) => child.on('exit', resolve))
assert.equal(code, 0)
console.log(`shutdown ok; non-JSON stdout lines seen: ${nonJson}`)
console.log('PASS')
