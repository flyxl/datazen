/**
 * The full-database value search (DB-19): the scan that answers "which table
 * holds this value?".
 *
 * The scan is **front-end driven on purpose**: one command per table, because
 * that is what makes pause and resume real — pausing is simply not issuing
 * the next table's command, and the host never holds a long-running job the
 * UI cannot stop. The host owns the one thing the front end must never do:
 * turning a search value into a statement (bound parameters, wildcard
 * escaping — `datazen::services::value_search`).
 *
 * The rules that matter:
 *
 * * **The caller picks the columns.** A table's schema says which columns are
 *   searchable — the same normalised type table the grid renders with, plus
 *   the type groups the *driver* declared it can text-match (DBR-03). A
 *   table without any searchable column is skipped, not an error.
 * * **A failed table is a recorded row, not a failed scan.** One unreadable
 *   table out of two hundred must not lose the other one hundred ninety-nine.
 * * **"Most likely" is a ranking, not a guess.** The tables with the most
 *   hits answer the user's real question ("where is this value?"), so they
 *   lead the result list.
 * * **Every answer says which table it is about, schema included** (DBR-03).
 *   Two tables may share a name across schemas; an answer that dropped the
 *   schema would send the "open and locate" click to the wrong one.
 */

import { databaseCommands } from '../commands/database';
import { resolveCellType } from './cellTypes';
import { DB_REGISTRY } from './databaseTypes';
import type { ValueSearchMode } from '@datazen/backend-client';
import type { ColumnSchema, FilterCondition, FilterOperator, TableInfo } from '../types';

/**
 * The cell types a value search can always run over: the text-ish ones every
 * SQL dialect's `LIKE` accepts without a cast. Numbers and date/times join
 * only where the driver declared it can text-match them (see
 * {@link valueSearchTypeGroupsFor}).
 */
const BASE_SEARCHABLE_CELL_TYPES: ReadonlySet<string> = new Set([
  'text',
  'enum',
  'unknown',
]);

/** A driver-declared extra type group of the value search (DBR-03). */
export type ValueSearchTypeGroup = 'number' | 'datetime';

/** The cell types each extra group covers. */
const EXTRA_GROUP_CELL_TYPES: Record<ValueSearchTypeGroup, readonly string[]> = {
  number: ['number'],
  datetime: ['date', 'time', 'datetime'],
};

/** The type scopes the dialog offers: base, plus the declared groups. */
export type ValueSearchTypeScope =
  | 'text'
  | 'text+number'
  | 'text+datetime'
  | 'text+number+datetime';

/**
 * The extra type groups a driver declares its `LIKE` can text-match
 * (DBR-03): MySQL/MariaDB/SQLite cast non-text operands to their text
 * rendering, PostgreSQL rejects the statement outright, so it declares
 * nothing. A driver that says nothing keeps the safe text set it always had.
 */
export function valueSearchTypeGroupsFor(
  driverType: string | null | undefined,
): readonly ValueSearchTypeGroup[] {
  const declared = driverType
    ? (DB_REGISTRY as Record<string, { dataGrid?: { valueSearchExtraTypes?: readonly ValueSearchTypeGroup[] } } | undefined>)[
        driverType
      ]?.dataGrid?.valueSearchExtraTypes
    : undefined;
  return declared ?? [];
}

/** The type scopes a driver's dialog may offer, in display order. */
export function typeScopesForDriver(
  driverType: string | null | undefined,
): readonly ValueSearchTypeScope[] {
  const groups = valueSearchTypeGroupsFor(driverType);
  const number = groups.includes('number');
  const datetime = groups.includes('datetime');
  const scopes: ValueSearchTypeScope[] = ['text'];
  if (number) scopes.push('text+number');
  if (datetime) scopes.push('text+datetime');
  if (number && datetime) scopes.push('text+number+datetime');
  return scopes;
}

/** The searchable cell types of a scope: the base set plus its groups. */
export function cellTypesForScope(scope: ValueSearchTypeScope): ReadonlySet<string> {
  const types = new Set<string>(BASE_SEARCHABLE_CELL_TYPES);
  if (scope.includes('number')) types.add('number');
  if (scope.includes('datetime')) {
    for (const cellType of EXTRA_GROUP_CELL_TYPES.datetime) types.add(cellType);
  }
  return types;
}

/** The tables of the scan, in scan order. */
export interface ValueSearchTarget {
  table: string;
  schema: string | null;
}

