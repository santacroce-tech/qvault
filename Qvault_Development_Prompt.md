# QVAULT — Development Prompt

> **Paste this entire prompt into your LLM to start building Qvault. It contains the complete technical specification, project structure, and implementation details.**

---

## WHAT YOU ARE BUILDING

Qvault is a cross-platform desktop application (Windows + macOS) that allows lawyers to automatically detect and redact personally identifiable information (PII) from documents before sharing them. The entire processing pipeline runs locally — no document data ever leaves the user's computer.

**Tech Stack:** Tauri v2 (Rust backend) + React 18 + TypeScript + Tailwind CSS frontend.

**Core Principle:** Zero data transmission. The only network calls are license key validation (sends a token, never document content) and auto-update checks.

---

## PROJECT STRUCTURE

```
qvault/
├── src-tauri/                    # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── tauri.appstore.conf.json  # Mac App Store overrides
│   ├── tauri.msstore.conf.json   # Microsoft Store overrides
│   ├── Entitlements.plist        # macOS sandbox entitlements
│   ├── icons/
│   ├── src/
│   │   ├── main.rs
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── document.rs       # Upload, parse, export
│   │   │   ├── redaction.rs      # PII pipeline, manage redactions
│   │   │   ├── history.rs        # Document history CRUD
│   │   │   ├── license.rs        # License validation, credits
│   │   │   └── settings.rs
│   │   ├── pipeline/             # PII detection engine
│   │   │   ├── mod.rs
│   │   │   ├── regex_scanner.rs  # Layer 1: regex patterns
│   │   │   ├── ner_engine.rs     # Layer 2: ONNX NER model
│   │   │   ├── heuristics.rs     # Layer 3: contextual rules
│   │   │   ├── patterns/         # Jurisdiction-specific regex
│   │   │   │   ├── global.rs     # Email, URL, phone, credit card, IBAN
│   │   │   │   ├── us.rs         # SSN, EIN, state IDs
│   │   │   │   ├── eu.rs         # IBAN, VAT, national IDs
│   │   │   │   ├── br.rs         # CPF, CNPJ, RG
│   │   │   │   └── de.rs         # Personalausweis, Steuer-ID
│   │   │   └── types.rs          # PiiEntity, PiiCategory, Redaction
│   │   ├── document/
│   │   │   ├── mod.rs
│   │   │   ├── pdf_parser.rs
│   │   │   ├── docx_parser.rs
│   │   │   └── redactor.rs       # Apply redactions, produce clean output
│   │   ├── storage/
│   │   │   ├── mod.rs
│   │   │   ├── database.rs       # SQLite schema, migrations
│   │   │   └── encryption.rs     # AES-256-GCM file encryption
│   │   └── licensing/
│   │       ├── mod.rs
│   │       ├── keygen_client.rs   # License API client
│   │       └── credits.rs         # Credit balance management
│   └── models/
│       └── ner-model.onnx        # NER model (~150MB)
├── src/                          # React frontend
│   ├── App.tsx
│   ├── main.tsx
│   ├── components/
│   │   ├── DocumentViewer.tsx     # PDF renderer with redaction overlays
│   │   ├── RedactionSidebar.tsx   # Approve/reject/edit PII entities
│   │   ├── QueueDashboard.tsx
│   │   ├── HistoryView.tsx
│   │   ├── CreditBalance.tsx
│   │   ├── LicenseActivation.tsx
│   │   └── Settings.tsx
│   ├── hooks/
│   │   ├── useDocument.ts
│   │   ├── useRedactions.ts
│   │   └── useCredits.ts
│   └── lib/
│       ├── tauri-commands.ts      # Typed IPC wrappers
│       └── types.ts
├── package.json
├── tsconfig.json
├── tailwind.config.js
└── vite.config.ts
```

---

## PII DETECTION PIPELINE

Three layers executed sequentially. Each adds to a shared `Vec<PiiEntity>`. Overlaps merged by highest confidence.

