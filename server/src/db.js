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

    CREATE INDEX IF NOT EXISTS idx_license_keys_key ON license_keys(key);
    CREATE INDEX IF NOT EXISTS idx_activations_status ON activations(license_key_id, status);
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
