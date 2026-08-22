import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { WorkspaceView } from '../WorkspaceView';
import { useWorkspaceTabsStore } from '../../../stores/workspaceTabsStore';
import type { PluginSummary } from '../../../types/plugin';

const { listenMock, pluginState, getManifestMock } = vi.hoisted(() => ({
  listenMock: vi.fn(),
  pluginState: {
    plugins: [] as Array<Record<string, unknown>>,
    loaded: true,
    error: null as string | null,
  },
  getManifestMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: Parameters<typeof listenMock>) => listenMock(...args),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/plugins', () => ({
  PLUGINS_CHANGED_EVENT: 'plugins:changed',
  pluginCommands: {
    getPluginManifest: (...args: unknown[]) => getManifestMock(...args),
  },
}));

vi.mock('../../../stores/pluginStore', () => ({
  usePluginStore: Object.assign((sel: (s: typeof pluginState) => unknown) => sel(pluginState), {
    getState: () => ({
      ...pluginState,
      fetch: vi.fn().mockResolvedValue(undefined),
      byId: (id: string) => (pluginState.plugins as Array<{ id: string }>).find((p) => p.id === id),
    }),
  }),
}));

// NOTE: `workspaceTabsStore` is intentionally NOT mocked — the real store
// drives the TabBar ⇆ DefaultCards mutual exclusion and shell lifecycle.

function makePlugin(overrides: Partial<PluginSummary> = {}): PluginSummary {
  return {
    id: 'acme.bill-audit',
    name: 'Bill Audit',
    version: '1.0.0',
    apiVersion: 2,
    enabled: true,
    permissions: [],
    pages: [{ id: 'quota-check', title: 'Quota Check' }],
    themes: [],
    ...overrides,
  };
}

function openPageHandler(): ((event: { payload?: unknown }) => void) | undefined {
  const call = listenMock.mock.calls.find(([event]) => event === 'plugins:open-page');
  return call?.[1] as ((event: { payload?: unknown }) => void) | undefined;
}

beforeEach(() => {
  useWorkspaceTabsStore.setState({ tabs: [], activeKey: null });
  pluginState.plugins = [];
  pluginState.loaded = true;
  pluginState.error = null;
  listenMock.mockReset().mockResolvedValue(() => {});
  getManifestMock.mockReset().mockResolvedValue({
    id: 'acme.bill-audit',
    version: '1.0.0',
    entry: 'index.html',
  });
});

afterEach(cleanup);

