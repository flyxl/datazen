/**
 * The named-view helpers: the pure functions the saved-views menu, the
 * import and the export need.
 *
 * The DTO itself is **not** defined here. `NamedView` and `NamedViewScope`
 * live in `@datazen/backend-client`, which is the cross-shape contract the
 * host and every other form factor bind against; this module re-exports them
 * so a caller has one import, and adds nothing that is a second definition of
 * the same shape.
 *
 * Everything here is a pure function of its arguments. That is what makes the
 * rules testable without a store, and what keeps the host and the shared
 * Rust layer reaching the same answers.
 */
import { getBackendClient } from '@datazen/backend-client';
import type {
  FilterCondition as PortFilterCondition,
  NamedView,
  NamedViewScope,
} from '@datazen/backend-client';
import type { FilterCondition, FilterOperator, SortCondition } from '../types';
import { DIALECT_FILTER_OPERATORS, SHARED_FILTER_OPERATORS } from '../types';

export type { NamedView, NamedViewScope };
export type { FilterCondition, SortCondition };

/**
 * Whether the bound backend can serve the saved views.
 *
 * A browser build has no host store, so it declares the three commands as a
 * **capability** gap; the entry is then hidden rather than offered and
 * refused later. An unbound backend cannot serve them either.
 */
export function namedViewsAvailable(): boolean {
  const client = getBackendClient();
  if (!client) return false;
  return client.transport.unsupportedMethods?.listNamedViews === undefined;
}

/** The version of the export file. A file of another version is refused. */
export const NAMED_VIEW_EXPORT_VERSION = 1;

/** The file an export writes and an import reads. */
export interface NamedViewExportFile {
  version: number;
  exportedAt: string;
  views: NamedView[];
}

/** Trim a name and collapse every run of whitespace into one space. */
export function normalizeViewName(name: string): string {
  return name.trim().replace(/\s+/g, ' ');
}

/** The key two names are compared by: normalized and lower-cased. */
export function normalizedNameKey(name: string): string {
  return normalizeViewName(name).toLowerCase();
}

/**
 * Whether two names are the same name.
 *
 * The comparison is case-insensitive: `Users` and `users` would be two
 * entries the user cannot tell apart in a menu.
 */
export function viewNamesEqual(left: string, right: string): boolean {
  return normalizedNameKey(left) === normalizedNameKey(right);
}

/**
 * The stable identity of a scope.
 *
 * The parts are tagged so that a missing database (`null`) and an empty one
 * (`''`) cannot produce the same key — a table in "no database" and a table
 * in the database named "" are different tables, and their views must not
 * share a scope.
 */
export function namedViewScopeKey(scope: NamedViewScope): string {
  const part = (value: string | null): string => (value === null ? 'n:' : `s:${value}`);
  return `c:${scope.connectionId}|d:${part(scope.database)}|s:${part(scope.schema)}|t:${scope.table}`;
}

/**
 * The name an imported view gets.
 *
 * The name the file carries, unless the scope already has it — then
 * `"Users"`, `"Users (2)"`, `"Users (3)"`, … The comparison is the same
 * case-insensitive one the scope uses, so an import cannot smuggle in a
 * second `users` next to a `Users`.
 */
export function resolveImportName(name: string, taken: ReadonlySet<string>): string {
  const base = normalizeViewName(name);
  if (base === '') return base;
  if (!containsName(taken, base)) return base;
  // The suffix grows until it finds a free name. The bound is a safety net:
  // a file cannot realistically hold a million views of one name, and an
  // unbounded loop would hang the import.
  for (let suffix = 2; suffix < 100_000; suffix += 1) {
    const candidate = `${base} (${suffix})`;
    if (!containsName(taken, candidate)) return candidate;
  }
  return `${base} (imported)`;
}

/** Whether the set already holds this name, compared case-insensitively. */
function containsName(taken: ReadonlySet<string>, name: string): boolean {
  const key = normalizedNameKey(name);
  for (const existing of taken) {
    if (normalizedNameKey(existing) === key) return true;
  }
  return false;
}

