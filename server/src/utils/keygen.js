import crypto from "crypto";

// Charset — no 0/O/1/I to avoid visual confusion
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

export function generateLicenseKey() {
  const segments = [PREFIX];
  for (let i = 0; i < NUM_SEGMENTS; i++) {
    segments.push(generateSegment());
  }
  return segments.join("-");
}
