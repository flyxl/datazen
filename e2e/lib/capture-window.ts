/**
 * Window geometry for release-gallery captures.
 *
 * Why this exists: the WebDriver screenshot is taken at the window's *backing*
 * pixel size, so a maximized window on a Retina display yields 2880x1648 —
 * true 2x pixels, the same density as the best legacy gallery images. Pinning
 * the window from the driver instead (`browser.setWindowSize`, or a `set_size`
 * IPC) is honoured 1:1 in CSS points and silently discards devicePixelRatio,
 * which is how the gallery ended up holding 14 different sizes.
 *
 * So: maximize, never resize. `ensureMaximized()` is idempotent and cheap — it
 * only acts when the window is not already maximized, which makes it safe to
 * call before every capture.
 *
 * One trap worth knowing: `set_size` without a label applies to whichever
 * window the driver is attached to, and `switchToWindow` does not reliably
 * re-point the Tauri WebDriver at the child window. A "child window" resize
 * can therefore land on the main window and shrink it. Anything that resizes a
 * window must re-maximize the main window afterwards — hence `ensureMaximized`
 * being cheap enough to call unconditionally.
 */
import { browser } from '@wdio/globals';

/** The gallery size: a maximized window on a 1440x824-point Retina display. */
export const GALLERY_WIDTH = 2880;
export const GALLERY_HEIGHT = 1648;

async function invoke<T = unknown>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r ?? 'ok'))
        .catch((e: unknown) => done(`ERR: ${String(e)}`));
    },
    cmd,
    JSON.stringify(args),
  ) as Promise<T>;
}

/** Current window size in physical pixels. */
async function outerSize(): Promise<{ width: number; height: number }> {
  return invoke<{ width: number; height: number }>('plugin:window|outer_size');
}

/**
 * Maximize the window unless it already is, and wait for the webview to
 * re-lay-out. Returns the resulting physical size.
 */
export async function ensureMaximized(settleMs = 1200): Promise<{ width: number; height: number }> {
  const isMax = await invoke<boolean>('plugin:window|is_maximized').catch(() => false);
  if (!isMax) {
    await invoke('plugin:window|maximize');
    // The resize is animated by the window server; give the webview a beat to
    // reflow, otherwise the first screenshot can catch a half-sized layout.
    await browser.pause(settleMs);
  }
  return outerSize();
}

/**
 * Fail loudly if a capture is not the expected size, rather than shipping an
 * off-size image into the gallery. Off-size frames are the reason the gallery
 * needed a normalizer at all; catching one here points at the real cause.
 */
export async function assertGallerySize(where: string): Promise<void> {
  const { width, height } = await outerSize();
  if (width !== GALLERY_WIDTH || height !== GALLERY_HEIGHT) {
    throw new Error(
      `${where}: window is ${width}x${height}, expected ${GALLERY_WIDTH}x${GALLERY_HEIGHT}. ` +
        'A child-window resize probably leaked onto the main window.',
    );
  }
}
