# Qvault

Local-first PII detection and redaction for lawyers. Scan PDF and DOCX documents for personally identifiable information, review flagged entities, and export clean redacted copies — all without any data leaving your machine.

## Features

- **PDF & DOCX Support** — Upload documents via drag-and-drop or file dialog
- **PDF Rendering** — Real PDF preview powered by PDF.js (not a text dump)
- **Multi-layer PII Detection** — Regex patterns (emails, SSNs, credit cards, dates, money) + heuristic name detection tuned for legal documents
- **Jurisdiction-aware Scanning** — US, EU, BR, DE pattern sets with global defaults
- **Advanced Mode** — Manually select text to add custom redactions
- **Redacted PDF Export** — Black-box redactions drawn directly on the PDF
- **AES-256-GCM Encryption** — All stored documents encrypted at rest
- **SQLite Storage** — Local database for documents, redactions, credits, and audit log
- **Free Tier** — 3 documents per month, no account required
- **Zero Data Transmission** — Everything runs locally on your device

## Tech Stack

| Layer    | Technology                                      |
|----------|------------------------------------------------|
| Frontend | React 18, TypeScript, Tailwind CSS, PDF.js 4.x |
| Backend  | Rust, Tauri v2                                  |
| Storage  | SQLite (rusqlite), AES-256-GCM encryption       |
| PDF      | lopdf (redaction export), PDF.js (rendering)    |
| DOCX     | quick-xml + zip                                 |

## Prerequisites

- **Node.js** >= 20 (tested with v24.14.0)
- **Rust** >= 1.75 (tested with cargo 1.94.0)
- **Tauri CLI** v2

### macOS

```bash
xcode-select --install
```

### Linux (Debian/Ubuntu)

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

### Windows

Install [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).

## Setup

```bash
# Clone the repository
git clone https://github.com/your-org/qvault.git
cd qvault

# Install frontend dependencies
npm install

# Install Tauri CLI (if not already installed)
cargo install tauri-cli@^2
```

## Development

```bash
# Start the app in development mode (hot-reload frontend + Rust backend)
npx tauri dev
```

This starts the Vite dev server on `localhost:1420` and launches the Tauri window. Frontend changes hot-reload; Rust changes trigger a recompile.

### Run Rust Tests

```bash
# Run the PII pipeline integration tests
cargo test --manifest-path src-tauri/Cargo.toml --test pipeline_test

# Run with output visible
cargo test --manifest-path src-tauri/Cargo.toml --test pipeline_test -- --nocapture

# Run all Rust tests
cargo test --manifest-path src-tauri/Cargo.toml
```

### Type Check Frontend

```bash
npx tsc --noEmit
```

## Build for Production

```bash
# Build a release binary for your current platform
npx tauri build
```

Output location varies by platform:

| Platform | Output Path |
|----------|------------|
| macOS    | `src-tauri/target/release/bundle/dmg/Qvault_0.1.0_aarch64.dmg` |
| Windows  | `src-tauri/target/release/bundle/msi/Qvault_0.1.0_x64_en-US.msi` |
| Linux    | `src-tauri/target/release/bundle/deb/qvault_0.1.0_amd64.deb` |

### Build Options

```bash
# Build only the app binary (no installer)
npx tauri build --no-bundle

# Build a specific bundle format
npx tauri build --bundles dmg        # macOS disk image
npx tauri build --bundles app        # macOS .app
npx tauri build --bundles msi        # Windows installer
npx tauri build --bundles nsis       # Windows NSIS installer
npx tauri build --bundles deb        # Debian package
npx tauri build --bundles appimage   # Linux AppImage

# Debug build (faster compile, larger binary)
npx tauri build --debug
```

## Project Structure

```
qvault/
  src/                          # React frontend
    App.tsx                     # Main app with view routing
    components/
      DocumentViewer.tsx        # PDF.js rendering + redaction overlays
      QueueDashboard.tsx        # Upload view with drag-and-drop
      RedactionSidebar.tsx      # Approve/reject/edit redactions
      HistoryView.tsx           # Document history
      SettingsView.tsx          # App settings
    lib/
      tauri-commands.ts         # Typed IPC wrappers
      types.ts                  # Shared TypeScript types
  src-tauri/                    # Rust backend
    src/
      lib.rs                    # Tauri app setup, command registration
      commands/
        document.rs             # Upload, scan, export IPC handlers
      pipeline/
        mod.rs                  # Pipeline orchestration (regex + heuristic)
        regex_scanner.rs        # Priority-based regex PII matching
        heuristic_scanner.rs    # Name detection for legal documents
        patterns/               # Jurisdiction-specific regex patterns
          global.rs             # Email, phone, credit card, date, money
          us.rs                 # SSN, EIN
          eu.rs                 # EU Tax ID
          br.rs                 # CPF, CNPJ
          de.rs                 # German Tax ID
        types.rs                # PiiEntity, PiiCategory enums
      storage/
        database.rs             # SQLite schema & CRUD
        encryption.rs           # AES-256-GCM file encryption
      document/
        redactor.rs             # Black-box PDF redaction export
    tests/
      pipeline_test.rs          # 18 integration tests
    Cargo.toml
    tauri.conf.json
```

## Troubleshooting

### Reset App Data

If you encounter issues with stored documents or database state:

```bash
# macOS
rm -rf ~/Library/Application\ Support/io.qvault.app/

# Linux
rm -rf ~/.local/share/io.qvault.app/

# Windows
rmdir /s %APPDATA%\io.qvault.app
```

### Common Issues

- **PDF not rendering**: Ensure `pdfjs-dist` is v4.x (v5 has a `ReadableStream` bug in Tauri's WebView)
- **Drag-and-drop not working**: Check that `dialog:default`, `dialog:allow-open`, `fs:default` are in `src-tauri/capabilities/default.json`
- **0 PII detected**: The app uses frontend-side PDF.js text extraction. Scanned/image-only PDFs will not yield text for analysis
- **Rust compile errors**: Run `cargo clean --manifest-path src-tauri/Cargo.toml` and rebuild

## License

Proprietary. All rights reserved.
