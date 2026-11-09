import { describe, expect, it, vi } from 'vitest';
import type { QueryStreamEvent } from '../../types';
import {
  buildTableSelectSql,
  createTableExportStreamer,
  isStreamableExportFormat,
  streamQueryIntoExporter,
  streamTableExportToSaveDialog,
} from '../exportStream';
import { SQL_INSERT_BATCH_SIZE } from '../exportData';

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
      dbSessionId: 'c1',
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
      dbSessionId: 'c1',
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

describe('isStreamableExportFormat', () => {
  it('rejects xlsx and accepts the text formats', () => {
    expect(isStreamableExportFormat('xlsx')).toBe(false);
    expect(isStreamableExportFormat('csv')).toBe(true);
    expect(isStreamableExportFormat('sql_update')).toBe(true);
  });
});

describe('createTableExportStreamer tsv', () => {
  it('sanitizes tabs and line breaks, renders nulls/undefined/objects', () => {
    const s = createTableExportStreamer({
      format: 'tsv',
      tableName: 't',
      columns: ['a', 'b'],
    });
    const body =
      s.header() +
      s.formatRows([
        { a: 'x\ty', b: null },
        { a: { k: 1 }, b: 'line1\nline2\r\n' },
        { a: undefined, b: 'z' },
      ]) +
      s.footer();
    expect(body).toBe('a\tb\nx y\t\n{"k":1}\tline1 line2 \n\tz\n');
    expect(s.formatRows([])).toBe('');
  });
});

describe('createTableExportStreamer markdown', () => {
  it('renders the header separator and rows', () => {
    const s = createTableExportStreamer({
      format: 'markdown',
      tableName: 't',
      columns: ['id', 'name'],
    });
    const body = s.header() + s.formatRows([{ id: 1, name: 'A' }]) + s.footer();
    expect(body).toBe('| id | name |\n| --- | --- |\n| 1 | A |\n');
    expect(s.formatRows([])).toBe('');
  });
});

describe('createTableExportStreamer json', () => {
  it('continues the array across formatRows calls', () => {
    const s = createTableExportStreamer({
      format: 'json',
      tableName: 't',
      columns: ['id'],
    });
    const body = s.header() + s.formatRows([{ id: 1 }]) + s.formatRows([{ id: 2 }]) + s.footer();
    expect(JSON.parse(body)).toEqual([{ id: 1 }, { id: 2 }]);
    expect(s.formatRows([])).toBe('');
  });
});

describe('createTableExportStreamer sql_update', () => {
  it('emits UPDATE statements keyed by pkName', () => {
    const s = createTableExportStreamer({
      format: 'sql_update',
      tableName: 'users',
      columns: ['id', 'name'],
      databaseType: 'postgresql',
      pkName: 'id',
    });
    const body = s.header() + s.formatRows([{ id: 1, name: 'Al' }]) + s.footer();
    expect(body.startsWith('BEGIN;')).toBe(true);
    expect(body).toContain('UPDATE "users" SET "name" = \'Al\' WHERE "id" = 1;');
    expect(body).toContain('COMMIT;');
  });

  it('defaults the key to the first column, or to id when there are none', () => {
    const s = createTableExportStreamer({
      format: 'sql_update',
      tableName: 't',
      columns: ['uid', 'v'],
      databaseType: 'postgresql',
    });
    expect(s.formatRows([{ uid: 7, v: 'x' }])).toContain('WHERE "uid" = 7;');
    const noColumns = createTableExportStreamer({
      format: 'sql_update',
      tableName: 't',
      columns: [],
      databaseType: 'postgresql',
    });
    expect(noColumns.formatRows([{ id: 3 }])).toContain('WHERE "id" = 3;');
    expect(s.formatRows([])).toBe('');
  });
});

