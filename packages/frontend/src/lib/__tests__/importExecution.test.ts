import { describe, expect, it, vi } from 'vitest';
import {
  buildMultiRowInsertSql,
  createIpcImportTransport,
  executeImport,
  planImportChunks,
  type ImportTransport,
} from '../importExecution';

/**
 * DBR-01 execution semantics: batch planning (row + byte caps, transaction
 * wrapping), and the run loop's stop/skip/cancel behavior with an exact
 * committed-row count in every outcome.
 */

function fakeRows(count: number, make?: (i: number) => Record<string, unknown>) {
  return Array.from({ length: count }, (_, i) => make?.(i) ?? { id: i + 1, name: `row${i + 1}` });
}

const basePlanOptions = {
  table: 't',
  targetColumns: ['id', 'name'],
  databaseType: 'postgresql',
  columnPairs: [
    { source: 'id', target: 'id' },
    { source: 'name', target: 'name' },
  ],
  rows: fakeRows(10),
} as const;

describe('buildMultiRowInsertSql', () => {
  it('builds one statement with every tuple', () => {
    const sql = buildMultiRowInsertSql({
      table: 'users',
      targetColumns: ['id', 'name'],
      databaseType: 'postgresql',
      rows: [
        { id: 1, name: "O'Brien" },
        { id: 2, name: null },
      ],
      rowStart: 0,
      rowCount: 2,
    });
    expect(sql).toBe(
      'INSERT INTO "users" ("id", "name") VALUES\n  (1, \'O\'\'Brien\'),\n  (2, NULL);',
    );
  });

  it('escapes identifiers and values with the dialect escaper', () => {
    const sql = buildMultiRowInsertSql({
      table: 'order items',
      targetColumns: ['select'],
      databaseType: 'postgresql',
      rows: [{ select: 'x' }],
      rowStart: 0,
      rowCount: 1,
    });
    expect(sql).toContain('"order items"');
    expect(sql).toContain('"select"');
  });
});

describe('planImportChunks', () => {
  it('autocommit mode: one batch per chunk, no BEGIN/COMMIT', () => {
    const chunks = planImportChunks({ ...basePlanOptions, commitMode: 'autocommit' });
    expect(chunks).toHaveLength(1);
    expect(chunks[0].batches).toHaveLength(1);
    expect(chunks[0].sql.startsWith('INSERT INTO')).toBe(true);
    expect(chunks[0].sql).not.toContain('BEGIN');
  });

  it('transaction mode: batches wrapped in BEGIN/COMMIT, grouped by cap', () => {
    const chunks = planImportChunks({
      ...basePlanOptions,
      commitMode: 'transaction',
      maxRowsPerBatch: 4,
      maxBatchesPerChunk: 2,
    });
    expect(chunks).toHaveLength(2);
    expect(chunks[0].batches).toHaveLength(2);
    expect(chunks[0].sql.startsWith('BEGIN;')).toBe(true);
    expect(chunks[0].sql.endsWith('COMMIT;')).toBe(true);
    expect(chunks[0].sql.match(/INSERT INTO/g)).toHaveLength(2);
    expect(chunks[0].rowCount).toBe(8);
    expect(chunks[1].rowCount).toBe(2);
  });

  it('respects the row cap per batch', () => {
    const chunks = planImportChunks({
      ...basePlanOptions,
      commitMode: 'autocommit',
      maxRowsPerBatch: 3,
    });
    expect(chunks.map((chunk) => chunk.rowCount)).toEqual([3, 3, 3, 1]);
  });

  it('respects the byte cap per batch', () => {
    const chunks = planImportChunks({
      ...basePlanOptions,
      commitMode: 'autocommit',
      maxRowsPerBatch: 1000,
      maxSqlBytes: 120,
    });
    // 10 rows à ~13 bytes plus a ~37-byte prefix: the byte cap must split the
    // run into several batches, none of them materially over the cap.
    expect(chunks.length).toBeGreaterThan(1);
    for (const chunk of chunks) {
      expect(chunk.sql.length).toBeLessThanOrEqual(120 + 40);
      expect(chunk.rowCount).toBeGreaterThan(0);
    }
  });

  it('projects source columns onto target columns through the mapping', () => {
    const chunks = planImportChunks({
      table: 't',
      targetColumns: ['a', 'b'],
      databaseType: 'postgresql',
      columnPairs: [
        { source: 'src_a', target: 'a' },
        { source: 'src_b', target: 'b' },
      ],
      rows: [{ src_a: 1, src_b: 2, ignored: 'never lands' }],
      commitMode: 'autocommit',
    });
    expect(chunks[0].sql).toContain('(1, 2)');
    expect(chunks[0].sql).not.toContain('never lands');
  });
});

/** A transport script: [{resolve: rows} | {reject: error}] consumed per call. */
function scriptedTransport(script: Array<{ resolve?: number; reject?: Error }>): ImportTransport & {
  calls: string[];
} {
  const calls: string[] = [];
  let index = 0;
  return {
    calls,
    runStream: (sql) => {
      calls.push(sql);
      const step = script[Math.min(index, script.length - 1)];
      index += 1;
      if (step.reject) return Promise.reject(step.reject);
      return Promise.resolve(step.resolve ?? 0);
    },
    cancelInFlight: () => Promise.resolve(),
  };
}

