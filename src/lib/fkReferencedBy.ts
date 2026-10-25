/**
 * The reverse foreign-key flow (DB-20): "which rows reference **this** row?".
 *
 * The forward jump (DB-07) follows a cell's foreign key out. The reverse flow
 * starts from a **row**: the tables whose foreign keys point at this table are
 * the reverse catalog's answer, and each of them opens as a new tab filtered
 * to the rows that reference this one — `orders.customer_id = 42` for the
 * customer row whose `id` is 42.
 *
 * The order of the steps mirrors `openForeignKeyTarget` on purpose, because
 * every other order produces a reproducible defect:
 *
 * 1. the catalog is fetched — once per session and table, then cached;
 * 2. a catalog with no references is a **refusal**, not an empty tab;
 * 3. a constraint whose referenced columns hold a NULL in this row is
 *    skipped — a NULL key cannot be referenced, and an unfiltered tab would
 *    silently show the whole table;
 * 4. the conditions are built **before** any tab exists;
 * 5. one tab per viable constraint, a small bounded handful, each recorded
 *    in the navigation stack so "back" returns to the source row's tab.
 *
 * The host owns the catalog's correctness: it bounds the scan and normalizes
 * the drivers' different `referenced_table` spellings. This module never
 * splits a table name and never scans on its own.
 */

import { databaseCommands } from '../commands/database';
import { subscribeSchemaInvalidation } from './schemaCache';
import { useFkNavigationStore } from '../stores/fkNavigationStore';
import { usePanelStore } from '../stores/panelStore';
import { useTableDataStore } from '../stores/tableDataStore';
import { nextPanelId, type TablePanel } from '../stores/panelTypes';
import { fkNavKey, type GridViewport } from './fkNavigation';
import { classifyGridError, type GridErrorCode } from './gridErrors';
import { hostErrorMessage } from './hostErrorMessage';
import type { TableChangeContext } from './tableChanges';
import type { FilterCondition, Value } from '../types';
import type { InboundForeignKeyCatalog, InboundForeignKeyRef } from '@datazen/backend-client';

/** How many referencing tabs one reverse jump may open. */
export const MAX_REFERENCING_TABS = 4;

/** What a reverse jump did. */
export interface ReferencedByOutcome {
  /** `false` means nothing was opened and the caller renders the reason. */
  ok: boolean;
  /** The tabs the flow opened, in constraint order. */
  panelIds: string[];
  /** The classified reason of a refusal; rendered by the caller. */
  error?: { code: GridErrorCode | 'unknown'; raw: string };
  /** More viable constraints than the tab cap allows were left unopened. */
  skipped?: number;
}

/** Everything the flow needs to know about its source row. */
export interface OpenReferencingRowsInput {
  /** The tab the flow starts from. */
  sourcePanelId: string;
  /** The source table (the request and the breadcrumb both name it). */
  sourceTable: string;
  /** The runtime session of the source tab. */
  dbSessionId: string;
  /** The database of the source tab. */
  database: string;
  /** The namespace of the source tab; `null` when the driver has none. */
  schema: string | null;
  /** The connection of the source tab. */
  connectionId: string | null;
  /** The driver of the source tab. */
  driverType: string | null;
  /** The **full** row record (every column of the table), the references point at. */
  row: Readonly<Record<string, unknown>>;
  /** The viewport of the source tab, snapshotted when it is left. */
  sourceViewport?: GridViewport | null;
}

/** The cached catalog of one scope. */
interface CatalogEntry {
  status: 'loading' | 'ready';
  catalog?: InboundForeignKeyCatalog;
}

/**
 * The catalog cache, keyed by session and table.
 *
 * A right-click asks for the catalog synchronously; the fetch takes a
 * round trip, so the first ask starts it and waits, and every later ask on
 * the same table reads the cache. The schema invalidation drops the entry —
 * a constraint added or dropped changes the answer.
 */
const catalogCache = new Map<string, CatalogEntry>();

