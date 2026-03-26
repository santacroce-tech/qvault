# Qvault License Server

License key management and payment processing API for the Qvault desktop application.

## Prerequisites

- Node.js 18+ (or Docker)
- SQLite (bundled via better-sqlite3)

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `PORT` | No | `3000` | Server listen port |
| `ADMIN_KEY` | **Yes** | — | Secret key for admin API access |
| `DB_PATH` | No | `./data/licenses.db` | SQLite database file path |
| `STRIPE_SECRET_KEY` | For payments | — | Stripe API secret key |
| `STRIPE_WEBHOOK_SECRET` | For payments | — | Stripe webhook signing secret |
| `STRIPE_PRICE_ID` | For payments | — | Stripe Price ID for yearly plan |

## Quick Start (Local)

```bash
cp .env.example .env
# Edit .env with your ADMIN_KEY and optional Stripe keys

npm install
npm run dev    # nodemon with auto-reload
# or
npm start      # production
```

## Docker

```bash
# Build
docker build -t qvault-api .

# Run
docker run -d \
  --name qvault-api \
  --restart unless-stopped \
  -p 127.0.0.1:3001:3000 \
  -v /opt/qvault-api/data:/data \
  --env-file .env \
  qvault-api
```

## API Reference

### Health Check

```
GET /health
→ { "status": "ok", "timestamp": "2026-03-19T..." }
```

### License Endpoints

#### Activate a License

```bash
curl -X POST https://api.qvault.tech/api/license/activate \
  -H "Content-Type: application/json" \
  -d '{"license_key": "QVLT-XXXX-XXXX-XXXX", "machine_id": "abc123"}'
```

Response:
```json
{ "valid": true, "license_key": "QVLT-...", "expires_at": "2027-03-19T...", "plan": "yearly" }
```

#### Validate a License

```bash
curl -X POST https://api.qvault.tech/api/license/validate \
  -H "Content-Type: application/json" \
  -d '{"license_key": "QVLT-XXXX-XXXX-XXXX", "machine_id": "abc123"}'
```

#### Deactivate a License

```bash
curl -X POST https://api.qvault.tech/api/license/deactivate \
  -H "Content-Type: application/json" \
  -d '{"license_key": "QVLT-XXXX-XXXX-XXXX", "machine_id": "abc123"}'
```

### Admin Endpoints

All admin endpoints require the `X-Admin-Key` header.

#### List All Licenses

```bash
curl https://api.qvault.tech/api/admin/licenses \
  -H "X-Admin-Key: YOUR_ADMIN_KEY"
```

#### Generate License Keys

```bash
curl -X POST https://api.qvault.tech/api/admin/licenses/generate \
  -H "X-Admin-Key: YOUR_ADMIN_KEY" \
  -H "Content-Type: application/json" \
  -d '{"count": 5, "plan": "yearly", "max_machines": 2}'
```

Response:
```json
{ "keys": ["QVLT-ABCD-EFGH-JKLM", ...] }
```

### Stripe Endpoints

#### Create Checkout Session

```bash
curl -X POST https://api.qvault.tech/api/stripe/create-checkout-session \
  -H "Content-Type: application/json" \
  -d '{"locale": "en"}'
```

Response:
```json
{ "url": "https://checkout.stripe.com/..." }
```

#### Webhook (called by Stripe)

```
POST /api/stripe/webhook
```

Handles `checkout.session.completed` events to auto-generate license keys.

### Admin Panel

Access the web dashboard at:

```
https://api.qvault.tech/admin?key=YOUR_ADMIN_KEY
```

## Deployment

The server is deployed as a Docker container, reverse-proxied by nginx at `api.qvault.tech`.

### Updating

```bash
ssh root@YOUR_SERVER_IP

# Sync code
cd /opt/qvault-api
# (copy updated server/ files here)

# Rebuild and restart
docker stop qvault-api && docker rm qvault-api
docker build -t qvault-api .
docker run -d --name qvault-api --restart unless-stopped \
  -p 127.0.0.1:3001:3000 \
  -v /opt/qvault-api/data:/data \
  --env-file .env \
  qvault-api
```

### Stripe Webhook Setup

1. Go to Stripe Dashboard → Developers → Webhooks
2. Add endpoint: `https://api.qvault.tech/api/stripe/webhook`
3. Listen for event: `checkout.session.completed`
4. Copy the signing secret to `.env` as `STRIPE_WEBHOOK_SECRET`
