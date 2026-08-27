import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Browser } from '@wdio/globals';

const E2E_ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
export const E2E_SCREENSHOT_DIR = path.join(E2E_ROOT, 'screenshots');

let journeySpec: string | undefined;
let journeyTest: string | undefined;
let stepSeq = 0;
let savedStepCount = 0;
/** Consecutive-frame dedupe within the active `it()`. */
let lastDigest: string | null = null;
/** Cross-`it()` dedupe within the active spec file (`beforeSuite` resets). */
let specDigests = new Set<string>();

export function isScreenshotTraceEnabled(): boolean {
  return process.env.E2E_SCREENSHOT === '1';
}

/** Create output root on demand (gitignored; may not exist before first run). */
export function ensureScreenshotRoot(): void {
  fs.mkdirSync(E2E_SCREENSHOT_DIR, { recursive: true });
}

export function sanitizeScreenshotLabel(label: string): string {
  return (
    label
      .replace(/[^\w\u4e00-\u9fff-]+/g, '_')
      .replace(/^_+|_+$/g, '')
      .slice(0, 120) || 'test'
  );
}

/** Spec basename without extension, e.g. `main-window`. */
export function screenshotSpecDir(specFilePath: string | undefined): string {
  const base = specFilePath?.split(/[/\\]/).pop()?.replace(/\.ts$/, '') ?? 'unknown';
  return path.join(E2E_SCREENSHOT_DIR, base);
}

export function journeyStepsSaved(): number {
  return savedStepCount;
}

/** Reset per-spec digest set (wdio `beforeSuite`). */
export function beginJourneySuite(_specFilePath: string | undefined) {
  specDigests = new Set();
}

/** Reset per-`it()` journey state. */
export function beginJourneyTest(specFilePath: string | undefined, testTitle: string) {
  journeySpec = specFilePath;
  journeyTest = testTitle;
  stepSeq = 0;
  savedStepCount = 0;
  lastDigest = null;
}

function journeyTestDir(): string {
  ensureScreenshotRoot();
  const dir = path.join(
    screenshotSpecDir(journeySpec),
    sanitizeScreenshotLabel(journeyTest ?? 'unknown'),
  );
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

function digestFile(filePath: string): string {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function removeDirIfEmpty(dir: string): void {
  try {
    if (fs.existsSync(dir) && fs.readdirSync(dir).length === 0) {
      fs.rmdirSync(dir);
    }
  } catch {
    /* ignore */
  }
}

function discardScreenshot(out: string, testDir: string): undefined {
  fs.unlinkSync(out);
  stepSeq -= 1;
  removeDirIfEmpty(testDir);
  return undefined;
}

/**
 * Save a journey screenshot when `E2E_SCREENSHOT=1`.
 * Skips pixel-identical frames within the same `it()` or anywhere in the spec file
 * (except `fail` frames).
 */
export async function saveJourneyScreenshot(
  browser: Browser,
  stepLabel: string,
  settleMs = 400,
  force = false,
): Promise<string | undefined> {
  if (!isScreenshotTraceEnabled()) return;
  if (!journeyTest && !force) return;
  ensureScreenshotRoot();
  if (settleMs > 0) await browser.pause(settleMs);

  stepSeq += 1;
  const num = String(stepSeq).padStart(2, '0');
  const testDir = journeyTestDir();
  const out = path.join(testDir, `${num}_${sanitizeScreenshotLabel(stepLabel)}.png`);
  await browser.saveScreenshot(out);

  const digest = digestFile(out);
  const isFailFrame = sanitizeScreenshotLabel(stepLabel) === 'fail';

  if (!isFailFrame && specDigests.has(digest)) {
    return discardScreenshot(out, testDir);
  }

  if (!force && !isFailFrame && lastDigest !== null && digest === lastDigest) {
    return discardScreenshot(out, testDir);
  }

  lastDigest = digest;
  specDigests.add(digest);
  savedStepCount += 1;
  return out;
}