const baseExecuteOptions = {
  table: 't',
  targetColumns: ['id', 'name'],
  databaseType: 'postgresql',
  columnPairs: [
    { source: 'id', target: 'id' },
    { source: 'name', target: 'name' },
  ],
  rowOrigins: [2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
  onError: 'stop',
  commitMode: 'autocommit',
  signal: { aborted: false },
} as const;

describe('executeImport', () => {
  it('commits every chunk and completes', async () => {
    const transport = scriptedTransport([{ resolve: 10 }]);
    const progress: string[] = [];
    const result = await executeImport({
      ...baseExecuteOptions,
      rows: fakeRows(10),
      transport,
      onProgress: (p) => progress.push(p.status),
    });
    expect(result.status).toBe('completed');
    expect(result.committedRows).toBe(10);
    expect(result.failedRows).toEqual([]);
    expect(progress.at(-1)).toBe('completed');
  });

  it('stop strategy: a failed chunk stops the run and names the failing row', async () => {
    const transport = scriptedTransport([
      { resolve: 4 },
      { reject: new Error('[INSERT …] boom') },
    ]);
    const result = await executeImport({
      ...baseExecuteOptions,
      rows: fakeRows(8),
      maxRowsPerBatch: 4,
      transport,
    });
    expect(result.status).toBe('stopped');
    expect(result.committedRows).toBe(4);
    expect(result.stoppedBy?.message).toContain('boom');
    // The failing batch starts at row index 4 → source line 6.
    expect(result.stoppedBy?.sourceLine).toBe(6);
    expect(result.failedRows).toEqual([]);
  });

  it('skip strategy: a failed chunk is retried row by row, failures carry source lines', async () => {
    const transport = scriptedTransport([
      { reject: new Error('chunk failed') }, // chunk 1 fails
      { resolve: 1 }, // row retry succeeds
      { reject: new Error('row bad') }, // row retry fails
      { resolve: 1 }, // row retry succeeds
      { resolve: 2 }, // chunk 2 succeeds
    ]);
    const result = await executeImport({
      ...baseExecuteOptions,
      rows: fakeRows(5),
      maxRowsPerBatch: 3,
      onError: 'skip',
      transport,
    });
    expect(result.status).toBe('completed');
    expect(result.committedRows).toBe(4); // 2 retried + chunk 2's 2 rows
    // rowOrigins[1] is 3: the failing retried row names its source line.
    expect(result.failedRows).toEqual([
      { rowIndex: 1, sourceLine: 3, message: 'row bad' },
    ]);
  });

  it('transaction mode: a failed chunk rolls back and stop reports zero for it', async () => {
    const transport = scriptedTransport([{ reject: new Error('tx aborted') }]);
    const result = await executeImport({
      ...baseExecuteOptions,
      rows: fakeRows(10),
      commitMode: 'transaction',
      transport,
    });
    expect(result.status).toBe('stopped');
    expect(result.committedRows).toBe(0);
    expect(result.stoppedBy?.sourceLine).toBe(2);
  });

  it('cancel between chunks reports the committed prefix as cancelled, not failed', async () => {
    const transport = scriptedTransport([{ resolve: 4 }, { resolve: 4 }]);
    const signal = { aborted: false };
    const result = await executeImport({
      ...baseExecuteOptions,
      rows: fakeRows(8),
      maxRowsPerBatch: 4,
      signal,
      transport,
      onProgress: (p) => {
        // The dialog cancels after the first chunk commits.
        if (p.committedRows === 4) signal.aborted = true;
      },
    });
    expect(result.status).toBe('cancelled');
    expect(result.committedRows).toBe(4);
    expect(result.failedRows).toEqual([]);
    expect(transport.calls).toHaveLength(1);
  });

  it('a rejection after abort is classified as cancellation, not a data failure', async () => {
    const signal = { aborted: false };
    const transport: ImportTransport = {
      runStream: () => {
        signal.aborted = true; // the cancel raced the stream
        return Promise.reject(new Error('Query cancelled'));
      },
      cancelInFlight: () => Promise.resolve(),
    };
    const result = await executeImport({
      ...baseExecuteOptions,
      rows: fakeRows(2),
      signal,
      transport,
    });
    expect(result.status).toBe('cancelled');
    expect(result.committedRows).toBe(0);
  });
});

describe('createIpcImportTransport', () => {
  it('uses the cancellable stream command without recording history', async () => {
    const { queryCommands } = await import('../../commands/query');
    const spy = vi.spyOn(queryCommands, 'executeQueryStream').mockResolvedValue();
    const transport = createIpcImportTransport({
      dbSessionId: 's1',
      database: 'db',
      schema: 'public',
    });
    await transport.runStream('INSERT INTO t VALUES (1);');
    expect(spy).toHaveBeenCalledWith(
      's1',
      'INSERT INTO t VALUES (1);',
      expect.any(Function),
      expect.objectContaining({ recordHistory: false, applyResultLimit: false, database: 'db' }),
    );
    spy.mockRestore();
  });
});
