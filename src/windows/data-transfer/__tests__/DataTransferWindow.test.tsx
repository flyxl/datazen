import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock('../../hooks/useSettings', () => ({
  useSettings: () => undefined,
}));

vi.mock('../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: () => void }) => unknown) =>
    sel({ loadSettings: vi.fn() }),
}));

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getDatabases: vi.fn().mockResolvedValue([]),
  },
}));

describe('DataTransferWindow', () => {
  it('renders wizard shell', async () => {
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);
    expect(screen.getByTestId('data-transfer-window')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-step-endpoints')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-source')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-target')).toBeTruthy();
  });
});