/** The views of one scope. */
export function viewsForScope(
  views: readonly NamedView[],
  scope: NamedViewScope,
): NamedView[] {
  const key = namedViewScopeKey(scope);
  return views.filter((view) => namedViewScopeKey(view.scope) === key);
}

/**
 * The order the menu shows: the default first, then by name.
 *
 * The same order the host uses, so a list the host sent and a list the
 * frontend sorted again look the same.
 */
export function sortNamedViews(views: readonly NamedView[]): NamedView[] {
  return [...views].sort((left, right) => {
    if (left.isDefault !== right.isDefault) return left.isDefault ? -1 : 1;
    const byName = normalizedNameKey(left.name).localeCompare(normalizedNameKey(right.name));
    if (byName !== 0) return byName;
    const byRawName = left.name.localeCompare(right.name);
    if (byRawName !== 0) return byRawName;
    return left.id.localeCompare(right.id);
  });
}

/** The table state a view is taken from. */
export interface TableStateForView {
  /** Where the view belongs. */
  scope: NamedViewScope;
  /** The **applied** filters — the ones the query actually ran with. */
  filters: FilterCondition[];
  /**
   * The draft filters.
   *
   * They are deliberately **not** used: a view saves what is in effect, not
   * what is being typed. The field is part of the shape so a caller that has
   * both cannot pass the wrong one by accident.
   */
  draftFilters?: FilterCondition[];
  /** The applied logic. */
  filterLogic: 'and' | 'or';
  /** The sorts in effect. */
  sorts?: SortCondition[];
  /** The visible columns; `null` means every column is visible. */
  visibleColumns?: string[] | null;
  /** The column order; `null` means the driver's default order. */
  columnOrder?: string[] | null;
}

/**
 * The view of a table state.
 *
 * A new view carries no id: the host mints one and returns the stored
 * record, and the caller replaces its local copy with that answer.
 */
export function viewFromTableState(
  state: TableStateForView,
  options: { id?: string; name?: string; isDefault?: boolean; now?: Date } = {},
): NamedView {
  const now = options.now ?? new Date();
  return {
    id: options.id ?? '',
    name: options.name ?? '',
    scope: state.scope,
    filters: state.filters,
    filterLogic: state.filterLogic,
    sorts: state.sorts ?? [],
    visibleColumns: state.visibleColumns ?? null,
    columnOrder: state.columnOrder ?? null,
    isDefault: options.isDefault ?? false,
    createdAt: now.toISOString(),
    updatedAt: null,
  };
}

/** The operators the host knows. */
const HOST_FILTER_OPERATORS: ReadonlySet<string> = new Set<string>([
  ...SHARED_FILTER_OPERATORS,
  ...DIALECT_FILTER_OPERATORS,
]);

/** Whether a wire operator is one the host knows. */
export function isHostFilterOperator(operator: string): operator is FilterOperator {
  return HOST_FILTER_OPERATORS.has(operator);
}

/** What reading a view's filters found. */
export interface ViewFilterReadout {
  /** The filters the host can apply. */
  filters: FilterCondition[];
  /** The operators the host does not know — the view cannot be applied. */
  unknownOperators: string[];
}

/**
 * The host-shaped filters of a view.
 *
 * The port carries the operator as a plain string — it must not know which
 * operators a dialect declares — so reading a view back means narrowing the
 * operator to the host's union. An operator the host does not know is a
 * **refusal**, not a silent drop: the view cannot be applied, and the menu
 * says so instead of running a different query than the user saved.
 */