### Layer 1: Regex Scanner (Rust `regex` crate)

Deterministic, <1ms per page. Curated patterns per jurisdiction.

**PiiCategory enum:**
PersonName, Email, PhoneNumber, Address, DateOfBirth, SocialSecurityNumber, NationalId, PassportNumber, DriversLicense, BankAccount, CreditCard, IBAN, TaxId, CaseNumber, CourtFileNumber, ClientId, IpAddress, Url, LicensePlate, MedicalRecordNumber

**Key patterns:**
```rust
// Global
EMAIL: r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}"
PHONE_INTL: r"\+?[1-9]\d{1,14}"
CREDIT_CARD: r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13})\b"
IBAN: r"\b[A-Z]{2}\d{2}[A-Z0-9]{4,30}\b"
IP_V4: r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b"

// US
SSN: r"\b\d{3}-\d{2}-\d{4}\b"
EIN: r"\b\d{2}-\d{7}\b"

// Brazil
CPF: r"\b\d{3}\.\d{3}\.\d{3}-\d{2}\b"
CNPJ: r"\b\d{2}\.\d{3}\.\d{3}/\d{4}-\d{2}\b"

// EU/DE
DE_TAX_ID: r"\b\d{2}\s?\d{3}\s?\d{5}\b"
```

Each pattern produces: `PiiEntity { text, start, end, page, category, layer: "regex", confidence: 0.95 }`

### Layer 2: NER Model (ONNX Runtime)

Export a spaCy or GLiNER NER model to ONNX. Run via the `ort` crate in Rust. Catches: PersonName, Organization, Location, Date, MonetaryAmount in natural text. Model file ~150MB, bundled or downloaded on first launch.

```rust
// Load once at startup
let session = ort::Session::builder()?.commit_from_file("models/ner-model.onnx")?;

// For each page of text:
let tokens = tokenize(&text);
let outputs = session.run(ort::inputs![token_ids, attention_mask]?)?;
let entities = decode_ner_output(&outputs, &tokens, &text);
```

### Layer 3: Heuristic Rules

Configurable rules that operate on combined Layer 1+2 output:

- **Proximity rule:** PersonName + Address within 3 sentences → flag paragraph
- **Date + Location:** Specific date + location + personal descriptor → flag
- **Quasi-identifiers:** 3+ of (age, gender, zip, profession) in same paragraph → flag
- **Metadata stripping:** Always redact PDF/DOCX metadata (author, company, dates)

### Pipeline Output

```rust
struct PiiEntity {
    id: String,           // UUID
    text: String,         // Original detected text
    start: usize,         // Character offset start
    end: usize,           // Character offset end
    page: u32,
    category: PiiCategory,
    layer: String,        // "regex" | "ner" | "heuristic"
    confidence: f64,      // 0.0 - 1.0
    replacement: String,  // e.g., "[REDACTED NAME]"
}
```

---

## DOCUMENT PROCESSING

**Supported:** PDF (text-based), DOCX. Future: TXT, RTF, scanned PDF (OCR).

**Flow:** Upload → copy to encrypted local storage → extract text per page → run pipeline → store results in SQLite → render in frontend with highlights → lawyer reviews → generate clean version.

**Clean version:** For PDF: use `lopdf` to replace text at redacted spans. For DOCX: modify XML text nodes. Save alongside original, both encrypted.

---

## LICENSE & CREDIT SYSTEM

### Freemium Model
- **Free:** 3 docs/month, no account needed. Local counter in SQLite (resets by month-year key).
- **Credit packs (via Stripe):** Starter (10 docs/€5), Pro (50/€19), Business (200/€59), Firm (1000/€199).
- **License key:** `QVLT-XXXX-XXXX-XXXX`, entered in-app, validated via HTTPS.

