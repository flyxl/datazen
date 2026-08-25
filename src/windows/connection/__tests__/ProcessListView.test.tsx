import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen } from '@testing-library/react';
import type { ReactNode } from 'react';

const executeMock = vi.hoisted(() => vi.fn());

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(false), null],
}));

vi.mock('../../../commands/driver', () => ({
  driverCommands: {
    execute: (...args: unknown[]) => executeMock(...args),
  },
}));

vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: ({ rows }: { rows: unknown[][] }) => (
    <div data-testid="process-rows">{JSON.stringify(rows)}</div>
  ),
}));

vi.mock('../../../components/ui/Button', () => ({
  Button: ({ children, ...rest }: { children?: ReactNode }) => (
    <button type="button" {...rest}>
      {children}
    </button>
  ),
}));

vi.mock('../../../components/ui/CopyableError', () => ({
  CopyableError: ({ message }: { message: string }) => <div>{message}</div>,
}));

import { ProcessListView } from '../ProcessListView';

describe('ProcessListView connection binding', () => {
  beforeEach(() => {
    executeMock.mockReset();
    executeMock.mockImplementation(async ({ dbSessionId }: { dbSessionId: string }) => ({
      data: {
        columns: [{ name: 'pid', dataType: 'int' }],
        rows: [[dbSessionId === 'conn-pg' ? 111 : 222]],
      },
    }));
  });

  afterEach(() => {
    cleanup();
  });

  it('reloads when connectionId changes so PG/MySQL tabs do not share rows', async () => {
    const { rerender } = render(<ProcessListView connectionId="conn-pg" connectionName="pg" />);

    await waitFor(() => {
      expect(executeMock).toHaveBeenCalledWith(
        expect.objectContaining({ dbSessionId: 'conn-pg', command: 'list_processes' }),
      );
    });
    await waitFor(() => {
      expect(screen.getByTestId('process-rows').textContent).toContain('111');
    });

    rerender(<ProcessListView connectionId="conn-mysql" connectionName="mysql" />);

    await waitFor(() => {
      expect(executeMock).toHaveBeenCalledWith(
        expect.objectContaining({ dbSessionId: 'conn-mysql', command: 'list_processes' }),
      );
    });
    await waitFor(() => {
      expect(screen.getByTestId('process-rows').textContent).toContain('222');
      expect(screen.getByTestId('process-rows').textContent).not.toContain('111');
    });
  });
});
