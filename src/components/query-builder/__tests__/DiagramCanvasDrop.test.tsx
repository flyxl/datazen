/**
 * Journey test: drag a table from the connection navigator onto the Visual
 * Builder canvas.
 *
 * The producer half is the real `setDragPayload` used by the navigator's table
 * rows (`SchemaTreeRow` / `NavigatorTreeRow`); the consumer half is
 * `DiagramCanvas`'s drop handler, reached through the real `QueryBuilderPanel`
 * wiring and Zustand store. The two halves previously drifted (the canvas read
 * a MIME the tree never wrote), so this test pins the cross-module contract
 * instead of stubbing either side.
 */
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { DiagramCanvas } from '../DiagramCanvas/DiagramCanvas';
import { QueryBuilderPanel } from '../QueryBuilderPanel';
import { setDragPayload } from '../../../windows/connection/schema-tree/schemaTreeDrag';
import { useQueryBuilderStore } from '../../../stores/queryBuilderStore';

/**
 * jsdom implements neither `DragEvent` nor `DataTransfer` (jsdom#1568), so
 * `fireEvent.drop` falls back to a plain `Event`: `clientX/clientY` are dropped
 * and the canvas would compute `NaN` coordinates. A `MouseEvent`-backed
 * `DragEvent` keeps the drag init the app relies on. Scoped to this file —
 * Vitest isolates each test file's environment.
 */
beforeAll(() => {
  class JsdomDragEvent extends MouseEvent {
    readonly dataTransfer: unknown;
    constructor(type: string, init: MouseEventInit & { dataTransfer?: unknown } = {}) {
      super(type, init);
      this.dataTransfer = init.dataTransfer;
    }
  }
  (globalThis as unknown as { DragEvent: unknown }).DragEvent = JsdomDragEvent;
});

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/schemaCache', () => ({
  getCachedTableSchema: vi
    .fn()
    .mockResolvedValue({ columns: [], primaryKeys: [], indexes: [], foreignKeys: [] }),
}));

/**
 * Minimal `DataTransfer` double. jsdom does not implement the drag-and-drop
 * data store, so tests have to model the parts the app actually uses; keeping
 * the Map means the payload written by the producer is exactly what the
 * consumer reads back.
 */
function createDataTransfer(): DataTransfer {
  const store = new Map<string, string>();
  return {
    setData: (format: string, data: string) => {
      store.set(format, String(data));
    },
    getData: (format: string) => store.get(format) ?? '',
    clearData: () => store.clear(),
    get types() {
      return [...store.keys()];
    },
    effectAllowed: 'uninitialized',
    dropEffect: 'none',
    files: [] as unknown as FileList,
    items: [] as unknown as DataTransferItemList,
    setDragImage: () => {},
  } as unknown as DataTransfer;
}

/** Drag a table row the way the navigator does. */
function dragTableRow(dt: DataTransfer, table: string, schema?: string): void {
  setDragPayload(dt, {
    kind: 'table',
    database: 'app',
    schema,
    table,
    connectionId: 'cfg-1',
    dbSessionId: 'session-1',
    databaseType: 'postgresql',
  });
}

const noop = () => {};

function renderCanvas(
  onDropTable: (table: string, pos: { x: number; y: number }) => void,
  tablePositions: Record<string, { x: number; y: number }> = {},
) {
  return render(
    <DiagramCanvas
      selectedTables={Object.keys(tablePositions)}
      tablePositions={tablePositions}
      columnMap={{}}
      columnInfoMap={{}}
      selectedColumns={[]}
      tableAliases={{}}
      onToggleColumn={noop}
      onUpdatePosition={noop}
      onSetTableAlias={noop}
      onDropTable={onDropTable}
      zoom={2}
    />,
  );
}

describe('table drop onto the Visual Builder canvas', () => {
  beforeEach(() => {
    useQueryBuilderStore.getState().reset();
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('accepts the navigator payload and reports a grid-snapped canvas position', () => {
    const onDropTable = vi.fn();
    renderCanvas(onDropTable);
    const canvas = screen.getByTestId('qb-diagram-canvas');

    const dt = createDataTransfer();
    dragTableRow(dt, 'users', 'public');

    const dragOver = new DragEvent('dragover', {
      bubbles: true,
      cancelable: true,
      dataTransfer: dt,
    });
    canvas.dispatchEvent(dragOver);
    expect(dragOver.defaultPrevented).toBe(true);

    fireEvent.drop(canvas, { dataTransfer: dt, clientX: 100, clientY: 60 });

    // Raw canvas coords are (100 - 0) / 2 = 50 by (60 - 0) / 2 = 30 with no
    // scroll; the drop resolves onto the card grid so cards line up.
    expect(onDropTable).toHaveBeenCalledWith('users', { x: 48, y: 24 });
  });

  it('aligns the top of a second drop with the card already on that row', () => {
    const onDropTable = vi.fn();
    renderCanvas(onDropTable, { users: { x: 48, y: 24 } });
    const canvas = screen.getByTestId('qb-diagram-canvas');

    const dt = createDataTransfer();
    dragTableRow(dt, 'orders', 'public');
    // Dropped 13px lower than the existing card — same visual row.
    fireEvent.drop(canvas, { dataTransfer: dt, clientX: 400, clientY: 80 });

    expect(onDropTable).toHaveBeenCalledTimes(1);
    const [, pos] = onDropTable.mock.calls[0]!;
    expect(pos.y).toBe(24);
    // …and it lands to the right of the card already in that row.
    expect(pos.x).toBeGreaterThanOrEqual(48 + 264);
  });

  it('ignores drags that carry no schema-object payload', () => {
    const onDropTable = vi.fn();
    renderCanvas(onDropTable);
    const canvas = screen.getByTestId('qb-diagram-canvas');

    // Connection reordering uses its own MIME; the canvas must not add a table.
    const dt = createDataTransfer();
    dt.setData('application/datazen-connection', 'cfg-1');
    dt.setData('text/plain', 'cfg-1');

    fireEvent.dragOver(canvas, { dataTransfer: dt });
    fireEvent.drop(canvas, { dataTransfer: dt, clientX: 100, clientY: 60 });

    expect(onDropTable).not.toHaveBeenCalled();
  });

  it('adds the dropped table as a positioned card through the real panel', async () => {
    render(
      <QueryBuilderPanel
        panelId="panel-test"
        dbSessionId="session-1"
        databaseType="postgresql"
        currentSql=""
        onCommit={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    const canvas = screen.getByTestId('qb-diagram-canvas');

    const dt = createDataTransfer();
    dragTableRow(dt, 'orders', 'public');
    fireEvent.dragOver(canvas, { dataTransfer: dt });
    fireEvent.drop(canvas, { dataTransfer: dt, clientX: 80, clientY: 40 });

    await waitFor(() => {
      expect(useQueryBuilderStore.getState().selectedTables).toContain('orders');
    });
    // Raw drop point (80, 40) resolved onto the card grid.
    expect(useQueryBuilderStore.getState().tablePositions.orders).toEqual({ x: 72, y: 48 });
    expect(await screen.findByTestId('qb-drag-orders')).toBeInTheDocument();
  });
});
