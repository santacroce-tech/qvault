import { Router } from "express";
import { getAllLicenses, insertLicenseKey } from "../db.js";
import { adminAuth } from "../middleware/auth.js";
import { generateLicenseKey } from "../utils/keygen.js";

const router = Router();

// All admin routes require X-Admin-Key header
router.use(adminAuth);

/**
 * GET /api/admin/licenses
 *
 * Lists all license keys with their activations.
 */
router.get("/licenses", (req, res) => {
  try {
    const licenses = getAllLicenses();
    return res.json({ licenses });
  } catch (err) {
    console.error("Failed to list licenses:", err);
    return res.status(500).json({ error: "Internal server error" });
  }
});

/**
 * POST /api/admin/licenses/generate
 *
 * Body: { count: number, plan?: string, max_machines?: number }
 * Generates new license keys and inserts them into the database.
 * Returns: { keys: string[] }
 */
router.post("/licenses/generate", (req, res) => {
  const { count = 1, plan = "yearly", max_machines = 2 } = req.body;

  if (!Number.isInteger(count) || count < 1 || count > 1000) {
    return res
      .status(400)
      .json({ error: "count must be an integer between 1 and 1000" });
  }

  const keys = [];

  try {
    for (let i = 0; i < count; i++) {
      const key = generateLicenseKey();
      insertLicenseKey(key, plan, max_machines);
      keys.push(key);
    }

    console.log(`Generated ${count} license key(s) (plan: ${plan})`);

    return res.json({ keys });
  } catch (err) {
    console.error("Failed to generate keys:", err);
    return res.status(500).json({ error: "Internal server error" });
  }
});

export default router;
