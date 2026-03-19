import { Router } from "express";
import { getLicenseStats } from "../db.js";
import { adminAuth } from "../middleware/auth.js";

const router = Router();

// Stats API (protected)
router.get("/api/stats", adminAuth, (req, res) => {
  try {
    const stats = getLicenseStats();
    return res.json(stats);
  } catch (err) {
    console.error("Failed to get stats:", err);
    return res.status(500).json({ error: "Internal server error" });
  }
});

// Admin dashboard HTML
router.get("/", (req, res) => {
  const key = req.query.key || "";
  if (!key) {
    return res.status(401).send("Missing ?key= parameter");
  }

  // Verify key before serving dashboard
  if (key !== process.env.ADMIN_KEY) {
    return res.status(403).send("Invalid admin key");
  }

  res.type("html").send(getDashboardHTML(key));
});

function getDashboardHTML(adminKey) {
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Qvault Admin</title>
  <style>
    :root {
      --navy: #0F1B2D;
      --navy-light: #1A2B45;
      --blue: #3B82F6;
      --blue-hover: #2563EB;
      --green: #10B981;
      --red: #EF4444;
      --amber: #F59E0B;
      --gray-50: #F9FAFB;
      --gray-100: #F3F4F6;
      --gray-200: #E5E7EB;
      --gray-400: #9CA3AF;
      --gray-600: #4B5563;
      --gray-800: #1F2937;
    }
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif; background: var(--gray-50); color: var(--gray-800); }
    .header { background: var(--navy); color: white; padding: 1rem 2rem; display: flex; align-items: center; justify-content: space-between; }
    .header h1 { font-size: 1.25rem; font-weight: 600; }
    .header span { color: var(--gray-400); font-size: 0.85rem; }
    .container { max-width: 1200px; margin: 0 auto; padding: 2rem; }
    .stats-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 1rem; margin-bottom: 2rem; }
    .stat-card { background: white; border-radius: 12px; padding: 1.25rem; box-shadow: 0 1px 3px rgba(0,0,0,0.08); }
    .stat-card .label { font-size: 0.75rem; color: var(--gray-600); text-transform: uppercase; letter-spacing: 0.05em; }
    .stat-card .value { font-size: 1.75rem; font-weight: 700; margin-top: 0.25rem; }
    .stat-card .value.blue { color: var(--blue); }
    .stat-card .value.green { color: var(--green); }
    .stat-card .value.amber { color: var(--amber); }
    .stat-card .value.red { color: var(--red); }
    .card { background: white; border-radius: 12px; padding: 1.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.08); margin-bottom: 1.5rem; }
    .card h2 { font-size: 1.1rem; margin-bottom: 1rem; }
    .form-row { display: flex; gap: 1rem; align-items: end; flex-wrap: wrap; }
    .form-group { display: flex; flex-direction: column; gap: 0.25rem; }
    .form-group label { font-size: 0.75rem; color: var(--gray-600); font-weight: 500; }
    .form-group input, .form-group select { padding: 0.5rem 0.75rem; border: 1px solid var(--gray-200); border-radius: 8px; font-size: 0.9rem; }
    .btn { padding: 0.5rem 1.25rem; border: none; border-radius: 8px; font-weight: 600; cursor: pointer; font-size: 0.9rem; transition: background 0.15s; }
    .btn-primary { background: var(--blue); color: white; }
    .btn-primary:hover { background: var(--blue-hover); }
    .btn-sm { padding: 0.25rem 0.75rem; font-size: 0.8rem; }
    table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
    th { text-align: left; padding: 0.75rem; color: var(--gray-600); font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.05em; border-bottom: 2px solid var(--gray-200); }
    td { padding: 0.75rem; border-bottom: 1px solid var(--gray-100); }
    tr:hover { background: var(--gray-50); }
    .badge { display: inline-block; padding: 0.15rem 0.5rem; border-radius: 9999px; font-size: 0.75rem; font-weight: 600; }
    .badge-green { background: #D1FAE5; color: #065F46; }
    .badge-red { background: #FEE2E2; color: #991B1B; }
    .badge-gray { background: var(--gray-100); color: var(--gray-600); }
    .badge-amber { background: #FEF3C7; color: #92400E; }
    .mono { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.85rem; }
    .generated-keys { margin-top: 1rem; padding: 1rem; background: var(--gray-50); border-radius: 8px; display: none; }
    .generated-keys .key { padding: 0.25rem 0; font-family: monospace; font-size: 0.9rem; cursor: pointer; }
    .generated-keys .key:hover { color: var(--blue); }
    .activations-row { display: none; }
    .activations-row td { padding: 0.5rem 0.75rem 0.5rem 2rem; background: var(--gray-50); }
    .activations-row table { font-size: 0.8rem; }
    .activations-row th { font-size: 0.7rem; }
    .toast { position: fixed; bottom: 2rem; right: 2rem; background: var(--navy); color: white; padding: 0.75rem 1.25rem; border-radius: 8px; font-size: 0.9rem; display: none; z-index: 100; }
    .empty { text-align: center; padding: 2rem; color: var(--gray-400); }
    @media (max-width: 768px) {
      .container { padding: 1rem; }
      .form-row { flex-direction: column; }
      .stats-grid { grid-template-columns: repeat(2, 1fr); }
    }
  </style>
</head>
<body>
  <div class="header">
    <h1>Qvault Admin</h1>
    <span>License Management Dashboard</span>
  </div>

  <div class="container">
    <!-- Stats -->
    <div class="stats-grid" id="stats">
      <div class="stat-card"><div class="label">Total Keys</div><div class="value blue" id="stat-total">-</div></div>
      <div class="stat-card"><div class="label">Active</div><div class="value green" id="stat-active">-</div></div>
      <div class="stat-card"><div class="label">Expired</div><div class="value red" id="stat-expired">-</div></div>
      <div class="stat-card"><div class="label">Unused</div><div class="value amber" id="stat-unused">-</div></div>
      <div class="stat-card"><div class="label">Purchases</div><div class="value blue" id="stat-purchases">-</div></div>
      <div class="stat-card"><div class="label">Revenue</div><div class="value green" id="stat-revenue">-</div></div>
    </div>

    <!-- Generate Keys -->
    <div class="card">
      <h2>Generate License Keys</h2>
      <div class="form-row">
        <div class="form-group">
          <label>Count</label>
          <input type="number" id="gen-count" value="1" min="1" max="100" style="width:80px">
        </div>
        <div class="form-group">
          <label>Plan</label>
          <select id="gen-plan">
            <option value="yearly">Yearly</option>
            <option value="lifetime">Lifetime</option>
          </select>
        </div>
        <div class="form-group">
          <label>Max Machines</label>
          <input type="number" id="gen-machines" value="2" min="1" max="10" style="width:80px">
        </div>
        <button class="btn btn-primary" onclick="generateKeys()">Generate</button>
      </div>
      <div class="generated-keys" id="generated-keys">
        <strong>Generated Keys:</strong>
        <div id="keys-list"></div>
      </div>
    </div>

    <!-- License Table -->
    <div class="card">
      <h2>All Licenses</h2>
      <div id="license-table-wrap">
        <div class="empty">Loading...</div>
      </div>
    </div>
  </div>

  <div class="toast" id="toast"></div>

  <script>
    const ADMIN_KEY = ${JSON.stringify(adminKey)};
    const API_BASE = window.location.origin;

    function headers() {
      return { "X-Admin-Key": ADMIN_KEY, "Content-Type": "application/json" };
    }

    function showToast(msg) {
      const t = document.getElementById("toast");
      t.textContent = msg;
      t.style.display = "block";
      setTimeout(() => t.style.display = "none", 3000);
    }

    async function loadStats() {
      try {
        const res = await fetch(API_BASE + "/admin/api/stats", { headers: { "X-Admin-Key": ADMIN_KEY } });
        const s = await res.json();
        document.getElementById("stat-total").textContent = s.total;
        document.getElementById("stat-active").textContent = s.active;
        document.getElementById("stat-expired").textContent = s.expired;
        document.getElementById("stat-unused").textContent = s.unused;
        document.getElementById("stat-purchases").textContent = s.purchases;
        document.getElementById("stat-revenue").textContent = "$" + (s.revenue / 100).toFixed(2);
      } catch (e) {
        console.error("Stats error:", e);
      }
    }

    async function loadLicenses() {
      try {
        const res = await fetch(API_BASE + "/api/admin/licenses", { headers: { "X-Admin-Key": ADMIN_KEY } });
        const data = await res.json();
        renderLicenses(data.licenses || []);
      } catch (e) {
        document.getElementById("license-table-wrap").innerHTML = '<div class="empty">Failed to load licenses</div>';
      }
    }

    function renderLicenses(licenses) {
      if (licenses.length === 0) {
        document.getElementById("license-table-wrap").innerHTML = '<div class="empty">No licenses yet. Generate some above.</div>';
        return;
      }
      let html = '<table><thead><tr><th>Key</th><th>Plan</th><th>Created</th><th>Expires</th><th>Activations</th><th>Status</th></tr></thead><tbody>';
      for (const lic of licenses) {
        const activeCount = (lic.activations || []).filter(a => a.status === "active").length;
        const isExpired = lic.expires_at && new Date(lic.expires_at) < new Date();
        const statusBadge = isExpired
          ? '<span class="badge badge-red">Expired</span>'
          : activeCount > 0
            ? '<span class="badge badge-green">Active</span>'
            : '<span class="badge badge-gray">Unused</span>';

        html += '<tr class="license-row" onclick="toggleActivations(\\'act-' + lic.id + '\\')" style="cursor:pointer">';
        html += '<td class="mono">' + lic.key + '</td>';
        html += '<td>' + lic.plan + '</td>';
        html += '<td>' + formatDate(lic.created_at) + '</td>';
        html += '<td>' + (lic.expires_at ? formatDate(lic.expires_at) : '—') + '</td>';
        html += '<td>' + activeCount + '/' + lic.max_machines + '</td>';
        html += '<td>' + statusBadge + '</td>';
        html += '</tr>';

        // Expandable activations row
        html += '<tr class="activations-row" id="act-' + lic.id + '"><td colspan="6">';
        if (lic.activations && lic.activations.length > 0) {
          html += '<table><thead><tr><th>Machine ID</th><th>Activated</th><th>Last Validated</th><th>Status</th></tr></thead><tbody>';
          for (const a of lic.activations) {
            const aBadge = a.status === "active" ? '<span class="badge badge-green">Active</span>' : '<span class="badge badge-red">' + a.status + '</span>';
            html += '<tr><td class="mono">' + a.machine_id + '</td><td>' + formatDate(a.activated_at) + '</td><td>' + formatDate(a.last_validated_at) + '</td><td>' + aBadge + '</td></tr>';
          }
          html += '</tbody></table>';
        } else {
          html += '<em style="color:var(--gray-400)">No activations</em>';
        }
        html += '</td></tr>';
      }
      html += '</tbody></table>';
      document.getElementById("license-table-wrap").innerHTML = html;
    }

    function toggleActivations(id) {
      const row = document.getElementById(id);
      row.style.display = row.style.display === "table-row" ? "none" : "table-row";
    }

    function formatDate(d) {
      if (!d) return "—";
      return new Date(d).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric" });
    }

    async function generateKeys() {
      const count = parseInt(document.getElementById("gen-count").value) || 1;
      const plan = document.getElementById("gen-plan").value;
      const max_machines = parseInt(document.getElementById("gen-machines").value) || 2;

      try {
        const res = await fetch(API_BASE + "/api/admin/licenses/generate", {
          method: "POST",
          headers: headers(),
          body: JSON.stringify({ count, plan, max_machines })
        });
        const data = await res.json();
        if (data.keys) {
          const container = document.getElementById("generated-keys");
          const list = document.getElementById("keys-list");
          list.innerHTML = data.keys.map(k => '<div class="key" onclick="copyKey(this)" title="Click to copy">' + k + '</div>').join("");
          container.style.display = "block";
          showToast("Generated " + data.keys.length + " key(s)");
          loadStats();
          loadLicenses();
        }
      } catch (e) {
        showToast("Failed to generate keys");
      }
    }

    function copyKey(el) {
      navigator.clipboard.writeText(el.textContent).then(() => showToast("Copied: " + el.textContent));
    }

    // Init
    loadStats();
    loadLicenses();
  </script>
</body>
</html>`;
}

export default router;
