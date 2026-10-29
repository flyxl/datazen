/**
 * The reverse foreign-key flow (DB-20).
 *
 * `openReferencingRows` is the only place the referencing tabs are opened,
 * so the tests here are the contract of the whole flow: the catalog is read
 * once and cached, "no references" is a refusal rather than an empty tab, a
 * NULL key is skipped rather than shown the whole table, and every opened tab
 * carries its conditions from the first frame — and a way back.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const { mockListInboundForeignKeys } = vi.hoisted(() => ({
  mockListInboundForeignKeys: vi.fn(),
}));

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    listInboundForeignKeys: mockListInboundForeignKeys,
  },
}));

vi.mock('../schemaCache', () => ({
  subscribeSchemaInvalidation: vi.fn(() => () => undefined),
}));

import {
  buildInboundFilters,
  MAX_REFERENCING_TABS,
  openReferencingRows,
  referencedValuesOfRow,
  resetInboundCatalogCache,
} from '../fkReferencedBy';
import { usePanelStore } from '../../stores/panelStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useFkNavigationStore, resetFkNavigationEntryIds } from '../../stores/fkNavigationStore';
import type { InboundForeignKeyCatalog } from '@datazen/backend-client';
import type { TablePanel } from '../../stores/panelTypes';

function catalog(overrides: Partial<InboundForeignKeyCatalog> = {}): InboundForeignKeyCatalog {
  return {
    table: 'customers',
    database: 'app',
    references: [
      {
        table: 'orders',
        schema: null,
        constraintName: 'orders_customer_fk',
        columns: ['customer_id'],
        referencedColumns: ['id'],
      },
    ],
    scannedTables: 2,
    truncated: false,
    ...overrides,
  };
}

function sourcePanel(): TablePanel {
  return {
    id: 'panel-source',
    type: 'table',
    tableName: 'customers',
    database: 'app',
    tableSchema: null,
    subTab: 'data',
    connectionId: 'conn-1',
    dbSessionId: 'session-1',
    connectionName: 'Local',
    databaseType: 'postgresql',
  };
}

function input(row: Record<string, unknown>) {
  return {
    sourcePanelId: 'panel-source',
    sourceTable: 'customers',
    dbSessionId: 'session-1',
    database: 'app',
    schema: null,
    connectionId: 'conn-1',
    driverType: 'postgresql' as const,
    row,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockListInboundForeignKeys.mockReset();
  resetInboundCatalogCache();
  resetFkNavigationEntryIds();
  usePanelStore.setState({ panels: [], activePanelId: null });
  useTableDataStore.getState().reset();
  useFkNavigationStore.getState().reset();
  // Every flow starts from a tab that exists.
  usePanelStore.getState().addPanel(sourcePanel());
});

describe('buildInboundFilters', () => {
  it('pairs the referencing columns with the row values positionally', () => {
    const filters = buildInboundFilters(
      { columns: ['tenant_id', 'customer_id'], referencedColumns: ['tenant', 'id'] },
      ['t1', 42],
    );

    expect(filters).toEqual([
      { column: 'tenant_id', operator: 'eq', value: 't1' },
      { column: 'customer_id', operator: 'eq', value: 42 },
    ]);
  });

  it('builds nothing when a value is missing — a half filter points at the wrong rows', () => {
    expect(
      buildInboundFilters({ columns: ['customer_id'], referencedColumns: ['id'] }, []),
    ).toEqual([]);
  });
});

describe('referencedValuesOfRow', () => {
  it('reads the referenced columns from the full row record', () => {
    expect(referencedValuesOfRow({ referencedColumns: ['id'] }, { id: 42 })).toEqual([42]);
  });

  it('refuses a row whose key member is NULL — a NULL key cannot be referenced', () => {
    expect(referencedValuesOfRow({ referencedColumns: ['id'] }, { id: null })).toBeNull();
    expect(referencedValuesOfRow({ referencedColumns: ['id'] }, {})).toBeNull();
  });
});

describe('openReferencingRows', () => {
  it('refuses with its own sentence when nothing references the table', async () => {
    mockListInboundForeignKeys.mockResolvedValue(catalog({ references: [] }));

    const outcome = await openReferencingRows(input({ id: 42 }));

    expect(outcome.ok).toBe(false);
    expect(outcome.error?.code).toBe('grid.fk.noReferences');
    // No tab was opened.
    // No tab was opened.
    expect(usePanelStore.getState().panels).toHaveLength(1);
  });

  it('opens one tab per referencing constraint, filtered from the first frame', async () => {
    mockListInboundForeignKeys.mockResolvedValue(
      catalog({
        references: [
          {
            table: 'orders',
            schema: null,
            constraintName: 'orders_customer_fk',
            columns: ['customer_id'],
            referencedColumns: ['id'],
          },
        ],
      }),
    );

    const outcome = await openReferencingRows(input({ id: 42 }));

    expect(outcome.ok).toBe(true);
    expect(outcome.panelIds).toHaveLength(1);
    const panelId = outcome.panelIds[0];
    const panel = usePanelStore.getState().panels.find((item) => item.id === panelId);
    expect(panel?.type === 'table' && panel.tableName).toBe('orders');
    const slice = useTableDataStore.getState().byPanel.get(panelId);
    expect(slice?.filters).toEqual([{ column: 'customer_id', operator: 'eq', value: 42 }]);
  });

  it('records the step in the navigation stack, so back returns to the source row', async () => {
    mockListInboundForeignKeys.mockResolvedValue(catalog());

    const outcome = await openReferencingRows(input({ id: 42 }));

    const nav = useFkNavigationStore.getState();
    expect(nav.back).toHaveLength(1);
    const entry = nav.back[0];
    expect(entry.panelId).toBe(outcome.panelIds[0]);
    expect(entry.origin.sourcePanelId).toBe('panel-source');
    expect(entry.origin.localColumns).toEqual(['customer_id']);
    expect(entry.origin.targetColumns).toEqual(['id']);
  });

  it('skips a constraint this row cannot satisfy and refuses when none remain', async () => {
    // The orders constraint reads `customers.id`; this row's `id` is NULL —
    // no row can reference it, and an unfiltered tab would lie.
    mockListInboundForeignKeys.mockResolvedValue(catalog());

    const outcome = await openReferencingRows(input({ id: null }));

    expect(outcome.ok).toBe(false);
    expect(outcome.error?.code).toBe('grid.fk.noReferences');
    expect(usePanelStore.getState().panels).toHaveLength(1);
  });

  it('skips only the unsatisfiable constraint and opens the rest', async () => {
    mockListInboundForeignKeys.mockResolvedValue(
      catalog({
        references: [
          {
            table: 'orders',
            schema: null,
            constraintName: 'orders_customer_fk',
            columns: ['customer_id'],
            referencedColumns: ['id'],
          },
          {
            table: 'notes',
            schema: null,
            constraintName: 'notes_customer_fk',
            columns: ['customer_id'],
            referencedColumns: ['badge'],
          },
        ],
      }),
    );

    const outcome = await openReferencingRows(input({ id: 42 }));

    expect(outcome.ok).toBe(true);
    // The notes constraint reads a column this row lacks.
    expect(outcome.panelIds).toHaveLength(1);
    const slice = useTableDataStore.getState().byPanel.get(outcome.panelIds[0]);
    expect(slice?.filters).toEqual([{ column: 'customer_id', operator: 'eq', value: 42 }]);
  });

  it('caps the tabs it opens and says how many it skipped', async () => {
    const references = Array.from({ length: 6 }, (_, index) => ({
      table: `child_${index}`,
      schema: null,
      constraintName: `fk_${index}`,
      columns: ['customer_id'],
      referencedColumns: ['id'],
    }));
    mockListInboundForeignKeys.mockResolvedValue(catalog({ references }));

    const outcome = await openReferencingRows(input({ id: 42 }));

    expect(outcome.ok).toBe(true);
    expect(outcome.panelIds).toHaveLength(MAX_REFERENCING_TABS);
    expect(outcome.skipped).toBe(2);
  });

  it('reads the catalog once and caches it for the next ask', async () => {
    mockListInboundForeignKeys.mockResolvedValue(catalog());

    await openReferencingRows(input({ id: 42 }));
    await openReferencingRows(input({ id: 43 }));

    expect(mockListInboundForeignKeys).toHaveBeenCalledTimes(1);
  });

  it('classifies a failed catalog read like every other grid read', async () => {
    mockListInboundForeignKeys.mockRejectedValue(new Error('boom'));

    const outcome = await openReferencingRows(input({ id: 42 }));

    expect(outcome.ok).toBe(false);
    expect(outcome.error?.code).toBe('unknown');
    expect(outcome.error?.raw).toContain('boom');
  });
});
