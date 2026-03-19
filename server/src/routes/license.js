import { Router } from "express";
import {
  getDb,
  findLicenseByKey,
  countActiveActivations,
  findActivation,
  createActivation,
  reactivate,
  setExpiresAt,
  updateLastValidated,
  revokeActivation,
} from "../db.js";

const router = Router();

/**
 * Find a revoked activation for a specific license+machine combo.
 * Used to allow re-activation after deactivation.
 */
function findRevokedActivation(licenseKeyId, machineId) {
  return getDb()
    .prepare(
      "SELECT * FROM activations WHERE license_key_id = ? AND machine_id = ? AND status = 'revoked'"
    )
    .get(licenseKeyId, machineId);
}

/**
 * POST /api/license/activate
 *
 * Body: { license_key, machine_id }
 * Returns: { valid, license_key, expires_at, plan } or { valid: false, error }
 */
router.post("/activate", (req, res) => {
  const { license_key, machine_id } = req.body;

  if (!license_key || !machine_id) {
    return res
      .status(400)
      .json({ valid: false, error: "Missing license_key or machine_id" });
  }

  // Look up the key
  const license = findLicenseByKey(license_key);
  if (!license) {
    return res
      .status(403)
      .json({ valid: false, error: "License key not found" });
  }

  // Check if already expired (only if expires_at is set, i.e., previously activated)
  if (license.expires_at && new Date(license.expires_at) < new Date()) {
    return res
      .status(403)
      .json({ valid: false, error: "License key expired" });
  }

  // Check if this machine already has an active activation
  const existingActivation = findActivation(license.id, machine_id);
  if (existingActivation) {
    // Already active on this machine — return success (idempotent)
    return res.json({
      valid: true,
      license_key: license.key,
      expires_at: license.expires_at,
      plan: license.plan,
    });
  }

  // Check if there's a revoked activation for this machine we can reactivate
  const revokedRow = findRevokedActivation(license.id, machine_id);
  if (revokedRow) {
    reactivate(revokedRow.id);

    // Set expires_at on first activation of this key
    let expiresAt = license.expires_at;
    if (!expiresAt) {
      expiresAt = new Date(
        Date.now() + 365 * 24 * 60 * 60 * 1000
      ).toISOString();
      setExpiresAt(license.id, expiresAt);
    }

    return res.json({
      valid: true,
      license_key: license.key,
      expires_at: expiresAt,
      plan: license.plan,
    });
  }

  // Check max machines
  const activeCount = countActiveActivations(license.id);
  if (activeCount >= license.max_machines) {
    return res.status(403).json({
      valid: false,
      error: `License key already activated on ${activeCount} machine(s) (max: ${license.max_machines})`,
    });
  }

  // First activation of this key — set expires_at
  let expiresAt = license.expires_at;
  if (!expiresAt) {
    expiresAt = new Date(
      Date.now() + 365 * 24 * 60 * 60 * 1000
    ).toISOString();
    setExpiresAt(license.id, expiresAt);
  }

  // Create the activation
  createActivation(license.id, machine_id);

  console.log(
    `Activated: ${license_key} on machine ${machine_id} (${activeCount + 1}/${license.max_machines})`
  );

  return res.json({
    valid: true,
    license_key: license.key,
    expires_at: expiresAt,
    plan: license.plan,
  });
});

/**
 * POST /api/license/validate
 *
 * Body: { license_key, machine_id }
 * Returns: { valid: true, expires_at } or { valid: false, error }
 */
router.post("/validate", (req, res) => {
  const { license_key, machine_id } = req.body;

  if (!license_key || !machine_id) {
    return res
      .status(400)
      .json({ valid: false, error: "Missing license_key or machine_id" });
  }

  const license = findLicenseByKey(license_key);
  if (!license) {
    return res.json({ valid: false, error: "License key not found" });
  }

  // Check expiry
  if (license.expires_at && new Date(license.expires_at) < new Date()) {
    return res.json({ valid: false, error: "License key expired" });
  }

  // Check this machine has an active activation
  const activation = findActivation(license.id, machine_id);
  if (!activation) {
    return res.json({
      valid: false,
      error: "License key not activated on this machine",
    });
  }

  // Update last_validated_at
  updateLastValidated(activation.id);

  console.log(`Validated: ${license_key} on machine ${machine_id}`);

  return res.json({
    valid: true,
    expires_at: license.expires_at,
  });
});

/**
 * POST /api/license/deactivate
 *
 * Body: { license_key, machine_id }
 * Returns: { deactivated: true }
 */
router.post("/deactivate", (req, res) => {
  const { license_key, machine_id } = req.body;

  if (!license_key || !machine_id) {
    return res
      .status(400)
      .json({ success: false, error: "Missing license_key or machine_id" });
  }

  const license = findLicenseByKey(license_key);
  if (!license) {
    // Still return success — idempotent
    return res.json({ deactivated: true });
  }

  revokeActivation(license.id, machine_id);

  console.log(`Deactivated: ${license_key} on machine ${machine_id}`);

  return res.json({ deactivated: true });
});

export default router;
