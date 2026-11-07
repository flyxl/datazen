import { beforeEach, describe, expect, it, vi } from 'vitest';
import { historyCommands } from '../history';

const invokeMock = vi.hoisted(() => vi.fn());
import { setBackendClient } from '@datazen/backend-client';
// form-split R6: the invoke wrappers ride the bound backend transport now
// (same binding pattern as file.test.ts — the mock keeps its invoke shape).
setBackendClient('test', {
  transport: {
    call: (c: string, p?: unknown) =>
      p === undefined ? invokeMock(c) : invokeMock(c, p),
  },
} as never);
describe('migration history commands', () => {
  beforeEach(() => invokeMock.mockReset());

  it('uses bounded pagination filters and fetches detail by run id', async () => {
    invokeMock.mockResolvedValueOnce({ items: [], total: 0, offset: 25, limit: 25 });
    invokeMock.mockResolvedValueOnce({ id: 'run-1' });

    await historyCommands.listMigrationRuns({ operation: 'dataSync', status: 'failed' }, 25, 25);
    await historyCommands.getMigrationRun('run-1');

    expect(invokeMock).toHaveBeenNthCalledWith(1, 'list_migration_runs', {
      filter: { operation: 'dataSync', status: 'failed' }, offset: 25, limit: 25,
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'get_migration_run', { runId: 'run-1' });
  });
});
