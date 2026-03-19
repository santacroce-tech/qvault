# Qvault License System

## Overview

Qvault uses a **yearly license key** model with **server-side validation**. Every time the app starts, it phones home to verify the license is still valid. No document processing limits — a valid license means unlimited usage for the subscription period.

## How It Works

```
┌──────────┐      activate       ┌──────────────┐     POST /validate     ┌──────────────┐
│  Qvault  │ ──────────────────→ │  License API │ ──────────────────────→│   Database    │
│  Client  │ ←────────────────── │   Server     │ ←──────────────────────│  (your DB)    │
│          │   { valid, expiry } │              │   license row lookup   │              │
└──────────┘                     └──────────────┘                        └──────────────┘
     │                                  ↑
     │  on every app startup            │
     └──────────────────────────────────┘
            POST /validate
```

### Activation Flow

1. User purchases a license and receives a key: `QVLT-XXXX-XXXX-XXXX`
2. User enters the key in **Settings → License**
3. Client sends `POST /api/license/activate` to the license server
4. Server verifies the key exists, is unused, and marks it as activated
5. Server returns `{ valid: true, expires_at: "2027-03-19T..." }`
6. Client stores the license locally in SQLite for offline grace period

### Startup Validation Flow

1. App opens → reads stored license from local SQLite
2. If a license key exists, sends `POST /api/license/validate` to the server
3. Server checks: key exists, is active, not expired, not revoked
4. **Server responds valid** → app runs normally, updates local expiry
5. **Server responds invalid** → license is deactivated locally, upload blocked
6. **Server unreachable** → offline grace period (7 days from last successful check)

### Offline Grace Period

If the user has no internet, the app still works for **7 days** after the last successful server validation. After 7 days offline, upload is blocked until the app can reach the server again. This prevents users from simply blocking the validation URL.

## Key Format

```
QVLT-XXXX-XXXX-XXXX

Prefix:  QVLT (fixed, identifies the product)
Segment: 4 alphanumeric characters each (A-Z, 0-9)
Total:   19 characters including dashes
```

Examples:
- `QVLT-A1B2-C3D4-E5F6`
- `QVLT-RJKM-49TX-WQLN`

## Generating License Keys

### Using the Generator Script

A Node.js script is included at `tools/generate-license.js`:

```bash
# Generate a single key
node tools/generate-license.js

# Generate multiple keys
node tools/generate-license.js 10

# Generate and save to file
node tools/generate-license.js 50 > keys.txt
```

Output:
```
QVLT-R7KM-49TX-WQLN
```

### Key Generation Algorithm

Keys are generated using `crypto.randomBytes()` (cryptographically secure random). Each segment is 4 characters from the charset `ABCDEFGHJKLMNPQRSTUVWXYZ23456789` (no 0/O/1/I to avoid visual confusion).

There is no checksum or embedded data in the key — validation is purely server-side. This means:
- Keys cannot be "cracked" by reverse-engineering a checksum algorithm
- A brute-force attack would need to guess from 32^12 = ~1.15 quadrillion combinations
- All validation logic lives on your server, not in the client binary

## License Server API

You need to deploy a simple REST API. Below is the contract — implement it in whatever stack you prefer (Node.js, Python, Go, Cloudflare Workers, etc.).

### `POST /api/license/activate`

Called when the user enters a key for the first time.

**Request:**
```json
{
  "license_key": "QVLT-R7KM-49TX-WQLN",
  "machine_id": "a1b2c3d4e5f6..."
}
```

**Success Response (200):**
```json
{
  "valid": true,
  "license_key": "QVLT-R7KM-49TX-WQLN",
  "expires_at": "2027-03-19T00:00:00Z",
  "plan": "yearly"
}
```

**Error Response (403):**
```json
{
  "valid": false,
  "error": "License key not found"
}
```

Possible errors:
- `"License key not found"` — key doesn't exist in your database
- `"License key already activated"` — key bound to a different machine
- `"License key expired"` — key past its expiry date
- `"License key revoked"` — manually revoked by admin

### `POST /api/license/validate`

Called on every app startup to verify the license is still valid.

**Request:**
```json
{
  "license_key": "QVLT-R7KM-49TX-WQLN",
  "machine_id": "a1b2c3d4e5f6..."
}
```

**Success Response (200):**
```json
{
  "valid": true,
  "expires_at": "2027-03-19T00:00:00Z"
}
```

**Failure Response (200, valid=false):**
```json
{
  "valid": false,
  "error": "License key revoked"
}
```

### `POST /api/license/deactivate`

Called when user deactivates their license (allows re-activation on another machine).

**Request:**
```json
{
  "license_key": "QVLT-R7KM-49TX-WQLN",
  "machine_id": "a1b2c3d4e5f6..."
}
```

**Response (200):**
```json
{
  "deactivated": true
}
```

## Server Database Schema

Minimal schema for the license server:

```sql
CREATE TABLE licenses (
  id            TEXT PRIMARY KEY,
  license_key   TEXT UNIQUE NOT NULL,
  plan          TEXT NOT NULL DEFAULT 'yearly',
  status        TEXT NOT NULL DEFAULT 'available',  -- available, active, expired, revoked
  machine_id    TEXT,                                -- bound on activation
  activated_at  TIMESTAMP,
  expires_at    TIMESTAMP,
  created_at    TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Index for fast lookups
CREATE INDEX idx_license_key ON licenses(license_key);
```

Status lifecycle:
```
available → active → expired
              ↓
           revoked
              ↓
          available  (if you allow re-issue)
```

## Configuration

The license server URL is configured in the Qvault client:

```rust
// src-tauri/src/commands/license.rs
const LICENSE_SERVER_URL: &str = "https://api.qvault.io";
```

For development, you can:
- Set it to `http://localhost:3000` and run a local mock server
- Or skip validation entirely — in dev builds (`npx tauri dev`), the license check on upload is bypassed via `#[cfg(not(debug_assertions))]`

## Security Considerations

### What this system prevents
- **Key sharing**: Each key is bound to a `machine_id` on first activation
- **Clock manipulation**: Expiry is checked server-side, not just locally
- **Network blocking**: 7-day offline grace period, then the app stops working
- **Binary patching**: Server validation means patching the client binary alone isn't enough — the server must return `valid: true`

### What this system does NOT prevent
- **Determined reverse engineering**: A skilled attacker could patch out the validation call entirely. No client-side DRM is unbreakable.
- **Sharing a server**: If someone runs their own validation server. This is acceptable risk for a B2B legal tool.

### Recommendations for production
1. **Use HTTPS** for the license server (mandatory)
2. **Sign the response** with HMAC so the client can verify it came from your server (not a MITM proxy returning `valid: true`)
3. **Rate-limit** the validation endpoint to prevent brute-force key guessing
4. **Log activations** with IP + machine_id for abuse detection
5. **Certificate pinning** (optional, advanced) — pin your server's TLS certificate in the client to prevent proxy interception

## Testing

### Dev mode (no license needed)
```bash
npx tauri dev
# License check is skipped — upload works freely
```

### Test with a key
```bash
# Generate a test key
node tools/generate-license.js
# Enter it in Settings → License
# Use QVLT-TEST-TEST-TEST if no server is running (format passes, server call will fail gracefully)
```

### Test server validation
```bash
# Start a local mock server (see tools/mock-license-server.js)
node tools/mock-license-server.js
# Then run the app — it will validate against localhost:3000
```
