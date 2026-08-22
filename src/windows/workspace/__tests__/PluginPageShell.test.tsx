import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { PluginPageShell, clearPluginEntryCache } from '../PluginPageShell';
import type { WorkspaceTab } from '../../../stores/workspaceTabsStore';

const { getManifestMock, summaryHolder } = vi.hoisted(() => ({
  getManifestMock: vi.fn(),
  summaryHolder: { current: undefined as Record<string, unknown> | undefined },
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

async function renderActive(tab: WorkspaceTab) {
  const utils = render(<PluginPageShell tab={tab} active />);
  await act(async () => {});
  return utils;
}

beforeEach(() => {
  clearPluginEntryCache();
  getManifestMock.mockReset();
  summaryHolder.current = undefined;
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('PluginPageShell', () => {
  it('lazily mounts the iframe only after first activation', async () => {
    getManifestMock.mockResolvedValue({ id: 'x', version: '1.0.0', entry: 'index.html' });
    const utils = render(<PluginPageShell tab={makeTab()} active={false} />);
    expect(screen.queryByTestId('plugin-iframe')).not.toBeInTheDocument();
    // Shell itself is hidden while its tab is inactive.
    expect(screen.getByTestId('plugin-page-shell').className).toMatch(/hidden/);

    await act(async () => {
      utils.rerender(<PluginPageShell tab={makeTab()} active />);
    });
    expect(await screen.findByTestId('plugin-iframe')).toBeInTheDocument();
  });

  it('renders a sandboxed iframe with the datazen:// entry URL and caches the manifest entry', async () => {
    getManifestMock.mockResolvedValue({
      id: 'acme.bill-audit',
      version: '1.0.0',
      entry: './index.html',
    });

    const tab = makeTab();
    await renderActive(tab);

    const iframe = screen.getByTestId('plugin-iframe');
    expect(iframe.getAttribute('sandbox')).toBe('allow-scripts');
    expect(iframe.getAttribute('src')).toBe('datazen://acme.bill-audit/index.html?v=1.0.0');
    expect(iframe.getAttribute('key')).toBeNull();
    expect(getManifestMock).toHaveBeenCalledTimes(1);
    expect(getManifestMock).toHaveBeenCalledWith('acme.bill-audit');

    // Second instance of the same plugin/version resolves from cache.
    cleanup();
    await renderActive(makeTab({ key: 'acme.bill-audit:other' }));
    expect(screen.getByTestId('plugin-iframe')).toBeInTheDocument();
    expect(getManifestMock).toHaveBeenCalledTimes(1);

    // A version change invalidates the cache.
    cleanup();
    await renderActive(makeTab({ version: '2.0.0' }));
    expect(getManifestMock).toHaveBeenCalledTimes(2);
  }, 15000);

  it('prefers an inline `entry` on the plugin summary when present', async () => {
    summaryHolder.current = { id: 'acme.bill-audit', version: '1.0.0', entry: 'main.html' };

    await renderActive(makeTab());

    expect(screen.getByTestId('plugin-iframe').getAttribute('src')).toBe(
      'datazen://acme.bill-audit/main.html?v=1.0.0',
    );
    expect(getManifestMock).not.toHaveBeenCalled();
  });

  it('keeps inactive tabs mounted but hidden (instance preserved)', async () => {
    getManifestMock.mockResolvedValue({ id: 'x', version: '1.0.0', entry: 'index.html' });
    const utils = await renderActive(makeTab());

    utils.rerender(<PluginPageShell tab={makeTab()} active={false} />);

    const shell = screen.getByTestId('plugin-page-shell');
    expect(shell.className).toMatch(/hidden/);
    expect(shell.getAttribute('aria-hidden')).toBe('true');
    // Same iframe element is still mounted.
    expect(screen.getByTestId('plugin-iframe')).toBeInTheDocument();
  });

  it('shows a reload button after the 10s load timeout and remounts on reload', async () => {
    vi.useFakeTimers();
    getManifestMock.mockResolvedValue({ id: 'x', version: '1.0.0', entry: 'index.html' });

    render(<PluginPageShell tab={makeTab()} active />);
    await act(async () => {});
    expect(screen.getByTestId('plugin-iframe')).toBeInTheDocument();
    expect(screen.queryByTestId('plugin-shell-reload')).not.toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(screen.getByTestId('plugin-shell-reload')).toBeInTheDocument();
    expect(screen.getByText('workspace.shell.loadFailed')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('plugin-shell-reload'));

    expect(screen.getByTestId('plugin-iframe')).toBeInTheDocument();
    expect(screen.queryByTestId('plugin-shell-reload')).not.toBeInTheDocument();

    // Watchdog re-arms for the fresh frame.
    act(() => {
      vi.advanceTimersByTime(9_999);
    });
    expect(screen.queryByTestId('plugin-shell-reload')).not.toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.getByTestId('plugin-shell-reload')).toBeInTheDocument();
  });

  it('shows the failure state when no manifest entry exists and retries on demand', async () => {
    getManifestMock.mockResolvedValue({ id: 'acme.bill-audit', version: '1.0.0' });

    await renderActive(makeTab());

    expect(screen.queryByTestId('plugin-iframe')).not.toBeInTheDocument();
    expect(screen.getByText('workspace.shell.loadFailed')).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(screen.getByTestId('plugin-shell-retry'));
    });
    expect(getManifestMock).toHaveBeenCalledTimes(2);
    expect(screen.getByText('workspace.shell.loadFailed')).toBeInTheDocument();
  });

  it('unmounts the iframe with the shell', async () => {
    getManifestMock.mockResolvedValue({ id: 'x', version: '1.0.0', entry: 'index.html' });
    const { unmount } = await renderActive(makeTab());

    expect(screen.getByTestId('plugin-iframe')).toBeInTheDocument();
    unmount();
    expect(screen.queryByTestId('plugin-iframe')).not.toBeInTheDocument();
  });
});
