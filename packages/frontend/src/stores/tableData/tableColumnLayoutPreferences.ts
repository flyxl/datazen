import type { TableChangeContext } from '../../lib/tableChanges';

/**
 * Per-table column layout persistence (DB-12): the render order and the
 * user-resized widths, keyed by the same table identity the sorts use (DB-11).
 *
 * What is deliberately **not** here: column *membership* (`visibleColumns`)
 * and the freeze count. Membership belongs to the panel that chose it, and
 * freezing is a reading posture of one tab — persisting either would reopen
 * a tab in a shape the user does not recognise. Widths and order are the two
 * things TablePlus persists and users expect back; they are also the two
 * things #1394/#3752 asked for.
 *
 * Like the sort persistence, this is a UI preference of one machine: it lives
 * in `localStorage`, never in the connection profile.
 */
const STORAGE_PREFIX = 'datazen:table-columns:v1';

/** What one table's stored layout can carry. Every field is optional. */
export interface PersistedColumnLayout {
  order: string[] | null;
  widths: Record<string, number>;
}

interface StoredLayout {
  order?: unknown;
  widths?: unknown;
}

/** The storage key of a table's layout, or `null` when the table has no stable identity. */
export function columnLayoutStorageKey(context: TableChangeContext): string | null {
  if (!context.connectionId) return null;
  const identity = [
    context.connectionId,
    context.database ?? '',
    context.schema ?? '',
    context.table,
  ];
  return `${STORAGE_PREFIX}:${JSON.stringify(identity)}`;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** A stored order is untrusted: only an array of strings is accepted. */
function validOrder(value: unknown): string[] | null {
  if (!Array.isArray(value)) return null;
  const names = value.filter((name): name is string => typeof name === 'string');
  return names.length > 0 ? names : null;
}

/** A stored width map is untrusted: only finite positive numbers survive. */
function validWidths(value: unknown): Record<string, number> {
  if (!isPlainObject(value)) return {};
  const widths: Record<string, number> = {};
  for (const [name, width] of Object.entries(value)) {
    if (typeof width === 'number' && Number.isFinite(width) && width > 0) {
      widths[name] = width;
    }
  }
  return widths;
}

function readStored(context: TableChangeContext): StoredLayout {
  const key = columnLayoutStorageKey(context);
  if (!key || typeof window === 'undefined' || !window.localStorage) return {};
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    return isPlainObject(parsed) ? (parsed as StoredLayout) : {};
  } catch {
    return {};
  }
}

function writeStored(
  context: TableChangeContext,
  next: { order?: string[]; widths?: Record<string, number> },
): void {
  const key = columnLayoutStorageKey(context);
  if (!key || typeof window === 'undefined' || !window.localStorage) return;
  try {
    const hasOrder = (next.order?.length ?? 0) > 0;
    const hasWidths = Object.keys(next.widths ?? {}).length > 0;
    if (!hasOrder && !hasWidths) {
      window.localStorage.removeItem(key);
      return;
    }
    const record: Record<string, unknown> = {};
    if (hasOrder) record.order = next.order;
    if (hasWidths) record.widths = next.widths;
    window.localStorage.setItem(key, JSON.stringify(record));
  } catch {
    // Quota exceeded or storage disabled: the layout still works for this
    // session, it just will not survive a restart.
  }
}

/**
 * The layout a table was left with. Anything unusable reads as "nothing
 * stored" — a corrupt or stale entry must never break the panel it decorates.
 */
export function readPersistedColumnLayout(context: TableChangeContext): PersistedColumnLayout {
  const stored = readStored(context);
  return {
    order: validOrder(stored.order),
    widths: validWidths(stored.widths),
  };
}

/** Store (or clear) the column order, keeping the widths that are already stored. */
export function persistColumnOrder(context: TableChangeContext, order: string[] | null): void {
  const stored = readStored(context);
  writeStored(context, {
    order: (order?.length ?? 0) > 0 ? (order as string[]) : undefined,
    widths: validWidths(stored.widths),
  });
}

/** Store the resized column widths, keeping the order that is already stored. */
export function persistColumnWidths(
  context: TableChangeContext,
  widths: Record<string, number>,
): void {
  const stored = readStored(context);
  writeStored(context, {
    order: validOrder(stored.order) ?? undefined,
    widths: Object.keys(widths).length > 0 ? widths : undefined,
  });
}
