import { beforeEach, describe, expect, it, vi } from 'vitest';
import { historyCommands } from '../history';

const invokeMock = vi.hoisted(() => vi.fn());
import { createBackendClient, setBackendClient } from '@datazen/backend-client';
// form-split R6: the invoke wrappers ride the bound backend transport now
// (same binding pattern as file.test.ts — the mock keeps its invoke shape).
// W8 刀3e web 收口：迁移历史读面走 dataSync 域门面——真门面架在假
// transport 上，listMigrationRuns / getMigrationRun 经 guard → transport.call。
setBackendClient(
  'test',
  createBackendClient('test', {
    call: (c: string, p?: unknown) => (p === undefined ? invokeMock(c) : invokeMock(c, p)),
  } as never) as never,
);
describe('migration history commands', () => {
  beforeEach(() => invokeMock.mockReset());

  it('uses bounded pagination filters and fetches detail by run id', async () => {
    invokeMock.mockResolvedValueOnce({ items: [], total: 0, offset: 25, limit: 25 });
    invokeMock.mockResolvedValueOnce({ id: 'run-1' });

    await historyCommands.listMigrationRuns({ operation: 'dataSync', status: 'failed' }, 25, 25);
    await historyCommands.getMigrationRun('run-1');

    expect(invokeMock).toHaveBeenNthCalledWith(1, 'listMigrationRuns', {
      filter: { operation: 'dataSync', status: 'failed' },
      offset: 25,
      limit: 25,
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'getMigrationRun', { runId: 'run-1' });
  });

  it('purgeHistory rides the queryAssets domain with a saveUserAsset token (W8 刀3a rail)', async () => {
    invokeMock.mockResolvedValueOnce({
      idempotencyKey: 'test-key',
      expiresAt: '2026-01-01T00:00:00Z',
    });
    invokeMock.mockResolvedValueOnce(4);

    await historyCommands.purgeHistory({ scope: 'query', retainDays: 30 });

    expect(invokeMock).toHaveBeenNthCalledWith(1, 'issueSubmissionToken', {
      operation: 'saveUserAsset',
      handle: null,
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'purgeHistory', {
      scope: 'query',
      retainDays: 30,
      submissionToken: { idempotencyKey: 'test-key', expiresAt: '2026-01-01T00:00:00Z' },
    });
  });
});
