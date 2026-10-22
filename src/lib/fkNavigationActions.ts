/**
 * The foreign-key jump (DB-07) — the **only** place a jump is orchestrated.
 *
 * Every other part of the grid may *ask* for a jump; none of them may build
 * the target tab itself. The order of the steps is fixed, because every other
 * order produces a reproducible defect:
 *
 * 1. the front end checks the shape of the request (no IPC for a request that
 *    cannot succeed);
 * 2. the host resolves the target (`resolve_foreign_key_targets`);
 * 3. a failure opens **no** tab — the source tab shows the classified error;
 * 4. a jump that is already in the navigation stack reuses its tab;
 * 5. otherwise a **new** tab is created — never the existing "open table"
 *    path, which de-duplicates by table name and would silently drop the
 *    conditions of the jump;
 * 6. the conditions are seeded into the tab's data slice, in the same event
 *    turn, so the first frame already carries them;
 * 7. the jump is recorded in the navigation stack and the source tab's
 *    viewport is snapshotted.
 *
 * The host is the only authority on where the target lives: the driver
 * reports the referenced table differently per driver (PostgreSQL prefixes the
 * schema, MySQL does not), so nothing here may split a table name.
 */

import { databaseCommands } from '../commands/database';
import { useFkNavigationStore } from '../stores/fkNavigationStore';
import { usePanelStore } from '../stores/panelStore';
import { useTableDataStore } from '../stores/tableDataStore';
import { nextPanelId, type Panel, type TablePanel } from '../stores/panelTypes';
import { buildFkFilters, fkNavKey, type GridViewport } from './fkNavigation';
import { classifyGridError, type GridErrorCode } from './gridErrors';
import type { ForeignKeyInfo, Value } from '../types';
import type { ForeignKeyTargetRef } from '../types/connection';
import type { TableChangeContext } from './tableChanges';

/** What a jump did. */
export interface FkJumpOutcome {
  /** `false` means nothing happened: no tab was opened, no error was shown. */
  ok: boolean;
  /** The classified reason of a refused jump. */
  error?: { code: GridErrorCode | 'unknown'; raw: string };
  /** The tab the jump landed on. */
  panelId?: string;
  /**
   * The jump reused a tab that was already open (a repeated jump or a cycle)
   * instead of opening a new one.
   */
  reused?: boolean;
  /** The target table is readable but the referenced row is gone. */
  targetMissing?: boolean;
}

/** Everything a jump needs to know about its source. */
export interface OpenForeignKeyTargetInput {
  /** The tab the jump starts from. */
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
  /** The constraint the cell belongs to. */
  foreignKey: ForeignKeyInfo;
  /** The values of the constraint's local columns, in pairing order. */
  values: Value[];
  /** The viewport of the source tab, snapshotted when it is left. */
  sourceViewport?: GridViewport | null;
}

/**
 * The message of a failed call, whatever the transport threw.
 *
 * The function itself lives in `./hostErrorMessage` — it is a pure function
 * about error text, and a component that needs it must not pull the
 * navigation stores in with it. It is re-exported here because this is the
 * path the rest of the code already uses.
 */
export { hostErrorMessage } from './hostErrorMessage';

import { hostErrorMessage } from './hostErrorMessage';

/** The identity of a table, as the panel store and the host both name it. */
interface TableIdentity {
  database: string;
  schema: string | null;
  table: string;
}

function identityOfTarget(target: Pick<ForeignKeyTargetRef, 'database' | 'schema' | 'table'>): TableIdentity {
  return { database: target.database, schema: target.schema ?? null, table: target.table };
}

function identityOfPanel(panel: Panel): TableIdentity | null {
  if (panel.type !== 'table') return null;
  return {
    database: panel.database,
    schema: panel.tableSchema ?? null,
    table: panel.tableName,
  };
}

function sameIdentity(left: TableIdentity, right: TableIdentity): boolean {
  return (
    left.database === right.database &&
    left.schema === right.schema &&
    left.table === right.table
  );
}

/**
 * The tab a jump may reuse instead of opening a new one.
 *
 * Two cases, and only two:
 *
 * - **the same jump**: the same target with the same values is already in the
 *   stack. Clicking the arrow twice must not pile up two identical tabs;
 * - **the way back**: the target is the table the navigation came *from*.
 *   That is the round trip A → B → A, and it is the case that used to open a
 *   third tab every time the user walked back and forth.
 *
 * A tab the user opened by hand is never reused: it carries its own
 * conditions, and overwriting them would be a surprise.
 */
function findReusablePanel(input: {
  target: TableIdentity;
  navKey: string;
  sourcePanelId: string;
  panels: Panel[];
}): { panelId: string; entryId: string } | null {
  const nav = useFkNavigationStore.getState();

  // The same jump: the key covers the target **and** the values, so it only
  // matches a jump to the very same row.
  const duplicateId = nav.findEntryIdByNavKey(input.navKey);
  if (duplicateId !== null) {
    const entry = nav.back.find((item) => item.id === duplicateId);
    if (entry && input.panels.some((panel) => panel.id === entry.panelId)) {
      return { panelId: entry.panelId, entryId: entry.id };
    }
  }

  // The way back: the target is the table of a tab the navigation came from.
  for (let index = nav.back.length - 1; index >= 0; index -= 1) {
    const entry = nav.back[index];
    const origin = input.panels.find((panel) => panel.id === entry.origin.sourcePanelId);
    if (!origin || origin.id === input.sourcePanelId) continue;
    const identity = identityOfPanel(origin);
    if (!identity) continue;
    if (sameIdentity(identity, input.target)) {
      return { panelId: origin.id, entryId: entry.id };
    }
  }
  return null;
}

