# The Architecture Behind Qvault: Why Your Documents Never Leave Your Machine

**A deep dive into how Qvault processes, detects, and redacts sensitive information entirely on your device — with zero data transmission.**

---

## Introduction

Legal professionals handle some of the most sensitive information in existence: Social Security numbers, financial records, personal addresses, medical data, and confidential business details. Every day, law firms process thousands of documents containing personally identifiable information (PII) that, if exposed, could harm clients and violate regulations like GDPR, CCPA, and LGPD.

Most document processing tools require uploading files to a cloud server. For a law firm, that means client data — privileged, confidential, protected by attorney-client privilege — leaves the machine and travels across the internet to be processed on someone else's infrastructure.

Qvault takes a fundamentally different approach. **Every byte of every document stays on your computer.** There is no cloud processing, no data upload, no server-side analysis. The entire PII detection and redaction pipeline runs locally, inside a native desktop application built with Rust and modern web technologies.

This article explains how we built it, why we made the architectural decisions we did, and what makes Qvault fast enough to process documents in real time without ever phoning home.

---

## The Local-First Architecture

Qvault is built on **Tauri v2**, a framework that pairs a Rust backend with a lightweight web frontend. Unlike Electron (which bundles an entire Chromium browser), Tauri uses the operating system's native webview — WebKit on macOS, WebView2 on Windows, WebKitGTK on Linux. The result is a binary that's a fraction of the size of an Electron app, with lower memory usage and faster startup times.

### Why Rust?

The backend is written entirely in Rust, and this choice is central to Qvault's performance story. Rust provides:

- **Memory safety without garbage collection.** There are no pause-the-world GC cycles interrupting document processing. Memory is freed deterministically, which matters when scanning documents with thousands of text spans.
- **Zero-cost abstractions.** The regex engine, encryption routines, and PDF parser all operate at native speed. There's no interpreter overhead, no JIT warmup.
- **Fearless concurrency.** Tauri's async runtime (Tokio) handles IPC commands concurrently. Multiple documents can be processed in parallel without data races.

The frontend is React 18 with TypeScript and Tailwind CSS, rendered inside the native webview. The two layers communicate through Tauri's IPC bridge — a type-safe, serialized message-passing system that connects JavaScript function calls to Rust command handlers.

### The IPC Bridge

Qvault exposes 18 IPC commands that the frontend can invoke. These are not REST endpoints or WebSocket messages — they're direct function calls serialized through Tauri's command system, with no network stack involved.

The commands cover the full document lifecycle:

- **Document management:** Upload, retrieve, delete, and reset operations
- **PII scanning:** Text extraction and dual-layer detection pipeline
- **Redaction review:** Individual approval/rejection and bulk operations
- **Entity tracking:** Cross-document PII knowledge base queries
- **Export:** Coordinate-mapped PDF redaction with black-box rendering
- **License management:** Activation and validation (the *only* network calls in the entire app)

Every command runs in the same process as the application. There's no HTTP overhead, no serialization to JSON over a wire, no latency from network hops. A scan command that might take 200ms over a REST API completes in single-digit milliseconds locally.

---

## The Dual-Layer PII Detection Engine

The core of Qvault is its PII detection pipeline, which runs two complementary scanners in sequence.

### Layer 1: Pattern-Based Regex Scanner

The first layer uses compiled regular expressions to detect structured PII — data that follows a known format. The scanner contains patterns organized across five jurisdictions:

**Global patterns** cover universally formatted data:
- Email addresses (RFC-compliant matching)
- Credit card numbers (Visa, Mastercard, Amex, Diners Club formats with Luhn-compatible digit groups)
- IBAN numbers (international bank account format)
- IPv4 addresses (with valid octet range validation)
- URLs (HTTP/HTTPS with full path and query string support)
- Monetary amounts (multi-currency: USD, EUR, GBP, BRL, R$)
- Dates (multiple formats: "January 15, 2024" and "01/15/2024")
- Phone numbers (international and US domestic formats)

