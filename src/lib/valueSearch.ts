/**
 * The full-database value search (DB-19): the scan that answers "which table
 * holds this value?".
 *
 * The scan is **front-end driven on purpose**: one command per table, because
 * that is what makes pause and resume real — pausing is simply not issuing
 * the next table's command, and the host never holds a long-running job the
 * UI cannot stop. The host owns the one thing the front end must never do:
 * turning a search value into a statement (bound parameters, wildcard
 * escaping — `datazen_feature_core::services::value_search`).
 *
 * The rules that matter:
 *
 * * **The caller picks the columns.** A table's schema says which columns are
 *   text-ish (the same normalised type table the grid renders with); the
 *   search runs over those, and a table without any is skipped, not an error.
 * * **A failed table is a recorded row, not a failed scan.** One unreadable
 *   table out of two hundred must not lose the other one hundred ninety-nine.
 * * **"Most likely" is a ranking, not a guess.** The tables with the most
 *   hits answer the user's real question ("where is this value?"), so they
 *   lead the result list.
 */

import { databaseCommands } from '../commands/database';
import { resolveCellType } from './cellTypes';
import type { ValueSearchMode } from '@datazen/backend-client';
import type { ColumnSchema, TableInfo } from '../types';

/** The normalised cell types a value search can run over. */
const SEARCHABLE_CELL_TYPES: ReadonlySet<string> = new Set(['text', 'enum', 'unknown']);

/** The tables of the scan, in scan order. */
export interface ValueSearchTarget {
  table: string;
  schema: string | null;
}

/** One table's answer. */
export interface ValueSearchTableResult {
  table: string;
  /** `null` when the table could not be searched. */
  matches: number | null;
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
}

/** The column picking, as a pure function over a table's schema. */
export function searchableColumns(columns: readonly ColumnSchema[]): string[] {
  return columns
    .filter((column) => SEARCHABLE_CELL_TYPES.has(resolveCellType(column.dataType)))
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

/** Everything the driver needs; the schema reader is injected for tests. */
export interface ValueSearchDriverParams {
  dbSessionId: string;
  database: string;
  /** The tables to scan, in order. */
  targets: readonly ValueSearchTarget[];
  value: string;
  mode: ValueSearchMode;
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
        const names = searchableColumns(columns);
        if (names.length === 0) {
          this.results.push({ table: target.table, matches: 0 });
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
          this.results.push({ table: result.table, matches: result.matches });
        }
      } catch (cause) {
        // One failed table is one recorded row — the scan keeps its promise
        // about the rest.
        this.results.push({
          table: target.table,
          matches: null,
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
    this.emit();
  }
}
