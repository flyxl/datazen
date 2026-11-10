/**
 * Capture the onboarding wizard screenshot.
 *
 * The wizard is a separate native Tauri window. We navigate to it directly,
 * wait for it to render, and capture a full-resolution screenshot.
 *
 * Outputs: e2e/screenshots/site/wizard-open.png
 */
import { browser, $ } from '@wdio/globals';
import { assertGallerySize, ensureMaximized } from '../lib/capture-window';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..', '..');
const OUT = path.join(ROOT, 'e2e', 'screenshots', 'site');

async function invoke<T = unknown>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  ) as Promise<T>;
}

async function shot(name: string, settleMs = 1500) {
  await browser.pause(settleMs);
  // Maximize, never resize: a `set_size` IPC is honoured 1:1 in CSS points and
  // drops devicePixelRatio, which is how this image ended up at 1920x1440.
  await ensureMaximized();
  await assertGallerySize(name);
  fs.mkdirSync(OUT, { recursive: true });
  const buf = Buffer.from(await browser.takeScreenshot(), 'base64');
  fs.writeFileSync(path.join(OUT, name), buf);
  console.log(`📸 ${name} (${buf.length} bytes)`);
}

describe('Wizard Screenshot', () => {
  it('captures the onboarding wizard', async function () {
    this.timeout(120000);

    // Navigate to the onboarding window
    await browser.url('tauri://localhost/window.html?window=onboarding');

    // Wait for the wizard to render
    await $('[data-testid="onboarding-wizard"]').waitForDisplayed({ timeout: 30000 });
    await browser.pause(2000);

    // Resize to standard demo resolution
    await browser.pause(1500);

    // Screenshot
    await shot('wizard-open.png', 2000);

    console.log('✅ Wizard screenshot captured');
  });
});