**US-specific patterns** detect Social Security Numbers and Employer Identification Numbers (EINs).

**EU patterns** match VAT identification numbers across member states.

**Brazil patterns** detect CPF (individual tax ID) and CNPJ (corporate tax ID) numbers.

**Germany patterns** match Steuernummer (tax identification) formats.

Every regex pattern is compiled once at initialization and reused across all document scans. Rust's `regex` crate compiles patterns to a highly optimized finite automaton — there's no backtracking, no catastrophic regex performance. Each pattern match is assigned a confidence score of 0.95.

The scanner also implements overlap prevention: when a high-priority pattern matches a region of text, lower-priority patterns skip that region entirely. This prevents false double-detections (for example, a phone number inside a URL).

### Layer 2: Heuristic Context Scanner

Structured patterns only catch PII that follows a predictable format. But some of the most sensitive information in legal documents — **people's names and company names** — doesn't follow any fixed pattern.

The heuristic scanner addresses this with six detection passes, each using different contextual signals:

1. **Company names with legal suffixes.** Sequences of capitalized words followed by entity types (LLC, Inc, Corp, GmbH, SA, SARL, and 11 others). This catches "Acme Holdings LLC" and "Schmidt & Weber GmbH" alike.

2. **Names from distribution tables.** Legal documents frequently contain shareholder or ownership tables formatted as "Entity Name — XX.XXXX%". The scanner recognizes this pattern.

3. **Names from entity descriptions.** Phrases like "...of Meridian Partners, a Delaware limited liability company..." contain entity names that can be extracted using surrounding legal language as context.

4. **Person names with titles.** "John Smith, Manager" or "Maria da Silva, Director" — the scanner matches capitalized name sequences followed by common legal titles and roles. It supports international name connectors: de, da, do, dos, das, van, von, del, di, el, la, le.

5. **Person names in distribution contexts.** Similar to #2, but for individual names rather than companies.

6. **General capitalized sequences.** As a catch-all, consecutive capitalized words are evaluated with positive context signals — nearby terms like "Member", "Shareholder", "signed by", "DocuSigned", "Party", or "Witness" boost confidence.

To avoid false positives, the scanner maintains a curated list of **25 stop phrases** (legal terms like "Amendment No", "Limited Liability", "Social Security Number") and **71 stop words** (common English and legal vocabulary like "Agreement", "Section", "Federal"). Any match consisting entirely of stop words is discarded.

Confidence scores for heuristic matches range from 0.75 to 0.92, depending on the detection pass and contextual evidence.

### Why Two Layers?

A single-layer approach would miss too much. Regex alone can't detect names. Machine learning alone is slow, requires large models, and often needs cloud inference. Qvault's dual-layer approach combines the precision of pattern matching with the flexibility of contextual analysis — all running locally, all in milliseconds.

The two layers communicate through a shared matched-ranges tracker. When the regex scanner claims a text region, the heuristic scanner skips it. This prevents the heuristic layer from incorrectly classifying a phone number as a name, for example.

---

## Document Processing Pipeline

### Extraction

Qvault supports two document formats, each with its own extraction engine:

**PDF extraction** uses `lopdf`, a pure-Rust PDF library. It parses the PDF's internal object tree and extracts text content per page, preserving page boundaries. On the frontend, PDF.js (Mozilla's JavaScript PDF renderer) independently extracts text spans with precise coordinate data — position, width, and height for every word on every page. This dual extraction gives Qvault both the raw text (for scanning) and the spatial layout (for overlay rendering and export).

**DOCX extraction** treats the file as what it actually is: a ZIP archive containing XML. Qvault opens the archive, locates `word/document.xml`, and parses it with `quick-xml`. It walks the XML tree, collecting text from `<w:t>` elements and inserting newlines at `<w:p>` paragraph boundaries. The result is clean, structured text ready for scanning.

### Encryption at Rest

Every document that enters Qvault is encrypted before it touches the disk. The encryption scheme uses **AES-256-GCM** (Galois/Counter Mode), which provides both confidentiality and authentication in a single pass.

