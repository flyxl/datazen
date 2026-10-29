/**
 * DB-03 §9.2 — `useClipboardCopyFeedback`.
 *
 * The three guarantees of the shared `useCopyFeedback` (window edges, timer cleared on
 * unmount, a late rejection bound to its own request) are asserted through the same harness
 * the migrated affordances use, plus the one thing this hook adds: the **downgrade chain**.
 * On WKWebView `navigator.clipboard.writeText` is refused once the user gesture is gone — the
 * normal case for a context-menu item — so a copy that only used it would fail silently (G5).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { clearPlatformServices, setPlatformServices } from '@datazen/backend-client';
import { useClipboardCopyFeedback } from '../useClipboardCopyFeedback';
import {
  advanceBy,
  installControlledClipboard,
  spyOnWindowTimers,
} from '../../test/copyFeedbackHarness';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

// Form-split R6/F6-prep: the copy chain's native fallback leg rides the
// PlatformServices port — bind it with the invoke mock behind it so the wire
// assertions (command name + payload) keep reading the same mock.
function bindClipboardPort() {
  setPlatformServices({
    writeClipboard: (text: string) => invoke('write_clipboard', { text }),
    writeClipboardHtml: (input: { html: string; altText: string }) =>
      invoke('write_clipboard_html', { html: input.html, altText: input.altText }),
  } as never);
}

const WINDOW_MS = 1200;

/**
 * Render **before** faking the timers.
 *
 * `renderHook` under fake timers leaves `result.current` unset in this environment (React 18
 * schedules its own work through `setTimeout`), and the hook only arms its window when a copy
 * starts, so faking the timers after the mount is both sufficient and stable.
 */
function renderTimed() {
  const harness = renderHook(() => useClipboardCopyFeedback(WINDOW_MS));
  vi.useFakeTimers();
  return harness;
}

describe('useClipboardCopyFeedback', () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockResolvedValue(undefined);
    bindClipboardPort();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    clearPlatformServices();
  });

  it('lights up optimistically and reverts at the window edge, not before', async () => {
    const clipboard = installControlledClipboard();
    const { result } = renderTimed();

    let pending: Promise<boolean> = Promise.resolve(false);
    act(() => {
      pending = result.current.copy('hello');
    });
    // Optimistic: the marker is up before the write has settled at all.
    expect(result.current.copied).toBe(true);

    await act(async () => {
      await clipboard.settle(0);
    });
    expect(await pending).toBe(true);
    expect(result.current.copied).toBe(true);

    advanceBy(WINDOW_MS - 1);
    expect(result.current.copied).toBe(true);
    advanceBy(1);
    expect(result.current.copied).toBe(false);
  });

  it('clears its timer when unmounted inside the window', async () => {
    const clipboard = installControlledClipboard();
    const { result, unmount } = renderTimed();
    // The watch must go on **after** `renderTimed`, because faking the timers replaces
    // `window.setTimeout` — a spy installed earlier would be discarding the real handle.
    const watch = spyOnWindowTimers();

    let pending: Promise<boolean> = Promise.resolve(false);
    act(() => {
      pending = result.current.copy('hello');
    });
    const armed = watch.lastArmedHandle();
    unmount();
    expect(watch.clearedHandles()).toContain(armed);

    await act(async () => {
      await clipboard.settle(0);
      await pending;
    });
  });

  it('a late rejection from an older copy does not erase a newer success', async () => {
    const clipboard = installControlledClipboard();
    const { result } = renderTimed();

    let first: Promise<boolean> = Promise.resolve(false);
    act(() => {
      first = result.current.copy('first');
    });
    let second: Promise<boolean> = Promise.resolve(false);
    act(() => {
      second = result.current.copy('second');
    });
    expect(result.current.copied).toBe(true);

    // The newer write lands first, then the older one is refused.
    await act(async () => {
      await clipboard.settle(1);
      await second;
    });
    await act(async () => {
      await clipboard.settle(0, 'reject');
      await first;
    });

    // The confirmation of the successful, newer copy is still up.
    expect(result.current.copied).toBe(true);
  });

  it('rolls back when every step of the chain is refused', async () => {
    const clipboard = installControlledClipboard();
    // The whole chain has to fail for the copy to be a failure: the Clipboard API, the IPC
    // write and `execCommand`.
    invoke.mockRejectedValue(new Error('no ipc'));
    const execCommand = vi.fn(() => false);
    Object.defineProperty(document, 'execCommand', { configurable: true, value: execCommand });
    const { result } = renderTimed();

    let pending: Promise<boolean> = Promise.resolve(true);
    act(() => {
      pending = result.current.copy('hello');
    });
    await act(async () => {
      await clipboard.settle(0, 'reject');
    });
    expect(await pending).toBe(false);
    expect(result.current.copied).toBe(false);
  });

  it('downgrades to the Tauri command when the Clipboard API is refused', async () => {
    // The WKWebView case: `writeText` throws, the IPC write succeeds.
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: vi.fn(() => Promise.reject(new Error('NotAllowedError'))),
      },
    });
    const { result } = renderTimed();

    let ok = false;
    await act(async () => {
      ok = await result.current.copy('via ipc');
    });

    expect(ok).toBe(true);
    expect(invoke).toHaveBeenCalledWith('write_clipboard', { text: 'via ipc' });
    expect(result.current.copied).toBe(true);
  });

  it('writes both clipboard flavors when an html payload is given (DB-10)', async () => {
    // jsdom has no ClipboardItem, so the rich write lands on the Tauri
    // command — exactly the WKWebView-shaped path it exists for.
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn() },
    });
    const { result } = renderTimed();

    let ok = false;
    await act(async () => {
      ok = await result.current.copy('a\tb', { html: '<table></table>' });
    });

    expect(ok).toBe(true);
    // The rich command carries the markup **and** the plain-text fallback.
    expect(invoke).toHaveBeenCalledWith('write_clipboard_html', {
      html: '<table></table>',
      altText: 'a\tb',
    });
    expect(result.current.copied).toBe(true);
  });

  it('falls back to the plain write when the rich command is unavailable', async () => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: vi.fn(() => Promise.resolve()),
      },
    });
    invoke.mockRejectedValueOnce(new Error('unknown command'));
    const { result } = renderTimed();

    let ok = false;
    await act(async () => {
      ok = await result.current.copy('plain text', { html: '<table></table>' });
    });

    // The chain's last step: a table as plain text beats markup source as text.
    expect(ok).toBe(true);
    expect(invoke).toHaveBeenCalledWith('write_clipboard_html', {
      html: '<table></table>',
      altText: 'plain text',
    });
  });

  it('treats empty text as a failure without lighting up', async () => {
    installControlledClipboard();
    const { result } = renderTimed();

    let ok = true;
    await act(async () => {
      ok = await result.current.copy('');
    });
    expect(ok).toBe(false);
    expect(result.current.copied).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });
});