describe('WorkspaceView integration: TabBar ⇆ DefaultCards mutual exclusion', () => {
  it('shows default cards only while no tab is open', async () => {
    pluginState.plugins = [makePlugin()];

    render(<WorkspaceView />);
    await act(async () => {});

    expect(screen.getByTestId('workspace-default-cards')).toBeInTheDocument();
    expect(screen.queryByTestId('workspace-tabbar')).not.toBeInTheDocument();
    expect(screen.queryByTestId('plugin-page-shell')).not.toBeInTheDocument();
  });

  it('opens a tab from the navigator click: tab bar appears, cards disappear, sandboxed iframe mounts', async () => {
    pluginState.plugins = [makePlugin()];

    render(<WorkspaceView />);
    await act(async () => {});

    fireEvent.click(screen.getByTestId('workspace-nav-item'));

    expect(await screen.findByTestId('workspace-tabbar')).toBeInTheDocument();
    expect(screen.queryByTestId('workspace-default-cards')).not.toBeInTheDocument();

    const iframe = await screen.findByTestId('plugin-iframe');
    expect(iframe.getAttribute('sandbox')).toBe('allow-scripts');
    expect(iframe.getAttribute('src')).toBe('datazen://acme.bill-audit/index.html?v=1.0.0');
    // Navigator stays usable next to the open tab.
    expect(screen.getByTestId('workspace-nav-item')).toBeInTheDocument();
  });

  it('opens a tab from a default card click as well', async () => {
    pluginState.plugins = [
      makePlugin({ id: 'acme.afi', name: 'AFI', pages: [{ id: 'pricing', title: 'Pricing' }] }),
    ];
    getManifestMock.mockResolvedValue({ id: 'acme.afi', version: '1.0.0', entry: 'ui.html' });

    render(<WorkspaceView />);
    await act(async () => {});

    fireEvent.click(screen.getByTestId('workspace-default-card'));

    expect(await screen.findByTestId('workspace-tabbar')).toBeInTheDocument();
    expect(screen.getByTestId('workspace-tab')).toHaveTextContent('Pricing');
    const iframe = await screen.findByTestId('plugin-iframe');
    expect(iframe.getAttribute('src')).toBe('datazen://acme.afi/ui.html?v=1.0.0');
  });

  it('closing the last tab restores the default card view and unmounts the iframe', async () => {
    pluginState.plugins = [makePlugin()];

    render(<WorkspaceView />);
    await act(async () => {});
    fireEvent.click(screen.getByTestId('workspace-nav-item'));
    await screen.findByTestId('plugin-iframe');

    fireEvent.click(screen.getByTestId('workspace-tab-close'));

    await waitFor(() => expect(screen.queryByTestId('workspace-tabbar')).not.toBeInTheDocument());
    expect(screen.getByTestId('workspace-default-cards')).toBeInTheDocument();
    expect(screen.queryByTestId('plugin-iframe')).not.toBeInTheDocument();
    // Navigator still offers the page for reopening.
    fireEvent.click(screen.getByTestId('workspace-nav-item'));
    expect(await screen.findByTestId('plugin-iframe')).toBeInTheDocument();
  });

  it('keeps inactive shells mounted-but-hidden while another tab is active', async () => {
    pluginState.plugins = [
      makePlugin(),
      makePlugin({ id: 'acme.afi', name: 'AFI', pages: [{ id: 'pricing', title: 'Pricing' }] }),
    ];

    render(<WorkspaceView />);
    await act(async () => {});

    const items = screen.getAllByTestId('workspace-nav-item');
    fireEvent.click(items[0]!);
    await screen.findByTestId('plugin-iframe');
    fireEvent.click(items[1]!);
    await act(async () => {});

    expect(screen.getAllByTestId('workspace-tab')).toHaveLength(2);
    const shells = screen.getAllByTestId('plugin-page-shell');
    expect(shells).toHaveLength(2);
    const hidden = shells.filter((s) => s.className.includes('hidden'));
    expect(hidden).toHaveLength(1);
    expect(hidden[0]!.getAttribute('aria-hidden')).toBe('true');
    // Both iframes stay mounted (instance preserved), only one visible.
    expect(screen.getAllByTestId('plugin-iframe')).toHaveLength(2);
  });

  it('ignores malformed plugins:open-page payloads without opening any tab', async () => {
    pluginState.plugins = [makePlugin()];

    render(<WorkspaceView />);
    await act(async () => {});

    const handler = openPageHandler();
    await act(async () => {
      handler?.({ payload: null });
      handler?.({ payload: undefined });
      handler?.({ payload: {} });
      handler?.({ payload: { pluginId: '', pageId: '' } });
      handler?.({ payload: { pluginId: 'acme.bill-audit' } });
      handler?.({});
    });

    expect(screen.queryByTestId('workspace-tabbar')).not.toBeInTheDocument();
    expect(screen.getByTestId('workspace-default-cards')).toBeInTheDocument();
  });

  it('closes an open plugin tab when an external refresh disables the plugin (BUG-F4-01)', async () => {
    pluginState.plugins = [makePlugin()];
    const view = render(<WorkspaceView />);
    await act(async () => {});

    fireEvent.click(screen.getByTestId('workspace-nav-item'));
    expect(await screen.findByTestId('workspace-tabbar')).toBeInTheDocument();

    // Another window disables the plugin; the refreshed plugin list arrives
    // through the shared store while this window never touched the toggle.
    pluginState.plugins = [makePlugin({ enabled: false })];
    await act(async () => {
      view.rerender(<WorkspaceView />);
    });

    await waitFor(() => expect(screen.queryByTestId('workspace-tabbar')).not.toBeInTheDocument());
    // The default (empty) view returns and the iframe is gone (no zombie tab).
    expect(screen.getByTestId('workspace-empty')).toBeInTheDocument();
    expect(screen.queryByTestId('plugin-iframe')).not.toBeInTheDocument();

    // Re-enabling restores the navigator entry so the page can reopen.
    pluginState.plugins = [makePlugin()];
    await act(async () => {
      view.rerender(<WorkspaceView />);
    });
    fireEvent.click(screen.getByTestId('workspace-nav-item'));
    expect(await screen.findByTestId('plugin-iframe')).toBeInTheDocument();
  });
});
