import Database from "better-sqlite3";
import fs from "fs";
import path from "path";

let db;

export function initDatabase(dbPath) {
  const dir = path.dirname(dbPath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }

  db = new Database(dbPath);

  // Enable WAL mode for better concurrent read performance
  db.pragma("journal_mode = WAL");
  db.pragma("foreign_keys = ON");

  db.exec(`
    CREATE TABLE IF NOT EXISTS license_keys (
      id            INTEGER PRIMARY KEY AUTOINCREMENT,
      key           TEXT UNIQUE NOT NULL,
      plan          TEXT NOT NULL DEFAULT 'yearly',
      max_machines  INTEGER NOT NULL DEFAULT 2,
      created_at    TEXT NOT NULL DEFAULT (datetime('now')),
      expires_at    TEXT
    );

    CREATE TABLE IF NOT EXISTS activations (
      id              INTEGER PRIMARY KEY AUTOINCREMENT,
      license_key_id  INTEGER NOT NULL REFERENCES license_keys(id),
      machine_id      TEXT NOT NULL,
      activated_at    TEXT NOT NULL DEFAULT (datetime('now')),
      last_validated_at TEXT NOT NULL DEFAULT (datetime('now')),
      status          TEXT NOT NULL DEFAULT 'active',
      UNIQUE(license_key_id, machine_id)
    );

    CREATE TABLE IF NOT EXISTS purchases (
      id                INTEGER PRIMARY KEY AUTOINCREMENT,
      stripe_session_id TEXT UNIQUE NOT NULL,
      customer_email    TEXT,
      license_key_id    INTEGER REFERENCES license_keys(id),
      amount_cents      INTEGER,
      currency          TEXT DEFAULT 'usd',
      created_at        TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE INDEX IF NOT EXISTS idx_license_keys_key ON license_keys(key);
    CREATE INDEX IF NOT EXISTS idx_activations_status ON activations(license_key_id, status);
    CREATE INDEX IF NOT EXISTS idx_purchases_session ON purchases(stripe_session_id);
  `);

  return db;
}

export function getDb() {
  if (!db) {
    throw new Error("Database not initialized. Call initDatabase() first.");
  }
  return db;
}

// ─── License key queries ───

export function findLicenseByKey(key) {
  return getDb().prepare("SELECT * FROM license_keys WHERE key = ?").get(key);
}

export function countActiveActivations(licenseKeyId) {
  const row = getDb()
    .prepare(
      "SELECT COUNT(*) as count FROM activations WHERE license_key_id = ? AND status = 'active'"
    )
    .get(licenseKeyId);
  return row.count;
}

export function findActivation(licenseKeyId, machineId) {
  return getDb()
    .prepare(
      "SELECT * FROM activations WHERE license_key_id = ? AND machine_id = ? AND status = 'active'"
    )
    .get(licenseKeyId, machineId);
}

export function createActivation(licenseKeyId, machineId) {
  return getDb()
    .prepare(
      "INSERT INTO activations (license_key_id, machine_id) VALUES (?, ?)"
    )
    .run(licenseKeyId, machineId);
}

export function reactivate(activationId) {
  return getDb()
    .prepare(
      "UPDATE activations SET status = 'active', activated_at = datetime('now'), last_validated_at = datetime('now') WHERE id = ?"
    )
    .run(activationId);
}

export function setExpiresAt(licenseKeyId, expiresAt) {
  return getDb()
    .prepare("UPDATE license_keys SET expires_at = ? WHERE id = ?")
    .run(expiresAt, licenseKeyId);
}

export function updateLastValidated(activationId) {
  return getDb()
    .prepare(
      "UPDATE activations SET last_validated_at = datetime('now') WHERE id = ?"
    )
    .run(activationId);
}

export function revokeActivation(licenseKeyId, machineId) {
  return getDb()
    .prepare(
      "UPDATE activations SET status = 'revoked' WHERE license_key_id = ? AND machine_id = ? AND status = 'active'"
    )
    .run(licenseKeyId, machineId);
}

export function getAllLicenses() {
  const licenses = getDb()
    .prepare("SELECT * FROM license_keys ORDER BY created_at DESC")
    .all();

  const stmtActivations = getDb().prepare(
    "SELECT * FROM activations WHERE license_key_id = ? ORDER BY activated_at DESC"
  );

  return licenses.map((lic) => ({
    ...lic,
    activations: stmtActivations.all(lic.id),
  }));
}

export function insertLicenseKey(key, plan = "yearly", maxMachines = 2) {
  return getDb()
    .prepare(
      "INSERT INTO license_keys (key, plan, max_machines) VALUES (?, ?, ?)"
    )
    .run(key, plan, maxMachines);
}

// ─── Purchase queries ───

export function insertPurchase(stripeSessionId, customerEmail, licenseKeyId, amountCents, currency = "usd") {
  return getDb()
    .prepare(
      "INSERT INTO purchases (stripe_session_id, customer_email, license_key_id, amount_cents, currency) VALUES (?, ?, ?, ?, ?)"
    )
    .run(stripeSessionId, customerEmail, licenseKeyId, amountCents, currency);
}

export function findPurchaseBySessionId(sessionId) {
  return getDb()
    .prepare("SELECT p.*, lk.key as license_key FROM purchases p LEFT JOIN license_keys lk ON lk.id = p.license_key_id WHERE p.stripe_session_id = ?")
    .get(sessionId);
}

// ─── Stats queries ───

export function getLicenseStats() {
  const total = getDb().prepare("SELECT COUNT(*) as count FROM license_keys").get().count;
  const active = getDb().prepare(
    "SELECT COUNT(DISTINCT lk.id) as count FROM license_keys lk JOIN activations a ON a.license_key_id = lk.id WHERE a.status = 'active'"
  ).get().count;
  const expired = getDb().prepare(
    "SELECT COUNT(*) as count FROM license_keys WHERE expires_at IS NOT NULL AND expires_at < datetime('now')"
  ).get().count;
  const purchases = getDb().prepare("SELECT COUNT(*) as count FROM purchases").get().count;
  const revenue = getDb().prepare("SELECT COALESCE(SUM(amount_cents), 0) as total FROM purchases").get().total;
  return { total, active, expired, unused: total - active, purchases, revenue };
}
