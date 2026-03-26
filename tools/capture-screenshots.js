#!/usr/bin/env node
/**
 * Screenshot capture script using Puppeteer.
 * Run: npx puppeteer browsers install chrome && node tools/capture-screenshots.js
 * Requires the Vite dev server running on localhost:1420
 */
import puppeteer from 'puppeteer';
import { mkdirSync } from 'fs';
import { resolve } from 'path';

const OUTPUT_DIR = resolve('landing/screenshots');
mkdirSync(OUTPUT_DIR, { recursive: true });

const WIDTH = 1280;
const HEIGHT = 800;
const SCALE = 2;

const sleep = (ms) => new Promise(r => setTimeout(r, ms));

async function main() {
  const browser = await puppeteer.launch({ headless: true });
  const page = await browser.newPage();
  await page.setViewport({ width: WIDTH, height: HEIGHT, deviceScaleFactor: SCALE });

  // Navigate to app
  await page.goto('http://localhost:1420', { waitUntil: 'networkidle0' });
  await sleep(1500);

  // Screenshot 1: Upload View
  await page.screenshot({ path: resolve(OUTPUT_DIR, 'upload-view.png'), fullPage: false });
  console.log('✓ upload-view.png');

  // Screenshot 2: History View
  await page.click('nav button[title="History"]');
  await sleep(800);
  await page.screenshot({ path: resolve(OUTPUT_DIR, 'history-view.png'), fullPage: false });
  console.log('✓ history-view.png');

  // Screenshot 3: Review View (open first doc)
  await page.click('table tbody tr:first-child button:first-child');
  await sleep(800);
  await page.screenshot({ path: resolve(OUTPUT_DIR, 'review-view.png'), fullPage: false });
  console.log('✓ review-view.png');

  // Screenshot 4: Entity Database View
  await page.click('nav button[title="Sensitive Data"]');
  await sleep(800);
  await page.screenshot({ path: resolve(OUTPUT_DIR, 'entity-view.png'), fullPage: false });
  console.log('✓ entity-view.png');

  // Screenshot 5: Settings View
  await page.click('nav button[title="Settings"]');
  await sleep(800);
  await page.screenshot({ path: resolve(OUTPUT_DIR, 'settings-view.png'), fullPage: false });
  console.log('✓ settings-view.png');

  await browser.close();
  console.log(`\nAll screenshots saved to ${OUTPUT_DIR}`);
}

main().catch(console.error);