export function readViewFilters(
  conditions: readonly PortFilterCondition[],
): ViewFilterReadout {
  const unknownOperators: string[] = [];
  const filters: FilterCondition[] = [];
  for (const condition of conditions) {
    if (!isHostFilterOperator(condition.operator)) {
      if (!unknownOperators.includes(condition.operator)) {
        unknownOperators.push(condition.operator);
      }
      continue;
    }
    filters.push({
      column: condition.column,
      operator: condition.operator,
      ...(condition.value === undefined ? {} : { value: condition.value }),
      ...(condition.connector === undefined ? {} : { connector: condition.connector }),
    });
  }
  return { filters, unknownOperators };
}

/** The names already taken in a scope. */
export function takenNamesInScope(
  views: readonly NamedView[],
  scope: NamedViewScope,
): Set<string> {
  return new Set(viewsForScope(views, scope).map((view) => view.name));
}

/**
 * The export file of a set of views.
 *
 * The file is versioned, so a later build can add a field without making the
 * old files unreadable.
 */
export function buildNamedViewExport(
  views: readonly NamedView[],
  now: Date = new Date(),
): NamedViewExportFile {
  return {
    version: NAMED_VIEW_EXPORT_VERSION,
    exportedAt: now.toISOString(),
    views: [...views],
  };
}

/**
 * Read an export file.
 *
 * The payload is untrusted input: a file the user picked may be anything, so
 * the version is checked and every view is checked against the shape the
 * host expects. A file that does not hold is **refused** with a readable
 * reason, never imported half way.
 */
export function parseNamedViewExport(text: string): NamedViewExportFile {
  let payload: unknown;
  try {
    payload = JSON.parse(text);
  } catch {
    throw new Error('The file is not valid JSON.');
  }
  if (!isRecord(payload)) {
    throw new Error('The file does not hold a saved-view export.');
  }
  if (payload.version !== NAMED_VIEW_EXPORT_VERSION) {
    throw new Error(`The file has version ${String(payload.version)}, expected ${NAMED_VIEW_EXPORT_VERSION}.`);
  }
  if (!Array.isArray(payload.views)) {
    throw new Error('The file does not hold a list of views.');
  }
  const views: NamedView[] = [];
  for (const entry of payload.views) {
    const view = readExportedView(entry);
    if (view === null) {
      throw new Error('The file holds a view that is not a saved view.');
    }
    views.push(view);
  }
  return {
    version: NAMED_VIEW_EXPORT_VERSION,
    exportedAt: typeof payload.exportedAt === 'string' ? payload.exportedAt : new Date().toISOString(),
    views,
  };
}

/** Read one view out of an export file, or `null` when it is not one. */
function readExportedView(entry: unknown): NamedView | null {
  if (!isRecord(entry)) return null;
  if (typeof entry.id !== 'string') return null;
  if (typeof entry.name !== 'string') return null;
  if (!isRecord(entry.scope)) return null;
  if (typeof entry.scope.connectionId !== 'string') return null;
  if (typeof entry.scope.table !== 'string') return null;
  if (!Array.isArray(entry.filters)) return null;
  if (entry.filterLogic !== 'and' && entry.filterLogic !== 'or') return null;
  if (!Array.isArray(entry.sorts)) return null;
  return {
    id: entry.id,
    name: entry.name,
    scope: {
      connectionId: entry.scope.connectionId,
      database: typeof entry.scope.database === 'string' ? entry.scope.database : null,
      schema: typeof entry.scope.schema === 'string' ? entry.scope.schema : null,
      table: entry.scope.table,
    },
    filters: entry.filters as NamedView['filters'],
    filterLogic: entry.filterLogic,
    sorts: entry.sorts as NamedView['sorts'],
    visibleColumns: Array.isArray(entry.visibleColumns)
      ? (entry.visibleColumns as string[])
      : null,
    columnOrder: Array.isArray(entry.columnOrder) ? (entry.columnOrder as string[]) : null,
    isDefault: entry.isDefault === true,
    createdAt: typeof entry.createdAt === 'string' ? entry.createdAt : new Date().toISOString(),
    updatedAt: typeof entry.updatedAt === 'string' ? entry.updatedAt : null,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
