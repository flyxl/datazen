import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { AppearanceSection } from '../AppearanceSection';
import { EXTENSION_API_VERSION, type ExtensionSummary } from '../../../types/extension';
import { encodePluginThemePackId } from '../../../lib/themePackApply';

const { pluginState, fetchMock, settingsState, updateSettingsMock } = vi.hoisted(() => {
  const updateSettingsFn = vi.fn();
  return {
    pluginState: {
      extensions: [] as Array<Record<string, unknown>>,
      loaded: true,
      error: null as string | null,
    },
    fetchMock: vi.fn(),
    settingsState: {
      settings: {
        theme: { mode: 'dark', packId: null as string | null },
        language: 'en',
        pluginSettings: {},
      },
      updateSettings: updateSettingsFn,
    },
    updateSettingsMock: updateSettingsFn,
  };
});

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string, params?: Record<string, string | number>) => key }),
}));

vi.mock('../../../stores/extensionStore', () => ({
  useExtensionStore: Object.assign((sel: (s: typeof pluginState) => unknown) => sel(pluginState), {
    getState: () => ({ ...pluginState, fetch: fetchMock }),
  }),
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: Object.assign(
    (sel: (s: typeof settingsState) => unknown) => sel(settingsState),
    {
      getState: () => settingsState,
    },
  ),
}));

function makePlugin(overrides: Partial<ExtensionSummary> = {}): ExtensionSummary {
  return {
    id: 'acme.bill-audit',
    name: 'Bill Audit',
    version: '1.0.0',
    apiVersion: EXTENSION_API_VERSION,
    author: 'Acme',
    enabled: true,
    permissions: [],
    pages: [],
    themes: [
      { id: 'midnight-blue', name: 'Midnight Blue', modes: ['dark'] },
      { id: 'solar', name: 'Solar', modes: ['light'] },
    ],
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  updateSettingsMock.mockResolvedValue(undefined);
  pluginState.extensions = [];
  pluginState.loaded = true;
  pluginState.error = null;
  settingsState.settings = {
    theme: { mode: 'dark', packId: null },
    language: 'en',
    pluginSettings: {},
  };
});

afterEach(cleanup);

