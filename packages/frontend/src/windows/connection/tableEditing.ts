/**
 * The panel-level half of the cell editor (DB-05).
 *
 * `TableView.tsx` is close to its size limit, so everything that is not
 * layout lives here: the editability of a cell, the single entry point that
 * opens an editor, the guard that runs before a navigation moves the page
 * under an open editor, and the loader that reads the candidate values of an
 * enumeration column.
 *
 * The module keeps a small registry of the open panels' editing contexts.
 * The registry exists so that the **entry points** can be plain functions
 * (`requestEdit(panelId, coord)`) instead of props that every caller has
 * to thread through: the keyboard chapter (DB-02) opens an editor with
 * `Enter`, and the paste chapter (DB-03) coerces without opening one.
 */
import { useCallback, useEffect, useMemo } from 'react';
import { useTableDataStore } from '../../stores/tableDataStore';
import type { CellCoord } from '../../stores/tableData/gridSelection';
import { getActiveCellEditor } from '../../components/DataTable/editors/activeEditor';
import { buildDistinctCandidatesSql } from '../../lib/cellEditorCandidates';
import {
  candidatesToPickerOptions,
  FK_SELECTOR_CANDIDATE_LIMIT,
  type FkCandidateSource,
} from '../../lib/fkSelector';
import { databaseCommands } from '../../commands/database';
import { t } from '../../locales/t';
import {
  buildColumnEditorInfo,
  resolveCellEditable,
  type CellEditability,
  type CellEditingContext,
  type CellReadOnlyReason,
  type CellValueErrorReason,
  type ColumnEditorInfo,
} from '../../lib/cellEditorKind';
import { isReadOnlyCellType, type CellTypeHints } from '../../lib/cellTypes';
import type { SetValueActionId, SetValueState } from '../../lib/setValueActions';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { ColumnSchema, DatabaseType, TableType, Value } from '../../types';
import { queryCommands } from '../../commands/query';

/**
 * What the panel knows about its own editability.
 *
 * Everything here is a fact about the **panel**, not about one row: the
 * row-level checks (is this row's identity unique?) stay in the store, which
 * is the last line of defence before a write (00-contracts §4.5).
 */
export interface TableEditingContext {
  /** The panel this context belongs to. */
  readonly panelId: string;
  /** The connection (or the driver) is read-only. */
  readonly connectionReadOnly: boolean;
  /** A more precise reason the mount point knows (the view panel: `view`). */
  readonly mountReason?: CellReadOnlyReason;
  /** What kind of relation the panel shows. */
  readonly relationKind: TableType;
  /** `DatabaseTypeMeta.dataGrid?.editable`; absent means editable. */
  readonly capabilityEditable?: boolean;
  /** The table has a primary key. */
  readonly hasPrimaryKey: boolean;
  /** The table has columns at all (an unloaded panel has none). */
  readonly hasColumns: boolean;
  /** The columns of the table. */
  readonly columns: readonly ColumnSchema[];
  /** The driver's own type rules. */
  readonly hints?: CellTypeHints;
}

const contexts = new Map<string, TableEditingContext>();

/**
 * Register the editing context of a panel.
 *
 * The returned function removes it again — use it as the effect's cleanup,
 * so a closed tab does not leave its context behind.
 */
export function registerTableEditingContext(context: TableEditingContext): () => void {
  contexts.set(context.panelId, context);
  return () => {
    if (contexts.get(context.panelId) === context) contexts.delete(context.panelId);
  };
}

/** The editing context of a panel, if it is registered. */
export function getTableEditingContext(panelId: string): TableEditingContext | undefined {
  return contexts.get(panelId);
}

/**
 * The editor facts of every column of the panel.
 *
 * The host computes them once per column change — the grid must not
 * re-normalise a type per cell and per frame (05 §4.5 #3).
 */
export function buildPanelColumnEditorInfo(
  context: TableEditingContext,
): Map<string, ColumnEditorInfo> {
  return buildColumnEditorInfo(context.columns, context.hints);
}

/**
 * Whether one column of the panel may be edited, and why not.
 *
 * The answer is a **cell-independent** one: the row-level facts (a unique
 * row identity) are checked by the store when the value is written.
 */
