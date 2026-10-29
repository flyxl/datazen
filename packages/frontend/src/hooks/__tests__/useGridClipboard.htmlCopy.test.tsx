import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, act } from '@testing-library/react';
import { useGridClipboard } from '../useGridClipboard';
import type { UseGridClipboardResult } from '../useGridClipboard';
import type { CellPasteWrite } from '../../lib/tableChanges';
import type { StagePasteWritesResult } from '../../stores/tableData/pendingEditActions';

/**
 * The HTML branch of `copySelectedCellsAs` (DB-10): the clipboard carries the
 * `<table>` as `text/html` **and** a TSV of the same cells as plain text —
 * Notion / Excel / Google Sheets turn the flavor into a real table, every
 * plain-text target still gets a paste-able table, and the paste-back
 * alignment source records the TSV (the text a paste actually reads).
 */
const copySpy = vi.fn().mockResolvedValue(true);

vi.mock('../useClipboardCopyFeedback', () => ({
  useClipboardCopyFeedback: (_ms: number) => ({ copied: false, copy: copySpy }),
}));

vi.mock('@datazen/backend-client', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    requirePlatformServices: () => ({ readClipboard: vi.fn() }),
  };
});

const columns = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
];

const context = {
  connectionId: 'conn-1',
  dbSessionId: 'sess-1',
  driverType: 'postgres',
  database: 'app',
  schema: null,
  table: 'users',
};

let latest: UseGridClipboardResult | null = null;

function Host() {
  const result = useGridClipboard({
    panelId: 'panel-html-copy',
    isEditable: true,
    columns,
    columnOrder: ['id', 'name'],
    rows: [
      { id: 1, name: 'Alice' },
      { id: 2, name: 'Bo<b>' },
    ],
    pendingChanges: new Map(),
    rowIdentityAnchors: new Map(),
    selectedRows: new Set<number>(),
    gridSelection: {
      mode: 'cell',
      anchor: { rowIndex: 0, columnName: 'id' },
      focus: { rowIndex: 1, columnName: 'name' },
      extraRanges: [],
    },
    context,
    tableLabel: 'users',
    stagePasteWrites: (_writes: readonly CellPasteWrite[]): StagePasteWritesResult => ({
      staged: 0,
      skipped: 0,
      problems: [],
    }),
  });
  latest = result;
  return <>{result.confirmElement}</>;
}

describe('copySelectedCellsAs html flavor (DB-10)', () => {
  beforeEach(() => {
    copySpy.mockClear();
    copySpy.mockResolvedValue(true);
  });

  afterEach(cleanup);

  it('hands the clipboard both flavors: html markup + TSV plain text', async () => {
    render(<Host />);
    let ok = false;
    await act(async () => {
      ok = await latest!.copySelectedCellsAs('html');
    });

    expect(ok).toBe(true);
    expect(copySpy).toHaveBeenCalledTimes(1);
    const [text, rich] = copySpy.mock.calls[0];
    // The plain flavor is the TSV of the same rectangle, not the markup.
    expect(text).toBe('1\tAlice\n2\tBo<b>');
    // The rich flavor is the table, header included, HTML-escaped.
    expect(rich).toEqual({ html: expect.stringContaining('<table>') });
    expect(rich.html).toContain('<th>id</th><th>name</th>');
    expect(rich.html).toContain('<td>Bo&lt;b&gt;</td>');
    expect(rich.html).not.toContain('Bo<b>');
  });

  it('other formats keep the single-text call they always had', async () => {
    render(<Host />);
    await act(async () => {
      await latest!.copySelectedCellsAs('markdown');
    });
    const [text, rich] = copySpy.mock.calls[0];
    expect(text).toContain('| id | name |');
    expect(text).toContain('| 1 | Alice |');
    expect(rich).toBeUndefined();
  });
});
