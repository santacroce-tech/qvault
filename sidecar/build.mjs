#!/usr/bin/env node
// Build the QVAC sidecar into the layout Tauri bundles:
//
//   src-tauri/binaries/qvac-sidecar-<target-triple>[.exe]   <- Bare runtime (bundle.externalBin)
//   src-tauri/resources/qvac-sidecar/                        <- worker.js + pruned node_modules (bundle.resources)
//
// At runtime Rust spawns `<externalBin> <resources>/qvac-sidecar/worker.js
// --cache-dir <app data>/qvac-cache` with piped stdio (see src-tauri/src/qvac/).
//
// This mirrors QVAC's own Electron packaging (runtime binary + real files on
// disk, no archive). Only the build host's platform is supported, because npm
// installs Bare/fabric platform packages for the host only; CI builds each
// platform on its own runner.
//
// Usage: node sidecar/build.mjs            (from anywhere)
//        TAURI_ENV_TARGET_TRIPLE is honoured when Tauri runs this as part of
//        beforeBuildCommand.
import { execFileSync } from 'node:child_process'
import {
  chmodSync, copyFileSync, cpSync, existsSync, mkdirSync, readFileSync,
  rmSync, statSync, writeFileSync
} from 'node:fs'
import { dirname, join, relative, sep, basename } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const repo = join(here, '..')
const tauriDir = join(repo, 'src-tauri')
const binDir = join(tauriDir, 'binaries')
const outDir = join(tauriDir, 'resources', 'qvac-sidecar')

const TRIPLE_TO_HOST = {
  'aarch64-apple-darwin': 'darwin-arm64',
  'x86_64-apple-darwin': 'darwin-x64',
  'x86_64-unknown-linux-gnu': 'linux-x64',
  'aarch64-unknown-linux-gnu': 'linux-arm64',
  'x86_64-pc-windows-msvc': 'win32-x64'
}

function log (msg) { console.log(`[qvac-sidecar] ${msg}`) }

function targetTriple () {
  if (process.env.TAURI_ENV_TARGET_TRIPLE) return process.env.TAURI_ENV_TARGET_TRIPLE
  const out = execFileSync('rustc', ['-vV'], { encoding: 'utf8' })
  const m = out.match(/^host:\s*(\S+)/m)
  if (!m) throw new Error('could not determine target triple from `rustc -vV`')
  return m[1]
}

const triple = targetTriple()
const host = TRIPLE_TO_HOST[triple]
if (!host) throw new Error(`unsupported target triple for the QVAC sidecar: ${triple}`)
const nodeHost = `${process.platform}-${process.arch}`
if (host !== nodeHost) {
  throw new Error(`sidecar must be built on its own platform (target ${host}, build host ${nodeHost}); cross-builds are not supported`)
}

// 1. Dependencies, exactly as locked.
if (!existsSync(join(here, 'node_modules', '.package-lock.json'))) {
  log('installing locked dependencies (npm ci --legacy-peer-deps)')
  const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm'
  execFileSync(npm, ['ci', '--legacy-peer-deps', '--no-audit', '--no-fund'], { cwd: here, stdio: 'inherit', shell: process.platform === 'win32' })
}

const nm = join(here, 'node_modules')
const exe = host.startsWith('win32') ? '.exe' : ''

// 2. externalBin: the Bare runtime.
const bareSrc = join(nm, `bare-runtime-${host}`, 'bin', `bare${exe}`)
if (!existsSync(bareSrc)) throw new Error(`Bare runtime not found at ${bareSrc}`)
mkdirSync(binDir, { recursive: true })
const bareDst = join(binDir, `qvac-sidecar-${triple}${exe}`)
copyFileSync(bareSrc, bareDst)
chmodSync(bareDst, 0o755)
log(`externalBin  ${relative(repo, bareDst)} (${mb(statSync(bareDst).size)})`)

// 3. resources: worker + pruned production node_modules for this host only.
rmSync(outDir, { recursive: true, force: true })
mkdirSync(outDir, { recursive: true })
copyFileSync(join(here, 'worker.js'), join(outDir, 'worker.js'))
const pkg = JSON.parse(readFileSync(join(here, 'package.json'), 'utf8'))
writeFileSync(join(outDir, 'package.json'), JSON.stringify({
  name: pkg.name, version: pkg.version, private: true, type: 'module', main: 'worker.js'
}, null, 2))

