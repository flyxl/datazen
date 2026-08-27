import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { AppearanceSection } from '../AppearanceSection';
import { EXTENSION_API_VERSION, type PluginSummary } from '../../../types/plugin';
import { encodePluginThemePackId } from '../../../lib/themePackApply';

const { pluginState, fetchMock, settingsState, updateSettingsMock } = vi.hoisted(() => {
  const updateSettingsFn = vi.fn();
  return {
    pluginState: {
      plugins: [] as Array<Record<string, unknown>>,
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

vi.mock('../../../stores/pluginStore', () => ({
  usePluginStore: Object.assign((sel: (s: typeof pluginState) => unknown) => sel(pluginState), {
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

function makePlugin(overrides: Partial<PluginSummary> = {}): PluginSummary {
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

/** All Select triggers in render order (color scheme is index 0, theme pack index 1). */
function allSelectTriggers(): HTMLElement[] {
  return screen
    .getAllByRole('button', { hidden: false })
    .filter((b) => b.getAttribute('aria-haspopup') === 'listbox');
}

/** Open a Select combobox and pick an option by visible label (targets the list item). */
function pickOption(trigger: HTMLElement, optionLabel: string) {
  fireEvent.click(trigger);
  const targets = screen.getAllByText(optionLabel);
  const listItem = targets.find((el) => el.closest('[aria-selected]'));
  fireEvent.mouseDown(listItem ?? targets[0]);
}

beforeEach(() => {
  vi.clearAllMocks();
  updateSettingsMock.mockResolvedValue(undefined);
  pluginState.plugins = [];
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
  it('renders color-scheme and theme-pack selects with the built-in default option', () => {
    pluginState.plugins = [makePlugin()];
    render(<AppearanceSection />);

    expect(screen.getByText('settings.colorScheme')).toBeInTheDocument();
    expect(screen.getByText('settings.theme.pack')).toBeInTheDocument();

    const [scheme, theme] = allSelectTriggers();
    expect(scheme).toHaveTextContent('theme.dark');
    // No plugin theme selected -> built-in default shown.
    expect(theme).toHaveTextContent('settings.theme.packDefault');
  });

  it('lists themes of enabled plugins as options in the theme select', () => {
    pluginState.plugins = [makePlugin()];
    render(<AppearanceSection />);

    const [, theme] = allSelectTriggers();
    fireEvent.click(theme);
    expect(screen.getByText('Midnight Blue')).toBeInTheDocument();
    expect(screen.getByText('Solar')).toBeInTheDocument();
  });

  it('hides themes contributed by disabled plugins', () => {
    pluginState.plugins = [
      makePlugin(),
      makePlugin({
        id: 'acme.disabled-themes',
        name: 'Disabled Themes',
        enabled: false,
        themes: [{ id: 'ghost', name: 'Ghost', modes: ['dark'] }],
      }),
    ];
    render(<AppearanceSection />);

    const [, theme] = allSelectTriggers();
    fireEvent.click(theme);
    expect(screen.queryByText('Ghost')).not.toBeInTheDocument();
    expect(screen.getByText('Midnight Blue')).toBeInTheDocument();
    expect(screen.getByText('Solar')).toBeInTheDocument();
  });

  it('applies a plugin theme on selection and persists the encoded pack id', async () => {
    pluginState.plugins = [makePlugin()];
    render(<AppearanceSection />);

    const [, theme] = allSelectTriggers();
    fireEvent.click(theme);
    fireEvent.mouseDown(screen.getByText('Midnight Blue'));
    await waitFor(() => expect(updateSettingsMock).toHaveBeenCalledTimes(1));

    const saved = updateSettingsMock.mock.calls[0][0] as {
      theme: { mode: string; packId: string | null };
    };
    expect(saved.theme.mode).toBe('dark');
    expect(saved.theme.packId).toBe(encodePluginThemePackId('acme.bill-audit', 'midnight-blue'));
  });

  it('lets the user switch back to the built-in default theme', async () => {
    settingsState.settings = {
      theme: { mode: 'dark', packId: encodePluginThemePackId('acme.bill-audit', 'solar') },
      language: 'en',
      pluginSettings: {},
    };
    pluginState.plugins = [makePlugin()];
    render(<AppearanceSection />);

    const [, theme] = allSelectTriggers();
    // Open and pick the built-in default sentinel option.
    fireEvent.click(theme);
    fireEvent.mouseDown(screen.getByText('settings.theme.packDefault'));
    await waitFor(() => expect(updateSettingsMock).toHaveBeenCalledTimes(1));

    const saved = updateSettingsMock.mock.calls[0][0] as {
      theme: { mode: string; packId: string | null };
    };
    expect(saved.theme.mode).toBe('dark');
    expect(saved.theme.packId).toBeNull();
  });

  it('skips re-applying when the same plugin theme is selected again', () => {
    settingsState.settings = {
      theme: {
        mode: 'dark',
        packId: encodePluginThemePackId('acme.bill-audit', 'solar'),
      },
      language: 'en',
      pluginSettings: {},
    };
    pluginState.plugins = [makePlugin()];
    render(<AppearanceSection />);

    const [, theme] = allSelectTriggers();
    pickOption(theme, 'Solar');
    expect(updateSettingsMock).not.toHaveBeenCalled();
  });

  it('changes the color scheme and persists the new mode', async () => {
    pluginState.plugins = [makePlugin()];
    render(<AppearanceSection />);

    const [scheme] = allSelectTriggers();
    fireEvent.click(scheme);
    fireEvent.mouseDown(screen.getByText('theme.system'));
    await waitFor(() => expect(updateSettingsMock).toHaveBeenCalledTimes(1));

    const saved = updateSettingsMock.mock.calls[0][0] as {
      theme: { mode: string; packId: string | null };
    };
    expect(saved.theme.mode).toBe('system');
    expect(saved.theme.packId).toBeNull();
  });

  it('shows an error when applying a theme fails', async () => {
    updateSettingsMock.mockRejectedValueOnce(new Error('tokens.css missing'));
    pluginState.plugins = [makePlugin()];
    render(<AppearanceSection />);

    const [, theme] = allSelectTriggers();
    fireEvent.click(theme);
    fireEvent.mouseDown(screen.getByText('Midnight Blue'));
    await waitFor(() =>
      expect(screen.getByTestId('appearance-error')).toHaveTextContent(
        'settings.appearance.applyError',
      ),
    );
  });

  it('shows the orphan hint when the persisted plugin theme is no longer offered', () => {
    settingsState.settings = {
      theme: {
        mode: 'dark',
        packId: encodePluginThemePackId('acme.gone-plugin', 'vanished'),
      },
      language: 'en',
      pluginSettings: {},
    };
    pluginState.plugins = [makePlugin()];
    render(<AppearanceSection />);

    expect(screen.getByTestId('appearance-orphan-hint')).toHaveTextContent(
      'settings.appearance.missingHint',
    );
  });

  it('fetches plugins once on mount when the store has not loaded yet', () => {
    pluginState.loaded = false;
    render(<AppearanceSection />);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('shows the empty state hint when no themes are contributed', () => {
    pluginState.plugins = [makePlugin({ themes: [] })];
    render(<AppearanceSection />);

    expect(screen.getByTestId('appearance-more-placeholder')).toBeInTheDocument();
    expect(screen.getByText('settings.appearance.emptyHint')).toBeInTheDocument();
  });
});
