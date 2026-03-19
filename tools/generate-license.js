#!/usr/bin/env node

/**
 * Qvault License Key Generator
 *
 * Usage:
 *   node generate-license.js          # Generate 1 key
 *   node generate-license.js 10       # Generate 10 keys
 *   node generate-license.js 50 > keys.txt  # Save to file
 */

import crypto from "crypto";

// Charset: no 0/O/1/I to avoid visual confusion
const CHARSET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const SEGMENT_LEN = 4;
const NUM_SEGMENTS = 3;
const PREFIX = "QVLT";

function generateSegment() {
  const bytes = crypto.randomBytes(SEGMENT_LEN);
  let segment = "";
  for (let i = 0; i < SEGMENT_LEN; i++) {
    segment += CHARSET[bytes[i] % CHARSET.length];
  }
  return segment;
}

function generateLicenseKey() {
  const segments = [PREFIX];
  for (let i = 0; i < NUM_SEGMENTS; i++) {
    segments.push(generateSegment());
  }
  return segments.join("-");
}

// --- Main ---
const count = parseInt(process.argv[2] || "1", 10);

if (isNaN(count) || count < 1 || count > 10000) {
  console.error("Usage: node generate-license.js [count]");
  console.error("  count: 1-10000 (default: 1)");
  process.exit(1);
}

for (let i = 0; i < count; i++) {
  console.log(generateLicenseKey());
}
