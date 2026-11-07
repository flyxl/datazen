import { beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { InstallLanguagePackDialog } from '../InstallLanguagePackDialog';
import type { LanguagePackPreview } from '../../../commands/languagePacks';

const { inspectMock, installMock, fetchMock, onCloseMock } = vi.hoisted(() => ({
  inspectMock: vi.fn(),
  installMock: vi.fn(),
  fetchMock: vi.fn(),
  onCloseMock: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params && params.count !== undefined ? `${key}:${params.count}` : key,
  }),
}));

vi.mock('../../../commands/languagePacks', () => ({
  LANGUAGE_PACKS_CHANGED_EVENT: 'language-packs:changed',
  languagePackCommands: {
    inspectLanguagePackFileWithDialog: (...args: unknown[]) => inspectMock(...args),
    installLanguagePack: (...args: unknown[]) => installMock(...args),
  },
}));

vi.mock('../../../stores/languagePackStore', () => ({
  useLanguagePackStore: {
    getState: () => ({ fetch: fetchMock }),
  },
}));

const PREVIEW: LanguagePackPreview = {
  pickToken: 'pick-token-1',
  packageLabel: 'datazen-locale-ja.zip',
  manifest: {
    packVersion: 1,
    code: 'ja',
    label: '日本語',
    version: '1.0.0',
    author: 'DataZen',
    description: 'Japanese UI translation',
  },
  messageCount: 12,
  domains: [
    { domain: 'core', messageCount: 7 },
    { domain: 'settings', messageCount: 5 },
  ],
};

describe('InstallLanguagePackDialog', () => {
  beforeEach(() => {
    cleanup();
    inspectMock.mockReset().mockResolvedValue(PREVIEW);
    installMock.mockReset().mockResolvedValue(undefined);
    fetchMock.mockReset().mockResolvedValue(undefined);
    onCloseMock.mockReset();
  });

  it('cancelling the native picker stays on the select step', async () => {
    inspectMock.mockResolvedValue(null);
    render(<InstallLanguagePackDialog open onClose={onCloseMock} />);
    fireEvent.click(screen.getByTestId('language-pack-install-pick'));
    await waitFor(() => expect(inspectMock).toHaveBeenCalled());
    expect(screen.getByTestId('language-pack-install-pick')).toBeTruthy();
    expect(onCloseMock).not.toHaveBeenCalled();
  });

  it('advances to review with manifest and per-domain counts', async () => {
    render(<InstallLanguagePackDialog open onClose={onCloseMock} />);
    fireEvent.click(screen.getByTestId('language-pack-install-pick'));
    await screen.findByTestId('language-pack-review');
    expect(screen.getByTestId('language-pack-review-domains')).toBeTruthy();
    expect(screen.getByTestId('language-pack-domain-core').textContent).toContain('7');
    expect(screen.getByTestId('language-pack-review-file').textContent).toContain(
      'datazen-locale-ja.zip',
    );
  });

  it('confirm installs with the pick token, refreshes the store and closes', async () => {
    render(<InstallLanguagePackDialog open onClose={onCloseMock} />);
    fireEvent.click(screen.getByTestId('language-pack-install-pick'));
    await screen.findByTestId('language-pack-review');
    fireEvent.click(screen.getByTestId('language-pack-install-confirm'));
    await waitFor(() => expect(installMock).toHaveBeenCalledWith('pick-token-1'));
    await waitFor(() => expect(onCloseMock).toHaveBeenCalled());
    expect(fetchMock).toHaveBeenCalled();
  });

  it('a rejected inspection shows the error inline', async () => {
    inspectMock.mockRejectedValue(new Error('invalid language pack: empty label'));
    render(<InstallLanguagePackDialog open onClose={onCloseMock} />);
    fireEvent.click(screen.getByTestId('language-pack-install-pick'));
    const error = await screen.findByTestId('language-pack-install-error');
    expect(error.textContent).toContain('invalid language pack: empty label');
    // Select step is retained so the user can pick another package.
    expect(screen.getByTestId('language-pack-install-pick')).toBeTruthy();
  });

  it('a rejected install returns to select and shows the error', async () => {
    installMock.mockRejectedValue(
      new Error('invalid language pack: locale code conflicts with built-in: en'),
    );
    render(<InstallLanguagePackDialog open onClose={onCloseMock} />);
    fireEvent.click(screen.getByTestId('language-pack-install-pick'));
    await screen.findByTestId('language-pack-review');
    fireEvent.click(screen.getByTestId('language-pack-install-confirm'));
    const error = await screen.findByTestId('language-pack-install-error');
    expect(error.textContent).toContain('conflicts with built-in');
    expect(onCloseMock).not.toHaveBeenCalled();
    expect(screen.getByTestId('language-pack-install-pick')).toBeTruthy();
  });
});
