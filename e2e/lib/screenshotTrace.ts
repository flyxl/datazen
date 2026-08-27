import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Browser } from '@wdio/globals';

const E2E_ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
export const E2E_SCREENSHOT_DIR = path.join(E2E_ROOT, 'screenshots');

let journeySpec: string | undefined;
let journeyTest: string | undefined;
let stepSeq = 0;

export function isScreenshotTraceEnabled(): boolean {
  return process.env.E2E_SCREENSHOT === '1';
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

/** Reset step counter for a new Mocha `it()` block. */
export function beginJourneyTest(specFilePath: string | undefined, testTitle: string) {
  journeySpec = specFilePath;
  journeyTest = testTitle;
  stepSeq = 0;
}

function journeyTestDir(): string {
  const dir = path.join(
    screenshotSpecDir(journeySpec),
    sanitizeScreenshotLabel(journeyTest ?? 'unknown'),
  );
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

/** Next numbered PNG path: `{spec}/{test}/01_{label}.png`. */
export function nextJourneyScreenshotPath(stepLabel: string): string {
  stepSeq += 1;
  const num = String(stepSeq).padStart(2, '0');
  const file = `${num}_${sanitizeScreenshotLabel(stepLabel)}.png`;
  return path.join(journeyTestDir(), file);
}

/** Save a journey step screenshot when `E2E_SCREENSHOT=1`. */
export async function saveJourneyScreenshot(
  browser: Browser,
  stepLabel: string,
  settleMs = 400,
): Promise<string | undefined> {
  if (!isScreenshotTraceEnabled()) return;
  await browser.pause(settleMs);
  const out = nextJourneyScreenshotPath(stepLabel);
  await browser.saveScreenshot(out);
  return out;
}
