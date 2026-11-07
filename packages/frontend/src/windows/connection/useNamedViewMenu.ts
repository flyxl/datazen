import { useCallback, useMemo } from 'react';
import { namedViewFiles, namedViewOperations } from '../../commands/namedViews';
import {
  namedViewsAvailable,
  readViewFilters,
  type NamedView,
  type NamedViewScope,
} from '../../lib/namedViews';
import { filterDraftEqualsApplied } from '../../stores/tableData/filterUtils';
import type { FilterCondition, SortCondition } from '../../types';
import type { NamedViewMenuConfig } from './TableFilterToolbar';
import type { TableActions } from './useTableActions';

/** What the saved-views menu needs from the panel. */
export interface UseNamedViewMenuOptions {
  /** The persistent connection id; without it there is no scope to save in. */
  connectionId?: string;
  /** The logical database of the panel. */
  database: string;
  /** The schema of the panel, when the engine has one. */
  schema?: string | null;
  /** The table the panel shows. */
  tableName: string;
  /** The applied filters. */
  filters: FilterCondition[];
  /** The applied logic. */
  filterLogic: 'and' | 'or';
  /** The sorts in effect. */
  sorts: SortCondition[];
  /** The draft filters. */
  draftFilters: FilterCondition[];
  /** The draft logic. */
  draftFilterLogic: 'and' | 'or';
  /** The visible columns; `null` means every column is visible. */
  visibleColumns: string[] | null;
  /** The panel's action table. */
  actions: TableActions;
}

/**
 * The saved-views menu of one panel, or nothing.
 *
 * The menu is **absent** — not broken — when the panel has no persistent
 * connection id (a view has to belong to one) or when the bound backend
 * cannot serve the saved views at all (a browser build has no host store to
 * keep them in).
 */
export function useNamedViewMenu(options: UseNamedViewMenuOptions): NamedViewMenuConfig | null {
  const {
    connectionId,
    database,
    schema,
    tableName,
    filters,
    filterLogic,
    sorts,
    draftFilters,
    draftFilterLogic,
    visibleColumns,
    actions,
  } = options;

  /**
   * The scope of the saved views (08 §3.7).
   *
   * The scope is the **persistent** identity of the table: the connection id,
   * the database, the schema and the table. A `dbSessionId` is runtime state
   * and is never persisted, so it must never appear here (AGENTS.md, "ID
   * 术语规范").
   */
  const scope = useMemo<NamedViewScope | null>(
    () =>
      connectionId
        ? {
            connectionId,
            database: database || null,
            schema: schema ?? null,
            table: tableName,
          }
        : null,
    [connectionId, database, schema, tableName],
  );

  /** Apply a saved view: the filters, the first sort and the column layout. */
  const onApply = useCallback(
    (view: NamedView) => {
      const readout = readViewFilters(view.filters);
      if (readout.unknownOperators.length > 0) return;
      actions.setFilters(readout.filters, view.filterLogic);
      const sort = view.sorts[0];
      if (sort) actions.setSort(sort);
      if (view.visibleColumns) actions.setVisibleColumns(view.visibleColumns);
    },
    [actions],
  );

  return useMemo<NamedViewMenuConfig | null>(() => {
    if (!scope) return null;
    if (!namedViewsAvailable()) return null;
    return {
      scope,
      applied: {
        scope,
        filters,
        filterLogic,
        sorts,
        visibleColumns,
        columnOrder: null,
      },
      draft: { filters: draftFilters, filterLogic: draftFilterLogic },
      dirty: !filterDraftEqualsApplied(draftFilters, draftFilterLogic, filters, filterLogic),
      onApply,
      onApplyDraft: actions.applyFilters,
      operations: namedViewOperations,
      files: namedViewFiles,
    };
  }, [
    actions,
    draftFilterLogic,
    draftFilters,
    filterLogic,
    filters,
    onApply,
    scope,
    sorts,
    visibleColumns,
  ]);
}
