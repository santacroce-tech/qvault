#!/usr/bin/env node

/**
 * Mock License Server for Qvault development/testing.
 *
 * Accepts ANY valid-format key and returns success.
 * Run: node tools/mock-license-server.js
 * Listens on: http://localhost:3000
 */

const http = require("http");

const PORT = 3000;

// In-memory store of activated keys (resets on restart)
const activatedKeys = new Map();

function isValidFormat(key) {
  const parts = key.split("-");
  if (parts.length !== 4 || parts[0] !== "QVLT") return false;
  return parts.slice(1).every((p) => /^[A-Z0-9]{4}$/.test(p));
}

function parseBody(req) {
  return new Promise((resolve) => {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      try {
        resolve(JSON.parse(body));
      } catch {
        resolve({});
      }
    });
  });
}

const server = http.createServer(async (req, res) => {
  res.setHeader("Content-Type", "application/json");
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Methods", "POST, OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");

  if (req.method === "OPTIONS") {
    res.writeHead(204);
    return res.end();
  }

  const body = await parseBody(req);
  const url = req.url;

  console.log(`${req.method} ${url}`, JSON.stringify(body));

  // POST /api/license/activate
  if (url === "/api/license/activate" && req.method === "POST") {
    const { license_key, machine_id } = body;

    if (!license_key || !isValidFormat(license_key)) {
      res.writeHead(403);
      return res.end(JSON.stringify({ valid: false, error: "Invalid license key format" }));
    }

    const existing = activatedKeys.get(license_key);
    if (existing && existing.machine_id !== machine_id) {
      res.writeHead(403);
      return res.end(
        JSON.stringify({ valid: false, error: "License key already activated on another machine" })
      );
    }

    const now = new Date();
    const expires = new Date(now.getTime() + 365 * 24 * 60 * 60 * 1000);

    activatedKeys.set(license_key, {
      machine_id,
      activated_at: now.toISOString(),
      expires_at: expires.toISOString(),
      status: "active",
    });

    console.log(`  ✓ Activated: ${license_key} for machine ${machine_id}`);

    res.writeHead(200);
    return res.end(
      JSON.stringify({
        valid: true,
        license_key,
        expires_at: expires.toISOString(),
        plan: "yearly",
      })
    );
  }

  // POST /api/license/validate
  if (url === "/api/license/validate" && req.method === "POST") {
    const { license_key, machine_id } = body;

    const record = activatedKeys.get(license_key);

    if (!record) {
      // In mock mode, auto-activate any valid-format key
      if (license_key && isValidFormat(license_key)) {
        const expires = new Date(Date.now() + 365 * 24 * 60 * 60 * 1000);
        activatedKeys.set(license_key, {
          machine_id,
          activated_at: new Date().toISOString(),
          expires_at: expires.toISOString(),
          status: "active",
        });
        console.log(`  ✓ Auto-validated: ${license_key}`);
        res.writeHead(200);
        return res.end(JSON.stringify({ valid: true, expires_at: expires.toISOString() }));
      }

      res.writeHead(200);
      return res.end(JSON.stringify({ valid: false, error: "License key not found" }));
    }

    if (record.status === "revoked") {
      res.writeHead(200);
      return res.end(JSON.stringify({ valid: false, error: "License key revoked" }));
    }

    if (new Date(record.expires_at) < new Date()) {
      res.writeHead(200);
      return res.end(JSON.stringify({ valid: false, error: "License key expired" }));
    }

    console.log(`  ✓ Valid: ${license_key}`);
    res.writeHead(200);
    return res.end(JSON.stringify({ valid: true, expires_at: record.expires_at }));
  }

  // POST /api/license/deactivate
  if (url === "/api/license/deactivate" && req.method === "POST") {
    const { license_key } = body;
    activatedKeys.delete(license_key);
    console.log(`  ✓ Deactivated: ${license_key}`);
    res.writeHead(200);
    return res.end(JSON.stringify({ deactivated: true }));
  }

  res.writeHead(404);
  res.end(JSON.stringify({ error: "Not found" }));
});

server.listen(PORT, () => {
  console.log(`\n🔑 Qvault Mock License Server running on http://localhost:${PORT}`);
  console.log(`   Accepts any valid-format key (QVLT-XXXX-XXXX-XXXX)\n`);
  console.log(`   Endpoints:`);
  console.log(`     POST /api/license/activate`);
  console.log(`     POST /api/license/validate`);
  console.log(`     POST /api/license/deactivate\n`);
});
