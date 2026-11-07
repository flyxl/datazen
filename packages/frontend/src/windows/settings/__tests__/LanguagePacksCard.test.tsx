import { beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { LanguagePacksCard } from '../LanguagePacksCard';
import type { LanguagePackSummary } from '../../../commands/languagePacks';

const { removeMock, updateSettingsMock, fetchMock, unloadMock } = vi.hoisted(() => ({
  removeMock: vi.fn(),
  updateSettingsMock: vi.fn(),
  fetchMock: vi.fn(),
  unloadMock: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params && params.count !== undefined ? `${key}:${params.count}` : key,
  }),
}));

vi.mock('../../../lib/languagePackRuntime', () => ({
  unloadLanguagePack: (...args: unknown[]) => unloadMock(...args),
}));

// Minimal in-memory stand-ins for the two zustand stores.
const packState = {
  packs: [] as LanguagePackSummary[],
  loaded: true,
  error: null as string | null,
  fetch: fetchMock,
  remove: removeMock,
};

const settingsState = {
  settings: { language: 'ja' },
  updateSettings: updateSettingsMock,
};

vi.mock('../../../stores/languagePackStore', () => ({
  useLanguagePackStore: (selector: (s: typeof packState) => unknown) => selector(packState),
  ensureLanguagePacksChangedListener: () => {},
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (selector: (s: typeof settingsState) => unknown) => selector(settingsState),
}));

vi.mock('../InstallLanguagePackDialog', () => ({
  InstallLanguagePackDialog: ({ open }: { open: boolean }) =>
    open ? <div data-testid="language-pack-install-pick" /> : null,
}));

const PACK: LanguagePackSummary = {
  code: 'ja',
  label: '日本語',
  version: '1.0.0',
  author: 'DataZen',
  description: null,
  messageCount: 3451,
  fileName: 'ja',
};

describe('LanguagePacksCard', () => {
  beforeEach(() => {
    cleanup();
    removeMock.mockReset().mockResolvedValue(undefined);
    updateSettingsMock.mockReset().mockResolvedValue(undefined);
    fetchMock.mockReset().mockResolvedValue(undefined);
    unloadMock.mockReset();
    packState.packs = [PACK];
    packState.loaded = true;
    packState.error = null;
    settingsState.settings.language = 'ja';
  });

  it('fetches the pack list on mount', () => {
    render(<LanguagePacksCard />);
    expect(fetchMock).toHaveBeenCalled();
  });

  it('lists installed packs with metadata and the in-use badge', () => {
    render(<LanguagePacksCard />);
    expect(screen.getByTestId('language-pack-row-ja')).toBeTruthy();
    expect(screen.getByTestId('language-pack-active-ja')).toBeTruthy();
    expect(screen.getByTestId('language-pack-remove-ja')).toBeTruthy();
    // messageCount interpolation flows through the mocked t() (followed by
    // the author suffix rendered in the same <p>).
    expect(screen.getByText(/messageCount:3451/)).toBeTruthy();
    expect(screen.getByText(/DataZen/)).toBeTruthy();
  });

  it('shows no rows when nothing is installed (hint rides the tooltip)', () => {
    packState.packs = [];
    render(<LanguagePacksCard />);
    expect(screen.queryByTestId('language-pack-row-ja')).toBeNull();
    expect(screen.getByTestId('language-pack-install-open')).toBeTruthy();
  });

  it('removing the active pack first switches language to en, then removes', async () => {
    render(<LanguagePacksCard />);
    fireEvent.click(screen.getByTestId('language-pack-remove-ja'));
    fireEvent.click(await screen.findByTestId('language-pack-remove-confirm'));
    await waitFor(() => expect(removeMock).toHaveBeenCalledWith('ja'));
    expect(updateSettingsMock).toHaveBeenCalledWith({ language: 'en' });
    // English fallback is persisted BEFORE the pack directory is removed.
    expect(updateSettingsMock.mock.invocationCallOrder[0]).toBeLessThan(
      removeMock.mock.invocationCallOrder[0],
    );
    expect(unloadMock).toHaveBeenCalledWith('ja');
  });

  it('removing a non-active pack never touches the language setting', async () => {
    settingsState.settings.language = 'en';
    render(<LanguagePacksCard />);
    fireEvent.click(screen.getByTestId('language-pack-remove-ja'));
    fireEvent.click(await screen.findByTestId('language-pack-remove-confirm'));
    await waitFor(() => expect(removeMock).toHaveBeenCalled());
    expect(updateSettingsMock).not.toHaveBeenCalled();
  });

  it('remove failure surfaces the error inline', async () => {
    removeMock.mockRejectedValue(new Error('language pack not found: ja'));
    render(<LanguagePacksCard />);
    fireEvent.click(screen.getByTestId('language-pack-remove-ja'));
    fireEvent.click(await screen.findByTestId('language-pack-remove-confirm'));
    await screen.findByTestId('language-pack-card-error');
    expect(screen.getByTestId('language-pack-card-error').textContent).toContain(
      'language pack not found: ja',
    );
  });

  it('the install button opens the install dialog', async () => {
    render(<LanguagePacksCard />);
    fireEvent.click(screen.getByTestId('language-pack-install-open'));
    await screen.findByTestId('language-pack-install-pick');
  });

  it('a store-level error is rendered', () => {
    packState.error = 'boom';
    render(<LanguagePacksCard />);
    expect(screen.getByTestId('language-pack-card-error').textContent).toContain('boom');
  });
});
