import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { DataTable } from '../DataTable';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({
    columnWidths: [160, 200],
    onResizeStart: vi.fn(),
  }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

vi.mock('../../../commands/file', () => ({
  fileCommands: {
    saveTextWithDialog: vi.fn().mockResolvedValue(true),
    saveBase64WithDialog: vi.fn().mockResolvedValue(true),
  },
}));

const exportTablesStream = vi.fn();

const downloadBlobMock = vi.hoisted(() => vi.fn());
vi.mock('../../../platform/blobTablesExporter', async (importOriginal) => {
  const actual =
    await importOriginal<typeof import('../../../platform/blobTablesExporter')>();
  return { ...actual, downloadBlob: (...args: unknown[]) => downloadBlobMock(...args) };
});

import { configurePlatform, testPlatform, clearPlatform } from '../../../test/setup';

function bindFakeExporter(): void {
  configurePlatform({
    ...testPlatform,
    tablesExporter: {
      exportTables: (...args: unknown[]) => exportTablesStream(...(args as [never])),
    },
  });
}

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

afterEach(() => {
  cleanup();
  configurePlatform(testPlatform);
  clearPlatform();
});

beforeEach(() => {
  vi.clearAllMocks();
  bindFakeExporter();
  exportTablesStream.mockResolvedValue({ Saved: 2 });
});

describe('DataTable session-less export entry (task 4)', () => {
  it('exports loaded rows without a session, exactly as the session-less surfaces render it', async () => {
    // ChartWidgetTile (tile + expanded overlay), RunHistoryDrawer and the
    // WorkflowPage step detail all render DataTable with an exportTableName
    // and no dbSessionId. The toolbar export opens DataExportDialog, whose
    // provided path must succeed with no session at all.
    const { getByTitle, getByText, getAllByText } = render(
      <DataTable
        columns={COLS}
        rows={[
          [1, 'a'],
          [2, 'b'],
        ]}
        selectedRows={new Set()}
        onSelectAll={vi.fn()}
        onRowSelect={vi.fn()}
        exportTableName="widget_title"
      />,
    );
    fireEvent.click(getByTitle('export.export'));
    await waitFor(() => {
      expect(getByText('common.exportData')).toBeInTheDocument();
    });
    const dialogExportBtns = getAllByText('export.export');
    fireEvent.click(dialogExportBtns[dialogExportBtns.length - 1]!);
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith(
      'id,name\n1,a\n2,b',
      'widget_title.csv',
      'text/csv',
    );
    expect(exportTablesStream).not.toHaveBeenCalled();
    await waitFor(() => expect(getByText('export.success')).toBeInTheDocument());
  });
});
