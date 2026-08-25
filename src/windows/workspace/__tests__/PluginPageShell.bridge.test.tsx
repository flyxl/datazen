/**
 * F6 — PluginPageShell ⇄ bridge wiring tests (test agent).
 *
 * Covers the shell-level trigger paths from PRD §4.4:
 * - bridge attached once per ready iframe with manifest permissions + locale
 * - `datazen:theme-pack-changed` pushes a fresh theme.apply snapshot
 * - MutationObserver on documentElement `class` (dark/light switch) ditto
 * - unmount detaches; post-detach theme events no longer push
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { WorkspaceTab } from '../../../stores/workspaceTabsStore';

const { attachBridgeMock, getManifestMock, summaryHolder } = vi.hoisted(() => ({
  attachBridgeMock: vi.fn(),
  getManifestMock: vi.fn(),
  summaryHolder: { current: undefined as Record<string, unknown> | undefined },
}));

vi.mock('../../../lib/extensionBridge', () => ({
  attachBridge: (...args: unknown[]) => attachBridgeMock(...args),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/plugins', () => ({
  pluginCommands: {
    getPluginManifest: (...args: unknown[]) => getManifestMock(...args),
  },
}));

vi.mock('../../../stores/pluginStore', () => ({
  usePluginStore: {
    getState: () => ({ byId: () => summaryHolder.current }),
  },
}));

import { PluginPageShell, clearPluginEntryCache } from '../PluginPageShell';

function makeHandle() {
  return { pushThemeSnapshot: vi.fn(), detach: vi.fn() };
}

function makeTab(overrides: Partial<WorkspaceTab> = {}): WorkspaceTab {
  return {
    key: 'acme.bill-audit:quota-check',
    pluginId: 'acme.bill-audit',
    pageId: 'quota-check',
    title: 'Quota Check',
    version: '1.0.0',
    ...overrides,
  };
}

async function flushObserver(): Promise<void> {
  // MutationObserver deliveries arrive as microtasks; give them a macrotask.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  clearPluginEntryCache();
  getManifestMock.mockReset().mockResolvedValue({
    id: 'acme.bill-audit',
    version: '1.0.0',
    entry: 'index.html',
  });
  attachBridgeMock.mockReset().mockImplementation(() => makeHandle());
  summaryHolder.current = {
    id: 'acme.bill-audit',
    version: '1.0.0',
    entry: 'index.html',
    permissions: ['context:connections', 'command:invoke'],
  };
});

afterEach(() => {
  cleanup();
  document.documentElement.classList.remove('dark');
  vi.useRealTimers();
});

describe('PluginPageShell bridge wiring (F6)', () => {
  /** Wait for the post-commit attach effect; returns the bridge handle. */
  async function waitForAttachedHandle(): Promise<ReturnType<typeof makeHandle>> {
    await vi.waitFor(
      () => {
        expect(attachBridgeMock.mock.results.length).toBeGreaterThan(0);
      },
      { timeout: 5_000 },
    );
    return attachBridgeMock.mock.results[0].value as ReturnType<typeof makeHandle>;
  }
  it('attaches the bridge once with the shell iframe, manifest permissions and locale', async () => {
    render(<PluginPageShell tab={makeTab()} active />);
    await screen.findByTestId('plugin-iframe', {}, { timeout: 5_000 });

    // The bridge attach runs in a post-commit effect; under CI load the iframe
    // can be observed one paint before that effect lands. Poll briefly.
    await vi.waitFor(
      () => {
        expect(attachBridgeMock).toHaveBeenCalledTimes(1);
      },
      { timeout: 5_000 },
    );
    const [iframeEl, opts] = attachBridgeMock.mock.calls[0] as [
      HTMLIFrameElement,
      Record<string, unknown>,
    ];
    expect(iframeEl).toBe(screen.getByTestId('plugin-iframe'));
    expect(opts.pluginId).toBe('acme.bill-audit');
    expect(opts.permissions).toEqual(['context:connections', 'command:invoke']);
    expect(typeof opts.locale).toBe('string');
  });

  it('pushes a theme snapshot on datazen:theme-pack-changed', async () => {
    render(<PluginPageShell tab={makeTab()} active />);
    await screen.findByTestId('plugin-iframe', {}, { timeout: 5_000 });
    const handle = await waitForAttachedHandle();
    expect(handle.pushThemeSnapshot).not.toHaveBeenCalled();

    await act(async () => {
      document.dispatchEvent(new CustomEvent('datazen:theme-pack-changed'));
      await Promise.resolve();
    });
    expect(handle.pushThemeSnapshot).toHaveBeenCalledTimes(1);

    await act(async () => {
      document.dispatchEvent(new CustomEvent('datazen:theme-pack-changed'));
      await Promise.resolve();
    });
    expect(handle.pushThemeSnapshot).toHaveBeenCalledTimes(2);
  });

  it('pushes a theme snapshot when documentElement class mutates (dark/light switch)', async () => {
    render(<PluginPageShell tab={makeTab()} active />);
    await screen.findByTestId('plugin-iframe', {}, { timeout: 5_000 });
    const handle = await waitForAttachedHandle();

    document.documentElement.classList.add('dark');
    await flushObserver();
    expect(handle.pushThemeSnapshot).toHaveBeenCalledTimes(1);

    document.documentElement.classList.remove('dark');
    await flushObserver();
    expect(handle.pushThemeSnapshot).toHaveBeenCalledTimes(2);

    // Unrelated attribute changes on <html> must NOT trigger pushes.
    document.documentElement.setAttribute('lang', 'de');
    await flushObserver();
    expect(handle.pushThemeSnapshot).toHaveBeenCalledTimes(2);
  });

  it('detaches on unmount and stops reacting to theme triggers afterwards', async () => {
    const { unmount } = render(<PluginPageShell tab={makeTab()} active />);
    await screen.findByTestId('plugin-iframe', {}, { timeout: 5_000 });
    const handle = await waitForAttachedHandle();

    unmount();
    expect(handle.detach).toHaveBeenCalledTimes(1);

    const callsAfterDetach = handle.pushThemeSnapshot.mock.calls.length;
    await act(async () => {
      document.dispatchEvent(new CustomEvent('datazen:theme-pack-changed'));
      document.documentElement.classList.add('dark');
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(handle.pushThemeSnapshot.mock.calls.length).toBe(callsAfterDetach);
  });

  it('reattaches a fresh bridge when the watchdog reload remounts the iframe', async () => {
    vi.useFakeTimers();
    render(<PluginPageShell tab={makeTab()} active />);
    await act(async () => {});
    expect(screen.getByTestId('plugin-iframe')).toBeInTheDocument();

    const firstHandle = await waitForAttachedHandle();

    // Watchdog fires → reload UI → click remounts the iframe with a new key.
    await act(async () => {
      vi.advanceTimersByTime(10_000);
    });
    fireEvent.click(screen.getByTestId('plugin-shell-reload'));
    await act(async () => {});
    expect(screen.getByTestId('plugin-iframe')).toBeInTheDocument();

    expect(firstHandle.detach).toHaveBeenCalled(); // old bridge torn down
    expect(attachBridgeMock).toHaveBeenCalledTimes(2); // fresh bridge for the fresh frame

    const secondHandle = attachBridgeMock.mock.results[1].value as ReturnType<typeof makeHandle>;
    await act(async () => {
      document.dispatchEvent(new CustomEvent('datazen:theme-pack-changed'));
      await Promise.resolve();
    });
    expect(secondHandle.pushThemeSnapshot).toHaveBeenCalled();
    expect(firstHandle.pushThemeSnapshot).not.toHaveBeenCalled();
  });
});
