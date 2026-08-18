import { describe, expect, it, vi } from 'vitest';
import {
  buildSchemaTreeContextMenuItems,
  type SchemaTreeContextMenuLabels,
} from '../schemaTreeContextMenu';

const labels: SchemaTreeContextMenuLabels = {
  open: 'Open',
  openStructure: 'Open Structure',
  copyName: 'Copy Name',
  copyDdl: 'Copy DDL',
  focusEr: 'Focus ER',
  exportData: 'Export',
  importData: 'Import',
  refresh: 'Refresh',
  newQuery: 'New Query',
  copyDatabaseName: 'Copy DB Name',
  newTable: 'New Table',
  batchExport: 'Export…',
  truncate: 'Truncate',
  drop: 'Drop',
  dropView: 'Drop View',
  viewErDiagram: 'View ER Diagram',
  newSchema: 'New Schema',
};

function ids(items: ReturnType<typeof buildSchemaTreeContextMenuItems>): string[] {
  return items.map((i) => (i.kind === 'item' ? i.id : i.kind));
}

describe('buildSchemaTreeContextMenuItems', () => {
  it('builds table menu in TablePlus order with structure / new query / ddl / truncate / drop', () => {
    const onOpen = vi.fn();
    const onImport = vi.fn();
    const onBatchExport = vi.fn();
    const onTruncate = vi.fn();
    const onDrop = vi.fn();
    const items = buildSchemaTreeContextMenuItems({
      kind: 'table',
      labels,
      handlers: {
        onOpen,
        onOpenStructure: vi.fn(),
        onFocusEr: vi.fn(),
        onNewQuery: vi.fn(),
        onCopyName: vi.fn(),
        onCopyDdl: vi.fn(),
        onExport: vi.fn(),
        onBatchExport,
        onImport,
        onTruncate,
        onDrop,
      },
      showOpenStructure: true,
      showErFocus: true,
      readOnly: false,
    });
    expect(ids(items)).toEqual([
      'open',
      'open-structure',
      'focus-er',
      'new-query',
      'copy-name',
      'copy-ddl',
      'export',
      'batch-export',
      'import',
      'separator',
      'truncate',
      'drop',
    ]);
    const open = items[0]!;
    if (open.kind === 'item') open.action();
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it('hides import / truncate / drop on table when readOnly', () => {
    const items = buildSchemaTreeContextMenuItems({
      kind: 'table',
      labels,
      handlers: {
        onOpen: vi.fn(),
        onCopyName: vi.fn(),
        onCopyDdl: vi.fn(),
        onNewQuery: vi.fn(),
        onExport: vi.fn(),
        onBatchExport: vi.fn(),
        onImport: vi.fn(),
        onTruncate: vi.fn(),
        onDrop: vi.fn(),
      },
      readOnly: true,
    });
    expect(ids(items)).toEqual([
      'open',
      'new-query',
      'copy-name',
      'copy-ddl',
      'export',
      'batch-export',
    ]);
  });

  it('omits optional table items when flags are false', () => {
    const items = buildSchemaTreeContextMenuItems({
      kind: 'table',
      labels,
      handlers: {
        onOpen: vi.fn(),
        onOpenStructure: vi.fn(),
        onFocusEr: vi.fn(),
        onNewQuery: vi.fn(),
        onCopyName: vi.fn(),
        onCopyDdl: vi.fn(),
        onExport: vi.fn(),
        onBatchExport: vi.fn(),
        onImport: vi.fn(),
        onTruncate: vi.fn(),
        onDrop: vi.fn(),
      },
      showOpenStructure: false,
      showErFocus: false,
    });
    expect(ids(items)).toEqual([
      'open',
      'new-query',
      'copy-name',
      'copy-ddl',
      'export',
      'batch-export',
      'import',
      'separator',
      'truncate',
      'drop',
    ]);
  });

  it('builds view menu with copy ddl / export / drop view', () => {
    const without = buildSchemaTreeContextMenuItems({
      kind: 'view',
      labels,
      handlers: {
        onOpen: vi.fn(),
        onCopyName: vi.fn(),
        onCopyDdl: vi.fn(),
        onExport: vi.fn(),
        onImport: vi.fn(),
        onDrop: vi.fn(),
      },
      readOnly: true,
    });
    expect(ids(without)).toEqual(['open', 'copy-name', 'copy-ddl']);

    const withExport = buildSchemaTreeContextMenuItems({
      kind: 'view',
      labels,
      handlers: {
        onOpen: vi.fn(),
        onCopyName: vi.fn(),
        onCopyDdl: vi.fn(),
        onExport: vi.fn(),
        onBatchExport: vi.fn(),
        onDrop: vi.fn(),
      },
      showExport: true,
      readOnly: false,
    });
    expect(ids(withExport)).toEqual([
      'open',
      'copy-name',
      'copy-ddl',
      'export',
      'batch-export',
      'separator',
      'drop-view',
    ]);
  });

  it('builds database menu with import and new table when supported', () => {
    const onRefresh = vi.fn();
    const onBatchExport = vi.fn();
    const onNewTable = vi.fn();
    const items = buildSchemaTreeContextMenuItems({
      kind: 'database',
      labels,
      handlers: {
        onRefresh,
        onNewQuery: vi.fn(),
        onCopyDatabaseName: vi.fn(),
        onBatchExport,
        onImport: vi.fn(),
        onNewTable,
      },
      readOnly: false,
      showNewTable: true,
    });
    expect(ids(items)).toEqual([
      'refresh',
      'new-query',
      'copy-database-name',
      'batch-export',
      'import',
      'new-table',
    ]);
    const first = items[0]!;
    if (first.kind === 'item') first.action();
    expect(onRefresh).toHaveBeenCalledOnce();
    const batch = items.find((i) => i.kind === 'item' && i.id === 'batch-export');
    if (batch?.kind === 'item') batch.action();
    expect(onBatchExport).toHaveBeenCalledOnce();
    const create = items.find((i) => i.kind === 'item' && i.id === 'new-table');
    if (create?.kind === 'item') create.action();
    expect(onNewTable).toHaveBeenCalledOnce();
  });

  it('builds blank menu with import and hides new table when read-only or unsupported', () => {
    const withNew = buildSchemaTreeContextMenuItems({
      kind: 'blank',
      labels,
      handlers: {
        onRefresh: vi.fn(),
        onNewQuery: vi.fn(),
        onBatchExport: vi.fn(),
        onImport: vi.fn(),
        onNewTable: vi.fn(),
      },
      readOnly: false,
      showNewTable: true,
    });
    expect(ids(withNew)).toEqual(['refresh', 'new-query', 'batch-export', 'import', 'new-table']);

    const readOnly = buildSchemaTreeContextMenuItems({
      kind: 'blank',
      labels,
      handlers: {
        onRefresh: vi.fn(),
        onNewQuery: vi.fn(),
        onBatchExport: vi.fn(),
        onImport: vi.fn(),
        onNewTable: vi.fn(),
      },
      readOnly: true,
      showNewTable: true,
    });
    expect(ids(readOnly)).toEqual(['refresh', 'new-query', 'batch-export']);

    const noSupport = buildSchemaTreeContextMenuItems({
      kind: 'blank',
      labels,
      handlers: {
        onRefresh: vi.fn(),
        onNewQuery: vi.fn(),
        onBatchExport: vi.fn(),
        onImport: vi.fn(),
        onNewTable: vi.fn(),
      },
      readOnly: false,
      showNewTable: false,
    });
    expect(ids(noSupport)).toEqual(['refresh', 'new-query', 'batch-export', 'import']);
  });

  it('skips items whose handlers are missing', () => {
    const items = buildSchemaTreeContextMenuItems({
      kind: 'table',
      labels,
      handlers: { onOpen: vi.fn() },
      showOpenStructure: true,
      showErFocus: true,
    });
    expect(ids(items)).toEqual(['open']);
  });

  it('hides batch-export when showBatchExport is false (no full-table export)', () => {
    const items = buildSchemaTreeContextMenuItems({
      kind: 'table',
      labels,
      handlers: {
        onOpen: vi.fn(),
        onCopyName: vi.fn(),
        onCopyDdl: vi.fn(),
        onNewQuery: vi.fn(),
        onExport: vi.fn(),
        onBatchExport: vi.fn(),
        onImport: vi.fn(),
        onTruncate: vi.fn(),
        onDrop: vi.fn(),
      },
      showBatchExport: false,
    });
    expect(ids(items)).not.toContain('batch-export');
    expect(ids(items)).toContain('export');
  });

  it('hides both export and batch-export when showExport and showBatchExport are false', () => {
    const items = buildSchemaTreeContextMenuItems({
      kind: 'table',
      labels,
      handlers: {
        onOpen: vi.fn(),
        onCopyName: vi.fn(),
        onCopyDdl: vi.fn(),
        onNewQuery: vi.fn(),
        onExport: vi.fn(),
        onBatchExport: vi.fn(),
        onImport: vi.fn(),
        onTruncate: vi.fn(),
        onDrop: vi.fn(),
      },
      showExport: false,
      showBatchExport: false,
    });
    expect(ids(items)).not.toContain('export');
    expect(ids(items)).not.toContain('batch-export');
  });
});