### Validation Flow
```
1. User enters key in app
2. App POSTs to api.qvault.io/v1/licenses/validate
   Body: { key: "QVLT-...", machine_fingerprint: "<sha256-hash>" }
3. Response: { valid: true, credits: 50, plan: "pro" }
4. Stored locally in encrypted SQLite
5. Credits decremented locally per document
6. Syncs with server every 24h or on launch
7. Works offline using local balance
```

**The API NEVER receives:** document text, filenames, PII data, redaction details. Only: license key, machine fingerprint (hashed, irreversible), credit balance, app version.

### Machine Fingerprint
SHA-256 hash of: OS type + CPU model + machine serial. Via `machine-uid` crate. Each license: up to 3 machines.

### Server-Side API Endpoints
```
POST /v1/licenses/validate    — Validate key, return credits
POST /v1/licenses/activate    — Activate on new machine
POST /v1/licenses/deactivate  — Remove machine
POST /v1/credits/sync         — Sync balance
POST /v1/credits/purchase     — Stripe webhook, create/top-up license
```

### Stripe Flow
User buys pack on qvault.io → Stripe Checkout → webhook → API creates/tops-up license → key emailed → user activates in app.

---

## AUTO-UPDATE SYSTEM

Use Tauri's built-in updater plugin (`@tauri-apps/plugin-updater`).

**Update manifest URL:** `https://releases.qvault.io/v1/update/{{target}}/{{arch}}/{{current_version}}`

**Flow:** Check on launch + every 6h → notify user → download in background → prompt restart → apply. Rollback on failure.

**Signing:** Generate keypair via `tauri signer generate`. Public key in `tauri.conf.json`, private key in CI/CD as `TAURI_SIGNING_PRIVATE_KEY`.

**Model updates:** Bundled with app updates for MVP. Separate model update channel in Phase 2.

---

## STORE DISTRIBUTION

### Microsoft Store
- Build as EXE/MSI → list as "EXE or MSI app"
- Offline WebView2 installer for Store builds
- EV code signing certificate (~$400/year)
- Microsoft Developer account ($19 individual / $99 company)

### Apple App Store
- App Sandbox entitlements in Entitlements.plist
- Universal binary: `--target universal-apple-darwin`
- Apple Distribution certificate + provisioning profile
- Package as .pkg, upload via xcrun altool
- Apple Developer Program ($99/year)

### Direct Download
- Windows: NSIS .exe (code-signed)
- macOS: DMG (code-signed + notarized)
- Linux: AppImage + .deb
- All include auto-update via Tauri updater

---

## SQLITE SCHEMA

```sql
CREATE TABLE documents (
  id TEXT PRIMARY KEY,
  filename TEXT NOT NULL,
  file_type TEXT NOT NULL,        -- 'pdf' | 'docx'
  file_hash TEXT NOT NULL,        -- SHA-256 of original
  page_count INTEGER,
  original_path TEXT NOT NULL,    -- Encrypted file path
  clean_path TEXT,
  status TEXT NOT NULL,           -- 'processing'|'review'|'complete'
  pii_count INTEGER DEFAULT 0,
  created_at TEXT NOT NULL,
  completed_at TEXT
);

CREATE TABLE redactions (
  id TEXT PRIMARY KEY,
  document_id TEXT REFERENCES documents(id),
  page INTEGER NOT NULL,
  start_offset INTEGER NOT NULL,
  end_offset INTEGER NOT NULL,
  original_text TEXT NOT NULL,
  replacement TEXT NOT NULL,
  category TEXT NOT NULL,
  detection_layer TEXT NOT NULL,  -- 'regex'|'ner'|'heuristic'
  confidence REAL NOT NULL,
  status TEXT NOT NULL,           -- 'pending'|'approved'|'rejected'
  reviewed_at TEXT
);

CREATE TABLE credits (
  id TEXT PRIMARY KEY,
  license_key TEXT,
  balance INTEGER NOT NULL DEFAULT 3,
  free_remaining INTEGER NOT NULL DEFAULT 3,
  free_month TEXT NOT NULL,       -- '2026-03'
  last_synced_at TEXT
);

CREATE TABLE audit_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  document_id TEXT,
  action TEXT NOT NULL,
  details TEXT,                   -- JSON
  created_at TEXT NOT NULL
);
```

