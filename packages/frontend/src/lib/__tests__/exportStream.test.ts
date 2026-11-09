import { describe, expect, it, vi } from 'vitest';
import type { QueryStreamEvent } from '../../types';
import {
  buildTableSelectSql,
  createTableExportStreamer,
  streamTableExportText,
  streamTableExportToSaveDialog,
  type TableExportRunHandle,
} from '../exportStream';

function emitAll(
  events: QueryStreamEvent[],
  onEvent: (event: QueryStreamEvent) => void,
): Promise<void> {
  for (const event of events) onEvent(event);
  return Promise.resolve();
}

const usersEvents: QueryStreamEvent[] = [
  {
    type: 'statementStart',
    index: 0,
    sql: 'SELECT * FROM users',
    columns: [
      { name: 'id', dataType: 'int', nullable: false },
      { name: 'name', dataType: 'text', nullable: true },
    ],
  },
  {
    type: 'rows',
    index: 0,
    rows: [
      [1, 'Alice'],
      [2, 'Bob'],
    ],
  },
  {
    type: 'statementEnd',
    index: 0,
    executionTimeMs: 1,
    truncated: false,
  },
  { type: 'done', totalTimeMs: 1 },
];

describe('buildTableSelectSql', () => {
  it('quotes postgres identifiers', () => {
    expect(buildTableSelectSql('users', ['id', 'name'], 'postgresql')).toBe(
      'SELECT "id", "name" FROM "users"',
    );
  });

  it('uses star when no columns', () => {
    expect(buildTableSelectSql('t', [], 'sqlite')).toBe('SELECT * FROM "t"');
  });
});

describe('createTableExportStreamer', () => {
  it('streams valid json array', () => {
    const s = createTableExportStreamer({
      format: 'json',
      tableName: 'users',
      columns: ['id', 'name'],
    });
    const body =
      s.header() +
      s.formatRows([
        { id: 1, name: 'A' },
        { id: 2, name: 'B' },
      ]) +
      s.footer();
    expect(JSON.parse(body)).toEqual([
      { id: 1, name: 'A' },
      { id: 2, name: 'B' },
    ]);
  });

  it('streams csv with header then rows', () => {
    const s = createTableExportStreamer({
      format: 'csv',
      tableName: 'users',
      columns: ['id', 'name'],
    });
    const body =
      s.header() +
      s.formatRows([{ id: 1, name: 'A' }]) +
      s.formatRows([{ id: 2, name: 'B' }]) +
      s.footer();
    expect(body.trim()).toBe('id,name\n1,A\n2,B');
  });

  it('streams batched sql insert in a transaction', () => {
    const s = createTableExportStreamer({
      format: 'sql_insert',
      tableName: 'users',
      columns: ['id'],
      databaseType: 'postgresql',
    });
    const body = s.header() + s.formatRows([{ id: 1 }, { id: 2 }]) + s.footer();
    expect(body.startsWith('BEGIN;')).toBe(true);
    expect(body).toContain('INSERT INTO "users" ("id") VALUES');
    expect(body).toContain('COMMIT;');
    expect(body.match(/INSERT INTO/g)?.length).toBe(1);
  });
});

describe('streamTableExportText', () => {
  it('consumes query_stream events into json', async () => {
    const streamQuery = vi.fn(async (_id, _sql, onEvent) => {
      await emitAll(usersEvents, onEvent);
    });
    const text = await streamTableExportText({
      dbSessionId: 'c1',
      tableName: 'users',
      columns: ['id', 'name'],
      format: 'json',
      streamQuery,
    });
    expect(JSON.parse(text)).toEqual([
      { id: 1, name: 'Alice' },
      { id: 2, name: 'Bob' },
    ]);
    expect(streamQuery).toHaveBeenCalledWith(
      'c1',
      expect.stringContaining('FROM'),
      expect.any(Function),
      { applyResultLimit: false, recordHistory: false },
    );
  });
});