function catalogKey(
  dbSessionId: string,
  database: string,
  schema: string | null,
  table: string,
): string {
  return JSON.stringify([dbSessionId, database, schema, table]);
}

// One subscription per module instance: the grid's schema cache invalidates
// when a table's structure changes, and the catalog is part of that answer.
subscribeSchemaInvalidation((dbSessionId, tableName) => {
  for (const key of [...catalogCache.keys()]) {
    const [sessionId, , , table] = JSON.parse(key) as [string, string, string | null, string];
    if (dbSessionId === sessionId && (!tableName || tableName === table)) {
      catalogCache.delete(key);
    }
  }
});

/** Read the catalog of one scope, through the cache. */
export async function inboundCatalog(
  input: Pick<OpenReferencingRowsInput, 'dbSessionId' | 'database' | 'schema' | 'sourceTable'> & {
    sourceTable: string;
  },
): Promise<InboundForeignKeyCatalog> {
  const key = catalogKey(
    input.dbSessionId,
    input.database,
    input.schema ?? null,
    input.sourceTable,
  );
  const cached = catalogCache.get(key);
  if (cached?.status === 'ready' && cached.catalog) return cached.catalog;
  if (cached?.status === 'loading') {
    // The fetch is already in flight for this scope; wait for it by polling
    // the cache — the in-flight promise writes its result there.
    for (let attempt = 0; attempt < 200; attempt += 1) {
      await new Promise((resolve) => setTimeout(resolve, 25));
      const entry = catalogCache.get(key);
      if (entry?.status === 'ready') return entry.catalog as InboundForeignKeyCatalog;
    }
    throw new Error('grid.fk.probeFailed: the referencing-table catalog did not settle');
  }
  catalogCache.set(key, { status: 'loading' });
  try {
    const catalog = await databaseCommands.listInboundForeignKeys({
      dbSessionId: input.dbSessionId,
      database: input.database,
      schema: input.schema,
      table: input.sourceTable,
    });
    catalogCache.set(key, { status: 'ready', catalog });
    return catalog;
  } catch (cause) {
    catalogCache.delete(key);
    throw cause;
  }
}

/** Drop every cached catalog (used by tests; production invalidates by event). */
export function resetInboundCatalogCache(): void {
  catalogCache.clear();
}

/**
 * The current row's values of one constraint's referenced columns.
 *
 * `null` when any of them is missing or NULL: a NULL key cannot be
 * referenced, and a filter that quietly dropped the NULL member would point
 * at the wrong rows — the same rule the forward jump holds its values to.
 */
export function referencedValuesOfRow(
  ref: Pick<InboundForeignKeyRef, 'referencedColumns'>,
  row: Readonly<Record<string, unknown>>,
): Value[] | null {
  const values: Value[] = [];
  for (const name of ref.referencedColumns) {
    const value = row[name];
    if (value === null || value === undefined) return null;
    values.push(value as Value);
  }
  return values;
}

