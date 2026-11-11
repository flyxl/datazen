/**
 * DataTable presentation wiring (issue-32 T05, design §7).
 *
 * End to end through DataTable with a fresh registry + the timestamp
 * contribution + an enabled snapshot: grid cells, draft rows and the
 * detail aside all read the same plans.
 *
 * PRD anchor: 2026-01-15T14:30:00Z with iso-tz / Asia/Shanghai must read
 * 2026-01-15T22:30:00+08:00. BUG-003: a BIGINT Unix-milliseconds column
 * needs an explicit custom override; 1705316400123 must keep its .123
 * (2024-01-15T19:00:00.123+08:00). Default-off: with no snapshot the grid
 * shows the old formatTimestamp text.
 *
 * Mocks mirror DataTable.test.tsx (i18n, column resize, virtual table,
 * filter bar, native context menu, file commands, blob exporter, platform).
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { ValuePresentationRegistryImpl, type ColumnSource } from '@datazen/extension-points';
import { DataTable } from '../DataTable';
import { DetailPanel } from '../DetailPanel';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  timestampContribution,
  columnOverrideKey,
  type ValuePresentationSnapshot,
} from '../../../lib/valuePresentation/index';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({
    columnWidths: [160, 200, 160, 200],
    onResizeStart: vi.fn(),
  }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../FilterBar', () => ({
  FilterBar: ({ onClear, loading }: { onClear: () => void; loading?: boolean }) => (
    <div data-testid="filter-bar" data-loading={String(Boolean(loading))}>
      <button type="button" onClick={onClear}>
        clear-filters
      </button>
    </div>
  ),
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

const { showNativeContextMenu } = vi.hoisted(() => ({
  showNativeContextMenu: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: (...args: unknown[]) => showNativeContextMenu(...args),
}));

const downloadBlobMock = vi.hoisted(() => vi.fn());
vi.mock('../../../platform/blobTablesExporter', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../platform/blobTablesExporter')>();
  return { ...actual, downloadBlob: (...args: unknown[]) => downloadBlobMock(...args) };
});

vi.mock('../../../commands/file', () => ({
  fileCommands: {
    saveTextWithDialog: vi.fn().mockResolvedValue(true),
    saveBase64WithDialog: vi.fn().mockResolvedValue(true),
  },
}));

const exportTablesStream = vi.fn();

import { configurePlatform, testPlatform } from '../../../test/setup';

function bindFakeExporter(): void {
  configurePlatform({
    ...testPlatform,
    tablesExporter: {
      exportTables: (...args: unknown[]) => exportTablesStream(...(args as [never])),
    },
  });
}

afterEach(() => {
  cleanup();
  configurePlatform(testPlatform);
});

beforeEach(() => {
  vi.clearAllMocks();
  bindFakeExporter();
  exportTablesStream.mockResolvedValue({ Saved: 2 });
});

const SOURCE: Readonly<Omit<ColumnSource, 'column'>> = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
};

const ENVIRONMENT = { locale: 'en-GB', localTimeZone: 'Asia/Shanghai' } as const;

/** Enabled snapshot: native iso-tz / Shanghai globally, Unix-ms custom override on epoch_ms. */
function makeSnapshot(): ValuePresentationSnapshot {
  return {
    revision: 1,
    environment: ENVIRONMENT,
    configs: {
      [TIMESTAMP_CONTRIBUTION_ID]: {
        configVersion: 1,
        config: {
          interpretation: { kind: 'native' },
          format: 'iso-tz',
          displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
        },
      },
    },
    enabledRendererIds: [TIMESTAMP_CONTRIBUTION_ID],
    overrides: {
      [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: {
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: {
          interpretation: { kind: 'unix', unit: 'milliseconds' },
          format: 'iso-tz',
          displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
        },
      },
    },
  };
}

function makeRegistry(): ValuePresentationRegistryImpl {
  const registry = new ValuePresentationRegistryImpl();
  registry.register(timestampContribution);
  return registry;
}

const COLS = [
  { id: 'created_at', name: 'created_at', type: 'timestamp with time zone' },
  { id: 'epoch_ms', name: 'epoch_ms', type: 'bigint' },
  { id: 'qty', name: 'qty', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

const ROWS: unknown[][] = [['2026-01-15T14:30:00Z', 1705316400123, 42, 'Alice']];

const ROW_RECORD = {
  created_at: '2026-01-15T14:30:00Z',
  epoch_ms: 1705316400123,
  qty: 42,
  name: 'Alice',
};

function cellText(container: HTMLElement, columnName: string): string {
  return container.querySelector(`[data-dt-col="${columnName}"]`)?.textContent ?? '';
}

describe('DataTable presentation wiring (issue-32 T05)', () => {
  it('renders the PRD anchor as Shanghai iso-tz in the grid', () => {
    const { container } = render(
      <DataTable
        columns={COLS}
        rows={ROWS}
        presentationSnapshot={makeSnapshot()}
        presentationSource={SOURCE}
        presentationRegistry={makeRegistry()}
      />,
    );
    expect(cellText(container, 'created_at')).toBe('2026-01-15T22:30:00+08:00');
    expect(cellText(container, 'created_at')).toContain('22:30');
    expect(cellText(container, 'created_at')).toContain('+08:00');
  });

  it('renders the BIGINT Unix column with milliseconds intact (BUG-003)', () => {
    const { container } = render(
      <DataTable
        columns={COLS}
        rows={ROWS}
        presentationSnapshot={makeSnapshot()}
        presentationSource={SOURCE}
        presentationRegistry={makeRegistry()}
      />,
    );
    // 1705316400123 ms = 2024-01-15T11:00:00.123Z = 19:00:00.123+08:00.
    expect(cellText(container, 'epoch_ms')).toContain('.123');
    expect(cellText(container, 'epoch_ms')).toContain('19:00');
    expect(cellText(container, 'epoch_ms')).not.toContain('1705316400123');
  });

  it('leaves a plain number column untouched', () => {
    const { container } = render(
      <DataTable
        columns={COLS}
        rows={ROWS}
        presentationSnapshot={makeSnapshot()}
        presentationSource={SOURCE}
        presentationRegistry={makeRegistry()}
      />,
    );
    expect(cellText(container, 'qty')).toBe('42');
  });

  it('shows the old formatTimestamp text with no snapshot (default-off)', () => {
    const { container } = render(<DataTable columns={COLS} rows={ROWS} />);
    expect(cellText(container, 'created_at')).toBe('2026-01-15T14:30:00.000Z');
    expect(cellText(container, 'created_at')).not.toContain('22:30');
    // Without a source the custom Unix override cannot apply either.
    expect(cellText(container, 'epoch_ms')).toBe('1705316400123');
  });

  it('renders a written draft time cell presented and an unwritten cell as DEFAULT', () => {
    const { container } = render(
      <DataTable
        columns={COLS}
        rows={ROWS}
        presentationSnapshot={makeSnapshot()}
        presentationSource={SOURCE}
        presentationRegistry={makeRegistry()}
        draftRows={[
          { draftId: 'draft-a', cells: { created_at: '2026-01-15T14:30:00Z', name: 'Bob' } },
        ]}
      />,
    );
    const draftCell = container.querySelector(
      '[data-draft-id="draft-a"] [data-draft-col="created_at"]',
    )!;
    expect(draftCell.textContent).toBe('2026-01-15T22:30:00+08:00');

    const unwritten = container.querySelector('[data-draft-id="draft-a"] [data-draft-col="qty"]')!;
    expect(unwritten).toHaveAttribute('data-draft-state', 'default');
    expect(unwritten.querySelector('[data-testid="draft-cell-default"]')).toHaveTextContent(
      'tableData.insertDefault',
    );
  });

  it('grid and aside read the same presented text for the same snapshot', () => {
    const snapshot = makeSnapshot();
    const registry = makeRegistry();
    const { container } = render(
      <>
        <DataTable
          columns={COLS}
          rows={ROWS}
          presentationSnapshot={snapshot}
          presentationSource={SOURCE}
          presentationRegistry={registry}
        />
        <DetailPanel
          open
          columns={COLS}
          row={ROW_RECORD}
          rowIndex={0}
          presentationSnapshot={snapshot}
          presentationSource={SOURCE}
          presentationRegistry={registry}
        />
      </>,
    );
    const gridText = cellText(container, 'created_at');
    const aside = container.querySelector('aside')!;
    const asidePres = Array.from(aside.querySelectorAll('pre')).map((pre) => pre.textContent);
    expect(asidePres).toContain('2026-01-15T22:30:00+08:00');
    expect(gridText).toBe('2026-01-15T22:30:00+08:00');
    expect(asidePres).toContain(gridText);
  });

  it('cancelling a draft edit calls the draft cancel handler (pre-existing path)', () => {
    const onDraftCellEditCancel = vi.fn();
    const { container } = render(
      <DataTable
        columns={COLS}
        rows={ROWS}
        presentationSnapshot={makeSnapshot()}
        presentationSource={SOURCE}
        presentationRegistry={makeRegistry()}
        draftRows={[{ draftId: 'draft-a', cells: { name: 'Bob' } }]}
        editingDraft={{ draftId: 'draft-a', column: 'name' }}
        onDraftCellEditCancel={onDraftCellEditCancel}
      />,
    );
    const input = container.querySelector('[data-draft-id="draft-a"] input')!;
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(onDraftCellEditCancel).toHaveBeenCalled();
  });
});