export function resolvePanelCellEditable(panelId: string, columnName: string): CellEditability {
  const context = contexts.get(panelId);
  if (!context) {
    return {
      editable: false,
      reason: 'connectionReadOnly',
      code: 'grid.cell.readOnly',
      messageKey: 'cellEditor.readOnly.connection',
    };
  }
  const column = context.columns.find((entry) => entry.name === columnName);
  const info = buildPanelColumnEditorInfo(context).get(columnName);
  return resolveCellEditable({
    connectionReadOnly: context.connectionReadOnly,
    mountReason: context.mountReason,
    relationKind: context.relationKind,
    capabilityEditable: context.capabilityEditable,
    hasPrimaryKey: context.hasPrimaryKey,
    rowIdentityUnique: true,
    column: column ?? { name: columnName, nullable: true },
    cellType: info?.cellType ?? 'unknown',
    inMemoryByteSize: 0,
    typeReadOnly: isReadOnlyCellType(column?.dataType, context.hints),
    rowKind: 'update',
  });
}

/**
 * The one entry point that opens an editor.
 *
 * It runs the editability check first and only opens the editor when the
 * cell may be edited; the refusal is **returned**, never thrown and never
 * silent, so the caller can tell the user why.
 */
export function requestEdit(panelId: string, coord: CellCoord): CellEditability {
  const editability = resolvePanelCellEditable(panelId, coord.columnName);
  if (!editability.editable) return editability;
  useTableDataStore.getState().startEdit(panelId, coord.rowIndex, coord.columnName);
  return editability;
}

/** The result of the navigation guard. */
export type EditorNavigationGuard =
  | { readonly ok: true }
  | { readonly ok: false; readonly reason: CellValueErrorReason };

/**
 * The guard a navigation runs before it moves the page.
 *
 * An open editor holds a draft the user typed. Navigating away would take
 * that draft with it, so the editor is asked to write it first — and a
 * draft that is not acceptable **stops the navigation** instead of being
 * dropped (05 §4.4).
 */
export function resolveEditorBeforeNavigation(panelId: string): EditorNavigationGuard {
  const editor = getActiveCellEditor();
  if (!editor || editor.panelId !== panelId) return { ok: true };
  const reason = editor.commit();
  if (reason) return { ok: false, reason };
  return { ok: true };
}

/**
 * Run a navigation action — but only when the open editor can be written.
 *
 * Returns `false` when the navigation was refused, so the caller can keep
 * the editor open with its inline error.
 */
export function guardNavigation(panelId: string, action: () => void): boolean {
  const guard = resolveEditorBeforeNavigation(panelId);
  if (!guard.ok) return false;
  action();
  return true;
}

export interface CandidateValuesLoaderArgs {
  /** The session the query runs in. */
  readonly dbSessionId: string;
  /** The table the column belongs to. */
  readonly table: string;
  /** The schema, when the dialect has one. */
  readonly schema?: string | null;
  /** The database, when the dialect has one. */
  readonly database?: string | null;
  /** The dialect whose quote character is used. */
  readonly dialect?: DatabaseType;
  /**
   * The single-column foreign keys of the table, keyed by the grid column
   * that carries them (DB-20).
   *
   * A column with an entry here loads its candidates from the **target**
   * table (browse mode) instead of a `SELECT DISTINCT` of its own values.
   */
  readonly fkSources?: ReadonlyMap<string, FkCandidateSource>;
}

/**
 * The loader that reads the candidate values of an enumeration column.
 *
 * It runs **one** statement — a `SELECT DISTINCT … LIMIT 200` — and only
 * when the user asks for it. Opening an editor never queries anything
 * (05 §4.3 #5).
 *
 * A single-column foreign-key column (DB-20) takes the other branch: its
 * candidates are the **target** table's referenced values, browsed through
 * the same resolution the jump uses — and a target whose referenced columns
 * are not unique refuses to offer candidates, because picking one would
 * write an ambiguous reference (07 §7 rule 12).
 */
export function createCandidateValuesLoader(
  args: CandidateValuesLoaderArgs,
): (columnName: string) => Promise<readonly string[]> {
  return async (columnName: string): Promise<readonly string[]> => {
    const fkSource = args.fkSources?.get(columnName);
    if (fkSource) return loadFkCandidates(args, fkSource);
    const sql = buildDistinctCandidatesSql({
      table: args.table,
      column: columnName,
      schema: args.schema,
      dialect: args.dialect,
    });
    if (!sql) return [];
    const result = await queryCommands.executeQuery(
      args.dbSessionId,
      sql,
      undefined,
      args.database,
      args.schema,
    );
    const statement = result.results[0];
    if (!statement) return [];
    const values: string[] = [];
    for (const row of statement.rows) {
      const first = row[0];
      if (first === null || first === undefined) continue;
      values.push(String(first));
    }
    return values;
  };
}

/**
 * Browse one foreign key's target for the in-cell selector (DB-20).
 *
 * The request carries no key values — the selector opens before anything is
 * picked — and one `limit`, capped at the resolver's own hard cap. The
 * candidates are the target rows' referenced-column values, in the
 * statement's own order.
 *
 * The uniqueness verdict travels with the resolution, and it is the one
 * refusal the loader raises on its own: a target whose referenced columns
 * are not covered by a primary key or a unique index would make a pick an
 * ambiguous write (07 §7 rule 12), so the picker is disabled with a reason
 * instead of offering rows.
 */
