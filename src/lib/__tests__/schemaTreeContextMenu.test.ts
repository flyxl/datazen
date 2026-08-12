import { describe, expect, it, vi } from 'vitest';
import {
  buildSchemaTreeContextMenuItems,
  type SchemaTreeContextMenuLabels,
} from '../schemaTreeContextMenu';

const labels: SchemaTreeContextMenuLabels = {
  open: 'Open',
  copyName: 'Copy Name',
  editStructure: 'Edit Structure',
  focusEr: 'Focus ER',
  exportData: 'Export',
  importData: 'Import',
  refresh: 'Refresh',
  newQuery: 'New Query',
  copyDatabaseName: 'Copy DB Name',
  newTable: 'New Table',
  batchExport: 'Batch Export…',
};

function ids(items: ReturnType<typeof buildSchemaTreeContextMenuItems>): string[] {
  return items.map((i) => (i.kind === 'item' ? i.id : i.kind));
}

describe('buildSchemaTreeContextMenuItems', () => {
  it('builds table menu with optional structure/ER and import when not read-only', () => {
    const onOpen = vi.fn();
    const onImport = vi.fn();
    const onBatchExport = vi.fn();
    const items = buildSchemaTreeContextMenuItems({
      kind: 'table',
      labels,
      handlers: {
        onOpen,
        onCopyName: vi.fn(),
        onEditStructure: vi.fn(),
        onFocusEr: vi.fn(),
        onExport: vi.fn(),
        onBatchExport,
        onImport,
      },
      showEditStructure: true,
      showErFocus: true,
      readOnly: false,
    });
    expect(ids(items)).toEqual([
      'open',
      'copy-name',
      'edit-structure',
      'focus-er',
      'export',
      'batch-export',
      'import',
    ]);
    const open = items[0]!;
    if (open.kind === 'item') open.action();
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it('hides import on table when readOnly', () => {
    const items = buildSchemaTreeContextMenuItems({
      kind: 'table',
      labels,
      handlers: {
        onOpen: vi.fn(),
        onCopyName: vi.fn(),
        onExport: vi.fn(),
        onBatchExport: vi.fn(),
        onImport: vi.fn(),
      },
      readOnly: true,
    });
    expect(ids(items)).toEqual(['open', 'copy-name', 'export', 'batch-export']);
  });

  it('omits optional table items when flags are false', () => {
    const items = buildSchemaTreeContextMenuItems({
      kind: 'table',
      labels,
      handlers: {
        onOpen: vi.fn(),
        onCopyName: vi.fn(),
        onEditStructure: vi.fn(),
        onFocusEr: vi.fn(),
        onExport: vi.fn(),
        onBatchExport: vi.fn(),
        onImport: vi.fn(),
      },
      showEditStructure: false,
      showErFocus: false,
    });
    expect(ids(items)).toEqual(['open', 'copy-name', 'export', 'batch-export', 'import']);
  });

  it('builds view menu without import; export only when showExport', () => {
    const without = buildSchemaTreeContextMenuItems({
      kind: 'view',
      labels,
      handlers: {
        onOpen: vi.fn(),
        onCopyName: vi.fn(),
        onExport: vi.fn(),
        onImport: vi.fn(),
      },
    });
    expect(ids(without)).toEqual(['open', 'copy-name']);

    const withExport = buildSchemaTreeContextMenuItems({
      kind: 'view',
      labels,
      handlers: {
        onOpen: vi.fn(),
        onCopyName: vi.fn(),
        onExport: vi.fn(),
        onBatchExport: vi.fn(),
      },
      showExport: true,
    });
    expect(ids(withExport)).toEqual(['open', 'copy-name', 'export', 'batch-export']);
  });

  it('builds database menu', () => {
    const onRefresh = vi.fn();
    const onBatchExport = vi.fn();
    const items = buildSchemaTreeContextMenuItems({
      kind: 'database',
      labels,
      handlers: {
        onRefresh,
        onNewQuery: vi.fn(),
        onCopyDatabaseName: vi.fn(),
        onBatchExport,
      },
    });
    expect(ids(items)).toEqual(['refresh', 'new-query', 'copy-database-name', 'batch-export']);
    const first = items[0]!;
    if (first.kind === 'item') first.action();
    expect(onRefresh).toHaveBeenCalledOnce();
    const batch = items.find((i) => i.kind === 'item' && i.id === 'batch-export');
    if (batch?.kind === 'item') batch.action();
    expect(onBatchExport).toHaveBeenCalledOnce();
  });

  it('builds blank menu and hides new table when read-only or unsupported', () => {
    const withNew = buildSchemaTreeContextMenuItems({
      kind: 'blank',
      labels,
      handlers: {
        onRefresh: vi.fn(),
        onNewQuery: vi.fn(),
        onBatchExport: vi.fn(),
        onNewTable: vi.fn(),
      },
      readOnly: false,
      showNewTable: true,
    });
    expect(ids(withNew)).toEqual(['refresh', 'new-query', 'batch-export', 'new-table']);

    const readOnly = buildSchemaTreeContextMenuItems({
      kind: 'blank',
      labels,
      handlers: {
        onRefresh: vi.fn(),
        onNewQuery: vi.fn(),
        onBatchExport: vi.fn(),
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
        onNewTable: vi.fn(),
      },
      readOnly: false,
      showNewTable: false,
    });
    expect(ids(noSupport)).toEqual(['refresh', 'new-query', 'batch-export']);
  });

  it('skips items whose handlers are missing', () => {
    const items = buildSchemaTreeContextMenuItems({
      kind: 'table',
      labels,
      handlers: { onOpen: vi.fn() },
      showEditStructure: true,
      showErFocus: true,
    });
    expect(ids(items)).toEqual(['open']);
  });
});
