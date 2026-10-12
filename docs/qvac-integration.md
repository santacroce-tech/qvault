# QVAC integration — Phase 0 design

Status: **Phase 0 complete; decisions recorded in [§9](#9-decisions-approved-2026-10-12). Phase 1B (sidecar, Model Manager, Layer 3 on QVAC) built — see [§4.3](#43-as-built-phase-1b) and [§6](#6-installer-size). Phase 2 (OCR for scanned PDFs and images) built — see [§10](#10-phase-2-as-built-ocr-for-scanned-pdfs-and-images).**
Date: 2026-10-12. Branch: `feature/qvac-integration`.

This document covers what Qvault looks like today, the export redaction audit, what QVAC actually provides at the version we would pin, the proposed sidecar architecture, measured size impact, and open questions. Where the original task prompt turned out to be wrong, that is called out in [§8](#8-corrections-to-the-task-prompt).

---

## 1. Qvault today (verified against the code)

### 1.1 IPC commands (25, registered in `src-tauri/src/lib.rs`)

| Group | Commands |
|---|---|
| Document (16) | `upload_document`, `upload_document_bytes`, `scan_document_text`, `add_manual_redaction`, `get_document`, `get_document_text`, `get_document_bytes`, `extract_docx_text`, `get_document_entities`, `get_entity_summary`, `get_document_pages`, `export_clean`, `export_clean_flat`, `open_file`, `delete_document`, `reset_app_data` |
| Redaction (3) | `get_redactions`, `update_redaction`, `bulk_approve` |
| History (2) | `get_history`, `search_history` |
| Layer 3 (4) | `llm_status`, `llm_download_model`, `llm_load_model`, `llm_set_enabled` |

### 1.2 Pipeline

- **Text extraction.** PDFs: the frontend renders with PDF.js and sends page text (with span coordinates) to `scan_document_text`; `lopdf` is used on the Rust side (`document/pdf_parser.rs`). DOCX: `document/docx_parser.rs` unzips and reads **only `word/document.xml`**.
- **Detection** (`pipeline/mod.rs`): `run_layers_1_2` (regex in `pipeline/regex_scanner.rs` + `pipeline/patterns/{global,us,eu,br,de}.rs`; heuristics in `pipeline/heuristic_scanner.rs`), then optional Layer 3 (`llm/llm_scanner.rs`). Later layers skip ranges already matched.
- **Review**: detections are stored in `redactions` with `detection_layer`, `confidence` and `status`; the user approves/rejects in `RedactionSidebar`.
- **Export**: see §2.

### 1.3 Storage (`storage/database.rs`)

Tables: `documents`, `redactions`, `entities` (cross-document occurrences, indexed on `normalized_text`), `document_pages` (full extracted page text), `audit_log`. Document files are stored **unencrypted**: `storage/encryption.rs` is a plain copy since commit `3f8e0f2` removed AES-256-GCM. **`redactions.original_text`, `entities.original_text` and `document_pages.raw_text` also hold PII in plaintext in SQLite.** (Corrected after Phase 2; Phase 0 wrongly said files were encrypted.) Not in scope for this task, but it should be tracked (see §7).

### 1.4 How Layer 3 actually runs

- Model: **GLiNER large v2.1** (span-based zero-shot NER, DeBERTa-v3-large, ~440 MB ONNX) from `onnx-community/gliner_large-v2.1` on the Hugging Face Hub. **Not** Qwen 2.5 0.5B.
- Runtime: in-process `ort 2.0.0-rc.12` with the **`ort-tract` pure-Rust backend, CPU only** (`Cargo.toml` feature `llm`, on by default). The `cuda`/`metal` features are aliases with no effect since the tract switch.
- Weights: `<app data>/models/gliner_large_v2.1.onnx` + tokenizer + config (`llm/model_manager.rs`).
- Download: via `hf-hub`, **only when the user clicks Download in Settings** (`Settings.tsx` → `llm_download_model`). Gaps: no size shown up front, progress jumps 10 % → 100 %, no cancel, no checksum, no import from a local file.
- Inference: 150-word chunks with 20-word overlap, threshold 0.5, 10 labels mapped to `PiiCategory`. Model unloads after 30 s idle.

### 1.5 Baseline tests

`cargo test`: lib unit tests 4/4 pass, `tests/pipeline_test.rs` 24/24 pass. There is no frontend test script.

---

## 2. Export redaction audit (critical)

Method: ran the real pipeline (Layers 1–2) on `landing/demo.pdf`, approved every detection, exported through the real redactor, re-extracted text with `pdftotext` and `lopdf`. Throwaway audit test: `src-tauri/tests/redaction_leak_audit.rs` (uncommitted, report-only).

| Path | Used by UI? | Result |
|---|---|---|
| PDF `export_clean` (vector, `redactor.rs:66-114`) | No — `RedactionSidebar.tsx:100-106` routes PDFs to `export_clean_flat` | **Leaks 37/37 approved strings.** It wraps the original content stream as `q … Q` and appends black (or white + label) rectangles. Tj/TJ/'/" operators, Form XObjects, annotations, AcroForm values, Info dict, XMP and outlines are untouched. The comment at `redactor.rs:133` claiming copy-paste protection is false. The command is still registered and callable. |
| PDF `export_clean_flat` (`create_pdf_from_images`, `redactor.rs:526-617`) | **Yes** | No text layer, no Info/outline — safe **for detections that were placed**. Any approved detection that `mapRedactionsToRects` cannot map to a rect is left visible in the image **with no warning**. |
| DOCX `export_clean` (`redactor.rs:400-509`) | **Yes** | **Leaks.** Raw string replace on `word/document.xml` only; all other zip parts copied verbatim. Confirmed leaks: names split across runs, text split by `proofErr`, tracked-deletion `w:delText`, headers, footers, footnotes, comments (incl. `w:author`), `docProps/core.xml` and `app.xml`. The raw replace can also corrupt XML attributes. And the scanner never reads those other parts. |

Side findings: on `demo.pdf` the regex layer misses the card number `4532-8901-2345-6789`, the routing number, the SWIFT code and the street addresses. (`landing/demo-cleaned.pdf` was not produced by this redactor; it is a re-typeset PDF.)

### Fix plan (first item of Phase 1)

1. **PDF.** Make the Rust side own flattening: rasterize every page that has an approved redaction, burn the boxes into the pixels, keep untouched pages as-is, and strip Info/XMP/outline/annotations/AcroForm/embedded files from the whole document. Remove the vector `export_clean` PDF branch (or gate it behind the same pipeline). **Refuse to export** if any approved detection could not be mapped to a rect. A true glyph-level text remover is a possible later improvement; it is hard to get right with `lopdf` (font encodings, Type3, TJ kerning, XObjects) and is not needed for safety.
2. **DOCX.** Run-aware replacement over the concatenated `w:t` text of each paragraph; process `document.xml`, `header*.xml`, `footer*.xml`, `footnotes.xml`, `endnotes.xml`, `comments.xml`; drop `w:delText`/`w:del`, comment authors and revision metadata; blank `docProps/core.xml` / `app.xml` personal fields. Extend `docx_parser` to scan the same parts.
3. **Regression gate.** A test (and a runtime post-export check) that extracts all text from every exported PDF/DOCX and fails if any approved string survives. This becomes the safety net for every later phase, including OCR export.

---

## 3. What QVAC provides at the pinned version

Full notes with sources: kept in the session scratchpad; key points below. Everything was checked against git tag `sdk-v0.21.0` (commit `4614406f`) and the npm tarballs, not the docs alone (the docs have drifted in places).

- **Pin:** `@qvac/inference@0.21.0` (released 2026-10-04) with the addon versions it pins exactly: `@qvac/ocr-ggml@0.26.0`, `@qvac/llm-llamacpp@0.55.0`, `@qvac/embed-llamacpp@0.43.0`. Committed lockfile; install with `--legacy-peer-deps` (otherwise npm pulls every optional peer, including a 542 MB image-generation addon). Do **not** mix in newer addon releases (`llm-llamacpp` 0.58, `ocr-ggml` 0.27). `@qvac/sdk` 1.0.0/1.1.0 on npm are deprecated leftovers; a `^1` range would be wrong. Breaking changes land in almost every minor release.
- **Licence:** Apache-2.0 throughout; each package ships `LICENSE` and `NOTICE`. The SDK `NOTICE` adds Gemma / HAI-DEF terms for some catalog models — per-model licences must be checked when we pick models.
- **Use `@qvac/inference` on Bare, not `@qvac/sdk`.** On Node, `@qvac/sdk` spawns its own Bare worker and talks to it over a Unix socket / named pipe. `@qvac/cli`'s `qvac serve` is HTTP-only (port 11434) and needs Node. Neither gives us stdio; a small Bare wrapper does.
- **API names (0.21.0):** `plugins([...])` → `loadModel({modelSrc, modelType, modelConfig})`, `completion({modelId, history, stream, responseFormat, requestId})`, `embed`, `ocr({modelId, image})` → `{blocks, stats}`, `finetune(...)` → handle with `progressStream`/`result`, `assessModelFit({models:[...]})` → `verdict: likely-fits | likely-too-large | unknown`, `transcribe` / `transcribeStream`, `cancel({requestId})`. `suspend()` does **not** stop running inference.
- **Constrained output:** `responseFormat: {type:'json_schema', json_schema:{…}}` is supported; raw GBNF grammars are not exposed. LoRA adapters load via `modelConfig.lora`.
- **OCR:** EasyOCR-style CRAFT detector (`craft_mlt_25k.gguf`, 83 MB) + CRNN recognizer (`latin_g2.gguf`, 15 MB) on GGML. Input JPEG/PNG/BMP only — Qvault must rasterize PDFs itself. Output: `{bbox:[xMin,yMin,xMax,yMax], text, confidence}` in **pixels of the input image**, at word-to-phrase granularity (not strictly per word, not per line). A Buffer input is written unencrypted to `<cacheDir>/tmp` during the run — we should pass a file path in our own encrypted-at-rest temp handling, or point `cacheDir` at an app-private dir and wipe it.
- **Networking:** there is **no global offline switch**. Catalog (`registry://`) models are fetched over **Hyperswarm (P2P)** lazily. Passing absolute local paths with an explicit `modelType` never touches the network. `assessModelFit` on files not on disk, `getModelInfo`, registry calls and `discoverRpcServers` can reach out — the sidecar must not expose them. No telemetry found. Logs go to stdout by default → set `loggerConsoleOutput: false` so they don't corrupt the protocol.

---

## 4. Architecture

```
React UI ──IPC──▶ Rust (Tauri) ──JSON-RPC over stdio──▶ qvac-sidecar (Bare + @qvac/inference)
                    │  QvacClient: lazy spawn, ids, timeouts,        │  ocr / completion / embed / finetune
                    │  cancel, crash restart w/ backoff, shutdown    │  local model paths only
                    ▼                                                ▼
                 SQLite, audit_log (metadata only)              no sockets (verified with lsof)
```

- **Transport: stdio, no HTTP.** Constraint 1 is met without a fallback.
- **Packaging:** ship the `bare` runtime as the Tauri `externalBin` (`qvac-sidecar-<target-triple>`) and the worker JS + native addons as Tauri `resources`. This is QVAC's own Electron packaging pattern and works on all three desktop platforms. The single-file `bare-build --standalone` build works on macOS arm64 (tested) but on Linux/Windows fabric loads ggml backends from a sibling directory at runtime, so single-file is unproven there; it also self-extracts ~44 MB of native libraries to `$TMPDIR` on first run (~10 s), which is likely to clash with macOS hardened-runtime notarization.
- **The sidecar exposes a fixed allow-list of methods** (`hello`, `loadModel{path,type}`, `unloadModel`, `ocr`, `complete`, `embed`, `finetune*`, `assessFit{path}`, `cancel`, `shutdown`). It rejects `registry://` sources and any non-absolute path. Rust never sees QVAC's API directly, so version churn is contained in one JS file.
- **Model distribution:** user-initiated HTTPS download from a pinned URL with a pinned SHA-256 and the size shown before the click, plus **Import from file** (verifies the same checksum or lets the user accept an unknown file explicitly). No Hyperswarm, ever. Models live in `<app data>/models/`.
- **Graceful degradation:** if the sidecar is missing, fails `hello`, crashes repeatedly, or `assessModelFit` says the model won't fit, AI features show as unavailable and Layers 1–2 run unchanged. Scans never block on the sidecar.
- **Mobile:** sidecar code compiled only for desktop targets (`cfg(desktop)`); mobile paths unchanged.

### 4.1 Sidecar protocol (JSON lines, one object per line)

```jsonc
// Rust → sidecar
{"id": 7, "method": "ocr", "params": {"modelId": "…", "image": "/abs/path/page-3.png"}}
{"id": 8, "method": "cancel", "params": {"target": 7}}
// sidecar → Rust
{"id": 7, "result": {"blocks": [{"bbox":[x0,y0,x1,y1],"text":"…","confidence":0.98}]}}
{"id": 7, "error": {"code": "CANCELLED" | "TIMEOUT" | "MODEL_NOT_LOADED" | "BAD_PARAMS" | "INTERNAL", "message": "…"}}
{"event": "progress", "id": 9, "data": {"stage": "train", "step": 120, "total": 800}}   // long jobs
{"ready": true, "version": "qvac-sidecar 0.1.0 / @qvac/inference 0.21.0"}            // first line after spawn
```

Lifecycle: spawned lazily on first AI use; `hello` handshake checks versions; each request carries a timeout (OCR per page, completion per chunk); cancellation maps to `cancel({requestId})`; on exit or crash, pending requests fail with `INTERNAL` and the client restarts with exponential backoff (cap 3 restarts per 5 min, then mark unavailable for the session); `shutdown` on app exit, then kill after 2 s. Stderr is captured to the app log; document text is never logged.

### 4.2 Layer 3: keep GLiNER for now, decide on numbers (superseded: see §9 — Layer 3 moves to QVAC in Phase 1)

The prompt says not to run two runtimes without a clear reason. There is a reason here, but it is a trade-off that deserves your call:

- **GLiNER is a span-NER model**: it returns exact word spans with scores, which is precisely what redaction needs, and it already works on CPU with no GPU dependency. A generative LLM via QVAC needs JSON-schema output plus offset re-mapping (duplicates, repeated names) and is slower per page at similar size.
- **But Phase 5 (LoRA fine-tuning) only works on QVAC LLMs**, and Phase 3's "bigger model on a GPU workstation" also points at QVAC.
- **Recommendation:** Phase 1 builds the sidecar for OCR (which we need regardless). Layer 3 stays on GLiNER/`ort-tract` through Phase 3, when the eval harness lets us compare GLiNER vs. a QVAC LLM per category with real precision/recall. If the QVAC LLM is competitive, migrate Layer 3 and delete `ort`/`ort-tract`/`hf-hub`; if not, keep GLiNER and either drop Phase 5 or apply it only to an optional QVAC "second opinion" layer. Either way the existing GLiNER download gets the Model Manager treatment (size, real progress, cancel, checksum, import from file).

### 4.3 As built (Phase 1B)

**Sidecar (`sidecar/`).** `package.json` + committed `package-lock.json` pin exactly `@qvac/inference@0.21.0`, `@qvac/ocr-ggml@0.26.0`, `@qvac/llm-llamacpp@0.55.0`, `@qvac/embed-llamacpp@0.43.0`, `bare-runtime@1.34.1`, `bare-process@4.5.1`, `bare-fs@4.8.4`, `bare-path@3.1.2`, plus `@qvac/langdetect-text@0.1.2`, which the LLM plugin imports unconditionally (the worker fails to start without it). Installed with `--legacy-peer-deps`; no other addons. `bare-build` is not used. `worker.js` implements §4.1 with the allow-list `hello, loadModel{path,type,config}, unloadModel, ocr, complete, embed, assessFit{path,type}, cancel{target}, shutdown`. Anything else returns `METHOD_NOT_ALLOWED`. It also does the following:
- Paths (model, image, `detectorModelSrc`, `lora`) must be absolute and must exist. Any URL scheme is rejected.
- `loadModel` config keys are allow-listed per model type. llama.cpp's `rpc-servers` (network) is not on the list.
- QVAC config is written to `<cacheDir>/qvac.config.json` with `loggerConsoleOutput:false`, `requireHttpChecksum`, `requireSecureTransport`. `console.log/info` are redirected to stderr.
- `<cacheDir>/tmp` is wiped after every OCR job and on cancel, start and exit.
- `complete` always streams internally. It emits `{"event":"progress","id","data":{"tokens"}}`, and adds the text deltas only when `stream:true` is requested.
- `cancel` maps to `cancel({requestId})` for completion, embed and loadModel, and to `cancel({modelId, kind:'ocr'})` for OCR, which has no request id. It answers the target with `CANCELLED` immediately.

**Packaging: runtime as `externalBin`, worker as resources (the preferred option; the single-file fallback was not needed).** `npm run sidecar:build` (`sidecar/build.mjs`) does three things:
1. Copies the host's Bare binary to `src-tauri/binaries/qvac-sidecar-<triple>`.
2. Writes `worker.js` + `node_modules` to `src-tauri/resources/qvac-worker/`. Only the host's `prebuilds/<platform>` are kept, and headers, typings, sources, docs and tests are dropped. `LICENSE`/`NOTICE` files are kept.
3. Replaces `bare-runtime-<host>` with a 2-line stub that returns the running executable. `assessModelFit` spawns its native fit probe through that package, so the stub saves shipping a second 68 MB copy of Bare.

The resource directory is called `qvac-worker`, not `qvac-sidecar`, because dev builds copy the externalBin and resources into the same `target/` directory and the names collided. `tauri.conf.json` declares `bundle.externalBin: ["binaries/qvac-sidecar"]` and `bundle.resources: {"resources/qvac-worker/": "qvac-worker/", …}`. `beforeBuildCommand` is `npm run build && npm run sidecar:build`. `src-tauri/build.rs` writes a zero-byte placeholder when the sidecar has not been built, so plain `cargo build`/`cargo test` work. The app detects the placeholder and reports "AI engine not bundled", and Layers 1–2 keep working. Measured result:
- `node_modules` goes from 521 MB down to 1,903 files / 69 MB.
- The tree passes the protocol test from the bundle layout, including under `sandbox-exec` with `(deny network*)`, which shows OCR, completion and `assessFit` on local files need no network at all.

**Rust (`src-tauri/src/qvac/`, `cfg(desktop)` only):**
- `client.rs` holds the `QvacClient`. It spawns via `std::process`, not tauri-plugin-shell: the mechanism is the same piped stdio, and avoiding the plugin keeps a shell API and its capability surface out of the webview and gives direct control over env, kill and tests.
  - Spawn is lazy, with a `ready` line + `hello` protocol check.
  - Every request carries an id and a timeout, and supports a `CancelToken`. A timeout or cancel sends `cancel{target}`.
  - A crash fails in-flight requests. The next call restarts the sidecar with exponential backoff (500 ms × 2ⁿ). After 3 restarts in 5 min it is unavailable for the session.
  - Shutdown sends `shutdown`, then kills after 2 s.
  - Stderr goes to the app log, truncated to 300 characters per line. Non-JSON stdout is ignored.
  - The child gets a cleared, allow-listed environment.
- `catalog.rs` is the pinned catalog (below).
- `models.rs` holds the `ModelStore`:
  - HTTPS-only `ureq` (rustls) downloads, streamed with progress, cancellable, verified against size and SHA-256, and atomically renamed into place.
  - Import from file: catalog files are matched by SHA-256. Unknown `.gguf` files are accepted only with explicit consent, as custom LLMs recorded with their hash.
  - Delete.
- `mod.rs` holds the `QvacService`:
  - Settings in `<app data>/ai-settings.json`.
  - SHA-256 re-check of each model before its first load in a session.
  - `assessFit` on installed local files only, cached.
  - A model-load cache keyed by sidecar generation.
  - Idle stop after 5 min.
  - The capability probe, OCR, and a self-test on synthetic inputs (the `ai_self_test` command, or the launch flag `--qvac-self-test <models dir>`).
- The app calls `shutdown()` on `RunEvent::Exit`.

**Layer 3 on QVAC (`llm/llm_scanner.rs`).**
- **Chunking:** 150-word chunks with 20 words of overlap, as before.
- **Prompt:** a system prompt plus one few-shot example.
- **Constrained output:** `responseFormat: json_schema` `{entities:[{text (2–120 chars), category ∈ 23 PiiCategory names}]}`, at most 40 items, temperature 0, at most 512 tokens, ctx 4096.
- **Validation:** JSON shape, a known category, and per-category plausibility rules. These reject the whole-sentence answers small models give when a chunk has no PII.
- **Offset mapping:** each returned string is mapped to *every* occurrence in the chunk. Matching is case-insensitive, any whitespace run matches any whitespace run, and matches must sit on word boundaries.
- **Merging:** ranges found by Layers 1–2 are skipped, duplicates across overlapping chunks are dropped, and offsets are page-global.
- **Output:** confidence 0.70, `detection_layer = "llm"` (as before).

Failures are handled per chunk or for the rest of the scan:
- A chunk error (timeout, invalid JSON, crash with restart) skips that chunk.
- A fatal error (sidecar unavailable or restart cap reached) stops Layer 3 for the rest of the scan.
- The scan always finishes with Layers 1–2. `scan_document_text` emits `layer3-status` (`ran|partial|skipped` + reason), and the viewer shows a notice.

**Audit log (metadata only):**
- `model_download_start`, `model_download`, `model_download_failed`, `model_download_cancelled`
- `model_import` (custom imports are flagged `user_accepted_unknown_checksum=true`)
- `model_delete`, `model_load` (model, duration), `ai_settings`, `ai_self_test`
- `ai_scan` per document: layer, model, status, duration, entity count, pages, failed chunks, skip reason.

**Removed:** GLiNER (`llm/model_manager.rs`, `LlmState`, `llm_*` commands); the `ort`, `ort-tract`, `ndarray`, `tokenizers` and `hf-hub` crates; the `llm`/`cuda`/`metal` features. New commands: `ai_capabilities`, `ai_list_models`, `ai_download_model`, `ai_cancel_download`, `ai_import_model`, `ai_delete_model`, `ai_assess_fit`, `ai_set_layer3`, `ai_self_test`. The log plugin now runs in release builds too (Warn; Info for `qvac`), so sidecar diagnostics reach the app log.

**Pinned model catalog:**

| id | File | Size (bytes) | SHA-256 | Licence | Source |
|---|---|---|---|---|---|
| `qwen2.5-0.5b-instruct-q4_k_m` (default Layer 3) | `qwen2.5-0.5b-instruct-q4_k_m.gguf` | 491,400,032 | `74a4da8c…7a9db` | Apache-2.0 | HF `Qwen/Qwen2.5-0.5B-Instruct-GGUF` @ `9217f5d` |
| `qwen2.5-1.5b-instruct-q4_k_m` | `qwen2.5-1.5b-instruct-q4_k_m.gguf` | 1,117,320,736 | `6a1a2eb6…e9407e` | Apache-2.0 | HF `Qwen/Qwen2.5-1.5B-Instruct-GGUF` @ `91cad51` |
| `easyocr-craft-mlt-25k` | `craft_mlt_25k.gguf` | 83,133,856 | `74501993…a875aa` | MIT (CRAFT) + Apache-2.0 (conversion) | **import only** |
| `easyocr-latin-g2` | `latin_g2.gguf` | 15,396,512 | `dd1c7a43…e10768` | Apache-2.0 | **import only** |

How the hashes were checked:
- I downloaded both Qwen files once and computed their SHA-256 myself. They match the HF LFS object ids.
- The two OCR hashes are QVAC's catalog values (`OCR_CRAFT`/`OCR_LATIN`), and they match the files fetched in the Phase 0 spike.

> ⚠ **Flag: OCR model hosting.** QVAC's GGUF conversions of CRAFT and latin_g2 exist only in the QVAC registry, which is Hyperswarm/P2P. The registry's `s3` source has no public URL. There is no copy on Hugging Face, and there are no GitHub release assets. I did not invent a URL. The OCR entries therefore have `url: None`: the UI offers *Import from file* only, and the file is verified against the pinned hash. To enable one-click download, upload the two files unchanged to a Qvault-controlled Hugging Face repo (both licences allow redistribution), then set `url` to a revision-pinned `resolve/<sha>/…` link. The hash and size stay the same. Since Phase 2 the scan pipeline uses OCR (§10); until the files are hosted, scanned pages are flagged "not scanned for PII" on machines without them.

**CI and other platforms (follow-up).** `build.mjs` builds only for the host platform. npm installs only the host's `bare-runtime-*` and `@qvac/fabric-*` optional packages, so cross-builds are refused. Each desktop CI job therefore has to run `npm --prefix sidecar ci --legacy-peer-deps` on its own runner before `tauri build`, which `beforeBuildCommand` does. Not done or verified in this phase:
- **Linux/Windows layout:** the fabric backend `.so`/`.dll` files (Vulkan ~100 MB; HIP ~75 MB on Linux) sit under `@qvac/fabric-<host>/addon/prebuilds/<host>/qvac__fabric/`, and the prune keeps them. Expect larger installers, and decide whether to drop HIP.
- **macOS signing:** the `.bare` dylibs in `Resources/qvac-worker` and the Bare binary need codesigning and notarization under the hardened runtime. This build is unsigned and local.
- **Intel Mac:** `x86_64-apple-darwin` needs a build on an Intel runner (or `npm --cpu=x64` installs).
- **Universal macOS binaries:** not possible, because the prebuilds are per-architecture.
- **Mobile:** the module and commands are `cfg(desktop)`, `ureq` is a non-mobile target dependency, and `build.rs` skips the placeholder on iOS/Android. Mobile builds were not run.

---

## 5. Spike results

A throwaway Bare + `@qvac/inference@0.21.0` sidecar (~40 lines, JSON lines over stdio) built with `bare-build --standalone`, driven from a Rust binary using piped stdio (the same mechanism as Tauri's sidecar `Command`). Machine: Apple Silicon Mac, CPU path.

| Measurement | Result |
|---|---|
| Sidecar ready after spawn | 0.2 s (warm; first ever run ~10 s for self-extraction) |
| OCR model load (CRAFT + latin_g2, local paths) | 0.08 s |
| OCR, `demo.pdf` page 1 rasterized at 200 DPI (1700×2200) | **17.2 s**, 131 boxes |
| Network sockets held by sidecar (`lsof -i`) after load and after OCR | **none** |
| Page-1 PII strings recovered from OCR text (whitespace-insensitive) | **7 / 9** |

The two misses are both emails: `j.richardson@atlasdigital.com` came back as `j richardson@atlas-` + `digital.com` (dot dropped, line-wrap hyphen kept), and `r.fernandes@atlas-digital.com` was split across two boxes at a line break. Implications for Phase 2: OCR text needs normalisation before Layers 1–2 (join line-wrapped tokens, tolerate dropped punctuation in email/ID patterns), and boxes must be split to word level by character proportion before mapping to redaction rects. At 300 DPI, expect roughly 2× the 200 DPI time (~35–40 s/page on CPU) — the DPI default should be revisited after measuring recall at 200 vs 300.

---

## 6. Installer size

Measured on 2026-10-12 with `npx tauri build --bundles dmg` on Apple Silicon. The build is release, unsigned, and has the sidecar bundled as described in §4.3.

| | macOS arm64 DMG | Installed `Qvault.app` |
|---|---|---|
| Baseline (`Qvault_0.2.2_aarch64.dmg`, GLiNER/`ort-tract` build) | 13.2 MB | main binary ~35 MB |
| **Phase 1B (QVAC sidecar bundled, GLiNER removed)** | **52.6 MB** (52,625,425 bytes, ≈4.0×) | **160 MB** |
| — main `qvault` binary | | 17.7 MB (ort/tract/tokenizers gone: about −17 MB) |
| — `MacOS/qvac-sidecar` (Bare 1.34.1 runtime) | | 67.9 MB |
| — `Resources/qvac-worker` (worker + pruned node_modules, 1,903 files) | | 74 MB on disk (69 MB of file data) |

This is about 4 MB under the Phase 0 single-file estimate (57 MB). No models are bundled.

**The built app was exercised end to end through the bundled sidecar.** I ran `Qvault.app/Contents/MacOS/qvault --qvac-self-test <models dir>`, mounted read-only from the DMG. The checks run on synthetic inputs only and use a throw-away cache:
- The sidecar was ready in 0.6 s.
- OCR on the bundled fixture returned `Client: Jane Doe | Phone: 555-867-5309 | Case No. 2024-CV-01187`.
- Layer 3 with Qwen2.5-0.5B returned 2 items for the sample sentence.
- The sidecar exited cleanly on shutdown.

First-run cost from a new install location is about 8 s per native addon load (OCR 7.9 s, LLM 8.7 s), because macOS scans the new dylibs. Warm loads are 0.09 s (OCR) and 0.6–0.9 s (Qwen 0.5B).

Warm timings on CPU, measured outside the app:

| Task | Time |
|---|---|
| OCR, 900×220 fixture | 1.2–1.8 s |
| Layer 3, one chunk (~40 words) | 0.2–0.6 s |
| `assessFit` on the local 0.5B file | verdict `likely-fits` (`native-fit`) |

Linux and Windows will be larger (see §4.3 CI notes); their real numbers come with the CI build.

---

## 7. Risks and open questions

1. **Windows needs Vulkan ≥ 1.4 even for CPU inference** (QVAC docs). VMs and older iGPUs may fail → those users get Layers 1–2 only. Needs a test on a real Windows VM.
2. **Windows bugs open upstream:** #4394 (first model load times out on fresh install), #4151 (pre-AVX2 crash; fix in 0.21.0 dependency chain, unconfirmed on hardware).
3. **Packaging:** built as runtime-as-externalBin + addons-as-resources (§4.3), so single-file self-extraction is no longer an issue. Still open: codesigning/notarizing the `.bare` dylibs under the hardened runtime, and the Linux/Windows backend libraries.
4. **Bundle size** (§6) roughly quadruples the macOS installer. Alternative: ship the sidecar as an optional "AI pack" download. That adds a download path we must secure, but keeps the base installer at ~13 MB. Your call.
5. **OCR quality/speed:** 17 s/page at 200 DPI on Apple Silicon CPU; 7/9 PII strings on one page. Phase 2 needs the normalisation and word-splitting described in §5 and a recall tolerance agreed up front (proposal: ≥ 95 % of the text-layer recall on the scanned `demo.pdf`). *Phase 2 result (§10.10): 97.7 % of text-layer detections and 100 % of the text layer's demo-entity recall at 200 DPI; ~16 s/page.*
6. **QVAC writes Buffer inputs unencrypted to `<cacheDir>/tmp`.** We'll set an app-private `cacheDir` and wipe it after each job; page images are already in plaintext while rendering, so this doesn't change the threat model, but it must be cleaned up.
7. **Model hosting:** without Hyperswarm, someone has to host the GGUF files. Resolved for the LLMs (Qwen on Hugging Face at a pinned revision). **Still open for OCR:** QVAC's CRAFT/latin_g2 GGUFs have no HTTPS host, so they are import-only until Qvault mirrors them (§4.3).
8. **No encryption at rest** (§1.3): document files and PII in SQLite are plaintext. Separate from QVAC; users must rely on full-disk encryption. Recommend a follow-up (SQLCipher or field-level encryption).
9. **API churn:** contained by the sidecar allow-list; upgrading QVAC means editing one JS file and re-running the protocol tests.
10. **Licensing contradiction in the repo:** `README.md` says "Proprietary. All rights reserved."; the website, Layer 3 post and release notes say MIT. Phase 7 should settle this. (The license server and `LICENSE_SYSTEM.md` were removed in `ab3c8ce`.)
11. **Layer 3 quality is unmeasured.** Qwen2.5-0.5B with the few-shot prompt finds names, addresses, dates and IDs in quick checks. It also produces false positives (e.g. "Riverside clinic" as an Address) and is not deterministic across runs. The 1.5B model was not better in a 4-sentence spot check. The Phase 3 eval harness has to set the bar; until then, every Layer 3 finding stays in review at 0.70.

---

## 8. Corrections to the task prompt

| Prompt said | Actually |
|---|---|
| Layer 3 is Qwen 2.5 0.5B, ~400 MB, CUDA/Metal, confidence 0.70 | GLiNER large v2.1 ONNX (~440 MB) on `ort-tract`, **CPU only**; CUDA/Metal features are no-ops. (The website and `layer-3-local-ai-pii-detection.md` also describe Qwen/Candle — out of date.) |
| ~18 IPC commands | 25 |
| Latest SDK v0.19.x | 0.21.0 (2026-10-04); 0.20 and 0.21 both have breaking changes |
| Use `@qvac/sdk` | Use `@qvac/inference` on Bare; `@qvac/sdk` on Node adds a second process over a socket |
| "Disable automatic or P2P fetching in QVAC" | No such switch exists; achieved by only ever passing absolute local paths and never exposing registry/discovery calls |
| OCR "now GGML-based" | Correct, but QVAC's own OCR docs page still says ONNX (stale) |
| Bounding boxes per word or per line? | Word-to-phrase boxes in input-image pixels; must be split to word level ourselves |
| PDF export "draws black rectangles" | True for `export_clean`, which leaks — but the UI actually uses the rasterizing `export_clean_flat` for PDFs. The live leak is DOCX. |
| `architecture.md` "no model downloads" | Already contradicted by the Layer 3 download; Phase 7 reconciles |
| Phase 2: rasterise at 300 DPI by default | Measured: 200 DPI recalls more on the scanned fixture (97.7 % vs 95.5 % of text-layer detections with PDF.js) at the same speed; default is 200 (§10.2) |
| Phase 2: "let the user cancel" OCR | QVAC's OCR cancel is a soft cancel (`scope: 'none'`); the abandoned page keeps running and its result leaked into the next request. Cancel therefore restarts the sidecar (§10.8) |
| "Encrypted at rest" uploads | `storage/encryption.rs` is a plain copy in this (MIT) build; image uploads use the same path, so they are stored exactly like PDFs/DOCX, not encrypted |

---

## 9. Decisions (approved 2026-10-12)

1. **Packaging:** bundle the sidecar in the installer (~57 MB macOS DMG).
2. **Layer 3:** move to a QVAC LLM **now** (Phase 1), not after the Phase 3 eval. GLiNER, `ort`, `ort-tract`, `tokenizers` and `hf-hub` are removed, so there is a single AI runtime. Consequence: Layer 3 accuracy is not benchmarked against GLiNER before the switch; the Phase 3 harness will measure the QVAC layer on its own.
3. **PDF export:** rasterize only pages that carry an approved redaction, burn boxes into the pixels in Rust, strip document-wide metadata, and refuse export if any approved redaction has no rect.
4. **Model hosting:** user-initiated HTTPS downloads from Hugging Face with pinned size and SHA-256, plus import from file. No P2P.

---

## 10. Phase 2 as built: OCR for scanned PDFs and images

Branch `worktree-agent-ac7b4188c600d0031` (based on `feature/qvac-integration` 6b6e5e6). Measured on Apple Silicon, CPU, with the pinned QVAC 0.21.0 sidecar and the catalog OCR models (CRAFT `craft_mlt_25k.gguf` + `latin_g2.gguf`).

### 10.1 Flow

```
Viewer (PDF.js)                       Rust                                  sidecar
text per page ──classify_pdf_pages──▶ ocr::classify (lopdf text + image coverage)
pages needing OCR, per page:
  render @ OCR DPI → PNG ──ocr_page──▶ temp file (0600, <app data>/qvac-cache/pages)
                                       QvacService::ocr_blocks ──ocr──▶   CRAFT + latin_g2
                                       delete file ◀── blocks (px) ──────
                                       ocr::layout::build_layout → PageLayout (text + boxes)
                                       ocr_pages table (status, DPI, layout)
  ◀── layout ────────────────────────
merge OCR text/boxes into page text/spans
scan_document_text (Layers 1–3 on text-layer text + "\n" + OCR text)
ocr_finish (unscanned pages, ocr_run audit line)
review: highlights from OCR word boxes
export: render redacted pages (scanned ones @ OCR DPI) ──export_redacted_pdf──▶
                                       boxes for OCR pages from the stored layout
                                       burn → JPEG → page = image only
                                       verify text layer (as before)
                                       OCR the burned JPEGs it wrote ──ocr──▶
                                       refuse + delete file on any hit
```

Code: `src-tauri/src/ocr/{classify,layout,image_import}.rs`, `commands/ocr.rs`, the OCR half of `document/pdf_redact.rs`, `QvacService::ocr_blocks`, `DocumentViewer.tsx` (`runOcr`, `ocrSpans`, `renderPagePng`), `RedactionSidebar.tsx` (unscanned warning).

### 10.2 Renderer and resolution

**Renderer: PDF.js** (already bundled; it renders the viewer and the export page images).
- It adds nothing to the installer. pdfium would add a native library per platform (~5–6 MB each, plus signing and notarisation on macOS).
- One renderer produces the OCR input, the review canvas and the export image. The burned export page and the image that was OCR'd are therefore the same rendering at the same DPI, so OCR boxes land on the same pixels.
- Rust receives a PNG and never parses page content for rendering. Rust still owns everything safety-relevant: classification, box placement for scanned pages, burn-in and verification.
- Costs: rendering runs in the webview (~0.3 s per page at 200 DPI), and the page crosses IPC as base64 PNG (~1–2 MB per page). Rust cannot render by itself, so the opt-in test rasterises with `pdftoppm` by default. With `QVAC_IT_RENDERER=pdfjs` it uses the app's own PDF.js build in headless Chrome (`tools/render-pdfjs.mjs`). The two disagree enough to matter (below), so the decision is based on the PDF.js numbers.

**Resolution: default 200 DPI** (Settings: 200 / 300 / 400; the prompt proposed 300). Results on the scanned fixture, Layers 1–2. "Detections" is the share of text-layer detections found on the scan; "known" is recall of the 25 demo entities, where the text layer itself scores 79.4 %:

| Renderer | 150 DPI | 200 DPI | 300 DPI |
|---|---|---|---|
| PDF.js (app) | — | **97.7 % / 79.4 %** | 95.5 % / 73.5 % |
| pdftoppm | 88.6 % / 67.6 % (below bar) | 97.7 % / 79.4 % | 90.9 % / 70.6 % (below the 71.5 % bar) |
| OCR time per page | 15.7 s | 15.9–16.1 s | 15.6–16.3 s |

- Time does not grow with DPI. QVAC's CRAFT detector works on a bounded canvas (the recogniser gets crops), so 300 DPI only adds rendering and IPC cost.
- 300 DPI loses accuracy on this fixture mostly through `$` being read as `8`: `(885,000.00)`, `(850,000.00)`, `(815,000.00)`.
- 400 DPI is offered for very small print; it was not measured.

### 10.3 Which pages are OCR'd (`ocr/classify.rs`)

A page is OCR'd when it has **fewer than 32 visible characters** (`MIN_TEXT_CHARS`: empty or near-empty, e.g. only a scanner's "Page 1 of 3"), or when **images cover ≥ 50 % of the page** (`MOSTLY_IMAGE`; e.g. scans that carry their own OCR layer, which cannot be trusted to contain everything visible).
- Text counts are the minimum of PDF.js's count and the backend's own extractor, so a page either one cannot read is OCR'd rather than trusted.
- Image coverage tracks the CTM (`q`/`Q`/`cm`) through image XObjects, Form XObjects (with `/Matrix`) and inline images, clipped to the crop box. Overlaps are summed and the result is capped at 1.
- Export re-runs the classification in Rust and does not rely on the viewer having reported every page.
- Side fix: `pdf_text` now strips inline images before parsing. Before, one inline image hid all the text on its page from the leak check.

### 10.4 OCR output → Qvault's model (`ocr/layout.rs`)

1. **Word boxes.** QVAC returns word-to-phrase boxes. Phrases are split at whitespace, and each word gets the width share of its character positions. This is an approximation (proportional fonts). Each split word box is widened by one average character width on each side, clamped to the phrase box, so errors cover a bit of the neighbouring word rather than leave a glyph visible. Single-word boxes are exact.
2. **Lines.** Words are grouped by vertical overlap and ordered left to right. A gap of more than 3 line heights becomes a newline (columns, table cells).
3. **Normalised text** for Layers 1–3. Every rule here was seen on the fixture:
   - A line-wrap hyphen is re-joined. `Kowal-`/`ski` becomes `Kowalski`; in e-mails, URLs and IDs the hyphen is kept: `atlas-`/`digital.com` becomes `atlas-digital.com`. A capitalised continuation (`Eighty-`/`Five`) stays on two lines.
   - E-mail repairs:
     - a dropped `@` between a local part and a domain, only with evidence (the gap is ≥ 0.45 line heights and the local part contains `.`/`_`/a digit): `maria.santos meridian-consulting.com`;
     - a stray glyph where the `@` was: `r.fernandes~`;
     - spaces around `@` and after dots: `consulting. com`, `maria. santos`;
     - a dropped dot after an initial: `j richardson@`;
     - local part and domain on two lines.
   - Currency signs:
     - `S50,000.00` and the mis-grouped `8150,000.00` become `$…`;
     - after "Dollars (", a missing `$` is *inserted* and the digits are not changed: `(885,000.00)` becomes `($885,000.00)`. Layer 1 then detects the amount and its box still covers every glyph.
4. **Character → box map.** The text is stored as consecutive segments, each mapped to one box or to none (separators). Inserted characters get the box of the gap they stand for (`@`) or of the number they belong to (`$`). Boxes are stored in **PDF points from the top-left of the displayed page**, so they do not depend on the DPI. The viewer scales them to its canvas; the exporter scales them to the burned image.
5. **Storage.** The layout (JSON) goes in a new `ocr_pages` table, together with status (`done`/`unavailable`/`cancelled`/`failed`), DPI, block count and duration. It is reused when the document is opened again at the same DPI. Like `document_pages.raw_text`, it is plaintext in SQLite (§7.8).

The viewer appends the OCR text to the page text (`text layer + "\n" + OCR text`) and the OCR segments to the page spans. As a result:
- all three detection layers run on OCR text. Layer 3 was exercised on it in the integration test (§10.10);
- highlights land on the OCR word boxes (now placed at each detection's own offset, not the first occurrence of its text);
- advanced mode puts a transparent, selectable span over each OCR word. Manual redactions made this way are matched against the OCR text at export, ignoring whitespace and punctuation. Their highlight can be missing when the selection does not reproduce the page text exactly. This path was not exercised in the UI run.

### 10.5 Export of scanned pages (`document/pdf_redact.rs`)

The existing burn-in path is used: pages with approved redactions are replaced by an image with the boxes burned into the pixels. For OCR'd pages:
- **Rust places the boxes** from the stored layout. Every occurrence of every approved string in the page's OCR text is found by an OCR-tolerant matcher (letters and digits only, word-bounded; `text_match::OcrNeedle`), and the boxes are merged per line. The viewer sends text-layer boxes only.
- **Refusals before writing:**
  - an occurrence with no box;
  - approved text on a scanned page that has no approved redaction (refused with page numbers and categories, never the text);
  - a scanned page rendered below 90 % of its OCR DPI.
- **After writing:** the JPEG actually embedded for each burned scanned page is OCR'd again, and the same normalisation and matcher are applied. Any hit fails the export and deletes the file. Untouched scanned pages keep their original pixels, so the stored OCR text checked before writing is exactly what OCR of the exported page would read. If OCR is not available at export time, a scanned page that needs verification makes the export refuse.
- **Never a text layer.** Burned pages contain one image and only `q/cm/Do/Q` (checked, as before). Nothing OCR-derived is written to the PDF.
- **Unscanned pages.** If a page that needs OCR was not scanned (models missing, engine unavailable, cancelled, failed), export returns `OCR_UNSCANNED:<pages>|…`. The sidebar then shows a warning with *Export anyway*, which passes `acknowledge_unscanned`. The acknowledgement is audit-logged. A fully scanned document with OCR unavailable has no detections, so Export stays disabled as before.

Export runs off the async runtime and emits `export-progress` ("Checking redacted scanned page 1/3 with OCR…"). On the fixture, export with OCR verification of 3 pages takes ~46 s.

### 10.6 Standalone images (`ocr/image_import.rs`)

PNG, JPEG and TIFF (including multi-page, 1-bit/CCITT, 16-bit, CMYK) are converted **once, at upload, into an image-only PDF** (one image per page; no text, fonts or metadata). The PDF is stored like any other upload; see §8 on "encrypted". Everything downstream (viewer, classification, OCR, burn-in, verification, export as PDF) then needs no image-specific code. The export of an image upload is a redacted PDF.
- JPEGs with 1 or 3 components and no EXIF rotation are embedded unchanged.
- Everything else is stored losslessly (Flate), as 1-bit when the image is pure black and white.
- Page size comes from the image's DPI metadata when it is plausible (100–1200). Otherwise the long side is set to 11 in.
- Limits: 80 MP per page, 500 pages.

### 10.7 UX

- **OCR progress.** The header shows "Reading scanned page N with OCR (i/n)…" with **Cancel OCR**. Each page shows a badge: waiting, reading, or "OCR · 200 DPI".
- **Pages not scanned.** They get a red banner, "Not scanned for PII — OCR unavailable / cancelled / failed", and the document shows a notice that explains why and what to do. For example: "the OCR models are not installed. Import them in Settings → AI Detection…".
- **Model Manager.** It says whether OCR is ready, and that pages will be flagged when it is not. The OCR models remain import-only (§4.3) and import is unchanged. It also holds the OCR resolution setting.
- **Upload.** Accepts `.png .jpg .jpeg .tif .tiff`.
- **Languages.** en / es / pt.

### 10.8 Cancellation (and a QVAC finding)

Cancel maps to the sidecar `cancel`. However, **QVAC's OCR plugin only soft-cancels** (`cancel: {scope: 'none'}` in `ggml-ocr/plugin.js`): the abandoned page keeps running in the addon. The opt-in test caught the result of the cancelled page 1 being returned for the *next* request, page 3. To prevent this:
- after a cancelled or timed-out OCR, `QvacService` stops the sidecar. This is a stop, not a crash, so it does not count toward the restart cap; the next page starts a fresh process (~3 s, model reload included);
- OCR requests are serialised.

`ocr_cancel_stops_the_page_and_the_engine_recovers` asserts that the next page returns its own text.

### 10.9 Audit log (metadata only)

- `ocr_run` per pass: `status=ran|partial|cancelled|skipped pages=… dpi=… duration_ms=… ocr_ms=… blocks=… unscanned=…`.
- `export` lines now include the number of OCR-verified pages and any acknowledged unscanned pages.
- `ai_settings ocr_dpi=…`.
- `upload` notes conversion from an image format.

No OCR text, file names of temp images or reasons containing document text are logged.

### 10.10 Results (opt-in tests with real models)

`QVAC_SIDECAR_IT=1 QVAC_MODELS_DIR=… QVAC_IT_RENDERER=pdfjs cargo test --release --test ocr_pipeline_it -- --nocapture --test-threads 1`.

The baseline is the text layer of `landing/demo.pdf`, read with the backend extractor. The scan is `tests/fixtures/demo-scanned.pdf`: 300 DPI greyscale, built by `tools/make-scanned-fixture.mjs`, rendered by PDF.js at 200 DPI. Layers 1–2:

| Text-layer detections, by category | Found on the scan |
|---|---|
| CompanyName | 5/5 |
| Date | 5/5 |
| Email | 3/3 |
| MonetaryAmount | 4/5 |
| PersonName | 23/23 |
| SocialSecurityNumber | 1/1 |
| TaxId | 2/2 |
| **Total** | **43/44 = 97.7 %** (bar: ≥ 90 % → passes) |

| 25 demo entities, per (entity, page) | Text layer | Scan |
|---|---|---|
| Person name | 8/9 | 8/9 |
| Organization | 5/5 | 5/5 |
| Email | 3/4 | **4/4** |
| Phone | 0/2 | 0/2 |
| Address | 0/2 | 0/2 |
| SSN | 1/1 | 1/1 |
| EIN | 1/1 | 1/1 |
| Credit card | 0/1 | 0/1 |
| Bank account | 1/1 | 1/1 |
| Financial | 4/4 | 3/4 |
| Date | 4/4 | 4/4 |
| **Total** | **27/34 = 79.4 %** | **27/34 = 79.4 %** (bar: ≥ 71.5 % → passes) |

Notes on these tables:
- The single miss on the scan is `$85,000.00`, read as `($885,000.00)`. It is detected and its box covers the glyphs, but the text differs, so recall counts it as missed.
- The text layer also misses some entities that the scan finds: the text-layer e-mail is split by the backend's extractor at the line wrap, which the OCR normalisation repairs.
- Phones in `(415) 555-0187` form, street addresses and the card number are missed by Layers 1–2 on *both* paths. This is a Layer 1 gap, not an OCR one: the phone regexes only cover `+…` and `555-555-5555`. It is left to the Phase 3 eval.
- With Layer 3 (Qwen2.5-0.5B), same setup: 47/49 text-layer detections (95.9 %) and 28/34 entities (82.4 % vs 79.4 % on the text layer). Layer 3 found the credit card on the scan. Layer 3 is not deterministic, so these numbers vary between runs.

**OCR of the export:** with all 53 scanned detections approved, the export succeeds. All 3 pages are burned and OCR-verified. `lopdf` and `pdftotext` extract nothing. An independent OCR pass of the exported file finds **0 approved strings** (with both pdftoppm and PDF.js rendering). The same flow was also driven through the real frontend: headless Chrome and the real `DocumentViewer`/`RedactionSidebar`, against the Rust code via a throw-away local bridge that was not committed. The checks were:
- OCR progress, classification and highlights on scanned pages;
- *Approve all* followed by export (3 pages verified);
- a mixed document (text page 1, scans 2–3) with OCR models missing: red notices, then the export warning and *Export anyway*.

### 10.11 Tests

- `cargo test`: lib 71 (was 45), `ocr_export_test` 7 (new, fake OCR engine, no models), `pipeline_test` 24, `redaction_export_test` 7, `qvac_sidecar_it` 1, `ocr_pipeline_it` 3 (skip without `QVAC_SIDECAR_IT`). `npx tsc --noEmit` clean.
- New unit tests cover:
  - classification thresholds and image coverage (CTM, forms, inline images, off-page);
  - box splitting and margins, line grouping, normalisation (each repair, and that ordinary prose is not altered), the char → box map, and px → pt transforms;
  - the loose matcher;
  - PNG / JPEG / multi-page TIFF import;
  - OCR runs and cancel.
- `ocr_export_test` covers:
  - placement from OCR boxes and OCR verification;
  - missed text on an unredacted scanned page;
  - text still readable after the burn (refused, file deleted);
  - no OCR at export time, no page image, and a low-resolution render (all refused).

### 10.12 Installer size

`npx tauri build --bundles dmg` (release, unsigned, macOS arm64):

| | Bytes | |
|---|---|---|
| Phase 1B DMG (§6) | 52,625,425 (52.6 MB) | prompt quotes 53.0 MB |
| **Phase 2 DMG** | **53,414,284 (53.4 MB)** | **+0.79 MB** |
| — main `qvault` binary | 20,304,288 (was ~17.7 MB) | `tiff` decoders (CCITT fax, LZW, Deflate, JPEG-in-TIFF) and the new code |
| Installed `Qvault.app` | 162 MB (was 160 MB) | sidecar and worker unchanged |

PDF.js was already bundled, and no models are bundled. The bundled build's `--qvac-self-test` passes from the mounted DMG: OCR 1.3 s, Layer 3 1.0 s.

### 10.13 Known limits

- **Rotated or skewed scans.** There is no deskew and no orientation detection. CRAFT tolerates a few degrees; pages scanned sideways or upside down OCR poorly. PDF `/Rotate` is handled, because PDF.js renders the displayed orientation.
- **Handwriting.** `latin_g2` reads print only. Handwritten names and signatures are not detected. Signature blocks need manual review.
- **Low-contrast, noisy or coloured backgrounds, and stamps over text.** There is no preprocessing; QVAC's `contrastRetry` is not enabled. The fixture is a clean scan, so expect lower recall on real faxes and photos.
- **Small print.** 200 DPI was best on 10–11 pt text. Use 300–400 DPI for footnotes; that setting was not measured.
- **Script.** OCR uses `langList: ["en"]` with the Latin recogniser, which covers es/pt accents; non-Latin scripts are not supported.
- **Reading order.** Multi-column pages are read line by line across columns. Entities are not joined across a column gap of more than 3 line heights, but a two-column layout with a narrow gutter can interleave text.
- **Text fidelity.** OCR misreads (`$`→`8`) can make the text shown in the review list differ from the page. Box placement follows the glyphs, but a string that OCR misread on one page and read correctly elsewhere is matched only where it was read the same way; the post-export OCR check is the backstop. Word boxes split out of phrases are approximate (§10.4), so they can over-cover a neighbouring character.
- **Speed.** ~16 s per page on CPU, plus ~16 s per redacted scanned page for verification at export. A 20-page scan takes ~5 minutes to read. Results are cached per DPI.
- **Cancel** restarts the sidecar (~3 s, §10.8). A cancelled or failed document must be reopened to retry.
- **Plaintext.** Page images exist in plaintext only briefly, in the app-private OCR work dir (0700/0600, deleted per page and at start-up). OCR text is stored in SQLite in plaintext, like the existing page text (§7.8).
- **OCR models are import-only** until they are hosted (§4.3). Without them, scanned pages are flagged, never treated as clean.
- **Not built yet:** "re-run OCR on this page" in the UI, per-page DPI, and OCR during DOCX import (embedded images in DOCX are not scanned).

### 10.14 Differences from the plan

- Default DPI is 200, not 300, based on the measurements above.
- Cancel had to restart the sidecar, because QVAC's OCR cancel is a soft cancel.
- Images are converted to PDFs at upload rather than kept as images. Stored uploads are not encrypted in this build (§8).
- For untouched scanned pages, export "OCR of the exported file" uses the stored OCR text of those (byte-identical) pages instead of OCR-ing them again. Burned pages are always OCR'd again.
- The recall measure uses word-bounded matching, so `$50,000.00` does not count as found inside `$150,000.00`. The bar is asserted at the default DPI only; other resolutions are reported.