const PRUNE_DIRS = new Set(['test', 'tests', '__tests__', 'example', 'examples', 'docs', '.github', 'benchmark', 'benchmarks'])
const PRUNE_FILE_RE = /(\.d\.[cm]?ts|\.map|\.md|\.markdown|\.ts|\.cc|\.cpp|\.c|\.h|\.hpp|\.cmake|CMakeLists\.txt|\.gyp)$/i
const KEEP_FILE_RE = /^(LICEN[CS]E|NOTICE|COPYING)/i

let files = 0
let bytes = 0
function filter (src) {
  const rel = relative(nm, src)
  if (rel === '') return true
  const parts = rel.split(sep)
  const name = basename(src)
  // Drop the Bare platform package (its binary ships as externalBin; a stub
  // pointing at the running executable is written below) and build tooling.
  if (parts[0] === `bare-runtime-${host}`) return false
  if (parts[0].startsWith('bare-runtime-') && parts[0] !== 'bare-runtime') return false
  if (parts[0] === '.bin' || parts[0].startsWith('bare-build')) return false
  // Other platforms' native prebuilds, and headers shipped for source builds.
  const pi = parts.lastIndexOf('prebuilds')
  if (pi >= 0 && pi + 1 < parts.length && parts[pi + 1] !== host) return false
  // Packages for other platforms (e.g. @qvac/fabric-linux-x64), if present.
  const scoped = parts[0].startsWith('@') ? `${parts[0]}/${parts[1] || ''}` : parts[0]
  const m = scoped.match(/-(darwin|linux|win32|android|ios)-(arm64|x64|ia32|arm)(-simulator)?$/)
  if (m && !scoped.endsWith(`-${host}`)) return false
  let st
  try { st = statSync(src) } catch { return false }
  if (st.isDirectory()) return !PRUNE_DIRS.has(name)
  if (KEEP_FILE_RE.test(name)) { files++; bytes += st.size; return true }
  if (PRUNE_FILE_RE.test(name)) return false
  files++
  bytes += st.size
  return true
}
cpSync(nm, join(outDir, 'node_modules'), { recursive: true, filter, dereference: true })

// assessModelFit runs its native fit probe in a child Bare process resolved
// through `bare-runtime-<host>`. Point that at the running executable (our
// externalBin) instead of shipping a second 70 MB copy of the runtime.
const stubDir = join(outDir, 'node_modules', `bare-runtime-${host}`)
mkdirSync(stubDir, { recursive: true })
writeFileSync(join(stubDir, 'package.json'), JSON.stringify({
  name: `bare-runtime-${host}`, version: '0.0.0-qvault-stub', main: 'index.js'
}, null, 2))
writeFileSync(join(stubDir, 'index.js'),
  "// Qvault stub: the Bare runtime ships as the Tauri externalBin.\n" +
  "module.exports = { bare: require('bare-os').execPath() }\n")

// Empty fabric header/share dirs left behind by the filter are harmless; drop them.
for (const d of ['include', 'share']) {
  rmSync(join(outDir, 'node_modules', '@qvac', 'fabric', 'prebuilds', d), { recursive: true, force: true })
}

const info = {
  triple,
  host,
  builtAt: new Date().toISOString(),
  inference: readJson(join(nm, '@qvac', 'inference', 'package.json')).version,
  addons: Object.fromEntries(['ocr-ggml', 'llm-llamacpp', 'embed-llamacpp', 'fabric'].map((a) =>
    [a, readJson(join(nm, '@qvac', a, 'package.json')).version])),
  bare: readJson(join(nm, `bare-runtime-${host}`, 'package.json')).version
}
writeFileSync(join(outDir, 'BUILD-INFO.json'), JSON.stringify(info, null, 2))
log(`resources    ${relative(repo, outDir)} (${files} files, ${mb(bytes)})`)
log(`QVAC ${info.inference}; addons ${JSON.stringify(info.addons)}; bare ${info.bare}`)

function readJson (p) { return JSON.parse(readFileSync(p, 'utf8')) }
function mb (n) { return `${(n / 1024 / 1024).toFixed(1)} MB` }