/** The schema-qualified key of a target — the only safe identity (DBR-03). */
export function targetKey(target: { table: string; schema: string | null }): string {
  return target.schema ? `${target.schema}.${target.table}` : target.table;
}

/** One table's answer. */
export interface ValueSearchTableResult {
  table: string;
  /** The table's schema — a same-named table in another schema is another table. */
  schema: string | null;
  /** `null` when the table could not be searched. */
  matches: number | null;
  /** The columns the statement actually searched; empty when none were searchable. */
  columns: readonly string[];
  /** Why the table could not be searched; only with `matches: null`. */
  reason?: string;
}

/** What the driver is doing right now. */
export type ValueSearchStatus = 'idle' | 'running' | 'paused' | 'done' | 'cancelled';

/** The state the UI renders. */
export interface ValueSearchProgress {
  status: ValueSearchStatus;
  /** How many tables the scan has answered (errors included). */
  completed: number;
  /** How many tables the plan holds. */
  total: number;
  /** The table being searched right now; `null` between tables. */
  current: string | null;
  /** The answers so far, in scan order. */
  results: readonly ValueSearchTableResult[];
  /** Wall-clock milliseconds since the scan started; `null` before it did. */
  elapsedMs: number | null;
}

/** The column picking, as a pure function over a table's schema. */
export function searchableColumns(
  columns: readonly ColumnSchema[],
  cellTypes: ReadonlySet<string> = BASE_SEARCHABLE_CELL_TYPES,
): string[] {
  return columns
    .filter((column) => cellTypes.has(resolveCellType(column.dataType)))
    .map((column) => column.name);
}

/** The scan targets of a table list: tables only, in the list's order. */
export function valueSearchTargets(tables: readonly TableInfo[]): ValueSearchTarget[] {
  return tables
    .filter((table) => table.tableType === 'table')
    .map((table) => ({ table: table.name, schema: table.schema ?? null }));
}

/** The tables most likely to hold the value: the most hits first. */
export function rankTables(
  results: readonly ValueSearchTableResult[],
  limit = 3,
): ValueSearchTableResult[] {
  return results
    .filter((result) => (result.matches ?? 0) > 0)
    .sort((left, right) => (right.matches ?? 0) - (left.matches ?? 0))
    .slice(0, limit);
}

/**
 * What a click on an answer asks for: open that table's tab **and** land on
 * the rows the scan matched (DBR-03). The columns are the ones the statement
 * searched, so the seeded filter is the scan's own question — not a re-guess.
 */
export interface ValueSearchLocateIntent {
  table: string;
  schema: string | null;
  database: string;
  value: string;
  mode: ValueSearchMode;
  /** The searched columns; empty when the table was never queried. */
  columns: readonly string[];
}

/** The grid filter operator that says what a search mode said (DBR-03). */
export function filterOperatorForMode(mode: ValueSearchMode): FilterOperator {
  switch (mode) {
    case 'contains':
      return 'contains';
    case 'notContains':
      return 'notContains';
    case 'prefix':
      return 'startsWith';
    case 'suffix':
      return 'endsWith';
    case 'exact':
      return 'eq';
  }
}

/**
 * The locate intent as the grid's own filter conditions (DBR-03): the
 * scan's question, re-asked in the vocabulary the filter bar already
 * renders. One condition per searched column, all with the same operator.
 */
export function locateConditions(intent: ValueSearchLocateIntent): FilterCondition[] {
  const operator = filterOperatorForMode(intent.mode);
  return intent.columns.map((column) => ({
    column,
    operator,
    value: intent.value,
  }));
}

/**
 * How the seeded conditions combine: the scan ORs its columns (a hit in any
 * column counts), and the negation of an OR is an AND — the same
 * distributivity the back end's statement applies.
 */
export function locateFilterLogic(mode: ValueSearchMode): 'and' | 'or' {
  return mode === 'notContains' ? 'and' : 'or';
}

/** Everything the driver needs; the schema reader is injected for tests. */
export interface ValueSearchDriverParams {
  dbSessionId: string;
  database: string;
  /** The tables to scan, in order. */
  targets: readonly ValueSearchTarget[];
  value: string;
  mode: ValueSearchMode;
  /** Which cell types count as searchable (the dialog's type scope). */
  cellTypes: ReadonlySet<string>;
  /** Reads one table's schema; the cache makes the production path free. */
  readSchema: (target: ValueSearchTarget) => Promise<readonly ColumnSchema[]>;
}

