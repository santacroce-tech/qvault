#!/usr/bin/env node
/**
 * Automated demo video recording for Qvault.
 *
 * How it works:
 *   1. Opens a visible Chrome window via Puppeteer (non-headless)
 *   2. Starts macOS screen recording with `screencapture -V`
 *   3. Puppeteer drives through the app views with smooth pauses
 *   4. Recording stops automatically after the timed duration
 *
 * Prerequisites:
 *   - Vite dev server running: npm run dev
 *   - Puppeteer installed: npm install (already a devDependency)
 *
 * Usage:
 *   node tools/record-demo.js
 *
 * Output:
 *   tools/demo-recording.mov
 */
import puppeteer from 'puppeteer';
import { execSync, spawn } from 'child_process';
import { resolve } from 'path';

const OUTPUT_FILE = resolve('tools/demo-recording.mov');
const RECORDING_DURATION = 50; // seconds
const APP_URL = 'http://localhost:1420';

const sleep = (ms) => new Promise(r => setTimeout(r, ms));

async function main() {
  console.log('🎬 Qvault Demo Video Recorder\n');

  // 1. Check if dev server is running
  try {
    const res = await fetch(APP_URL);
    if (!res.ok) throw new Error();
    console.log('✓ Dev server running at', APP_URL);
  } catch {
    console.error('✗ Dev server not running. Start it first: npm run dev');
    process.exit(1);
  }

  // 2. Find Chrome installed by Puppeteer
  const browser = await puppeteer.launch({
    headless: false,
    defaultViewport: null,
    args: [
      '--window-size=1280,850',
      '--window-position=100,100',
      '--disable-infobars',
      '--hide-scrollbars',
      `--app=${APP_URL}`, // Opens in app mode (no address bar, no tabs)
    ],
  });

  const pages = await browser.pages();
  const page = pages[0] || await browser.newPage();

  // Navigate and wait for load
  await page.goto(APP_URL, { waitUntil: 'networkidle0' });
  await sleep(2000);

  console.log('✓ Browser window open');
  console.log(`✓ Starting ${RECORDING_DURATION}s screen recording...`);
  console.log(`  Output: ${OUTPUT_FILE}\n`);

  // 3. Start screen recording (main display)
  //    -V = timed recording, -D1 = main display, -k = show clicks
  const recorder = spawn('screencapture', [
    `-V${RECORDING_DURATION}`,
    '-D1',
    '-k',
    OUTPUT_FILE
  ], { stdio: 'ignore' });

  // Give recorder a moment to start
  await sleep(1500);

  // ═══════════════════════════════════════════════
  // 4. DEMO FLOW — Puppeteer drives the UI
  // ═══════════════════════════════════════════════

  console.log('Scene 1: Upload view (showing clean dashboard)...');
  // Already on upload view — hold for a beat
  await sleep(4000);

  console.log('Scene 2: Navigating to History...');
  await page.click('nav button[title="History"]');
  await sleep(3500);

  console.log('Scene 3: Opening a document for review...');
  await page.click('table tbody tr:first-child button:first-child');
  await sleep(2000);

  // Slow scroll through the sidebar to show redactions
  console.log('Scene 4: Reviewing PII detections...');
  await sleep(3000);

  // Hover over some redactions in the sidebar
  const redactionItems = await page.$$('[class*="redaction"], [class*="sidebar"] li, [class*="Sidebar"] > div > div');
  if (redactionItems.length > 0) {
    for (let i = 0; i < Math.min(3, redactionItems.length); i++) {
      await redactionItems[i].hover();
      await sleep(1200);
    }
  }

  // Click approve on a couple items if checkmark buttons exist
  console.log('Scene 5: Approving redactions...');
  const approveButtons = await page.$$('button[title*="Approve"], button[title*="approve"]');
  for (let i = 0; i < Math.min(2, approveButtons.length); i++) {
    await approveButtons[i].click();
    await sleep(1000);
  }

  // Try clicking "Approve All" if visible
  const approveAllBtn = await page.$('button:has-text("Approve All")');
  if (approveAllBtn) {
    await sleep(1000);
    await approveAllBtn.click();
    await sleep(2000);
  }

  await sleep(3000);

  console.log('Scene 6: Navigating to Sensitive Data overview...');
  await page.click('nav button[title="Sensitive Data"]');
  await sleep(4000);

  // Scroll through the entity table
  const entityTable = await page.$('table');
  if (entityTable) {
    await page.evaluate(() => {
      const main = document.querySelector('main');
      if (main) {
        main.scrollTo({ top: 300, behavior: 'smooth' });
      }
    });
    await sleep(2000);
    await page.evaluate(() => {
      const main = document.querySelector('main');
      if (main) {
        main.scrollTo({ top: 0, behavior: 'smooth' });
      }
    });
  }

  await sleep(3000);

  console.log('Scene 7: Back to upload (full circle)...');
  await page.click('nav button[title="Upload"]');
  await sleep(4000);

  // ═══════════════════════════════════════════════
  // Wait for recording to finish
  // ═══════════════════════════════════════════════

  console.log('\n⏳ Waiting for recording to complete...');

  await new Promise((resolve, reject) => {
    recorder.on('close', (code) => {
      if (code === 0) resolve();
      else reject(new Error(`screencapture exited with code ${code}`));
    });
    recorder.on('error', reject);
  });

  console.log('✓ Recording saved to:', OUTPUT_FILE);

  await browser.close();
  console.log('✓ Browser closed');
  console.log('\n🎉 Done! Your demo video is ready.');
  console.log(`   File: ${OUTPUT_FILE}`);
  console.log('\n💡 Tips:');
  console.log('   - Hide your dock before recording (System Settings → Desktop & Dock)');
  console.log('   - Set a clean desktop wallpaper');
  console.log('   - Close other apps to avoid notifications');
  console.log('   - Re-run to get a cleaner take if needed');
}

main().catch((err) => {
  console.error('Error:', err.message);
  process.exit(1);
});