describe('streamTableExportToSaveDialog', () => {
  it('writes chunks then finishes', async () => {
    const chunks: string[] = [];
    const saveSession = {
      begin: vi.fn().mockResolvedValue('tok'),
      append: vi.fn(async (_token: string, chunk: string) => {
        chunks.push(chunk);
      }),
      finish: vi.fn().mockResolvedValue(undefined),
      abort: vi.fn().mockResolvedValue(true),
    };
    const result = await streamTableExportToSaveDialog({
      dbSessionId: 'c1',
      tableName: 'users',
      columns: ['id', 'name'],
      format: 'csv',
      streamQuery: async (_id, _sql, onEvent) => {
        await emitAll(usersEvents, onEvent);
      },
      saveSession,
    });
    expect(result).toEqual({ status: 'saved' });
    expect(saveSession.begin).toHaveBeenCalled();
    expect(saveSession.finish).toHaveBeenCalledWith('tok');
    expect(chunks.join('').trim()).toBe('id,name\n1,Alice\n2,Bob');
  });

  it('returns cancelled when dialog is dismissed', async () => {
    const result = await streamTableExportToSaveDialog({
      dbSessionId: 'c1',
      tableName: 'users',
      columns: ['id'],
      format: 'json',
      saveSession: {
        begin: vi.fn().mockResolvedValue(null),
        append: vi.fn(),
        finish: vi.fn(),
        abort: vi.fn().mockResolvedValue(true),
      },
    });
    expect(result).toEqual({ status: 'cancelled' });
  });

  /** DBR-06: a mid-run cancel is its own end state — aborted, with the
   * backend's honest answer about the partial file, and no `finish`. */
  it('reports an aborted run with the partial file deleted', async () => {
    const saveSession = {
      begin: vi.fn().mockResolvedValue('tok'),
      append: vi.fn(),
      finish: vi.fn(),
      abort: vi.fn().mockResolvedValue(true),
    };
    const cancelQuery = vi.fn().mockResolvedValue(undefined);
    let run: TableExportRunHandle | undefined;
    const result = await streamTableExportToSaveDialog({
      dbSessionId: 'c1',
      tableName: 'users',
      columns: ['id'],
      format: 'csv',
      streamQuery: async (_id, _sql, onEvent) => {
        onEvent({ type: 'executionStarted', executionId: 'exec-1' });
        onEvent({
          type: 'statementStart',
          index: 0,
          sql: 'SELECT id FROM users',
          columns: [{ name: 'id', dataType: 'int', nullable: false }],
        });
        onEvent({ type: 'rows', index: 0, rows: [[1]] });
        run?.cancel();
        // Late rows after the cancel must be dropped, not written.
        onEvent({ type: 'rows', index: 0, rows: [[2]] });
      },
      saveSession,
      cancelQuery,
      onRunStart: (handle) => {
        run = handle;
      },
    });
    expect(result).toEqual({ status: 'aborted', partialDeleted: true });
    expect(cancelQuery).toHaveBeenCalledWith('exec-1');
    expect(saveSession.abort).toHaveBeenCalledWith('tok');
    expect(saveSession.finish).not.toHaveBeenCalled();
  });

  /** The abort answer is reported as the backend gave it, not assumed. */
  it('reports an aborted run whose partial file was kept', async () => {
    const saveSession = {
      begin: vi.fn().mockResolvedValue('tok'),
      append: vi.fn(),
      finish: vi.fn(),
      abort: vi.fn().mockResolvedValue(false),
    };
    let run: TableExportRunHandle | undefined;
    const result = await streamTableExportToSaveDialog({
      dbSessionId: 'c1',
      tableName: 'users',
      columns: ['id'],
      format: 'csv',
      streamQuery: async (_id, _sql, onEvent) => {
        onEvent({ type: 'rows', index: 0, rows: [[1]] });
        run?.cancel();
      },
      saveSession,
      onRunStart: (handle) => {
        run = handle;
      },
    });
    expect(result).toEqual({ status: 'aborted', partialDeleted: false });
  });

  /** DBR-06: 大型导出任务显示行数进度 — one `onProgress` per rows event,
   * carrying the accumulated count. */
  it('reports rows-written progress per rows event', async () => {
    const onProgress = vi.fn();
    await streamTableExportToSaveDialog({
      dbSessionId: 'c1',
      tableName: 'users',
      columns: ['id'],
      format: 'csv',
      streamQuery: async (_id, _sql, onEvent) => {
        onEvent({
          type: 'statementStart',
          index: 0,
          sql: 'SELECT id FROM users',
          columns: [{ name: 'id', dataType: 'int', nullable: false }],
        });
        onEvent({ type: 'rows', index: 0, rows: [[1], [2]] });
        onEvent({ type: 'rows', index: 0, rows: [[3]] });
      },
      saveSession: {
        begin: vi.fn().mockResolvedValue('tok'),
        append: vi.fn(),
        finish: vi.fn(),
        abort: vi.fn().mockResolvedValue(true),
      },
      onProgress,
    });
    expect(onProgress).toHaveBeenNthCalledWith(1, 2);
    expect(onProgress).toHaveBeenNthCalledWith(2, 3);
  });

  /** DBR-06 列选择 × 过滤范围：the filtered scope streams the backend's own
   * full-column statement; the streamer must still write only the picked
   * columns, keyed by the statement's column names — not by position. */
  it('writes only the selected columns of a wider streamed statement', async () => {
    const chunks: string[] = [];
    const result = await streamTableExportToSaveDialog({
      dbSessionId: 'c1',
      tableName: 'users',
      columns: ['name'],
      format: 'csv',
      sql: 'SELECT "id", "name" FROM "users"',
      streamQuery: async (_id, _sql, onEvent) => {
        onEvent({
          type: 'statementStart',
          index: 0,
          sql: 'SELECT "id", "name" FROM "users"',
          columns: [
            { name: 'id', dataType: 'int', nullable: false },
            { name: 'name', dataType: 'text', nullable: true },
          ],
        });
        onEvent({
          type: 'rows',
          index: 0,
          rows: [
            [1, 'Alice'],
            [2, 'Bob'],
          ],
        });
      },
      saveSession: {
        begin: vi.fn().mockResolvedValue('tok'),
        append: vi.fn(async (_t: string, chunk: string) => {
          chunks.push(chunk);
        }),
        finish: vi.fn(),
        abort: vi.fn().mockResolvedValue(true),
      },
    });
    expect(result).toEqual({ status: 'saved' });
    const body = chunks.join('');
    expect(body.trim()).toBe('name\nAlice\nBob');
    expect(body).not.toContain('id');
    expect(body).not.toContain('1');
  });

  /** A disk failure is its own state: the session is aborted, the error
   * propagates — neither `saved` nor a cancel outcome (DBR-06). */
  it('aborts the session and rethrows on a write failure', async () => {
    const saveSession = {
      begin: vi.fn().mockResolvedValue('tok'),
      append: vi.fn().mockRejectedValue(new Error('disk full')),
      finish: vi.fn(),
      abort: vi.fn().mockResolvedValue(true),
    };
    await expect(
      streamTableExportToSaveDialog({
        dbSessionId: 'c1',
        tableName: 'users',
        columns: ['id'],
        format: 'csv',
        streamQuery: async (_id, _sql, onEvent) => {
          onEvent({
            type: 'statementStart',
            index: 0,
            sql: 'SELECT id FROM users',
            columns: [{ name: 'id', dataType: 'int', nullable: false }],
          });
          onEvent({ type: 'rows', index: 0, rows: [[1]] });
        },
        saveSession,
      }),
    ).rejects.toThrow('disk full');
    expect(saveSession.abort).toHaveBeenCalledWith('tok');
    expect(saveSession.finish).not.toHaveBeenCalled();
  });
});
