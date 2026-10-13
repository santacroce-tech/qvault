# QVAC integration — Phase 0 design

Status: **Phase 0 complete; decisions recorded in [§9](#9-decisions-approved-2026-10-12). Phase 1B (sidecar, Model Manager, Layer 3 on QVAC) built — see [§4.3](#43-as-built-phase-1b) and [§6](#6-installer-size). Phase 2 (OCR for scanned PDFs and images) built — see [§10](#10-phase-2-as-built-ocr-for-scanned-pdfs-and-images). Phase 3 (eval harness, Layer 1 gaps, Layer 3 quality, model choice) built — see [§11](#11-phase-3-layer-3-quality-model-choice-and-the-eval-harness). Phase 4 (entity grouping and known-entity suggestions) built — see [§12](#12-phase-4-entity-resolution). Phase 5 (LoRA fine-tuning) measured and dropped; replaced by learning from review decisions — see [§13](#13-learning-from-review-decisions-replaces-phase-5-lora). OpenAI Privacy Filter measured as a Layer 3 candidate, not integrated — see [§14](#14-openai-privacy-filter-measured-not-integrated).**
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

**Pinned model catalog** (Phase 1B; the LLM entries were extended and the default changed in Phase 3, see §11.4):

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
11. **Layer 3 quality was unmeasured** (*measured in Phase 3, §11: deterministic now, default model changed*). Qwen2.5-0.5B with the few-shot prompt finds names, addresses, dates and IDs in quick checks. It also produces false positives (e.g. "Riverside clinic" as an Address) and is not deterministic across runs. The 1.5B model was not better in a 4-sentence spot check. The Phase 3 eval harness has to set the bar; until then, every Layer 3 finding stays in review at 0.70.

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

---

## 11. Phase 3: Layer 3 quality, model choice and the eval harness

Measured on Apple M4 Pro (48 GB, Metal), pinned QVAC 0.21.0 / `@qvac/llm-llamacpp@0.55.0`. All reports behind these tables are in [`eval/reports/`](../eval/reports/); how to rerun everything is in [`eval/README.md`](../eval/README.md).

### 11.1 Method

- **Fixtures** (`eval/fixtures/`): 16 synthetic legal documents, 18 pages, **461 required labels** (+37 optional) covering all 23 `PiiCategory` values. EN 8 / ES 4 / PT 4 documents: contracts, court filings, letters, minutes, a notice; US, EU/DE, UK, ES, PT, BR and MX formats; the `landing/demo.pdf` text layer with its 25 website entities plus routing, SWIFT and every other date/amount; hard cases (`LAST, First`, initials, surnames alone, names and e-mails split across line breaks, values in tables); false-positive traps (defined terms, clause headings, company-like phrases, case-law citations, statutes, courts, percentages). All values are fictitious (555-01xx phones, documentation IBANs, test card numbers, `example.*` domains). Labels are inline `[[Category|text]]` markup compiled to JSON with UTF-8 byte offsets; the builder and the Rust harness both validate every span.
- **Held-out split.** Three fixtures (60 labels) were written *after* the Layer 1 work and never used to tune a rule or prompt. Numbers are reported for `dev` and `holdout` separately; the gap is the honest measure of overfitting.
- **Matching.** Category-agnostic (a redaction is a redaction); category agreement is reported as type accuracy. **Overlap** (≥ 1 shared byte) is the headline; **exact** (same boundaries, ignoring surrounding whitespace and `.,;:`) and **covered** (every letter/digit of the label inside some detection, the redaction view) are reported too. Optional labels never count as misses, and detections that only touch them are ignored.
- **Runner.** `src-tauri/tests/eval_harness.rs` (a Rust integration test, like the existing opt-in ITs): Layers 1, 2, 1+2 always (CI); Layer 3 alone and Layers 1+2+3 per model with `QVAULT_EVAL_L3=1` through the real sidecar, 3 runs per model, raw answers recorded so every post-processing heuristic can be ablated offline. Markdown + JSON reports.
- **CI** (`.github/workflows/ci.yml`, ubuntu-22.04, read-only token): `npm ci`, `tsc --noEmit`, `build-fixtures.mjs --check`, `cargo test --locked` (installs the same WebKitGTK packages as `build.yml`; the sidecar is not built, `build.rs` writes its placeholder), then the Layers 1–2 eval against `eval/baseline.json` (fails on a recall drop > 0.5 pt overall / per category / per language, or a precision drop > 2 pt).

### 11.2 Layer 1 gaps (before → after)

Fixed: US phones with parentheses/dots/`1-` prefix, BR and ES/PT national phones; cards with separators (4-4-4-4, Amex 4-6-5), Luhn required for undelimited numbers (grouped numbers that fail Luhn are kept at 0.80 confidence: the `demo.pdf` card `4532-8901-2345-6789` fails Luhn); ABA routing numbers (3-7-1 checksum + Fed prefix ranges, or after a routing/ABA label); SWIFT/BIC (ISO country + label, digit or branch); IBANs with spaces, validated with mod-97 (a German VAT number is no longer reported as an IBAN); street addresses (EN, ES, PT/BR, DE); "D Month YYYY" and es/pt dates; dates after a birth label become `DateOfBirth`; €/£/MXN/euro amounts; labelled passport, driver licence, account, MRN, case/court-file, client/claim/policy numbers and plates; ES DNI/NIE (mod-23), ES CIF, PT NIF and Cartão de Cidadão, UK NINO, MX CURP, BR RG, CNJ case numbers. Validators live in `pipeline/patterns/checks.rs` with unit tests. `pipeline_test` (24) is unchanged and green.

Overlap matching, all 461 labels:

| | P | R | F1 | exact R | covered R |
|---|---:|---:|---:|---:|---:|
| Layer 1 before | 99.4 | 33.8 | 50.5 | 31.5 | 31.7 |
| **Layer 1 after** | **100.0** | **63.3** | **77.6** | 61.8 | 62.3 |
| Layer 2 (unchanged) | 59.0 | 21.7 | 31.7 | 11.9 | 13.2 |
| Layers 1+2 before | 78.4 | 54.2 | 64.1 | 43.4 | 44.9 |
| **Layers 1+2 after** | **84.9** | **83.1** | **84.0** | 73.8 | 75.5 |
| Layer 1, holdout only: before → after | 100.0 → 100.0 | 26.7 → 41.7 | | | |
| Layers 1+2, holdout only: before → after | 83.8 → 86.7 | 51.7 → 66.7 | | | |

The holdout gain is real but much smaller than the dev gain: the new rules fit formats the dev fixtures contain. Holdout misses (dotted dates `2025.03.18`, `Feb. 3, 2021`, `10 jan. 2025`, unpunctuated CPF, Canadian SIN and address, `C/ Gran Vía…`) were deliberately left unfixed so the split stays held out. Per-category tables: `eval/reports/layers12.md`. Layers 1+2 after, per language (overlap F1): EN 80.0, ES 88.2, PT 90.2. Layer 2's person-name heuristic is now the main precision problem (PersonName P 51.8 — "Judgment Debtor", "Effective Date", "Código Civil"); it was out of scope here and is the obvious next fix.

### 11.3 Layer 3 changes

- **Strict validation** (`parse_strict`): only `{"entities":[…]}`; an element must be exactly `{text: string, category: string}`; unknown category, empty text, implausible for the category, stop-listed, or text not found in the chunk → dropped and counted (`DropStats`). The grammar (from the `json_schema` response format) already guarantees shape and the category enum, so on the 7 models `invalid_items` and `unknown_category` were 0; `not_in_chunk` (hallucinated or rewritten text, e.g. the 0.5B copying "Rua das Flores" from the prompt's example) was 107 of 863 items for Qwen2.5-0.5B and 6 of 349 for Qwen3-1.7B.
- **Truncation salvage.** Small models loop ("Alameda, CA 94501" ×20) until `predict` (512) cuts them off, leaving invalid JSON. Before: the chunk was lost (18 of 53 chunks for Qwen2.5-0.5B). Now complete items are recovered (key order independent: llama.cpp emits `category` first). 0 failed chunks on every model after the fix.
- **Offset mapping.** Case- and whitespace-insensitive, word-bounded; a hyphen may be followed by a line break (`j.richardson@atlas-⏎digital.com`); each accepted string is mapped to every occurrence in the **page** (not only the chunk); for names also `Last, First` ⇄ `First Last` and the surname after an honorific (`Mr. Feeney`, `Sra. Ferrándiz Olmedo`). Mapping every occurrence is deliberate: the same string on a page is the same entity, and the reviewer sees each one. Across overlapping chunks a containing span replaces a shorter one; repeats count as agreement.
- **Post-filters**: per-category plausibility (digits for numeric categories, word limits — from Phase 1B) and a stop-list of legal roles/defined terms in EN/ES/PT for names/companies/addresses.
- **Prompt `p3`**: inclusion and exclusion rules, a category glossary, and three short EN/ES/PT examples that each contain things *not* to extract (a citation, clause heading, court, statute). Few-shot texts share no values with the fixtures. `p1b` (Phase 1B) is kept for comparison; reports record the prompt id.
- **Determinism**: `temp 0`, `seed 42`, and `reasoning_budget: 0`, which turns off the Qwen3 / Gemma 4 thinking channel per request (QVAC 0.21 exposes it as a generation param; the sidecar allow-list now accepts it; the JSON grammar would forbid `<think>` anyway). **Variance: 3 runs per model, span-set Jaccard 100 % and 100 % byte-identical answers on every model**. Phase 1B noted run-to-run differences; with temp 0, a fixed seed, the grammar and no reasoning channel none were observed (which of these matters was not isolated). Other hardware/backends may still differ.
- **Chunking**: 150 words, 20 overlap (kept). 80-word chunks were measured on Qwen2.5-0.5B: Layer 3 recall 37.7 → 52.3 but precision 89.7 → 76.7, Layers 1+2+3 F1 84.6 → 80.4, and 1.7× slower. Rejected.
- **Streaming**: each page's Layer 1–2 detections are stored and announced (`scan-page-results`) before Layer 3 runs on that page, then Layer 3's; the viewer reloads the review list as they arrive. Not per chunk: a later chunk can widen an earlier span, so results are final per page. Timeouts, cancel and fatal-error behaviour unchanged.

**Every heuristic, measured** (ablation replayed on run 1's recorded answers; Δ of Layers 1+2+3 when the switch is turned off):

| switch | Qwen2.5-0.5B | Qwen3-0.6B | Qwen3-1.7B | Qwen3-4B | Gemma 4 E4B | verdict |
|---|---|---|---|---|---|---|
| stop-list | P −2.3, F1 −1.3 | P −3.2, F1 −1.8 | P −4.5, F1 −2.5 | 0 | 0 | **keep** (biggest gain for small models; large ones do not return legal terms) |
| page-wide mapping | R −0.7, F1 −0.3 | R −1.3, F1 −0.7 | R −1.3, F1 −0.6 | R −1.8, F1 −1.0 | R −0.4, F1 −0.3 | **keep** (but at 80-word chunks it cost 1.9 F1) |
| `Last, First` variants | R −0.2, F1 −0.2 | R −1.1, F1 −0.6 | R −0.5, F1 −0.2 | R −0.9, F1 −0.5 | R −0.8, F1 −0.5 | keep (no precision cost) |
| honorific + surname | R −0.2, F1 −0.2 | R −0.7, F1 −0.4 | R −1.6, F1 −0.8 | R −1.8, F1 −1.0 | R −1.3, F1 −0.7 | keep |
| hyphen line-wrap | 0 | 0 | R −0.3 | R −0.3 | R −0.2 | keep (the demo.pdf e-mail) |
| accent folding | 0 | 0 | 0 | 0 | 0 | **removed from the default** (no measured effect on any model; still available) |
| all of the above vs Phase 1B post-processing | F1 −1.1 | F1 −2.1 | F1 −2.8 | F1 −2.1 | F1 −1.7 | |

Prompt, end to end (Layers 1+2+3 overlap P / R / F1, run 1): Qwen2.5-0.5B Phase 1B (p1b prompt + p1b post) 80.6 / 83.9 / 82.2 → p3 + new post **82.3 / 87.0 / 84.6**; Qwen2.5-1.5B 83.5 / 89.2 / 86.2 → **83.4 / 95.2 / 88.9**; Qwen3-4B 84.9 / 89.2 / 87.0 → **85.4 / 94.4 / 89.7**. Layer 3 alone with Qwen2.5-0.5B: 83.6 / 28.2 / 42.2 → 89.7 / 37.7 / 53.1.

**Confidence.** The flat 0.70 is replaced by a per-category value, because the data shows Layer 3's *additions* to Layers 1–2 (what the user actually sees from it) differ sharply in precision by category — pooled over the 7 models: PersonName 96 % (n=224), BankAccount 100 % (21), NationalId 92 % (12), DateOfBirth 100 % (9), CompanyName 89 % (37), Date 61 % (23), Address 42 % (36), CaseNumber 13 % (23). Values: 0.80 for PersonName/BankAccount/NationalId/DateOfBirth, 0.70 CompanyName, 0.60 Date, 0.50 Address/CaseNumber, 0.65 for categories with too few additions to measure; +0.05 when two overlapping chunks agree (agreeing detections were 100 % / 96 % correct vs 97 % / 92 % single-chunk for Qwen3-1.7B / 0.6B). Always below Layer 1's 0.95; nothing is auto-applied. Small samples: treat these as an ordering, not probabilities.

### 11.4 Model choice

All files from the original authors on Hugging Face at pinned revisions (never the QVAC registry), SHA-256 computed after download; Qwen3-4B and both Gemma 4 files match QVAC's own catalog hashes (`QWEN3_4B_INST_Q4_K_M`, `GEMMA4_*_MULTIMODAL_Q4_K_M`). Qwen3 0.6B/1.7B: Qwen publishes only Q8_0 GGUFs (QVAC's catalog uses unsloth Q4_0 conversions), so the official Q8_0 was used. Gemma 4 and Qwen2.5-7B: no GGUF from the original author; the exact bartowski conversions were used (Gemma at QVAC's pinned revisions). Every model loaded on the pinned addon; none needed a QVAC upgrade.

3 runs each (identical; Qwen2.5-7B and Qwen3-8B: 1 run, for time), 461 labels, overlap matching; s/page = Layer 3 model time only (18 pages, 53 chunks); RSS = peak of the sidecar process tree; assessFit on this 48 GB machine.

| model | quant / file | licence | assessFit | s/page | peak RSS | L3 alone P / R / F1 | L1+2+3 P / R / F1 | L1+2+3 F1 EN / ES / PT | holdout L1+2+3 R / F1 |
|---|---|---|---|---:|---:|---|---|---|---|
| Layers 1+2 only | — | — | — | — | — | — | 84.9 / 83.1 / 84.0 | 80.0 / 88.2 / 90.2 | 66.7 / 75.4 |
| Qwen2.5-0.5B Instruct | Q4_K_M, 491 MB | Apache-2.0 | likely-fits | 4.4 | 0.94 GB | 89.7 / 37.7 / 53.1 | 82.3 / 87.0 / 84.6 | 80.7 / 89.0 / 90.2 | 76.7 / 82.0 |
| Qwen3-0.6B | Q8_0, 639 MB | Apache-2.0 | likely-fits | 3.7 | 1.30 GB | 92.0 / 49.9 / 64.7 | 82.7 / 90.5 / 86.4 | 84.7 / 86.8 / 90.3 | 80.0 / 81.2 |
| Qwen2.5-1.5B Instruct | Q4_K_M, 1.12 GB | Apache-2.0 | likely-fits | 8.1 | 1.56 GB | 90.3 / 52.7 / 66.6 | 83.4 / 95.2 / 88.9 | 88.0 / 88.9 / 91.5 | 78.3 / 82.3 |
| **Qwen3-1.7B** (new default) | Q8_0, 1.83 GB | Apache-2.0 | likely-fits | 6.2 | 2.43 GB | **97.2 / 67.0 / 79.3** | **84.9 / 93.1 / 88.8** | 87.1 / 89.7 / 92.5 | **85.0 / 87.1** |
| Qwen3-4B | Q4_K_M, 2.50 GB | Apache-2.0 | likely-fits | 12.1 | 3.08 GB | 98.3 / 76.8 / 86.2 | 85.4 / 94.4 / 89.7 | 88.1 / 90.6 / 93.0 | 83.3 / 85.4 |
| Gemma 4 E2B it ¹ | Q4_K_M, 3.46 GB | Apache-2.0 ¹ | likely-fits | 8.5 | 3.85 GB | 98.9 / 79.8 / 88.4 | 85.9 / 95.7 / 90.5 | 89.9 / 92.3 / 90.5 | 83.3 / 86.1 |
| Gemma 4 E4B it ¹ | Q4_K_M, 5.41 GB | Apache-2.0 ¹ | likely-fits | 15.5 | 5.75 GB | 99.5 / 89.2 / 94.1 | 86.4 / 96.7 / 91.3 | 89.0 / 93.8 / 94.8 | 88.3 / 89.0 |
| Qwen2.5-7B Instruct ² | Q4_K_M, 4.68 GB | Apache-2.0 | likely-fits | 19.0 | 5.00 GB | 98.7 / 81.8 / 89.4 | 85.9 / 96.7 / 91.0 | 89.2 / 92.8 / 94.0 | 88.3 / 89.0 |
| Qwen3-8B ³ | Q4_K_M, 5.03 GB | Apache-2.0 | likely-fits | 21.8 | 5.52 GB | 97.5 / 86.1 / 91.5 | 85.1 / 97.2 / 90.7 | 89.7 / 91.3 / 93.0 | **91.7** / 89.3 |

¹ **Gemma licence.** The task assumed Gemma models are under the Gemma Terms of Use (not an OSI licence). For **Gemma 4** that is no longer true: Google's model cards (`google/gemma-4-E2B-it`, `-E4B-it`) declare `license: apache-2.0` and link to <https://ai.google.dev/gemma/docs/gemma_4_license>, which is the Apache License 2.0 text (checked 2026-10-12). Earlier Gemma generations (and QVAC's NOTICE for some catalog models) still reference the Gemma terms. As instructed, Gemma 4 is reported for comparison only and is **not** in the catalog or the default; users can import it as a custom model.

² Qwen2.5-7B: Qwen's own GGUF is split into two shards, which the model store (one pinned file per model) does not support; measured with bartowski's single-file conversion for comparison only, not added to the catalog. 1 run.

³ Qwen3-8B: 1 run (the 3-run results of every other model were byte-identical).

Observations:
- **Every model** improves recall over Layers 1–2; the differences are in how many reviewer-visible false positives come with it and in what the model finds on its own. From 1.7B up, Layer 3 alone is ≥ 97 % precise.
- **Layers 1+2+3 precision is capped at ~85–86 % by Layer 2's name heuristic**, not by Layer 3. For Gemma 4 E4B, Layer 3 *alone* (F1 94.1) beats the combination (91.3). Improving or gating Layer 2 when a strong model runs is the next big win.
- Holdout recall rises with model size much more than dev recall does (Layers 1+2 alone: 66.7): 0.5B 76.7, 1.7B 85.0, Gemma E4B 88.3 — the model, not the regexes, is what generalises.
- Speeds are on a GPU (Metal). CPU-only machines will be several times slower; not measured here.

**Default: Qwen3-1.7B (Q8_0).** Against Qwen2.5-0.5B it raises Layers 1+2+3 F1 84.6 → 88.8 (recall 87.0 → 93.1, covered recall 79.2 → 85.0), holdout F1 82.0 → 87.1, and Layer 3's own precision 89.7 → 97.2 (fewer bad suggestions to reject), at 6.2 s/page instead of 4.4 here, needing ~2.4 GB of memory and a 1.83 GB download. That is "clearly better and still light enough" for an 8 GB laptop. Trade-off: 3.7× the download and 2.6× the memory of the 0.5B; on 4 GB machines it is unlikely to fit (assessFit decides after download). The catalog therefore offers four tiers, and the Model Manager shows size, memory need, the machine's memory and a pre-download estimate, then the real assessFit verdict after download (models that do not fit cannot be selected):

| tier | model | download | needs ≈ | when |
|---|---|---:|---:|---|
| light | Qwen3-0.6B Q8_0 | 639 MB | 1.5 GB | 4–8 GB machines; beats the old 0.5B default on dev (+1.8 F1) and holdout (+3.3 R) and is faster |
| **default** | **Qwen3-1.7B Q8_0** | 1.83 GB | 2.75 GB | most laptops |
| workstation | Qwen3-4B Q4_K_M | 2.50 GB | 3.5 GB | 16 GB+ or a GPU |
| large workstation | Qwen3-8B Q4_K_M | 5.03 GB | 6 GB | GPU workstations; best holdout recall (91.7), 3.5× slower than the default |

Qwen2.5-0.5B and 1.5B stay in the catalog (marked superseded) so existing installs keep working. Catalog entries pin the HF revision URL, exact size, SHA-256 and licence URL (`src-tauri/src/qvac/catalog.rs`).

### 11.5 Tests

- `cargo test`: lib 80, `eval_harness` 3 (Layers 1–2 vs baseline, scoring rules, opt-in Layer 3 skips), `ocr_export_test` 7, `pipeline_test` 24, `redaction_export_test` 7, `qvac_sidecar_it` 1, `ocr_pipeline_it` 3 (opt-in ITs skip without models). `npx tsc --noEmit` clean.
- Opt-in, real sidecar and models: `qvac_sidecar_it` 1 passed (OCR + Layer 3 through the real sidecar; Layer 3 confidences now 0.50–0.85). `ocr_pipeline_it` 3 passed: on `demo.pdf` the text layer now finds 32/34 of the 25 known entities per page (was 27/34 = 79.4 %, thanks to the Layer 1 fixes) and the 200 DPI scan 30/34 = 88.2 % (bar 84.7 %); the scan finds 48/52 = 92.3 % of text-layer detections (was 43/44 = 97.7 %: the new address/phone matches are more sensitive to OCR line breaks; bar 90 %); export of the scanned file still leaves nothing readable.

### 11.6 Known limits

- **Small, self-written eval set.** 461 labels, one author, synthetic documents. Many categories have < 10 labels; per-category numbers move a lot with one item. The dev set shaped the Layer 1 rules and the prompt; only the 60-label holdout is untouched (and it is now spent for Layer 1: new rules should come with new held-out fixtures).
- **Layer 2 precision** (names ~52 %) dominates the combined precision; not addressed in this phase.
- **Layer 3 false positives that remain:** addresses and case numbers suggested by the model are mostly wrong (hence 0.50 confidence); small models still copy values from the prompt's examples (dropped by the in-chunk check).
- **Determinism** was measured on one machine/backend (Metal). llama.cpp results can differ across CPU/GPU backends and thread counts.
- **Speed** numbers are GPU numbers; CPU-only and Windows/Vulkan were not measured. Larger models take 12–15 s per page here.
- **Eval runs need the machine awake** (`caffeinate`): a first Qwen2.5-7B run stalled for hours because the Mac slept between chunks; s/page uses a monotonic clock and was not affected, only wall time.
- **Reports** for the 3-run models were generated before the confidence calibration commit; precision/recall do not depend on confidence, and accent folding (on at the time) never changed a result, so the numbers stand.
- **Memory** is the peak RSS of the sidecar sampled every 300 ms; with Metal, GPU-private allocations may not be counted.
- **Pre-download fit** is an estimate from catalog memory needs vs. physical memory (macOS/Linux only); the authoritative check is assessFit after download, as before.
- **Accent folding** was removed from the default because it never changed a result; models in these runs always copied accents faithfully.

### 11.7 Differences from the plan

- Layer 3 became deterministic with temp 0 + seed; the variance report is therefore "zero across 3 runs" rather than a spread.
- Default model changed from Qwen2.5-0.5B to Qwen3-1.7B (Q8_0, the only official Qwen3 small quant), with Qwen3-0.6B as the light option; Qwen2.5-3B was excluded (licence), Gemma 4 kept out of the catalog as instructed even though its licence turned out to be Apache-2.0.
- Streaming is per page (Layers 1–2 first, then Layer 3), not per chunk.
- The eval runner is a Rust integration test (env-driven), not a separate binary, to reuse the app's pipeline and sidecar client unchanged.
- The CI workflow was written and its steps run locally on macOS; it was not run on Linux (no container runtime on this machine).

---

## 12. Phase 4: entity resolution

Measured on Apple M4 Pro (48 GB, Metal) with the pinned QVAC 0.21.0 / `@qvac/embed-llamacpp@0.43.0` (no upgrade). Reports: [`eval/reports/entity-resolution*.md`](../eval/reports/) and [`eval/reports/known-entity-*.md`](../eval/reports/); how to rerun: [`eval/README.md`](../eval/README.md#entity-resolution-and-known-entity-suggestions-phase-4).

### 12.1 What was built

- **`src-tauri/src/entities/`** (all platforms, no model needed):
  - `normalize` / `forms`: per-category analysis and pair rules. Covers accent folding, EN/ES/PT honorifics with the gender they imply, name connectors (de/da/dos/del/van/von…), "Last, First" order, initials ("J.M."), generational suffixes (Jr./Sr./Filho/Neto), post-nominals, company legal forms (LLC/Inc/Ltd/GmbH/SA/S.L./S/A/Ltda/Limitada/EIRELI/S.A. de C.V., written-out forms), address abbreviations, and numbers compared by their characters only.
  - `resolve`: grouping. Candidate pairs are found by blocking on shared tokens. Automatic merges use union-find, with a check that refuses any merge where a conflict or a user "keep apart" decision exists between the two groups, so a short name can never chain two people. Proposals are made per pair of groups.
  - `known`: known-entity suggestions on a page.
  - `service`: SQLite side.
- **Decision levels** (`forms::compare`):

| level | when | effect |
|---|---|---|
| Same | same text after normalisation (case, accents, honorifics, spacing, "Last, First"), a company with/without its legal form, the same number with other separators or a country code, a name of ≥ 3 full names extended by more full names | **merged automatically** |
| Likely | an initial ("M. Santos"), the surname alone ("Ms. Santos"), a two-name extension ("Maria Santos" / "Maria Fernanda Santos"), middle name used as first name, near-identical spelling (one edit, typo/OCR), a shorter company name, an acronym, most of an address, a different legal form (blocks automatic merging), Jr. on one side only, and (with a model) high cosine similarity | **proposed for review**, never applied |
| Conflict | same surname and another given name / other initial; same first and last name and other middle names; same given name and other surname; Mr vs Ms; Jr vs Sr; another house number; same first company word and another rest | blocks merges and proposals between the two groups |

  A weak proposal (initial or surname only) is flagged **ambiguous** when the short form also matches another group that is a different person. The reviewer sees the flag, and ambiguous proposals are listed last.
- **Never across categories.** Items are grouped per category and the pair function returns Conflict for different kinds; the eval asserts it.
- **Embeddings** (optional): they never merge anything. They can add a proposal (cosine ≥ 0.90 and whole-string Jaro-Winkler ≥ 0.80) for pairs the rules call unrelated. The sweep also measured withdrawing proposals (veto) and overriding conflicts; both are off by default (§12.3).
- **Storage** (schema version 1, `PRAGMA user_version`; databases from before Phase 4 upgrade in place and their entities are grouped on the first regroup):
  - `entity_variants`: one row per distinct (category, text), with its group, a "pinned by the user" flag, and the vector as a little-endian f32 blob plus the model id. Only vectors of the current model are compared. Comparison is brute-force cosine in Rust: per category the sets are small, so sqlite-vec was not needed.
  - `entity_groups`: stable ids across regroups.
  - `entity_cannot_link`: split and "keep separate" decisions.
  - `entity_proposals`.
  - `redaction_notes`: why a suggestion was made.
  - Like the rest of the database, this is **plaintext** (§7.8).
- **When grouping runs:**
  - after every scan, in the background, embedding new variants first when the model is installed (the sidecar starts if needed);
  - when the entity overview opens: rules plus stored vectors, no sidecar;
  - on *Re-check groups*.

  Without the model, or with the sidecar unavailable, grouping uses the rules and any vectors already stored; an embedding failure is logged and the regroup continues.
- **Known-entity suggestions:** after Layers 1–3, on every page of the document. Sources are:
  - strings the user **approved** in any document;
  - strings detected but **not yet reviewed in this document**.

  A string the user rejected and never approved is not a source; suggestions are not sources for themselves; dates and amounts are excluded. Patterns:
  - the same text (case-, accent- and spacing-insensitive);
  - "Last, First" ⇄ "First Last";
  - honorific + surname, gender-checked;
  - initial + surname;
  - a company without its legal form, when the remaining name has ≥ 2 words;
  - numbers with any separators.

  Ranges any layer already found are skipped. Each hit is a **pending** redaction with layer `known-entity` and a confidence of 0.40–0.60, below every other layer: 0.60 for the same or reordered text from an approved source, 0.45 for honorific/initial + surname, −0.10 when the source is unreviewed, −0.20 when the source is one of Layer 2's low-confidence (< 0.85) guesses. A note records the rule, the source text, how many documents approved it, and the group. Nothing is approved automatically. *Approve all* in the sidebar remains a user action, as before.
- **UI:**
  - *Sensitive Data* lists groups. Each group shows its variants, occurrences and documents (opening a document from there works), with *Split out* per variant and multi-select *Merge* (same type only). Suggested merges show the reason, the cosine when present and the ambiguity flag, with *Merge* / *Keep separate*. There is a *Re-check groups* button.
  - The review sidebar shows a "Known entity" badge and why the suggestion was made.
  - The Model Manager has an "Entity grouping model (optional)" section with download or import, an on/off switch and status, and the self-test covers it.
  - All of this is in en / es / pt with `_one` / `_other` plurals.
- **Commands:** `regroup_entities`, `get_entity_groups`, `get_entity_proposals`, `get_entity_group_documents`, `merge_entity_groups`, `split_entity_variant`, `dismiss_entity_proposal`, `get_redaction_notes`, `ai_set_entity_embeddings`.
- **Audit log (counts and ids only, never entity text):**
  - `known_entity_scan`: suggestions, pages, duration;
  - `entity_regroup`: variants, groups, proposals, number embedded, model, whether embedding failed, timings;
  - `entity_merge`, `entity_split`, `entity_proposal_dismissed`;
  - `ai_settings entity_embeddings=…`.
- **Sidecar:** the embeddings config allow-list used snake_case keys that QVAC 0.21.0's `embedConfigBaseSchema` ignores. It now lists the schema's keys (`device`, `gpuLayers`, `batchSize`, `pooling`, `attention`, `embdNormalize`, `flashAttention`). `embed` accepts 1–256 strings of 1–2,000 characters. The protocol test covers `embed`.

### 12.2 Embedding model

Two multilingual GGUF models from their original authors were downloaded at pinned revisions and their SHA-256 computed (both equal the HF LFS ids). Both load and run on the pinned addon.

| model | file | licence | dim | load | speed (198 strings, batches of 64) | peak RSS (sidecar) | cosine same entity, median | cosine different entities, median / p90 |
|---|---|---|---:|---:|---:|---:|---:|---:|
| **nomic-embed-text-v2-moe Q8_0** (catalog) | 512,225,120 B, `06e7a7e5…dc3cff`, `nomic-ai/nomic-embed-text-v2-moe-GGUF` @ `ffbcf4c` | Apache-2.0 | 768 | 1.7 s | **1.4 ms/string** | **1.55 GB** | 0.80 | 0.18 / 0.31 |
| Qwen3-Embedding-0.6B Q8_0 | 639,150,592 B, `06507c7b…e439`, `Qwen/Qwen3-Embedding-0.6B-GGUF` @ `370f27d` | Apache-2.0 | 1024 | 1.4 s | 8.6 ms/string | **5.18 GB** | 0.89 | 0.52 / 0.68 |

**Choice: nomic-embed-text-v2-moe** (catalog id `nomic-embed-text-v2-moe-q8_0`, `ram_needed` 1.75 GiB), not the plan's first candidate:
- Qwen3-Embedding-0.6B allocates its full 32k-token context. `@qvac/embed-llamacpp@0.43.0` exposes no context-size option (its config schema has none), so the model needs ~5 GB of memory on the pinned addon, which is too much for 8 GB laptops.
- Nomic separates different entities far better.
- It is 6× faster and a smaller download.
- Both produce the same grouping results (below).

Inputs use each model's documented query format: `search_query: <text>` for nomic, and an instruction for Qwen3. The self-test, the opt-in IT and the protocol test all pass with it.

### 12.3 Grouping results

`eval/entities/variants.json` has 59 entities and 199 mentions: EN 160, ES 76 and PT 78 same-category mention pairs that are the same entity. Pairwise scores are over all same-category mention pairs. "After review" assumes a reviewer accepts the correct proposals and dismisses the wrong ones.

| system | applied automatically P / R / F1 | B³ P / R | after review P / R | proposals (precision) | of which flagged ambiguous (correct) |
|---|---|---|---|---|---|
| **rules + string similarity, no model** (CI baseline) | **99.2 / 37.9 / 54.8** | 99.5 / 60.0 | **96.3 / 99.0** | 137 (67.9 %) | 55 (30) |
| + nomic, default (τ 0.90, s 0.80) | 99.2 / 37.9 / 54.8 | 99.5 / 60.0 | 96.3 / 99.0 | 137 (67.9 %) | 55 (30) |
| + nomic, veto < 0.6 | 99.2 / 37.9 | 99.5 / 60.0 | 96.2 / 97.8 | 122 (72.1 %) | 45 (30) |
| + Qwen3-Embedding, default | 99.2 / 37.9 | 99.5 / 60.0 | 96.3 / 99.0 | 138 (68.1 %) | 55 (30) |
| + Qwen3-Embedding, override conflicts ≥ 0.85 | 99.2 / 37.9 | 99.5 / 60.0 | 96.3 / **100.0** | 156 (62.8 %) | 54 (28) |
| rules, ambiguous proposals dropped instead of flagged | 99.2 / 37.9 | 99.5 / 60.0 | 99.6 / 82.8 | 82 (76.8 %) | – |

How to read these numbers:
- **Applied automatically there are no wrong merges.** The only false pair is the identical string "J. Santos" used for two people, which one variant per string cannot separate. Applied recall is low on purpose: everything that relies on an initial or a surname alone waits for review. By category (applied → after review, recall): companies 81 → 100, addresses 56 → 100, numbers and e-mails 100. Names: 24 → 99 (precision 98.3 applied).
- **Embeddings do not change the outcome on this set.** The rules already bring 99 % of same-entity pairs within reach of one review click. What remains is nicknames ("Bob" / "Robert Feeney", "Bill" / "William Hart") and a middle name used as first name. Qwen3 at an override threshold of 0.85 recovers "Bill Hart", at the cost of 18 extra proposals (4 of them correct). Nomic's veto at 0.6 removes 10 wrong proposals but also 5 correct ones. Neither is worth turning on by default. The model stays optional, adds proposals only, and is documented as such in the UI ("measured almost as well without it").
- **Proposal precision is 68 %.** Most wrong proposals are surname-only or initial links in families of people who share a surname. 55 of the proposals are flagged *ambiguous*, and only 30 of those are correct. Dropping them instead would raise proposal precision to 77 % but lose 16 points of reachable recall, so they are kept and flagged (the reviewer decides).

### 12.4 Known-entity suggestions

Measured on the 16 detection fixtures (461 labels). The fixtures are independent documents, so the realistic in-app scenario is the first row: the document's own unreviewed detections, right after the scan.

| on top of | knowledge base | labels missed before | recovered | suggestions | false | overlap R before → after | overlap P before → after |
|---|---|---:|---:|---:|---:|---|---|
| Layers 1–2 (CI) | same document, unreviewed | 78 | **13** | 23 | **10** | 83.1 → 85.9 | 84.9 → 83.5 |
| Layers 1–2 | same document, after review | 78 | 13 | 13 | 0 | 83.1 → 85.9 | 84.9 → 85.3 |
| Layers 1–2 | approved in an earlier document (upper bound) | 78 | 65 | 67 | 1 | 83.1 → 97.2 | 84.9 → 86.6 |
| Layers 1–3, Qwen3-1.7B | same document, unreviewed | 32 | **5** | 15 | **9** | 93.1 → 94.1 | 84.9 → 83.6 |
| Layers 1–3, Qwen3-1.7B | same document, after review | 32 | 5 | 6 | 0 | 93.1 → 94.1 | 84.9 → 85.1 |
| Layers 1–3, Qwen3-1.7B | approved in an earlier document (upper bound) | 32 | 26 | 27 | 0 | 93.1 → 98.7 | 84.9 → 85.7 |

What the suggestions recover:
- "Ms. Holloway"-style surnames after a title;
- "J. Whitfield" from a known full name;
- "LLORENTE ESCRIVÁ, Nuria" in the other order;
- upper-case repeats ("LORNA FETTERMAN");
- names split across a line break;
- companies written again in capitals.

All the false suggestions are repeats of Layer 2's own false positives elsewhere in the same document ("New York", "Managing Partner", "Entire Agreement", "JUICIO VERBAL"). They get the lowest confidence (0.40). Rejecting the source does not reject them automatically; that is deliberate, because nothing is skipped automatically. Excluding Layer 2's low-confidence guesses as sources was measured: false suggestions fell from 12 to 3, but recoveries fell from 13 to 3. So they are kept, at lower confidence. A one-word company core ("Delaware" from "Delaware limited liability company") produced only false suggestions and is no longer used. The upper-bound rows show the main value: once a matter's parties have been approved in one document, 83 % of what the layers miss in a related document comes back as suggestions, with ~1 false suggestion.

### 12.5 Tests

- `cargo test`:
  - lib 96 (16 new entity unit tests);
  - `entity_store_test` 5 (new);
  - `eval_harness` 7: the 4 new entity tests, of which 2 opt-in;
  - `ocr_export_test` 7, `pipeline_test` 24, `redaction_export_test` 7;
  - the opt-in ITs skip without models.
- `entity_store_test` covers:
  - **migration from the pre-Phase-4 schema** (data intact, user_version 1, lazy grouping, re-open is a no-op) and a fresh database;
  - stable group ids;
  - merge, split and dismiss surviving regroups;
  - cross-category merges refused;
  - deletion pruning variants and notes;
  - stored vectors and an engine failure falling back to rules;
  - audit text without entity strings;
  - known-entity suggestions with notes, rejected strings not used, no self-propagation.
- `npx tsc --noEmit` clean. `npm --prefix sidecar test` passes, and with `QVAC_EMBED` set it checks the dimension, the normalisation and a similarity ordering, with no inet sockets.
- Opt-in with real models: `QVAC_SIDECAR_IT=1 … qvac_sidecar_it` passes 2 tests, including the new `real_sidecar_embeddings_and_grouping` (load + embed 1.9 s on a warm cache; regroup with vectors stored). The entity embedding eval and the Layers 1–3 known-entity eval also ran (tables above).
- CI (`.github/workflows/ci.yml`) runs `eval_harness entities::` against `eval/entity-baseline.json` (no model).
- The UI was checked in the browser preview with mock data (groups, suggestions, expanded group with documents, the sidebar note). It was not exercised against the real backend in the Tauri app.

### 12.6 Known limits

- **Small, self-written sets.** There are 199 mentions and one author, and the rules were tuned on the same set; there is no held-out entity set yet. The detection fixtures are independent documents, so cross-document recovery is only estimated (the upper-bound rows).
- **Identical strings are one variant.** Two different people both written "J. Santos" share a group (the existing overview already worked per string). Use *Split out* only when the variants differ; a split cannot separate occurrences of the same string.
- **Nicknames and a middle name used as first name** ("Bob" / "Robert", "Inmaculada Ferrándiz" / "María Ferrándiz") are left apart. The rules call them conflicts, and embeddings did not separate them reliably.
- **Gender** comes only from honorifics. There is no first-name lexicon, so "Ms. Santos" is still proposed with "James Santos" (flagged ambiguous when another Santos exists).
- **Known-entity suggestions use text patterns, not embeddings.** Suggestions from unreviewed Layer 2 guesses repeat its false positives (at 0.40). `cleaned_text` of a page does not include suggestions (it never included review decisions either).
- **The grouping model** needs ~1.5 GB while loaded. It is loaded on the first regroup after a scan and unloaded with the sidecar's idle stop. Its suggestions were never better than the rules on the eval.
- **Memory** is measured as the peak RSS of the sidecar process tree (Metal). It is not measured on CPU-only machines or on Windows/Linux.
- **Plaintext:** variants, vectors and notes contain PII in SQLite, like `entities` already did (§7.8). Vectors of names are personal data too.

### 12.7 Differences from the plan

- The **embedding model is nomic-embed-text-v2-moe, not Qwen3-Embedding-0.6B** (the first candidate). Qwen3-Embedding loads on the pinned addon but needs ~5 GB of memory because its context cannot be configured. multilingual-e5 and bge-m3 have no GGUF from their original authors.
- **Embeddings only add proposals.** They never merge and do not take part in known-entity suggestions. The measured gain over rules on the eval is nil, so the model is optional and labelled as such.
- **Vectors are stored as SQLite blobs with brute-force cosine,** not sqlite-vec.
- **Two-name extensions are proposed, not merged** ("João Santos" / "João Pedro Silva Santos"). This is more conservative than "group variants into one canonical entity" for that case.
- **"Documents mentioning this entity" is built** (exact, from occurrences); there is no semantic search.
- **Known-entity suggestions also use the document's own unreviewed detections**, not only previously approved ones, because the fixtures showed that is where the in-document misses are recovered.

---

## 13. Learning from review decisions (replaces Phase 5 LoRA)

Measured on Apple M4 Pro (48 GB, Metal) with the pinned QVAC 0.21.0 and Qwen3-1.7B Q8_0 (the default; SHA-256 `061b54da…590cb1a` checked). Reports: [`eval/reports/learning-layers12.md`](../eval/reports/learning-layers12.md) and [`eval/reports/learning-fewshot-qwen3-1.7b-q8_0.md`](../eval/reports/learning-fewshot-qwen3-1.7b-q8_0.md). How to rerun: [`eval/README.md`](../eval/README.md#learning-from-review-decisions-13).

### 13.1 Why LoRA was dropped

Phase 5 planned opt-in local LoRA fine-tuning of the Layer 3 model on the user's own review decisions. It was built far enough to measure: QVAC `finetune` in the sidecar, a dataset from reviewed findings split by document, an adapter store and an adapter-aware Layer 3. Then it was stopped, because on a simulated review history **every adapter made held-out results worse**:

| model | Layer 3 F1 without adapter | with adapter |
|---|---:|---:|
| Qwen3-1.7B | 76.8 | 49.1 |
| Qwen3-0.6B | 63.0 | 62.9 / 16.7 (two training settings) |

Each training run took 14–36 minutes on this machine. A few hundred reviewed spans are far too little data for the model to learn anything general; it learned the documents instead. The code and reports are on branch `experiment/phase5-lora`, which is not merged.

Two ideas from that branch were kept in spirit but reimplemented independently and much smaller:
- evaluation with a simulated review;
- splitting that history **by document**, so nothing is ever scored on its own decisions.

### 13.2 What was built instead

Three local mechanisms, with no training. All of them work with Layers 1–2 only, so no AI model is needed. Only the few-shot option involves Layer 3, and it is not shipped (§13.4).

| | what it does | automatic? |
|---|---|---|
| **Decision memory** | Every approval and rejection is remembered per reviewed occurrence as (normalised text, category, layer, document). On later scans, a finding whose text and category were **rejected before and never approved** is *marked*. It stays listed and pending, with a "Rejected before (N times)" badge. Its confidence is halved and capped at 0.35, below every layer, and it is sorted after the other findings of its type. The sidebar offers **"Reject all previously rejected"**. | Marking is automatic. Rejecting always takes a click. Nothing is hidden. |
| **"Not personal data" list** | Whole strings, per category, that later scans leave out. Matching ignores case, accents and spacing and never uses substrings. It applies to Layers 1–3 and to known-entity suggestions alike. Every suppressed finding is stored with its position. The sidebar says how many were left out ("N findings are not listed: they are on your Not personal data list"), lists them, and can **restore** each one into the review list. | Items get on the list **only by explicit user action**: "Never flag this again" on a rejected finding, adding in Settings, accepting a suggestion, or importing a file after its preview. |
| **Suggestions for the list** | Settings lists strings rejected ≥ 3 times and never approved, each with an "Add to list" button. | Suggestion only; adding is a click. |

Details:

- **Normalisation** (`learning::key`): accents are folded and text is lower-cased (the Phase 4 `fold`), whitespace is collapsed, and surrounding punctuation and quotes are removed. So "EFFECTIVE  Date," is the same as "Effective Date". "Effective Dates" and "John Effective Date" are different strings.
- **"Rejected before"** means rejected at least once and never approved. A string approved anywhere is never marked; this is the same rule as Phase 4's known-entity sources.
- **Counting:**
  - counts are per occurrence, so clicking reject again does not count twice;
  - the latest decision wins;
  - setting a finding back to pending forgets its decision;
  - documents are counted separately.
- **Known-entity suggestions** (Phase 4) already re-suggest *approved* strings across documents, and that is unchanged. This feature adds only the other direction (rejections). The list and the marks also apply to known-entity suggestions, and a range the list suppressed is not re-suggested by the known-entity layer.
- **Settings → Learning from your reviews:**
  - the list, with search, remove, and add (text + type);
  - **export** to a JSON file and **import**, so a firm can share one list. Import validates the file and shows a preview before anything is applied: the items to add, the items already listed or repeated, and the invalid items with their reason;
  - the suggestions;
  - the number of remembered decisions;
  - **"Forget all review decisions"**.
- **File format**: `{"format":"qvault-not-personal-data","version":1,"exportedAt":…,"items":[{"text","category","note"?}]}`. Import rejects:
  - other formats or versions;
  - files over 8 MB or with more than 10,000 items;
  - items with an unknown category, empty text, more than 200 characters, or control characters.
- **Storage is plaintext.** Four SQLite tables in the app data folder, **unencrypted** like the rest of the database (§7.8); the Settings screen says so:
  - decisions (`review_decisions`);
  - the list (`not_personal_data`);
  - marks (`redaction_memory`);
  - suppressed findings (`suppressed_detections`).

  Deleting a document deletes its marks, its suppressed findings and its *approvals*. Its *rejections* stay in the memory, because they are strings the user said are not personal data, until "Forget all review decisions". "Reset app data" clears all four tables.
- **Schema version 2** (`PRAGMA user_version`) adds the four tables. A backfill turns already reviewed redactions into decisions, so existing users start with a memory.
- **Audit log** (counts and ids only, never the strings):
  - `review_memory_scan marked_rejected_before=… suppressed_by_list=… list_items=…` per scan;
  - `reject_previously_rejected count=…`;
  - `suppressed_restored redaction=…`;
  - `not_personal_data_add category=… source=… item=…`, `not_personal_data_remove item=…`, `not_personal_data_export items=…`, `not_personal_data_import offered=… added=…`;
  - `review_memory_cleared decisions=…`.

  Nothing about decisions is written to the app log.
- **Commands**: `get_document_learning`, `reject_previously_rejected`, `restore_suppressed_finding`, `get_not_personal_data`, `add_not_personal_data`, `remove_not_personal_data`, `export_not_personal_data`, `preview_not_personal_data_import`, `apply_not_personal_data_import`, `get_not_personal_data_candidates`, `get_learning_summary`, `clear_review_memory`. Approvals and rejections are recorded by the existing `update_redaction` and `bulk_approve`.

### 13.3 Decision memory and list, measured

The review is simulated on the 16 detection fixtures (461 labels, holdout included):
- a detection that overlaps a required label is approved;
- one that overlaps no label is rejected;
- one that touches only optional labels gets no decision.

Each document is scanned with the memory of the **other** documents (leave one document out). *Labels lost* = required labels that some detection covered before suppression and none covered after.

| on top of | scenario | false positives | FP marked | FP suppressed | correct detections marked / suppressed | labels lost | P / R before → after |
|---|---|---:|---:|---:|---:|---:|---|
| Layers 1–2 | memory only | 68 | **9 (13 %)** | 0 | 0 / 0 | 0 | 84.9 / 83.1, unchanged |
| Layers 1–2 | list = suggested candidates (≥ 3 rejections) | 68 | 9 | 0 | 0 / 0 | 0 | unchanged |
| Layers 1–2 | list = every rejected string | 68 | 0 | **9 (13 %)** | 0 / 0 | **0** | 84.9 / 83.1 → 86.6 / 83.1 |
| Layers 1–2 | stress: list = every string rejected anywhere, this document included | 68 | 0 | 68 (100 %) | 0 / 0 | **0** | 84.9 / 83.1 → 100.0 / 83.1 |
| Layers 1–2 | dev history → holdout documents | 6 | 0 | 0 | 0 / 0 | 0 | unchanged |
| Layers 1+2+3 (Qwen3-1.7B) | memory only | 76 | 9 (12 %) | 0 | 0 / 0 | 0 | 84.9 / 93.1, unchanged |
| Layers 1+2+3 | list = every rejected string | 76 | 0 | 9 (12 %) | 0 / 0 | **0** | 84.9 / 93.1 → 86.4 / 93.1 |
| Layers 1+2+3 | stress (as above) | 76 | 0 | 75 (99 %) | 0 / 0 | **0** | 84.9 / 93.1 → 99.8 / 93.1 |

- **Safety:** the list suppressed **no detection of labelled data and lost no label** in any scenario. That includes the stress case, where every false-positive string of the whole set is on the list. This is a CI test (`eval_harness learning::decision_memory_layers12`).
- **The effect is modest on this set.** The fixtures are independent documents by different (fictional) parties, so few false positives repeat across documents. What repeats is legal boilerplate taken for names: "Effective Date" (3 documents), and "Confidential Information", "Managing Director" and "São Paulo" (2 each). A firm's real documents reuse the same templates and defined terms, so the share should be higher there; that is not measured here.
- The ≥ 3 suggestion threshold produced no candidate in leave-one-out, because no false positive was rejected 3 times in the other documents. That is the conservative intent.
- No correct detection was ever marked "rejected before" on this set.

### 13.4 Few-shot examples for Layer 3: measured, not shipped

`llm_scanner::messages_with_examples` can append the user's own unanimous decisions to the system prompt:

> This user's earlier review decisions … rejected, do NOT extract: "Effective Date" (not a PersonName); … confirmed as personal data: …

`learning::select_examples` chooses them: most frequent first, spread over categories, at most 60 characters each. For each fixture, the examples came from the simulated review of the *other* fixtures' baseline Layers 1+2+3 detections. Each variant is one deterministic run (temp 0, seed 42); the baseline reproduced §11.4 exactly.

| variant | L1+2+3 P | R | F1 | covered R | holdout R | holdout F1 | s/page | prompt chars added |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| **no examples (app)** | 84.9 | 93.1 | 88.8 | 85.0 | 85.0 | 87.1 | 6.21 | 0 |
| 2 mixed (1 rejected + 1 approved) | 84.6 | 93.7 | 88.9 | 85.7 | 85.0 | 85.6 | 6.24 | 206 |
| 6 mixed (3 + 3) | 84.9 | 92.8 | 88.7 | 84.8 | **81.7** | 84.4 | 6.09 | 338 |
| 3 rejected | 84.7 | 94.4 | 89.3 | 86.3 | 85.0 | 86.3 | 6.25 | 224 |
| 6 rejected | 84.8 | 93.5 | 88.9 | 85.5 | 85.0 | 85.6 | 6.28 | 344 |
| **control: 3 generic "rejected" terms not from any review** | 84.4 | 94.4 | 89.1 | 86.3 | 85.0 | 85.6 | 6.23 | 216 |

**Decision: not shipped.** The app's Layer 3 prompt is unchanged and `Layer3Session.examples` stays empty, because:

- The best variant (3 rejected) gains +0.5 F1 and +1.3 recall overall. But the **control, with generic terms no reviewer chose, gains the same recall** (94.4) and almost the same F1 (89.1). The gain comes from changing the prompt, not from the user's decisions.
- No variant improves the **held-out** documents. Holdout F1 drops in every variant (87.1 → 84.4–86.3), and 6 mixed examples lose holdout recall (85.0 → 81.7). Recall is what matters for redaction.
- Precision never improves, though that is what the rejected examples were supposed to fix.
- Speed is unchanged within noise in the final run (6.09–6.28 s/page against 6.21; up to 344 more prompt characters per chunk). Earlier runs with other work on the machine showed 6.4–6.9 s/page; the accuracy numbers were identical across all runs.
- The differences are 1–6 labels out of 461 on single runs, within what an unrelated prompt change produces.

The code path and the opt-in eval stay, so this can be measured again on a larger, more repetitive document set or with another model.

### 13.5 Tests

- `cargo test`:
  - lib 103 (6 new in `learning`, 1 in `llm_scanner`);
  - `learning_store_test` 5 (new);
  - `eval_harness` 9: `learning::decision_memory_layers12` (new, runs in CI) and `learning::fewshot_layer3` (new, opt-in);
  - all earlier suites unchanged, except that `entity_store_test` now checks the current schema version constant.
- `learning_store_test` covers:
  - **migration from schema version 1**: data intact, decisions backfilled, re-opening is a no-op, and a fresh database starts at version 2;
  - decisions: per-occurrence counting, the latest decision wins, pending forgets, bulk approval, document deletion keeps only rejections, and clearing;
  - the scan outcome: a marked finding stays pending with a lowered confidence, a listed one is suppressed, the one-click rejection is remembered, restore works, removing an item works, and deletion cleans up;
  - the list applied to known-entity suggestions;
  - the list itself: add, validation, export, import preview and apply, candidates.
- CI (`.github/workflows/ci.yml`) runs `eval_harness learning::decision_memory_layers12`.

### 13.6 Known limits

- **Exact strings only.** "Effective Date" on the list does not cover a longer span such as "the Effective Date of this Agreement", nor another category. This is deliberate, so that a partial match can never hide personal data, but users may need several items for one term.
- **Across documents only.** Rejecting one occurrence does not change other pending occurrences of the same string in the same document. "Reject all previously rejected" covers only strings rejected in *earlier* scans.
- **Suppression is per category.** A string that Layer 2 calls a name and Layer 3 calls a company needs two items.
- **The list's effect depends on the user.** The safety result covers the eval set's false positives. A user can still put real personal data on the list, for example a client's name typed in Settings. Every suppression is counted and shown with a restore button so that it can be noticed, and the list is visible and editable.
- **Plaintext:** the list and the rejected strings are stored unencrypted (§7.8), and rejections survive document deletion until cleared.
- **Not measured on real firm documents**, where repeated boilerplate should make the memory more useful than on this synthetic, deliberately varied set.
- Branch `experiment/phase5-lora` also used schema version 2, for different tables; it must not be merged as is.

---

## 14. OpenAI Privacy Filter: measured, not integrated

[`openai/privacy-filter`](https://huggingface.co/openai/privacy-filter) (Apache-2.0, revision `7ffa9a04…385b`) is a 1.5B-parameter mixture-of-experts token classifier with about 50M active parameters. It tags tokens with BIOES labels over 8 PII labels and decodes them into spans with a constrained Viterbi decoder. It was measured offline on the eval fixtures, with OpenAI's reference code (`opf`, commit `f7f00ca7`) and decoding. App behaviour is unchanged.

- The only repository code is an opt-in harness path. `QVAULT_EVAL_EXTERNAL=preds.json` scores predictions made elsewhere, using UTF-8 byte offsets validated against the fixture text. They are scored alone, added to Layers 1+2 the way Layer 3 is added, and added to Layer 1 alone.
- Full report, file hashes and every table: [`eval/reports/privacy-filter.md`](../eval/reports/privacy-filter.md) and `.json`.
- Apple M4 Pro, 461 labels, overlap matching; Privacy Filter numbers are for the int8 ONNX file:

| | L3 alone P / R / F1 | L1+2+3 P / R / F1 | covered R | F1 EN / ES / PT | holdout R / F1 | s/page | memory |
|---|---|---|---:|---|---|---:|---:|
| Layers 1+2 only | — | 84.9 / 83.1 / 84.0 | 75.5 | 80.0 / 88.2 / 90.2 | 66.7 / 75.4 | — | — |
| Qwen3-1.7B (default) | 97.2 / 67.0 / 79.3 | 84.9 / 93.1 / 88.8 | 85.0 | 87.1 / 89.7 / 92.5 | 85.0 / 87.1 | 6.2 (Metal) | 2.4 GB |
| Gemma 4 E4B | 99.5 / 89.2 / 94.1 | 86.4 / 96.7 / 91.3 | 88.7 | 89.0 / 93.8 / 94.8 | 88.3 / 89.0 | 15.5 (Metal) | 5.8 GB |
| Privacy Filter, default | 93.1 / 67.5 / 78.2 | 83.0 / 96.3 / 89.2 | 88.3 | 88.7 / 90.7 / 89.0 | 90.0 / 87.7 | 0.08–0.19 (CPU) | 1.3–2.4 GB |
| Privacy Filter, recall point (`stay3`) | 90.3 / 76.1 / 82.6 | 81.2 / 97.2 / 88.5 | 90.0 | 88.5 / 89.2 / 87.8 | 95.0 / 89.9 | same | same |

- **Equivalent F1, different profile.** Compared with Qwen3-1.7B, Privacy Filter has:
  - higher recall: +3.2 overall, +3.3 covered, +5.0 held out, most of it from person names (94 % alone);
  - lower precision: 23 false suggestions in the combined output against 8;
  - lower Portuguese F1 (−3.5).
- **Taxonomy gaps.** It has no label for company names, money amounts, or case and court-file numbers; it adds nothing to Layers 1+2 for companies. Every other identifier (national ID, tax ID, passport, plate, IP address) is reported as `account_number`: it is often found, but with the wrong type (type accuracy 74 % vs 93 %). Its label policy can only be changed by fine-tuning.
- **Operating points.** The published calibration file contains only `default` (all biases 0); there is no recall preset. A hand-set background-stay bias of −3 (`stay3`, chosen on the dev split) trades 2.8 points of precision for 8.6 points of recall when the model runs alone.
- **Quantisation.** int8 (`model_quantized`, 1.62 GB) equals the bf16 reference. q4 and q4f16 (0.92 and 0.81 GB) lose 6.7 points of held-out recall. Shipping candidate: int8.
- **Complement.** Layers 1+2 with both Qwen3-1.7B and Privacy Filter would miss 7 labels instead of 30 (overlap recall about 98.5 %).
- **Runtimes, measured:**
  - (a) Rust `ort` 2.0.0-rc.13 adds 17.9 MB to the binary (static ONNX Runtime) and gives outputs identical to Python.
  - (b) Transformers.js in WKWebView runs only on WebGPU (q4f16, 0.51 s/page, about 5 GB in WebKit processes). The WASM backend cannot load the quantized files, and the default configuration fetches from `cdn.jsdelivr.net` and `huggingface.co` unless overridden.
  - (c) QVAC's LLM, embedding and OCR addons cannot run this model. QVAC does publish **`@qvac/onnx`** 0.15.1, a generic ONNX Runtime addon, and it runs the model **inside the existing Bare sidecar**: 20.7 MB on macOS arm64, 0.19 s/page, outputs identical to Python.

**Decision: not adopted now.** Qwen3-1.7B stays the default Layer 3. Privacy Filter is recorded as the leading candidate for a *fast, CPU-friendly complement*. If CPU-only speed or maximum recall becomes the priority, the next step is a prototype through `@qvac/onnx` in the sidecar, with:

- the int8 model, imported from a local file with its SHA-256 pinned;
- opf's Viterbi decoding;
- Layer 2 kept: without it F1 rises to 92.7, but recall falls by 5.4 points;
- low confidence for `account_number` suggestions;
- a broader Spanish and Portuguese check first. The 461-label synthetic set is small, and the model card says the model is "primarily English" and flags legal workflows as high-risk.