The encryption key is derived from a SHA-256 hash of a machine-bound seed: `qvault-encryption-{hostname}`. This means the encrypted files can only be decrypted on the machine that created them. There's no master password to forget, no key file to lose, and no key server to compromise.

Each file is encrypted with a fresh 96-bit random nonce generated from the OS's cryptographic random number generator. The file format is simple: 12 bytes of nonce, followed by the ciphertext with an appended 16-byte authentication tag. The authentication tag ensures that any tampering with the encrypted file — even a single flipped bit — is detected during decryption.

This is not a "nice to have" feature. If someone gains access to a lawyer's laptop, Qvault's documents are encrypted with a key that exists only in memory during runtime. The files on disk are opaque binary blobs.

### Storage

All metadata lives in a local **SQLite database** running in WAL (Write-Ahead Logging) mode for concurrent read/write performance. The schema consists of seven tables:

- **documents** — File metadata, SHA-256 hash (for deduplication), processing status, PII count
- **redactions** — Individual PII findings with character offsets, category, confidence score, detection layer, and review status
- **entities** — Cross-document PII knowledge base with normalized text and indexed lookups
- **document_pages** — Full page text (raw and cleaned versions) for reconstruction
- **license** — Subscription state with offline grace period tracking
- **audit_log** — Every action recorded for compliance: uploads, scans, approvals, exports
- **credits** — Legacy table from early development

The entities table is particularly noteworthy. When Qvault detects "John Smith" in one document, it normalizes and stores that entity. When processing subsequent documents, the system has a growing knowledge base of known PII. This cross-document intelligence means detection accuracy improves over time without any cloud-based learning.

---

## The Frontend Rendering Pipeline

Rendering a PDF with redaction overlays in real time is a non-trivial engineering challenge. Qvault's `DocumentViewer` component (over 1,300 lines of React/TypeScript) manages a multi-layer rendering stack:

1. **PDF canvas layer.** PDF.js renders each page to an HTML5 canvas at 1.5x viewport scale for crisp text on high-DPI displays.

2. **Text span extraction.** PDF.js's `getTextContent()` API returns every text fragment with its position in PDF coordinate space. Qvault transforms these coordinates to viewport space using the page's transformation matrix, creating a `TextSpan` array with both coordinate systems preserved.

3. **Redaction overlay layer.** Each PII detection is mapped from character offsets to spatial coordinates via the text span array. Colored overlays are rendered on top of the PDF canvas — color-coded by PII category with configurable opacity. Pending redactions show at 19% opacity; approved redactions at 33%.

4. **Selection layer.** In advanced mode, a transparent interaction layer sits above the text, allowing users to select arbitrary text regions and create manual redactions. Selection triggers a popup with the selected text and redaction controls.

5. **Export coordinate mapping.** When the user exports a clean document, the frontend translates viewport coordinates back to PDF coordinate space. These coordinates are sent to the Rust backend, which uses `lopdf` to inject black rectangle drawing operators into the PDF's content streams — permanently redacting the selected regions.

This entire pipeline runs locally. The PDF never leaves the app. The coordinates never leave the app. The redacted export is written directly to the local filesystem.

---

## Performance Characteristics

Qvault is designed to feel instant. Here's what makes that possible:

**Compiled regex patterns.** Rust's regex crate compiles patterns to deterministic finite automata (DFA). There's no backtracking, and matching is linear in the length of the input text. A typical legal document (50–100 pages) is scanned in under 100ms.

**Single-process IPC.** Tauri's command system is an in-process function call, not a network request. The serialization overhead is minimal — a few microseconds for typical payloads.

**SQLite WAL mode.** Write-Ahead Logging allows concurrent readers and writers. The frontend can query redaction data while the backend is still processing new pages.

**Streaming page processing.** Pages are scanned individually and results are stored incrementally. The user sees PII detections appearing in real time as pages are processed, rather than waiting for the entire document to finish.