async function loadFkCandidates(
  args: CandidateValuesLoaderArgs,
  source: FkCandidateSource,
): Promise<readonly string[]> {
  const resolution = await databaseCommands.resolveForeignKeyTargets({
    dbSessionId: args.dbSessionId,
    database: args.database ?? null,
    schema: args.schema ?? null,
    table: args.table,
    columns: [source.localColumn],
    constraintName: source.constraintName,
    // A browse ignores the values; the wire type keeps them required, so the
    // request carries the empty list.
    values: [],
    limit: FK_SELECTOR_CANDIDATE_LIMIT,
    browse: true,
  });
  if (!resolution.target.unique) {
    throw new Error(t('fkSelector.notUnique'));
  }
  return candidatesToPickerOptions(resolution.candidates);
}

/** The reason the whole grid is read-only, if it is. */
export function resolvePanelReadOnlyReason(
  context: TableEditingContext,
): CellReadOnlyReason | null {
  if (context.mountReason) return context.mountReason;
  if (context.connectionReadOnly) return 'connectionReadOnly';
  if (context.capabilityEditable === false) return 'driverCapability';
  if (context.relationKind !== 'table') return 'view';
  // A table **without** a primary key cannot be addressed. A panel without
  // columns is a panel that has not loaded yet — that is not a reason to
  // refuse an edit.
  if (context.hasColumns && !context.hasPrimaryKey) return 'noPrimaryKey';
  return null;
}

/** The reason one column of the panel cannot be edited, if it cannot. */
export function resolvePanelCellReadOnlyReason(
  panelId: string,
  columnName: string,
): CellReadOnlyReason | null {
  const context = contexts.get(panelId);
  if (!context) return null;
  const panelReason = resolvePanelReadOnlyReason(context);
  if (panelReason) return panelReason;
  const info = buildPanelColumnEditorInfo(context).get(columnName);
  return info?.readOnlyReason ?? null;
}

/**
 * What the "Set Value" menu may offer for one cell.
 *
 * The answer is asked for at the moment of the click: it depends on the
 * column (nullable?) and on the row (is there a staged change to revert?).
 */
export function resolveSetValueState(
  panelId: string,
  rowIndex: number,
  columnName: string,
): SetValueState | null {
  const context = contexts.get(panelId);
  if (!context) return null;
  const column = context.columns.find((entry) => entry.name === columnName);
  if (!column) return null;
  const info = buildPanelColumnEditorInfo(context).get(columnName);
  const ts = useTableDataStore.getState().byPanel.get(panelId);
  const key = ts?.rowIdentityAnchors.get(rowIndex);
  const change = key ? ts?.pendingChanges.get(key) : undefined;
  const hasPending = change ? change.changedColumns.includes(columnName) : false;
  const candidates = ts?.enumCandidates.get(columnName);
  return {
    canNull: column.nullable,
    canEmpty: true,
    rowKind: 'update',
    hasPending,
    canLoadCandidates: info?.cellType === 'enum' && !candidates,
  };
}

/**
 * Write the cell with the intent of a "Set Value" entry.
 *
 * `setDefault` and `revertCell` are the same write — "do not touch this
 * column" — and they are named differently because the two contexts mean
 * different things to the user (05 §3.6).
 */
export function applySetValueAction(
  panelId: string,
  rowIndex: number,
  columnName: string,
  action: SetValueActionId,
): void {
  const store = useTableDataStore.getState();
  switch (action) {
    case 'setNull':
      store.stageCellValue(panelId, rowIndex, columnName, null);
      return;
    case 'setEmpty':
      store.stageCellValue(panelId, rowIndex, columnName, '');
      return;
    case 'setDefault':
    case 'revertCell':
      store.stageCellValue(panelId, rowIndex, columnName, undefined);
      return;
  }
}

export interface UseTableEditingOptions {
  /** The panel (tab) the grid belongs to. */
  readonly panelId: string;
  /** The session the candidate query runs in. */
  readonly dbSessionId: string;
  /** The database the panel reads from. */
  readonly database: string;
  /** The schema, when the dialect has one. */
  readonly schema?: string | null;
  /** The table the panel shows. */
  readonly tableName: string;
  /** The dialect of the connection. */
  readonly databaseType?: string;
  /** The connection (or the driver) is read-only. */
  readonly connectionReadOnly: boolean;
  /** A more precise reason the mount point knows (the view panel: `view`). */
  readonly mountReason?: CellReadOnlyReason;
  /** The columns of the table. */
  readonly columns: readonly ColumnSchema[];
  /**
   * The single-column foreign keys of the table, keyed by their grid column
   * (DB-20). `undefined` keeps every column on the enum candidate path.
   */
  readonly fkSources?: ReadonlyMap<string, FkCandidateSource>;
}

