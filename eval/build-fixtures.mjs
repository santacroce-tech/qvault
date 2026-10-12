#!/usr/bin/env node
// Build eval/fixtures/<id>.json from the annotated sources in eval/fixtures/src/*.txt.
//
// Source format (see eval/README.md):
//   id: en_demo_pdf
//   lang: en
//   kind: contract
//   description: free text
//   ---
//   page text with [[Category|labelled text]] and [[?Category|optional text]]
//   === page ===
//   next page ...
//
// Output: {id, lang, kind, description, source, pages:[{text, spans:[{start,end,category,text,optional?}]}]}
// Offsets are UTF-8 byte offsets into `text` (what the Rust pipeline reports).
//
//   node eval/build-fixtures.mjs           # (re)build every fixture
//   node eval/build-fixtures.mjs --check   # fail if a JSON file is stale or invalid

import { readFileSync, writeFileSync, readdirSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const srcDir = join(here, 'fixtures', 'src')
const outDir = join(here, 'fixtures')

export const CATEGORIES = [
  'PersonName', 'Email', 'PhoneNumber', 'Address', 'DateOfBirth', 'Date', 'MonetaryAmount',
  'SocialSecurityNumber', 'NationalId', 'PassportNumber', 'DriversLicense', 'BankAccount',
  'CreditCard', 'IBAN', 'TaxId', 'CaseNumber', 'CourtFileNumber', 'ClientId', 'IpAddress',
  'Url', 'LicensePlate', 'MedicalRecordNumber', 'CompanyName',
]

const enc = new TextEncoder()
const blen = (s) => enc.encode(s).length

function parsePage (raw, where) {
  let text = ''
  const spans = []
  let i = 0
  while (i < raw.length) {
    const open = raw.indexOf('[[', i)
    if (open < 0) { text += raw.slice(i); break }
    text += raw.slice(i, open)
    const close = raw.indexOf(']]', open)
    if (close < 0) throw new Error(`${where}: unclosed [[`)
    const inner = raw.slice(open + 2, close)
    const bar = inner.indexOf('|')
    if (bar < 0) throw new Error(`${where}: missing | in [[${inner}]]`)
    let cat = inner.slice(0, bar)
    const value = inner.slice(bar + 1)
    const optional = cat.startsWith('?')
    if (optional) cat = cat.slice(1)
    if (!CATEGORIES.includes(cat)) throw new Error(`${where}: unknown category ${cat}`)
    if (!value.trim() || value !== value.trim()) throw new Error(`${where}: empty or padded value in [[${inner}]]`)
    if (value.includes('[[')) throw new Error(`${where}: nested markup in [[${inner}]]`)
    const start = blen(text)
    text += value
    const span = { start, end: blen(text), category: cat, text: value }
    if (optional) span.optional = true
    spans.push(span)
    i = close + 2
  }
  return { text, spans }
}

function parseSource (file) {
  const raw = readFileSync(join(srcDir, file), 'utf8')
  const sep = raw.indexOf('\n---\n')
  if (sep < 0) throw new Error(`${file}: missing --- header separator`)
  const meta = {}
  for (const line of raw.slice(0, sep).split('\n')) {
    const m = line.match(/^(\w+):\s*(.*)$/)
    if (m) meta[m[1]] = m[2].trim()
  }
  for (const k of ['id', 'lang', 'kind', 'description']) {
    if (!meta[k]) throw new Error(`${file}: header needs ${k}`)
  }
  if (!['en', 'es', 'pt'].includes(meta.lang)) throw new Error(`${file}: lang must be en|es|pt`)
  const body = raw.slice(sep + 5)
  const pages = body.split(/\n=== page ===\n/).map((p, n) => parsePage(p, `${file} page ${n + 1}`))
  return { id: meta.id, lang: meta.lang, kind: meta.kind, description: meta.description, source: `src/${file}`, pages }
}

// Same checks as the Rust validator (src-tauri/tests/eval_harness.rs).
export function validate (fx) {
  const errs = []
  fx.pages.forEach((p, n) => {
    const bytes = enc.encode(p.text)
    let prevEnd = -1
    for (const s of p.spans) {
      const slice = new TextDecoder().decode(bytes.slice(s.start, s.end))
      if (slice !== s.text) errs.push(`${fx.id} p${n + 1}: span ${s.start}-${s.end} is "${slice}", label says "${s.text}"`)
      if (!CATEGORIES.includes(s.category)) errs.push(`${fx.id} p${n + 1}: unknown category ${s.category}`)
      if (s.start < prevEnd) errs.push(`${fx.id} p${n + 1}: spans overlap or are unsorted at ${s.start}`)
      prevEnd = s.end
    }
  })
  return errs
}

const check = process.argv.includes('--check')
let failed = false
const files = readdirSync(srcDir).filter((f) => f.endsWith('.txt')).sort()
for (const f of files) {
  const fx = parseSource(f)
  const errs = validate(fx)
  if (errs.length) { failed = true; errs.forEach((e) => console.error(e)); continue }
  const out = JSON.stringify(fx, null, 2) + '\n'
  const dest = join(outDir, `${fx.id}.json`)
  if (check) {
    let cur = ''
    try { cur = readFileSync(dest, 'utf8') } catch {}
    if (cur !== out) { failed = true; console.error(`${dest} is stale: run node eval/build-fixtures.mjs`) }
  } else {
    writeFileSync(dest, out)
  }
  const n = fx.pages.reduce((a, p) => a + p.spans.length, 0)
  console.log(`${check ? 'ok ' : 'wrote'} ${fx.id}: ${fx.pages.length} page(s), ${n} spans`)
}
if (failed) process.exit(1)
