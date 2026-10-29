import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { GlobalObjectSearch } from '../navigator/GlobalObjectSearch';
import { QueryExecutionStatus } from '../../../components/query/QueryExecutionStatus';
import type { ObjectSearchResult } from '../../../lib/schemaObjectSearch';
import type { QueryExecutionViewModel } from '../../../lib/queryExecutionViewModel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string>) =>
      params ? `${key}:${Object.values(params).join(',')}` : key,
  }),
}));

afterEach(cleanup);

const objectIndex = [
  {
    connectionId: 'connection-1',
    dbSessionId: 'session-1',
    databaseType: 'postgresql',
    connectionName: 'Local database',
    host: 'db.example.test',
    database: 'sales',
    schema: 'public',
    tables: [{ name: 'orders', schema: 'public', tableType: 'table' }],
    columnMap: { orders: ['id', 'customer_email'] },
  },
] as const;

describe('page integration surfaces', () => {
  it('searches loaded schema objects and routes table actions with the matched result', () => {
    const onOpenResult = vi.fn<(result: ObjectSearchResult) => void>();
    const onOpenTableAction = vi.fn();
    render(
      <GlobalObjectSearch
        open
        index={objectIndex}
        onClose={vi.fn()}
        onOpenResult={onOpenResult}
        onOpenTableAction={onOpenTableAction}
      />,
    );

    const input = screen.getByTestId('global-object-search-input');
    fireEvent.change(input, { target: { value: 'customer_email' } });
    expect(screen.getByText('customer_email')).toBeInTheDocument();
    expect(screen.getByText(/objectSearch\.matched:column/)).toBeInTheDocument();
    fireEvent.click(screen.getByText('customer_email'));
    expect(onOpenResult).toHaveBeenCalledWith(
      expect.objectContaining({ objectType: 'column', tableName: 'orders', name: 'customer_email' }),
    );

    fireEvent.change(input, { target: { value: 'orders' } });
    fireEvent.click(screen.getByTestId('object-search-action-openData'));
    expect(onOpenTableAction).toHaveBeenCalledWith(expect.objectContaining({ name: 'orders' }), 'openData');
  });

  it('exposes cancel only for a supported execution and keeps terminal cancel state truthful', () => {
    const onCancel = vi.fn();
    const running: QueryExecutionViewModel = {
      phase: 'running',
      cancelCapability: 'supported',
      cancelState: 'available',
      elapsedMs: 125,
      rowCount: 4,
      affectedRows: null,
      error: null,
      executionId: 'execution-1',
    };
    const { rerender } = render(<QueryExecutionStatus viewModel={running} onCancel={onCancel} />);
    expect(screen.getByRole('status')).toHaveTextContent('query.running');
    fireEvent.click(screen.getByRole('button'));
    expect(onCancel).toHaveBeenCalledOnce();

    rerender(
      <QueryExecutionStatus
        viewModel={{ ...running, phase: 'cancel_requested', cancelState: 'requested' }}
        onCancel={onCancel}
      />,
    );
    expect(screen.getByRole('button')).toBeDisabled();

    rerender(
      <QueryExecutionStatus
        viewModel={{ ...running, phase: 'cancelled', cancelState: 'unavailable', executionId: null }}
        onCancel={onCancel}
      />,
    );
    expect(screen.getByRole('status')).toHaveTextContent('query.cancelled');
    expect(screen.queryByRole('button')).toBeNull();
  });
});
