import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { DataCleanupSection } from '../DataCleanupSection';

const purgeHistoryMock = vi.fn();
const confirmCleanupFn = vi.fn().mockResolvedValue(true);

vi.mock('../../../commands/history', () => ({
  historyCommands: {
    purgeHistory: (...args: unknown[]) => purgeHistoryMock(...args),
  },
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) => {
      if (params) {
        return `${key}:${JSON.stringify(params)}`;
      }
      return key;
    },
  }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [confirmCleanupFn, null],
}));

describe('DataCleanupSection', () => {
  beforeEach(() => {
    purgeHistoryMock.mockReset();
    confirmCleanupFn.mockResolvedValue(true);
  });

  afterEach(() => {
    cleanup();
  });

  it('requires at least one scope', async () => {
    render(<DataCleanupSection />);
    fireEvent.click(screen.getByTestId('data-cleanup-scope-query'));
    fireEvent.click(screen.getByTestId('data-cleanup-scope-workflow'));
    fireEvent.click(screen.getByTestId('data-cleanup-run'));
    expect(await screen.findByText('settings.dataCleanup.noScope')).toBeTruthy();
    expect(purgeHistoryMock).not.toHaveBeenCalled();
  });

  it('calls purge_history with preset retention', async () => {
    purgeHistoryMock.mockResolvedValue(3);
    render(<DataCleanupSection />);
    fireEvent.click(screen.getByTestId('data-cleanup-preset-30'));
    fireEvent.click(screen.getByTestId('data-cleanup-run'));
    await waitFor(() => {
      expect(purgeHistoryMock).toHaveBeenCalledWith({ scope: 'all', retainDays: 30 });
    });
  });

  it('calls purge_history with null retainDays for clear all', async () => {
    purgeHistoryMock.mockResolvedValue(5);
    render(<DataCleanupSection />);
    fireEvent.click(screen.getByTestId('data-cleanup-clear-all'));
    fireEvent.click(screen.getByTestId('data-cleanup-run'));
    await waitFor(() => {
      expect(purgeHistoryMock).toHaveBeenCalledWith({ scope: 'all', retainDays: null });
    });
  });
});