**Native webview.** By using the OS's built-in webview instead of bundling Chromium, Qvault's installed size is roughly 15–20 MB — compared to 150+ MB for a typical Electron app. Startup time is under a second on modern hardware.

**Zero network latency.** This is the biggest performance advantage of all. Cloud-based redaction tools require uploading the document (seconds to minutes depending on size and connection), waiting for server processing (variable), and downloading results. Qvault's processing time is purely CPU-bound, measured in milliseconds.

---

## What Qvault Does NOT Do

Understanding what Qvault explicitly avoids is as important as understanding what it does:

- **No cloud uploads.** Documents are never transmitted. The only network calls are license activation and periodic license validation — and these send zero document data. Just a license key and machine ID.
- **No telemetry.** There is no usage tracking, no analytics, no crash reporting that includes document content.
- **No third-party processing.** There is no OpenAI API call, no AWS Textract, no Google Cloud Vision. Every detection algorithm runs locally.
- **No external model downloads.** The detection engine is pure compiled code — regex patterns and heuristic rules. There are no model weights to download, no inference servers to spin up.
- **No temporary cloud storage.** There is no "we encrypt it in transit and delete it after processing" hand-waving. The data physically cannot leave because the code that processes it has no network capability.

---

## Cross-Platform Distribution

Qvault runs natively on macOS (Apple Silicon and Intel), Windows, and Linux. Each platform gets a native installer:

- **macOS:** `.dmg` disk images for both ARM64 and x86_64 architectures
- **Windows:** `.exe` (NSIS installer) and `.msi` packages with multi-language support (English, Spanish, Portuguese)
- **Linux:** `.deb` packages for Debian/Ubuntu and `.AppImage` for universal compatibility

All builds are produced by a single GitHub Actions workflow using a matrix strategy — four parallel jobs compile the Rust backend and bundle the frontend for each target platform. The build artifacts are attached to GitHub Releases, ready for download.

---

## Jurisdictional Coverage

Legal work is increasingly international. A firm in Barcelona might handle documents containing US Social Security Numbers, Brazilian CPFs, and German tax IDs — all in the same case.

Qvault's pattern library covers five jurisdictional categories:

| Jurisdiction | PII Types |
|---|---|
| **Global** | Email, credit cards, IBAN, IP addresses, URLs, monetary amounts, dates, phone numbers |
| **United States** | Social Security Numbers (SSN), Employer Identification Numbers (EIN) |
| **European Union** | VAT identification numbers (multi-country format) |
| **Brazil** | CPF (individual tax ID), CNPJ (corporate tax ID) |
| **Germany** | Steuernummer (tax identification number) |

Combined with the heuristic scanner's ability to detect names and company entities regardless of language or jurisdiction, Qvault provides broad coverage for international legal practices.

---

## The Trust Model

Qvault's security model is simple: **trust the machine, distrust the network.**

The application assumes that the user's device is a trusted environment — it's their laptop, their file system, their responsibility to secure physical access. Within that trusted perimeter, Qvault adds defense-in-depth:

- **Encryption at rest** ensures that even if the file system is compromised, document content is protected by AES-256-GCM.
- **Machine-bound keys** prevent encrypted files from being decrypted on another device.
- **Audit logging** records every action for compliance review.
- **No network surface** eliminates an entire class of attacks — MITM, data exfiltration, API key leakage, server breaches.

For legal professionals operating under strict confidentiality obligations, this model provides something cloud tools fundamentally cannot: **mathematical certainty that client data never left the building.**

---

## Conclusion

Qvault exists because we believe sensitive document processing shouldn't require trusting a third party with your data. By combining Rust's performance with a local-first architecture, we've built a tool that detects and redacts PII across multiple jurisdictions, supports PDF and DOCX formats, tracks entities across documents, and produces clean exports — all without a single byte of document data ever touching a network interface.

The legal industry is built on trust. Qvault is built to make that trust verifiable.

---

*Qvault is developed by [Santacroce SL](https://santacroce.es), Barcelona, Spain. Learn more at [qvault.tech](https://qvault.tech).*
