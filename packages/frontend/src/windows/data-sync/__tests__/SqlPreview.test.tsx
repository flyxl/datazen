import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SqlPreview } from '../SqlPreview';
import type { DataSyncSqlStatement } from '../../../commands/sync';
const generate = vi.hoisted(() => vi.fn());
vi.mock('../../../commands/sync', () => ({ syncCommands: { generateDataSyncSql: generate } }));
vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) =>
      key === 'sync.sqlPreviewLimitReached'
        ? 'SQL preview is limited to 16 MiB. Select fewer rows or operations to inspect the SQL. You can still review the row comparison and execute the reviewed changes.'
        : key,
  }),
}));
afterEach(() => {
  cleanup();
  generate.mockReset();
});
const props = {
  sourceConnId: 'source',
  targetConnId: 'target',
  sourceDatabase: 'src',
  targetDatabase: 'tgt',
  sourceSchema: '',
  targetSchema: '',
  tables: [],
  options: { insert: true, update: true, delete: false },
};
const statement: DataSyncSqlStatement = {
  table: 'users',
  operation: 'INSERT',
  sql: 'INSERT INTO users VALUES (?)',
  previewSql: 'INSERT INTO users VALUES (1)',
  parameters: [1],
  rowKey: [1],
};
describe('[tester] SQL preview request lifetime', () => {
  it('ignores a stale success after endpoint revision and exposes current generation failure', async () => {
    let oldResolve!: (rows: DataSyncSqlStatement[]) => void;
    generate.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          oldResolve = resolve;
        }),
    );
    const view = render(<SqlPreview {...props} />);
    generate.mockRejectedValueOnce(new Error('target disconnected'));
    view.rerender(<SqlPreview {...props} targetConnId="new-target" />);
    expect(await screen.findByRole('alert')).toHaveTextContent('target disconnected');
    await act(async () => oldResolve([statement]));
    expect(screen.queryByText(/INSERT INTO/)).toBeNull();
    expect(screen.getByRole('alert')).toHaveTextContent('target disconnected');
  });
  it('refresh recovers a failure, filters and copies only the current reviewed SQL', async () => {
    generate.mockRejectedValueOnce('missing projection');
    render(<SqlPreview {...props} />);
    await screen.findByRole('alert');
    generate.mockResolvedValue([statement]);
    fireEvent.click(screen.getByText('sync.refreshPreview'));
    await screen.findByText(/INSERT INTO users VALUES \(1\)/);
    expect(screen.queryByRole('alert')).toBeNull();
    fireEvent.click(screen.getByText('UPDATE'));
    expect(screen.queryByText(/INSERT INTO users VALUES/)).toBeNull();
    fireEvent.click(screen.getByText('INSERT'));
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    fireEvent.click(screen.getByText('common.copy'));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(
        expect.stringContaining('INSERT INTO users VALUES (1)'),
      ),
    );
  });

  it('explains the SQL preview size limit while keeping the comparison actionable', async () => {
    generate.mockRejectedValueOnce(
      new Error(
        'Data Sync SQL preview exceeds the 16 MiB IPC limit; select fewer rows or operations',
      ),
    );

    render(<SqlPreview {...props} />);

    const notice = await screen.findByRole('alert');
    expect(notice).toHaveTextContent('Select fewer rows or operations to inspect the SQL.');
    expect(notice).toHaveTextContent(
      'You can still review the row comparison and execute the reviewed changes.',
    );
    expect(notice).not.toHaveTextContent('IPC limit');
  });
});