/** Open the tabs of the rows that reference this one. */
export async function openReferencingRows(
  input: OpenReferencingRowsInput,
): Promise<ReferencedByOutcome> {
  // 1. The catalog. A failure is classified like every other grid read.
  let catalog: InboundForeignKeyCatalog;
  try {
    catalog = await inboundCatalog(input);
  } catch (error) {
    const raw = hostErrorMessage(error);
    return { ok: false, panelIds: [], error: { code: classifyGridError(raw), raw } };
  }

  // 2. Nothing references this table: a refusal with its own sentence, not
  //    an empty tab.
  if (catalog.references.length === 0) {
    return {
      ok: false,
      panelIds: [],
      error: {
        code: 'grid.fk.noReferences',
        raw: `grid.fk.noReferences: no table references '${input.sourceTable}'`,
      },
    };
  }

  // 3. The viable constraints: every referenced column carries a value in
  //    this row. A NULL key cannot be referenced — skipping is the honest
  //    answer, not an error.
  const viable = catalog.references
    .map((ref) => ({ ref, values: referencedValuesOfRow(ref, input.row) }))
    .filter(
      (entry): entry is { ref: InboundForeignKeyRef; values: Value[] } => entry.values !== null,
    );
  if (viable.length === 0) {
    return {
      ok: false,
      panelIds: [],
      error: {
        code: 'grid.fk.noReferences',
        raw: `grid.fk.noReferences: the key of this row is NULL, so no row can reference it`,
      },
    };
  }

  // The position the flow leaves is recorded **before** anything activates a
  // target tab — the same rule the forward jump follows.
  if (input.sourceViewport) {
    useFkNavigationStore.getState().recordViewport(input.sourcePanelId, input.sourceViewport);
  }

  // 4. One tab per viable constraint, bounded. The last one wins the focus,
  //    which is the tab the user asked about when there was only one.
  const panelStore = usePanelStore.getState();
  const panels = panelStore.panels;
  const sourcePanel = panels.find((panel) => panel.id === input.sourcePanelId);
  const opened: string[] = [];
  const capped = viable.slice(0, MAX_REFERENCING_TABS);
  for (const { ref, values } of capped) {
    const navKey = fkNavKey(
      {
        database: input.database,
        schema: input.schema,
        table: ref.table,
        columns: ref.columns,
      },
      values,
    );
    const panelId = nextPanelId('tbl');
    const panel: TablePanel = {
      id: panelId,
      type: 'table',
      tableName: ref.table,
      database: input.database,
      tableSchema: input.schema,
      subTab: 'data',
      connectionId: sourcePanel?.connectionId ?? input.connectionId ?? '',
      dbSessionId: sourcePanel?.dbSessionId ?? input.dbSessionId,
      connectionName: sourcePanel?.connectionName ?? '',
      databaseType: sourcePanel?.databaseType ?? (input.driverType as TablePanel['databaseType']),
    };
    panelStore.addPanel(panel);

    // 5. The conditions are seeded in the same event turn, so the first
    //    frame already carries them — the "flash the whole table" defect is
    //    the forward jump's hardest-learned lesson.
    const conditions = buildInboundFilters(ref, values);
    const context: TableChangeContext = {
      connectionId: panel.connectionId,
      dbSessionId: panel.dbSessionId,
      driverType: panel.databaseType,
      database: input.database,
      schema: input.schema,
      table: ref.table,
    };
    useTableDataStore.getState().seedPanel(panelId, context, { filters: conditions });

    // 6. The navigation records the step, and the tab carries where it came
    //    from — "back" returns to the row's own tab.
    const nav = useFkNavigationStore.getState();
    const entry = nav.push({
      panelId,
      navKey,
      origin: {
        sourcePanelId: input.sourcePanelId,
        sourceTable: input.sourceTable,
        localColumns: [...ref.columns],
        targetTable: ref.table,
        targetColumns: [...ref.referencedColumns],
      },
    });
    usePanelStore.getState().updatePanel(panelId, {
      fkOrigin: {
        sourcePanelId: input.sourcePanelId,
        sourceTable: input.sourceTable,
        localColumns: [...ref.columns],
        targetTable: ref.table,
        targetColumns: [...ref.referencedColumns],
        navEntryId: entry.id,
      },
    });
    opened.push(panelId);
  }

  return {
    ok: true,
    panelIds: opened,
    skipped: viable.length - capped.length,
  };
}

/**
 * The filter of one referencing tab: the referencing table's own columns,
 * paired positionally with this row's referenced values.
 */
export function buildInboundFilters(
  ref: Pick<InboundForeignKeyRef, 'columns' | 'referencedColumns'>,
  values: readonly Value[],
): FilterCondition[] {
  const filters: FilterCondition[] = [];
  for (let index = 0; index < ref.columns.length; index += 1) {
    const value = values[index];
    if (value === null || value === undefined) return [];
    filters.push({ column: ref.columns[index], operator: 'eq', value });
  }
  return filters;
}