export interface TableEditing {
  /**
   * The context the grid needs (DB-05).
   *
   * It is one object so the panel hands it over in one line — the panel file
   * is close to its size limit.
   */
  readonly cellEditing: CellEditingContext;
  /**
   * The driver's own column-type rules, read from its metadata.
   *
   * The filter editor needs the very same declaration (08 §3.5, "一处声明、
   * 两处生效"): a column the driver calls a boolean must not be filtered
   * with the operators of a number.
   */
  readonly cellTypeHints: CellTypeHints | undefined;
  /** Why one cell cannot be edited, if it cannot. */
  readonly resolveReadOnlyReason: (row: number, columnName: string) => CellReadOnlyReason | null;
}

/**
 * The editing wiring of one panel (DB-05).
 *
 * Everything the grid needs to edit a cell, in one place: the editor facts
 * of every column, the write path, the "Set Value" entries and the loader
 * for the candidate values of an enumeration column.
 */
export function useTableEditing(options: UseTableEditingOptions): TableEditing {
  const {
    panelId,
    dbSessionId,
    database,
    schema,
    tableName,
    databaseType,
    connectionReadOnly,
    mountReason,
    columns,
    fkSources,
  } = options;

  const driverMeta = databaseType ? DB_REGISTRY[databaseType as DatabaseType] : undefined;
  const hints: CellTypeHints | undefined = driverMeta?.cellTypes;
  const capabilityEditable = driverMeta?.dataGrid?.editable;

  const context = useMemo<TableEditingContext>(
    () => ({
      panelId,
      connectionReadOnly,
      mountReason,
      relationKind: 'table',
      capabilityEditable,
      hasPrimaryKey: columns.some((column) => column.isPrimaryKey),
      hasColumns: columns.length > 0,
      columns,
      hints,
    }),
    [capabilityEditable, columns, connectionReadOnly, hints, mountReason, panelId],
  );

  // The context is registered so that the entry points (`requestEdit`, the
  // navigation guard) can be plain functions.
  useEffect(() => registerTableEditingContext(context), [context]);

  const columnEditorInfo = useMemo(() => buildPanelColumnEditorInfo(context), [context]);
  const readOnlyReason = useMemo(() => resolvePanelReadOnlyReason(context), [context]);

  const rawLoader = useMemo(
    () =>
      createCandidateValuesLoader({
        dbSessionId,
        table: tableName,
        schema,
        database,
        dialect: databaseType as DatabaseType | undefined,
        fkSources,
      }),
    [database, databaseType, dbSessionId, fkSources, schema, tableName],
  );

  const loadCandidates = useCallback(
    async (columnName: string): Promise<readonly string[]> => {
      const values = await rawLoader(columnName);
      useTableDataStore.getState().setEnumCandidates(panelId, columnName, values);
      return values;
    },
    [panelId, rawLoader],
  );

  const onCellValue = useCallback(
    (row: number, col: string, value: Value | null | undefined) => {
      useTableDataStore.getState().stageCellValue(panelId, row, col, value);
    },
    [panelId],
  );

  const setValueState = useCallback(
    (_row: number, col: string) => resolveSetValueState(panelId, _row, col),
    [panelId],
  );

  const onSetValue = useCallback(
    (row: number, col: string, action: SetValueActionId) => {
      applySetValueAction(panelId, row, col, action);
    },
    [panelId],
  );

  const onLoadCandidates = useCallback(
    (_row: number, col: string) => {
      void loadCandidates(col).catch(() => {
        // The menu entry has no place to show an error; the editor shows one
        // when its own load fails.
      });
    },
    [loadCandidates],
  );

  const cellEditing = useMemo<CellEditingContext>(
    () => ({
      columnEditorInfo,
      readOnlyReason,
      panelId,
      loadCandidates,
      onCellValue,
      setValueState,
      onSetValue,
      onLoadCandidates,
    }),
    [
      columnEditorInfo,
      loadCandidates,
      onCellValue,
      onLoadCandidates,
      onSetValue,
      panelId,
      readOnlyReason,
      setValueState,
    ],
  );

  const resolveReadOnlyReason = useCallback(
    (_row: number, columnName: string) => resolvePanelCellReadOnlyReason(panelId, columnName),
    [panelId],
  );

  return { cellEditing, cellTypeHints: hints, resolveReadOnlyReason };
}