/**
 * Follow the foreign key of one cell.
 *
 * The returned outcome says what happened; the caller renders the error and
 * nothing else — the jump itself never touches the UI.
 */
export async function openForeignKeyTarget(
  input: OpenForeignKeyTargetInput,
): Promise<FkJumpOutcome> {
  const columns = input.foreignKey.columns;

  // 1. The shape of the request, checked before any IPC: a composite key with
  //    a missing value cannot be located, and "no condition at all" would
  //    silently show the whole table.
  if (columns.length === 0 || columns.length !== input.values.length) {
    return {
      ok: false,
      error: {
        code: 'grid.fk.shapeMismatch',
        raw: `grid.fk.shapeMismatch: the foreign key has ${columns.length} local column(s) but ${input.values.length} value(s)`,
      },
    };
  }
  if (input.values.some((value) => value === null || value === undefined)) {
    return {
      ok: false,
      error: {
        code: 'grid.fk.valueNull',
        raw: 'grid.fk.valueNull: a member of the foreign key has no value, so the target row cannot be addressed',
      },
    };
  }

  // 2. The host resolves the target.
  let resolution;
  try {
    resolution = await databaseCommands.resolveForeignKeyTargets({
      dbSessionId: input.dbSessionId,
      database: input.database,
      schema: input.schema,
      table: input.sourceTable,
      columns,
      constraintName: input.foreignKey.name,
      values: input.values,
    });
  } catch (error) {
    // 3. A failed resolution opens no tab: the source tab shows the error.
    const raw = hostErrorMessage(error);
    return { ok: false, error: { code: classifyGridError(raw), raw } };
  }

  const target = resolution.target;
  const navKey = fkNavKey(target, input.values);
  const panelStore = usePanelStore.getState();
  const panels = panelStore.panels;

  // 4. A jump that the navigation already knows.
  const reusable = findReusablePanel({
    target: identityOfTarget(target),
    navKey,
    sourcePanelId: input.sourcePanelId,
    panels,
  });
  if (reusable) {
    const nav = useFkNavigationStore.getState();
    nav.rewindTo(reusable.entryId);
    panelStore.setActivePanel(reusable.panelId);
    recordSourceViewport(input);
    return {
      ok: true,
      panelId: reusable.panelId,
      reused: true,
      targetMissing: !resolution.exists,
    };
  }

  // 5. The conditions of the jump, built **before** the tab exists: a target
  //    that does not line up with the values must not leave an empty tab
  //    behind (07 §10 #3 — the "flash the whole table" defect).
  const conditions = buildFkFilters(target, input.values);
  if (conditions === null) {
    // The host resolved a target that does not line up with the values: the
    // jump is refused rather than opened with a half-written condition.
    return {
      ok: false,
      error: {
        code: 'grid.fk.shapeMismatch',
        raw: `grid.fk.shapeMismatch: the resolved target has ${target.columns.length} referenced column(s) but ${input.values.length} value(s)`,
      },
    };
  }

  // 6. A **new** tab. Not the "open table" path: that one de-duplicates by
  //    table name and would switch to an existing tab without the conditions.
  const sourcePanel = panels.find((panel) => panel.id === input.sourcePanelId);
  const panelId = nextPanelId('tbl');
  const panel: TablePanel = {
    id: panelId,
    type: 'table',
    tableName: target.table,
    database: target.database,
    tableSchema: target.schema,
    subTab: 'data',
    connectionId: sourcePanel?.connectionId ?? input.connectionId ?? '',
    dbSessionId: sourcePanel?.dbSessionId ?? input.dbSessionId,
    connectionName: sourcePanel?.connectionName ?? '',
    databaseType: sourcePanel?.databaseType ?? (input.driverType as TablePanel['databaseType']),
  };
  panelStore.addPanel(panel);

  // 7. The conditions are seeded into the tab's data slice — in the same
  //    event turn, so the first frame already shows them. The tab's own first
  //    load is the one fetch, and it carries the conditions.
  const context: TableChangeContext = {
    connectionId: panel.connectionId,
    dbSessionId: panel.dbSessionId,
    driverType: panel.databaseType,
    database: target.database,
    schema: target.schema,
    table: target.table,
  };
  useTableDataStore.getState().seedPanel(panelId, context, { filters: conditions });

  // 8. The jump is recorded, and the tab carries where it came from.
  const nav = useFkNavigationStore.getState();
  const entry = nav.push({
    panelId,
    navKey,
    origin: {
      sourcePanelId: input.sourcePanelId,
      sourceTable: input.sourceTable,
      localColumns: [...columns],
      targetTable: target.table,
      targetColumns: [...target.columns],
    },
  });
  usePanelStore.getState().updatePanel(panelId, {
    fkOrigin: {
      sourcePanelId: input.sourcePanelId,
      sourceTable: input.sourceTable,
      localColumns: [...columns],
      targetTable: target.table,
      targetColumns: [...target.columns],
      navEntryId: entry.id,
    },
  });
  recordSourceViewport(input);

  // 9. The referenced row may be gone: that is a **successful** jump with an
  //    empty result, not an error.
  return { ok: true, panelId, targetMissing: !resolution.exists };
}

/** Snapshot the tab the jump leaves, so "back" can restore its position. */
function recordSourceViewport(input: OpenForeignKeyTargetInput): void {
  if (!input.sourceViewport) return;
  useFkNavigationStore.getState().recordViewport(input.sourcePanelId, input.sourceViewport);
}