/**
 * The scan of one value over one database's tables.
 *
 * The driver is framework-free on purpose: the dialog subscribes, the driver
 * never imports React. Pause takes effect **between** tables — a statement in
 * flight finishes (a mid-statement cancel would need a server-side KILL),
 * which is the same honesty the TablePlus documentation offers.
 */
export class ValueSearchDriver {
  private readonly listeners = new Set<(progress: ValueSearchProgress) => void>();
  private readonly gate = { resolvePause: null as (() => void) | null };
  private status: ValueSearchStatus = 'idle';
  private completed = 0;
  private results: ValueSearchTableResult[] = [];
  private current: string | null = null;
  private token = 0;
  private startedAt: number | null = null;
  private endedAt: number | null = null;

  constructor(private readonly params: ValueSearchDriverParams) {}

  /** The plan's size, before anything is searched. */
  get total(): number {
    return this.params.targets.length;
  }

  /** The current state, as a fresh object every call. */
  get progress(): ValueSearchProgress {
    return {
      status: this.status,
      completed: this.completed,
      total: this.total,
      current: this.current,
      results: this.results,
      elapsedMs:
        this.startedAt === null ? null : (this.endedAt ?? Date.now()) - this.startedAt,
    };
  }

  /** Subscribe to every state change; the unsubscribe is the return. */
  subscribe(listener: (progress: ValueSearchProgress) => void): () => void {
    this.listeners.add(listener);
    listener(this.progress);
    return () => this.listeners.delete(listener);
  }

  private emit(): void {
    for (const listener of [...this.listeners]) listener(this.progress);
  }

  /** Start (or, from `paused`, restart) the scan. A running scan ignores it. */
  start(): void {
    if (this.status === 'running' || this.status === 'done') return;
    if (this.status === 'paused') {
      this.status = 'running';
      this.emit();
      this.gate.resolvePause?.();
      this.gate.resolvePause = null;
      return;
    }
    this.status = 'running';
    this.startedAt = Date.now();
    this.endedAt = null;
    this.emit();
    const token = ++this.token;
    void this.run(token);
  }

  /** Pause after the table in flight. An idle or done scan ignores it. */
  pause(): void {
    if (this.status !== 'running') return;
    this.status = 'paused';
    this.emit();
  }

  /** Stop for good. A finished or cancelled scan ignores it. */
  cancel(): void {
    if (this.status === 'done' || this.status === 'cancelled') return;
    this.status = this.status === 'idle' ? 'idle' : 'cancelled';
    this.current = null;
    if (this.startedAt !== null) this.endedAt = Date.now();
    this.gate.resolvePause?.();
    this.gate.resolvePause = null;
    this.emit();
  }

  private async waitWhilePaused(token: number): Promise<boolean> {
    while (this.status === 'paused' && token === this.token) {
      await new Promise<void>((resolve) => {
        this.gate.resolvePause = resolve;
      });
    }
    return token === this.token;
  }

  private async run(token: number): Promise<void> {
    for (const target of this.params.targets) {
      if (token !== this.token || this.status === 'cancelled' || this.status === 'done') return;
      if (!(await this.waitWhilePaused(token))) return;
      this.current = target.table;
      this.emit();

      try {
        const columns = await this.params.readSchema(target);
        const names = searchableColumns(columns, this.params.cellTypes);
        if (names.length === 0) {
          this.results.push({
            table: target.table,
            schema: target.schema,
            matches: 0,
            columns: [],
          });
        } else {
          const result = await databaseCommands.searchTableValues({
            dbSessionId: this.params.dbSessionId,
            database: this.params.database,
            schema: target.schema,
            table: target.table,
            columns: names,
            value: this.params.value,
            mode: this.params.mode,
          });
          this.results.push({
            table: result.table,
            schema: target.schema,
            matches: result.matches,
            columns: result.searchedColumns,
          });
        }
      } catch (cause) {
        // One failed table is one recorded row — the scan keeps its promise
        // about the rest.
        this.results.push({
          table: target.table,
          schema: target.schema,
          matches: null,
          columns: [],
          reason: cause instanceof Error ? cause.message : String(cause),
        });
      }
      this.completed += 1;
      this.current = null;
      this.emit();
      // A pause requested while the statement was in flight lands here.
      if (this.status === 'paused' && !(await this.waitWhilePaused(token))) return;
    }
    if (token !== this.token) return;
    this.status = 'done';
    this.endedAt = Date.now();
    this.emit();
  }
}
