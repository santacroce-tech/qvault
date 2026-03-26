/**
 * Admin API key authentication middleware.
 * Requires X-Admin-Key header to match the ADMIN_KEY environment variable.
 */
export function adminAuth(req, res, next) {
  const adminKey = process.env.ADMIN_KEY;

  if (!adminKey || adminKey === "change-me-in-production") {
    console.warn(
      "WARNING: ADMIN_KEY is not set or is using the default value. Admin endpoints are insecure."
    );
  }

  const provided = req.headers["x-admin-key"];

  if (!provided || provided !== adminKey) {
    return res.status(401).json({ error: "Unauthorized: invalid or missing X-Admin-Key header" });
  }

  next();
}
