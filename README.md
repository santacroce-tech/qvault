# Qvault

Local PII detection and redaction for legal documents. Qvault finds personal
data in PDFs, Word files and scanned pages, lets you review every finding, and
exports redacted copies in which the redacted text is really gone. Everything
runs on your computer.

Desktop app for macOS, Windows and Linux, built with Tauri v2, Rust and React.
Free and open source under the [MIT License](LICENSE).

**Website:** <https://santacroce-tech.github.io/qvault/> ·
**Download:** [latest release](https://github.com/santacroce-tech/qvault/releases/latest)

## Privacy model

- Documents are opened, scanned, reviewed and exported locally. No document,
  extracted text or detection result leaves the device. There is no cloud
  service, account, licence key or telemetry.
- The only network use is an **optional, user-started model download**: Qvault
  shows the file size first, downloads over HTTPS from a pinned Hugging Face
  revision, and verifies the SHA-256 before the model is used. Models can also
  be imported from a file, so air-gapped machines get every feature.
- The AI runtime runs in a bundled sidecar process that talks to the app over
  stdin/stdout and opens no network sockets.

## Features

- **PDF, DOCX and scans.** Text PDFs, Word documents, and scanned PDFs or
  PNG/JPG/TIFF images through on-device OCR.
- **Three detection layers.**
  1. *Patterns*: regular expressions for emails, phone numbers, cards, IBANs,
     IPs, URLs, dates and amounts, with jurisdiction sets for Global, US (SSN,
     EIN), EU (VAT), BR (CPF, CNPJ, CEP) and DE (Steuernummer).
  2. *Heuristics*: names of people, company names with legal suffixes, and
     addresses.
  3. *Local AI (optional)*: a small LLM via Tether's
     [QVAC](https://github.com/tetherto/qvac) SDK (Qwen2.5-0.5B-Instruct by
     default, 1.5B optional) running in the bundled sidecar. Desktop only.
- **Review first.** Every detection goes to review with its confidence and
  source layer. Nothing is redacted automatically. Manual redactions, an entity
  overview across documents, and a metadata-only audit log.
- **OCR.** QVAC OCR (EasyOCR CRAFT text detector + Latin recognizer, GGUF)
  for Latin-script text; recognised text goes through the same layers and
  review.
- **Verified export.**
  - PDF: pages with redactions are rasterized and the boxes burned into the
    pixels; Info, XMP, outlines, forms, attachments and scripts are stripped.
    Export is refused if a redaction cannot be placed.
  - DOCX: run-aware redaction across body, headers, footers, footnotes,
    endnotes, comments and tracked changes; authors and document properties
    blanked.
  - Every export is re-read and refused (file deleted) if any approved string
    survives.
- **Model Manager** (Settings → AI Detection): download LLMs in-app with size,
  progress, cancel and checksum, or import any catalog model from a file.

## Install

Download the installer for your platform from
[GitHub Releases](https://github.com/santacroce-tech/qvault/releases/latest).

Installers are not yet code-signed. On macOS, if the app "can't be checked",
open System Settings → Privacy & Security → *Open Anyway* (or run
`xattr -cr /Applications/Qvault.app`). On Windows, choose *More info → Run
anyway* in SmartScreen.

## OCR model setup

OCR models are import-only: download them yourself, verify them, and import
them once.

| File | Size (bytes) | SHA-256 | Licence |
|---|---|---|---|
| [`craft_mlt_25k.gguf`](https://github.com/santacroce-tech/qvault/releases/download/ocr-models-v1/craft_mlt_25k.gguf) (CRAFT text detector) | 83,133,856 | `74501993caf4581ce09b280f49a1b3d249c0f5a78496e047718b398457a875aa` | MIT weights; GGUF conversion Apache-2.0 |
| [`latin_g2.gguf`](https://github.com/santacroce-tech/qvault/releases/download/ocr-models-v1/latin_g2.gguf) (EasyOCR Latin recognizer) | 15,396,512 | `dd1c7a436e9175904e63683e939635892f45c8c6377d5e14e87cf8b175e10768` | Apache-2.0 |

1. Download both files into one folder.
2. Verify the checksums:
   ```bash
   # macOS / Linux
   shasum -a 256 craft_mlt_25k.gguf latin_g2.gguf
   ```
   ```bat
   :: Windows
   certutil -hashfile craft_mlt_25k.gguf SHA256
   certutil -hashfile latin_g2.gguf SHA256
   ```
3. In Qvault, open **Settings → AI Detection** (the Model Manager), find
   **OCR models**, click **Import from file…** and pick each file. Qvault
   recognises them by SHA-256.

Air-gapped machines: download and verify the files on a connected computer,
copy them over on removable media, verify again, and import. The Qwen LLM
files can be imported the same way. Full instructions:
[Models and setup](https://santacroce-tech.github.io/qvault/models.html).

## Build from source

Prerequisites: Rust (stable), Node.js 20 or newer, and the
[Tauri v2 system dependencies](https://v2.tauri.app/start/prerequisites/)
for your OS (Xcode command line tools on macOS; WebView2 and the MSVC build
tools on Windows; `libwebkit2gtk-4.1-dev` and friends on Linux).

```bash
git clone https://github.com/santacroce-tech/qvault.git
cd qvault
npm install

# Build the QVAC sidecar for this machine (installs sidecar/ deps with
# --legacy-peer-deps, copies the Bare runtime to src-tauri/binaries/ and the
# worker to src-tauri/resources/qvac-worker/). Host platform only.
npm run sidecar:build

npx tauri dev      # run in development
npx tauri build    # release build and installers in src-tauri/target/release/bundle/
```

Without `npm run sidecar:build`, `cargo build`/`cargo test` still work: the
build writes a placeholder, the app reports "AI engine not bundled", and Layers
1–2 run as usual. `npx tauri build` runs the sidecar build itself
(`beforeBuildCommand`).

## Tests

```bash
cargo test --manifest-path src-tauri/Cargo.toml          # unit, pipeline and export-leak tests
npx tsc --noEmit                                         # frontend type check
npm --prefix sidecar test                                # sidecar protocol test (after sidecar deps are installed)

# Opt-in end-to-end test against the real sidecar and models:
QVAC_SIDECAR_IT=1 QVAC_MODELS_DIR=/path/to/models \
  cargo test --manifest-path src-tauri/Cargo.toml --test qvac_sidecar_it -- --nocapture
```

`src-tauri/tests/redaction_export_test.rs` is the regression gate for export:
it fails if any approved string can be extracted from an exported PDF or DOCX.

## Architecture

```
React UI ──Tauri IPC──▶ Rust backend ──JSON lines over stdio──▶ qvac-sidecar
                          extraction, Layers 1–2,                (Bare + @qvac/inference)
                          export + verification                  OCR, LLM; local files only
```

- `src/`: React + TypeScript frontend (PDF.js viewer, review sidebar, Model Manager).
- `src-tauri/src/pipeline/`: Layers 1–2 (`regex_scanner.rs`, `patterns/`, `heuristic_scanner.rs`).
- `src-tauri/src/llm/`: Layer 3 scanner on QVAC.
- `src-tauri/src/qvac/`: sidecar client, pinned model catalog, model store.
- `src-tauri/src/document/`: PDF/DOCX extraction and redacted export.
- `src-tauri/src/storage/`: SQLite and AES-256-GCM file encryption.
- `sidecar/`: the Bare worker and its build script.
- `site/`: the website (`node site/build.mjs` → `site/dist`, deployed by
  `.github/workflows/pages.yml`).

More detail: the [architecture page](https://santacroce-tech.github.io/qvault/architecture.html)
and the design notes in [docs/qvac-integration.md](docs/qvac-integration.md).

Known limits: detection is not exhaustive and Layer 3 quality has not yet been
formally measured; OCR is CPU-bound and imperfect; extracted text and findings
are stored unencrypted in the local SQLite database (document files are
encrypted); redacted PDF pages become images; on Windows the QVAC runtime needs
a recent Vulkan driver.

## Website

```bash
node site/build.mjs                       # build into site/dist
node site/check-links.mjs                 # check internal links and anchors
python3 -m http.server -d site/dist 8000  # preview at http://localhost:8000
```

## Licence

Qvault is released under the [MIT License](LICENSE), © 2026 Santacroce SL.

Third-party components (QVAC, the Bare runtime, pdf.js and Rust crates) and the
optional AI models are listed with their licences in
[ATTRIBUTION.txt](ATTRIBUTION.txt). Models are not bundled; each is downloaded
or imported only at your request and is covered by its own licence.