---

## FRONTEND SPECIFICATION

### Design System
- Dark navy sidebar (#0F1B2D), light content area (#F9FAFB)
- PII color coding: names (#3B82F6 blue), addresses (#10B981 green), dates (#F59E0B orange), financial (#EF4444 red), IDs (#8B5CF6 purple), heuristic flags (#F59E0B dashed)
- Tailwind CSS. Fonts: Inter (body), JetBrains Mono (technical).

### Main Views
1. **Upload/Home:** Drag-and-drop zone, free docs counter, recent documents
2. **Document Review (CORE):** Split layout — document with highlights (70%) + redaction sidebar (30%). Click highlight → select in sidebar. Actions: Approve, Reject, Edit. Toolbar: Approve All, Export Clean.
3. **History:** Searchable table of processed docs. Columns: filename, date, PII count, status, actions.
4. **Settings:** Jurisdiction picker, sensitivity slider, license management, data retention.

### Tauri IPC Commands
```typescript
// Documents
invoke('upload_document', { filePath: string }): Promise<string>  // returns doc ID
invoke('get_document', { id: string }): Promise<Document>
invoke('get_document_text', { id: string, page: number }): Promise<string>
invoke('export_clean', { id: string }): Promise<string>
invoke('delete_document', { id: string }): Promise<void>

// Redactions
invoke('get_redactions', { documentId: string }): Promise<Redaction[]>
invoke('update_redaction', { id: string, status: string, replacement?: string }): Promise<void>
invoke('bulk_approve', { documentId: string, category?: string }): Promise<void>

// License
invoke('activate_license', { key: string }): Promise<LicenseInfo>
invoke('get_credits'): Promise<CreditInfo>
invoke('deactivate_license'): Promise<void>

// History
invoke('get_history', { offset: number, limit: number }): Promise<Document[]>
invoke('search_history', { query: string }): Promise<Document[]>
```

---

## KEY RUST DEPENDENCIES

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon", "dialog"] }
tauri-plugin-updater = "2"
tauri-plugin-dialog = "2"
tauri-plugin-fs = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
rusqlite = { version = "0.31", features = ["bundled"] }
ort = "2"                     # ONNX Runtime
regex = "1"
lopdf = "0.32"                # PDF parsing
quick-xml = "0.31"            # DOCX XML
zip = "0.6"                   # DOCX ZIP
aes-gcm = "0.10"              # Encryption
ring = "0.17"                 # Hashing
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
keyring = "2"                 # OS keychain
machine-uid = "0.5"           # Hardware fingerprint
```

---

## DEVELOPMENT PHASES

**Phase 1 (Weeks 1-4):** Tauri project setup, PDF/DOCX extraction, regex scanner (Layer 1), SQLite + encryption, Upload view, basic Review view, free tier (3 docs/month).

**Phase 2 (Weeks 5-8):** ONNX NER integration, heuristic rules, full review workflow, clean version generation, History view.

**Phase 3 (Weeks 9-12):** Licensing API + Stripe, license activation in-app, code signing (Windows EV + Apple), CI/CD, Microsoft Store + App Store submission, auto-updates. Ship v1.0.

**Phase 4 (Weeks 13-16):** More jurisdiction patterns (BR, DE, FR, UK), NER fine-tuning, sensitivity slider, bulk processing, marketing site.

---

## START HERE

Begin with Phase 1. Create the Tauri v2 project, set up the React frontend with Tailwind, implement PDF text extraction in Rust, and build the regex scanner with global patterns. The first milestone is: user drags a PDF into the app, sees it rendered with email addresses and phone numbers highlighted in color.
