import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const mockGetUpdateChannel = vi.fn();
const mockCurrentVariant = vi.fn();
const mockCheckForUpdates = vi.fn();
const mockDownloadAndInstallUpdate = vi.fn();
const mockIsUpdaterSupported = vi.fn();
const mockOpenPath = vi.fn();

vi.mock('../../../lib/updater', () => ({
  getUpdateChannel: () => mockGetUpdateChannel(),
  currentVariant: () => mockCurrentVariant(),
  checkForUpdates: (...args: unknown[]) => mockCheckForUpdates(...args),
  downloadAndInstallUpdate: (...args: unknown[]) => mockDownloadAndInstallUpdate(...args),
  isUpdaterSupported: () => mockIsUpdaterSupported(),
}));

vi.mock('../../../commands/settings', () => ({
  settingsCommands: { openPath: (...args: unknown[]) => mockOpenPath(...args) },
}));

import { UpdateSection } from '../UpdateSection';

function renderSection(channel: 'auto' | 'manual' | 'none') {
  mockGetUpdateChannel.mockReturnValue(channel);
  mockIsUpdaterSupported.mockReturnValue(channel === 'auto');
  mockCurrentVariant.mockReturnValue('all');
  return render(<UpdateSection checkOnStartup={false} onCheckOnStartupChange={() => {}} />);
}

describe('UpdateSection by update channel', () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('renders the in-app updater only when the SKU has a channel', () => {
    renderSection('auto');
    expect(screen.getByText('settings.updater.check')).toBeTruthy();
    expect(screen.getByText('settings.updater.downloadInstall')).toBeTruthy();
    expect(screen.queryByText('settings.updater.openReleases')).toBeNull();
  });

  it('offers a manual download for a SKU with no channel', () => {
    // This is the branch that keeps the failure honest: before it, a variant
    // build hid the update UI entirely and the user had no way to know which
    // build they were on.
    renderSection('manual');
    expect(screen.getByText('settings.updater.title')).toBeTruthy();
    expect(screen.getByText('settings.updater.openReleases')).toBeTruthy();
    // The explanation lives in a hover hint, like every other settings hint.
    fireEvent.mouseEnter(screen.getByRole('button', { name: 'settings.updater.title' }));
    expect(screen.getByRole('tooltip').textContent).toBe('settings.updater.manualDescription');
    // The variant is named so the user can pick the matching installer.
    expect(screen.getByText('settings.updater.manualVariant')).toBeTruthy();
    // No self-update affordances at all.
    expect(screen.queryByText('settings.updater.check')).toBeNull();
    expect(screen.queryByText('settings.updater.downloadInstall')).toBeNull();
    expect(screen.queryByText('settings.updater.checkOnStartup')).toBeNull();
  });

  it('opens the Releases page on click', () => {
    renderSection('manual');
    screen.getByText('settings.updater.openReleases').click();
    expect(mockOpenPath).toHaveBeenCalledWith('https://github.com/flyxl/datazen/releases');
  });

  it('renders nothing outside a desktop build', () => {
    const { container } = renderSection('none');
    expect(container.textContent).toBe('');
  });
});