describe('createTableExportStreamer sql_insert batching', () => {
  it('flushes full batches while streaming and the remainder on footer', () => {
    const s = createTableExportStreamer({
      format: 'sql_insert',
      tableName: 'users',
      columns: ['id'],
      databaseType: 'postgresql',
    });
    const rows = Array.from({ length: SQL_INSERT_BATCH_SIZE }, (_, i) => ({ id: i }));
    const first = s.formatRows(rows);
    expect(first.match(/INSERT INTO/g)?.length).toBe(1);
    expect(s.formatRows([{ id: SQL_INSERT_BATCH_SIZE }])).toBe('');
    const foot = s.footer();
    expect(foot).toContain('INSERT INTO "users"');
    expect(foot).toContain('COMMIT;');
  });

  it('footer flushes an empty pending set without an INSERT', () => {
    const s = createTableExportStreamer({
      format: 'sql_insert',
      tableName: 'users',
      columns: ['id'],
      databaseType: 'postgresql',
    });
    expect(s.footer()).toBe('COMMIT;\n');
  });
});

describe('createTableExportStreamer empty batches', () => {
  it('returns an empty string for every format', () => {
    for (const format of ['csv', 'json', 'tsv', 'markdown', 'sql_update'] as const) {
      const s = createTableExportStreamer({ format, tableName: 't', columns: ['id'] });
      expect(s.formatRows([])).toBe('');
    }
  });
});

describe('streamQueryIntoExporter', () => {
  it('infers column names when rows arrive before statementStart', async () => {
    const chunks: string[] = [];
    const streamQuery = vi.fn(async (_id, _sql, onEvent) => {
      onEvent({ type: 'rows', index: 0, rows: [[1, null]] });
    });
    await streamQueryIntoExporter({
      dbSessionId: 'c1',
      sql: 'SELECT * FROM t',
      streamer: createTableExportStreamer({
        format: 'csv',
        tableName: 't',
        columns: ['col_0', 'col_1'],
      }),
      write: (chunk) => {
        chunks.push(chunk);
      },
      streamQuery,
    });
    expect(chunks.join('')).toBe('col_0,col_1\n1,\n');
  });

  it('infers no columns from an empty rows event', async () => {
    const chunks: string[] = [];
    await streamQueryIntoExporter({
      dbSessionId: 'c1',
      sql: 'SELECT * FROM t',
      streamer: createTableExportStreamer({ format: 'csv', tableName: 't', columns: [] }),
      write: (chunk) => {
        chunks.push(chunk);
      },
      streamQuery: vi.fn(async (_id, _sql, onEvent) => {
        onEvent({ type: 'rows', index: 0, rows: [] });
      }),
    });
    expect(chunks.join('')).toBe('\n');
  });

  it('writes only header and footer when no events arrive', async () => {
    const chunks: string[] = [];
    await streamQueryIntoExporter({
      dbSessionId: 'c1',
      sql: 'SELECT * FROM t',
      streamer: createTableExportStreamer({ format: 'json', tableName: 't', columns: ['id'] }),
      write: (chunk) => {
        chunks.push(chunk);
      },
      streamQuery: vi.fn(async () => {}),
    });
    expect(chunks.join('')).toBe('[\n]');
  });

  it('takes column names from statementStart and ignores a second one', async () => {
    const chunks: string[] = [];
    const streamQuery = vi.fn(async (_id, _sql, onEvent) => {
      onEvent({
        type: 'statementStart',
        index: 0,
        sql: 'SELECT * FROM users',
        columns: [
          { name: 'id', dataType: 'int', nullable: false },
          { name: 'name', dataType: 'text', nullable: true },
        ],
      });
      onEvent({
        type: 'statementStart',
        index: 0,
        sql: 'SELECT * FROM users',
        columns: [{ name: 'id', dataType: 'int', nullable: false }],
      });
      onEvent({ type: 'rows', index: 0, rows: [[1, 'Alice']] });
    });
    await streamQueryIntoExporter({
      dbSessionId: 'c1',
      sql: 'SELECT * FROM users',
      streamer: createTableExportStreamer({
        format: 'csv',
        tableName: 'users',
        columns: ['id', 'name'],
      }),
      write: (chunk) => {
        chunks.push(chunk);
      },
      streamQuery,
    });
    expect(chunks.join('').trim()).toBe('id,name\n1,Alice');
  });

  it('keeps caller-provided columns when statementStart arrives', async () => {
    const chunks: string[] = [];
    const streamQuery = vi.fn(async (_id, _sql, onEvent) => {
      await emitAll(usersEvents, onEvent);
    });
    await streamQueryIntoExporter({
      dbSessionId: 'c1',
      sql: 'SELECT * FROM users',
      streamer: createTableExportStreamer({
        format: 'csv',
        tableName: 'users',
        columns: ['id', 'name'],
      }),
      write: (chunk) => {
        chunks.push(chunk);
      },
      streamQuery,
      columns: ['id', 'name'],
    });
    expect(chunks.join('').trim()).toBe('id,name\n1,Alice\n2,Bob');
  });
});

