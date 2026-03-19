import "dotenv/config";
import express from "express";
import cors from "cors";
import { initDatabase } from "./db.js";
import licenseRoutes from "./routes/license.js";
import adminRoutes from "./routes/admin.js";

const PORT = parseInt(process.env.PORT || "3000", 10);
const DB_PATH = process.env.DB_PATH || "./data/licenses.db";

// Initialize database (creates data dir if needed)
initDatabase(DB_PATH);

const app = express();

// Middleware
app.use(cors());
app.use(express.json());

// Request logging
app.use((req, res, next) => {
  const start = Date.now();
  res.on("finish", () => {
    const duration = Date.now() - start;
    console.log(
      `${req.method} ${req.originalUrl} ${res.statusCode} ${duration}ms`
    );
  });
  next();
});

// Routes
app.use("/api/license", licenseRoutes);
app.use("/api/admin", adminRoutes);

// Health check
app.get("/health", (req, res) => {
  res.json({ status: "ok", timestamp: new Date().toISOString() });
});

// 404 handler
app.use((req, res) => {
  res.status(404).json({ error: "Not found" });
});

// Global error handler
app.use((err, req, res, _next) => {
  console.error("Unhandled error:", err);
  res.status(500).json({ error: "Internal server error" });
});

app.listen(PORT, () => {
  console.log(`\nQvault License Server running on http://localhost:${PORT}`);
  console.log(`Database: ${DB_PATH}`);
  console.log(`\nEndpoints:`);
  console.log(`  POST /api/license/activate`);
  console.log(`  POST /api/license/validate`);
  console.log(`  POST /api/license/deactivate`);
  console.log(`  GET  /api/admin/licenses`);
  console.log(`  POST /api/admin/licenses/generate`);
  console.log(`  GET  /health\n`);
});
