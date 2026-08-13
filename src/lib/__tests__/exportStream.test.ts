import { describe, expect, it, vi } from 'vitest';
import type { QueryStreamEvent } from '../../types';
import {
  buildTableSelectSql,
  createTableExportStreamer,
  streamTableExportText,
  streamTableExportToSaveDialog,
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
      { name: 'id', dataType: 'int' },
      { name: 'name', dataType: 'text' },
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
      connectionId: 'c1',
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
      abort: vi.fn().mockResolvedValue(undefined),
    };
    const result = await streamTableExportToSaveDialog({
      connectionId: 'c1',
      tableName: 'users',
      columns: ['id', 'name'],
      format: 'csv',
      streamQuery: async (_id, _sql, onEvent) => {
        await emitAll(usersEvents, onEvent);
      },
      saveSession,
    });
    expect(result).toBe('saved');
    expect(saveSession.begin).toHaveBeenCalled();
    expect(saveSession.finish).toHaveBeenCalledWith('tok');
    expect(chunks.join('').trim()).toBe('id,name\n1,Alice\n2,Bob');
  });

  it('returns cancelled when dialog is dismissed', async () => {
    const result = await streamTableExportToSaveDialog({
      connectionId: 'c1',
      tableName: 'users',
      columns: ['id'],
      format: 'json',
      saveSession: {
        begin: vi.fn().mockResolvedValue(null),
        append: vi.fn(),
        finish: vi.fn(),
        abort: vi.fn(),
      },
    });
    expect(result).toBe('cancelled');
  });
});