describe('streamTableExportToSaveDialog streaming failures', () => {
  it('aborts the save session when streaming fails', async () => {
    const saveSession = {
      begin: vi.fn().mockResolvedValue('tok'),
      append: vi.fn().mockResolvedValue(undefined),
      finish: vi.fn().mockResolvedValue(undefined),
      abort: vi.fn().mockRejectedValue(new Error('abort failed')),
    };
    await expect(
      streamTableExportToSaveDialog({
        dbSessionId: 'c1',
        tableName: 'users',
        columns: ['id'],
        format: 'csv',
        streamQuery: vi.fn(async () => {
          throw new Error('boom');
        }),
        saveSession,
      }),
    ).rejects.toThrow('boom');
    expect(saveSession.abort).toHaveBeenCalledWith('tok');
    expect(saveSession.finish).not.toHaveBeenCalled();
  });

  it('flushes large buffers mid-stream', async () => {
    const chunks: string[] = [];
    const saveSession = {
      begin: vi.fn().mockResolvedValue('tok'),
      append: vi.fn(async (_token: string, chunk: string) => {
        chunks.push(chunk);
      }),
      finish: vi.fn().mockResolvedValue(undefined),
      abort: vi.fn().mockResolvedValue(undefined),
    };
    const big = 'x'.repeat(70_000);
    const result = await streamTableExportToSaveDialog({
      dbSessionId: 'c1',
      tableName: 'users',
      columns: ['id'],
      format: 'csv',
      streamQuery: async (_id, _sql, onEvent) => {
        onEvent({
          type: 'statementStart',
          index: 0,
          sql: 'SELECT * FROM users',
          columns: [{ name: 'id', dataType: 'text', nullable: true }],
        });
        onEvent({ type: 'rows', index: 0, rows: [[big]] });
      },
      saveSession,
    });
    expect(result).toBe('saved');
    expect(saveSession.append).toHaveBeenCalledTimes(1);
    expect(chunks.join('').trim()).toBe(`id\n${big}`);
    expect(saveSession.finish).toHaveBeenCalledWith('tok');
  });

  it('streams the caller-provided sql when given', async () => {
    const streamQuery = vi.fn(async (_id, _sql, onEvent) => {
      await emitAll(usersEvents, onEvent);
    });
    const saveSession = {
      begin: vi.fn().mockResolvedValue('tok'),
      append: vi.fn().mockResolvedValue(undefined),
      finish: vi.fn().mockResolvedValue(undefined),
      abort: vi.fn().mockResolvedValue(undefined),
    };
    const result = await streamTableExportToSaveDialog({
      dbSessionId: 'c1',
      tableName: 'users',
      columns: ['id', 'name'],
      format: 'csv',
      sql: 'SELECT id FROM users WHERE id > 1',
      streamQuery,
      saveSession,
    });
    expect(result).toBe('saved');
    expect(streamQuery).toHaveBeenCalledWith(
      'c1',
      'SELECT id FROM users WHERE id > 1',
      expect.any(Function),
      { applyResultLimit: false, recordHistory: false },
    );
  });
});
