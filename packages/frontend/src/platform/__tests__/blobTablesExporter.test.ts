import { describe, expect, it, vi } from 'vitest';

import { createBlobTablesExporter, downloadBlob } from '../blobTablesExporter';
import type { TablesExportRequest } from '../contracts';

/**
 * The F4 dual-flow demonstration: the web-shaped exporter answers the same
 * `exportTables(request)` signature the desktop Rust-streaming one does —
 * pages in through an injected reader, CSV out through a Blob download.
 */

const REQUEST = (overrides?: Partial<TablesExportRequest>): TablesExportRequest => ({
  dbSessionId: 'sess-1',
  mode: 'data_only',
  dataFormat: 'csv',
  outputMode: 'single',
  tables: [{ tableName: 'users', columns: ['id', 'name'] }],
  ...overrides,
});

describe('createBlobTablesExporter', () => {
  it('assembles paged reads into CSV and delivers one blob', async () => {
    const delivered: Array<{ contents: string; fileName: string }> = [];
    const readPage = vi
      .fn<(table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>>()
      .mockResolvedValueOnce({
        columns: ['id', 'name'],
        rows: [
          [1, 'ada'],
          [2, 'grace'],
        ],
        nextCursor: 'page-2',
      })
      .mockResolvedValueOnce({
        columns: ['id', 'name'],
        rows: [[3, 'linus']],
        nextCursor: null,
      });

    const exporter = createBlobTablesExporter(readPage, {
      deliver: (contents, fileName) => delivered.push({ contents, fileName }),
    });

    await expect(exporter.exportTables(REQUEST())).resolves.toEqual({ Saved: 1 });

    expect(readPage).toHaveBeenCalledTimes(2);
    // WR-E3: 每次页读都携带请求域读目标（reader 看不到 request，由导出器过桥）。
    const context = { dbSessionId: 'sess-1', database: null, schema: null, driverType: null };
    expect(readPage).toHaveBeenNthCalledWith(1, 'users', null, context);
    expect(readPage).toHaveBeenNthCalledWith(2, 'users', 'page-2', context);
    expect(delivered).toHaveLength(1);
    expect(delivered[0]?.fileName).toBe('users.csv');
    expect(delivered[0]?.contents).toContain('id,name');
    expect(delivered[0]?.contents).toContain('1,ada');
    expect(delivered[0]?.contents).toContain('3,linus');
    expect(delivered[0]?.contents).not.toContain('page-2');
  });

  it('quotes cells that could re-enter as two fields or a formula', async () => {
    const readPage = vi
      .fn<(table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>>()
      .mockResolvedValue({
        columns: ['note'],
        rows: [['a,b'], ['say "hi"'], ['=SUM(A1)'], ['plain']],
        nextCursor: null,
      });
    const delivered: string[] = [];
    const exporter = createBlobTablesExporter(readPage, {
      deliver: (contents) => delivered.push(contents),
    });

    await exporter.exportTables(REQUEST());

    expect(delivered[0]).toContain('"a,b"');
    expect(delivered[0]).toContain('"say ""hi"""');
    expect(delivered[0]).toContain('"=SUM(A1)"');
    expect(delivered[0]).toContain('plain');
  });

  it('emits a provided source as CSV without re-reading the table (item 3c)', async () => {
    const readPage =
      vi.fn<(table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>>();
    const delivered: string[] = [];
    const exporter = createBlobTablesExporter(readPage, {
      deliver: (contents) => delivered.push(contents),
    });

    await expect(
      exporter.exportTables(
        REQUEST({
          tables: [
            {
              tableName: 'users',
              columns: ['id', 'name'],
              source: {
                type: 'provided',
                columns: [
                  { name: 'id', dataType: 'int', nullable: false },
                  { name: 'name', dataType: 'text', nullable: true },
                ],
                rows: [
                  [1, 'a,b'],
                  [2, null],
                ],
              },
            },
          ],
        }),
      ),
    ).resolves.toEqual({ Saved: 1 });

    expect(readPage).not.toHaveBeenCalled();
    expect(delivered[0]).toContain('id,name');
    expect(delivered[0]).toContain('1,"a,b"');
    expect(delivered[0]).toContain('2,');
  });

  it('refuses a sql source instead of silently exporting the whole table (item 3c)', async () => {
    const readPage =
      vi.fn<(table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>>();
    const exporter = createBlobTablesExporter(readPage);

    await expect(
      exporter.exportTables(
        REQUEST({
          tables: [
            {
              tableName: 'users',
              columns: ['id'],
              source: { type: 'sql', sql: 'SELECT id FROM users WHERE id = 1' },
            },
          ],
        }),
      ),
    ).rejects.toThrow('filtered export is not supported by the web exporter');
    expect(readPage).not.toHaveBeenCalled();
  });

  it('structure modes emit the DDL as a leading comment', async () => {
    const readPage =
      vi.fn<
        (table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>
      >();
    const delivered: string[] = [];
    const exporter = createBlobTablesExporter(readPage, {
      loadDdl: async () => 'CREATE TABLE users (id INT);',
      deliver: (contents) => delivered.push(contents),
    });

    await exporter.exportTables(REQUEST({ mode: 'structure_only' }));

    expect(readPage).not.toHaveBeenCalled();
    expect(delivered[0]).toContain('CREATE TABLE users (id INT);');
  });
});

describe('downloadBlob', () => {
  it('triggers an anchor download for the blob URL', () => {
    const clicks: unknown[] = [];
    const created: string[] = [];
    const revoked: string[] = [];
    // Minimal DOM doubles — jsdom has Blob/URL, but click is inert, so spy it.
    const originalCreate = URL.createObjectURL;
    const originalRevoke = URL.revokeObjectURL;
    URL.createObjectURL = ((blob: Blob) => {
      created.push(`${blob.size}`);
      return 'blob:test-url';
    }) as typeof URL.createObjectURL;
    URL.revokeObjectURL = ((url: string) => revoked.push(url)) as typeof URL.revokeObjectURL;
    const anchor = document.createElement('a');
    anchor.click = () => clicks.push(anchor.download);
    vi.spyOn(document, 'createElement').mockReturnValueOnce(anchor);

    downloadBlob('a,b\n1,2\n', 'table.csv');

    expect(clicks).toEqual(['table.csv']);
    expect(anchor.href).toBe('blob:test-url');
    expect(revoked).toContain('blob:test-url');
    URL.createObjectURL = originalCreate;
    URL.revokeObjectURL = originalRevoke;
  });
});