describe('AppearanceSection', () => {
  it('renders one card per theme of enabled plugins with version, source and mode badges', async () => {
    pluginState.extensions = [makePlugin()];
    render(<AppearanceSection />);

    const grid = screen.getByTestId('appearance-theme-grid');
    const cards = within(grid).getAllByTestId('appearance-theme-card');
    expect(cards).toHaveLength(2);
    expect(screen.getByText('Midnight Blue')).toBeInTheDocument();
    expect(screen.getAllByText('v1.0.0 · Bill Audit')).toHaveLength(2);
    expect(within(cards[0]).getByText('dark')).toBeInTheDocument();
  });

  it('hides themes contributed by disabled plugins', () => {
    pluginState.extensions = [
      makePlugin(),
      makePlugin({
        id: 'acme.disabled-themes',
        name: 'Disabled Themes',
        enabled: false,
        themes: [{ id: 'ghost', name: 'Ghost', modes: ['dark'] }],
      }),
    ];
    render(<AppearanceSection />);

    expect(screen.queryByText('Ghost')).not.toBeInTheDocument();
    expect(screen.getAllByTestId('appearance-theme-card')).toHaveLength(2);
  });

  it('applies a theme on click and persists the encoded pack id', async () => {
    pluginState.extensions = [makePlugin()];
    render(<AppearanceSection />);

    fireEvent.click(screen.getAllByTestId('appearance-theme-card')[0]);
    await waitFor(() => expect(updateSettingsMock).toHaveBeenCalledTimes(1));
    const saved = updateSettingsMock.mock.calls[0][0] as {
      theme: { mode: string; packId: string | null };
    };
    expect(saved.theme.mode).toBe('dark');
    expect(saved.theme.packId).toBe(encodePluginThemePackId('acme.bill-audit', 'midnight-blue'));
  });

  it('highlights the active theme with the current badge and skips re-applying it', async () => {
    settingsState.settings = {
      theme: {
        mode: 'dark',
        packId: encodePluginThemePackId('acme.bill-audit', 'solar'),
      },
      language: 'en',
      pluginSettings: {},
    };
    pluginState.extensions = [makePlugin()];
    render(<AppearanceSection />);

    const solarCard = screen
      .getAllByTestId('appearance-theme-card')
      .find((el) => el.getAttribute('data-theme-id') === 'solar')!;
    expect(solarCard).toHaveAttribute('aria-pressed', 'true');
    expect(within(solarCard).getByTestId('appearance-current-badge')).toBeInTheDocument();

    fireEvent.click(solarCard);
    await waitFor(() => expect(updateSettingsMock).not.toHaveBeenCalled());
  });

  it('shows an error when applying fails', async () => {
    updateSettingsMock.mockRejectedValueOnce(new Error('tokens.css missing'));
    pluginState.extensions = [makePlugin()];
    render(<AppearanceSection />);

    fireEvent.click(screen.getAllByTestId('appearance-theme-card')[0]);
    await waitFor(() =>
      expect(screen.getByTestId('appearance-error')).toHaveTextContent(
        'settings.appearance.applyError',
      ),
    );
  });

  it('shows the empty state guiding to the plugins page when no themes are available', () => {
    pluginState.extensions = [makePlugin({ themes: [] })];
    render(<AppearanceSection />);

    expect(screen.getByTestId('appearance-empty')).toBeInTheDocument();
    expect(screen.queryByTestId('appearance-theme-grid')).not.toBeInTheDocument();
    expect(screen.getByTestId('appearance-more-placeholder')).toBeInTheDocument();
  });

  it('fetches plugins once on mount when the store has not loaded yet', () => {
    pluginState.loaded = false;
    render(<AppearanceSection />);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('AS-M1: lists themes of multiple contributing plugins with per-plugin source labels', () => {
    pluginState.extensions = [
      makePlugin(),
      makePlugin({
        id: 'other.vendor-pack',
        name: 'Vendor Pack',
        themes: [{ id: 'nord', name: 'Nord', modes: ['dark', 'light'] }],
      }),
    ];
    render(<AppearanceSection />);

    const cards = screen.getAllByTestId('appearance-theme-card');
    expect(cards).toHaveLength(3);
    expect(screen.getByText('Midnight Blue')).toBeInTheDocument();
    expect(screen.getByText('Solar')).toBeInTheDocument();
    expect(screen.getByText('Nord')).toBeInTheDocument();
    expect(screen.getAllByText('v1.0.0 · Bill Audit')).toHaveLength(2);
    expect(screen.getByText('v1.0.0 · Vendor Pack')).toBeInTheDocument();
    const nordCard = cards.find((el) => el.getAttribute('data-theme-id') === 'nord')!;
    expect(nordCard.getAttribute('data-plugin-id')).toBe('other.vendor-pack');
    expect(within(nordCard).getByText('light')).toBeInTheDocument();
  });

  it('AS-M2: switches between themes of different plugins persisting each encoded pack id', async () => {
    pluginState.extensions = [
      makePlugin(),
      makePlugin({
        id: 'other.vendor-pack',
        name: 'Vendor Pack',
        themes: [{ id: 'nord', name: 'Nord', modes: ['dark'] }],
      }),
    ];
    render(<AppearanceSection />);

    const cardOf = (themeId: string) =>
      screen
        .getAllByTestId('appearance-theme-card')
        .find((el) => el.getAttribute('data-theme-id') === themeId)!;

    fireEvent.click(cardOf('nord'));
    await waitFor(() => expect(updateSettingsMock).toHaveBeenCalledTimes(1));
    let saved = updateSettingsMock.mock.calls[0][0] as { theme: { packId: string } };
    expect(saved.theme.packId).toBe(encodePluginThemePackId('other.vendor-pack', 'nord'));

    fireEvent.click(cardOf('solar'));
    await waitFor(() => expect(updateSettingsMock).toHaveBeenCalledTimes(2));
    saved = updateSettingsMock.mock.calls[1][0] as { theme: { packId: string } };
    expect(saved.theme.packId).toBe(encodePluginThemePackId('acme.bill-audit', 'solar'));
  });

  it('AS-M3: highlights only the active theme across plugins', () => {
    settingsState.settings = {
      theme: { mode: 'dark', packId: encodePluginThemePackId('other.vendor-pack', 'nord') },
      language: 'en',
      pluginSettings: {},
    };
    pluginState.extensions = [
      makePlugin(),
      makePlugin({
        id: 'other.vendor-pack',
        name: 'Vendor Pack',
        themes: [{ id: 'nord', name: 'Nord', modes: ['dark'] }],
      }),
    ];
    render(<AppearanceSection />);

    const pressed = screen
      .getAllByTestId('appearance-theme-card')
      .filter((el) => el.getAttribute('aria-pressed') === 'true');
    expect(pressed).toHaveLength(1);
    expect(pressed[0].getAttribute('data-plugin-id')).toBe('other.vendor-pack');
    expect(pressed[0].getAttribute('data-theme-id')).toBe('nord');
    expect(screen.getByTestId('appearance-current-badge')).toBeInTheDocument();
  });

  it('AS-M4: surfaces the orphan hint when the persisted plugin theme is no longer offered', () => {
    settingsState.settings = {
      theme: {
        mode: 'dark',
        packId: encodePluginThemePackId('acme.gone-plugin', 'vanished'),
      },
      language: 'en',
      pluginSettings: {},
    };
    pluginState.extensions = [makePlugin()];
    render(<AppearanceSection />);

    const grid = screen.getByTestId('appearance-theme-grid');
    expect(within(grid).getAllByTestId('appearance-theme-card')).toHaveLength(2);
    expect(screen.getByTestId('appearance-orphan-hint')).toHaveTextContent(
      'settings.appearance.missingHint',
    );
    expect(screen.queryByTestId('appearance-current-badge')).not.toBeInTheDocument();
  });

  it('AS-M5: string rejections render through the generic error message path', async () => {
    updateSettingsMock.mockRejectedValueOnce('boom');
    pluginState.extensions = [makePlugin()];
    render(<AppearanceSection />);

    fireEvent.click(screen.getAllByTestId('appearance-theme-card')[0]);
    await waitFor(() =>
      expect(screen.getByTestId('appearance-error')).toHaveTextContent(
        'settings.appearance.applyError',
      ),
    );
  });
});
