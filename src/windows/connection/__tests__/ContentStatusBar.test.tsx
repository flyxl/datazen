import { render, screen, cleanup } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { ContentStatusBar } from '../ContentStatusBar';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

const usePlatformMock = vi.fn(() => 'macos' as 'macos' | 'windows' | 'linux' | 'unknown');

vi.mock('../../../hooks/usePlatform', () => ({
  usePlatform: () => usePlatformMock(),
}));

beforeEach(() => {
  usePlatformMock.mockReturnValue('macos');
});

afterEach(() => {
  cleanup();
});

describe('ContentStatusBar', () => {
  it('exposes status semantics and the active database context', () => {
    render(
      <ContentStatusBar
        databaseType="redis"
        connectionName="Redis local"
        currentDatabase="db5"
        tableName=""
        columnCount={0}
        totalRows={0}
      />,
    );

    const status = screen.getByRole('status');
    expect(status).toHaveTextContent('connWin.connected');
    expect(status).toHaveTextContent('Redis local · db5');
    expect(status.querySelector('.bg-success')).not.toBeNull();
  });

  it('shows Mac modifier labels on macOS', () => {
    usePlatformMock.mockReturnValue('macos');
    render(
      <ContentStatusBar
        currentDatabase={null}
        tableName=""
        columnCount={0}
        totalRows={0}
      />,
    );

    const status = screen.getByRole('status');
    expect(status).toHaveTextContent('⌘N');
    expect(status).toHaveTextContent('⌘W');
  });

  it('shows Ctrl modifier labels on Windows and Linux', () => {
    usePlatformMock.mockReturnValue('windows');
    render(
      <ContentStatusBar
        currentDatabase={null}
        tableName=""
        columnCount={0}
        totalRows={0}
      />,
    );

    let status = screen.getByRole('status');
    expect(status).toHaveTextContent('Ctrl+N');
    expect(status).toHaveTextContent('Ctrl+W');

    cleanup();
    usePlatformMock.mockReturnValue('linux');
    render(
      <ContentStatusBar
        currentDatabase={null}
        tableName=""
        columnCount={0}
        totalRows={0}
      />,
    );

    status = screen.getByRole('status');
    expect(status).toHaveTextContent('Ctrl+N');
    expect(status).toHaveTextContent('Ctrl+W');
  });
});
