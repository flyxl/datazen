import { create } from 'zustand';
import type {
  QbAggregate,
  QbCondition,
  QbConditionGroup,
  QbSortItem,
  QbColumnSelection,
  QbGroupByItem,
  QbJoin,
  QbJoinType,
} from '../components/query-builder/types';
import {
  CARD_STRIDE,
  alignDroppedCard,
} from '../components/query-builder/DiagramCanvas/cardLayout';

// ── Types ─────────────────────────────────────────────────────

/** Bottom region tab of the visual builder. */
export type QbBottomTab = 'build' | 'preview';

/**
 * The canvas-config subset of the builder state.
 *
 * Used for the entry snapshot: it powers both the "discard changes?" dirty
 * check and the cancel rollback. View-only concerns (open state, tab, canvas
 * collapse) are intentionally excluded — toggling a tab is not "a change".
 */
export interface QbCanvasSnapshot {
  selectedTables: string[];
  selectedColumns: QbColumnSelection[];
  joins: QbJoin[];
  tableAliases: Record<string, string>;
  tablePositions: Record<string, { x: number; y: number }>;
  where: QbConditionGroup;
  /** HAVING clause root group (aggregate filters). */
  having: QbConditionGroup;
  orderBy: QbSortItem[];
  groupBy: QbGroupByItem[];
  distinct: boolean;
  limit: number | null;
  offset: number | null;
}

/**
 * Per-panel builder session. The builder belongs to the query panel that
 * opened it: switching panels swaps this record in, and destroying the panel
 * deletes it (a brand-new panel must start from a blank canvas).
 */
export interface QbPanelSession {
  canvas: QbCanvasSnapshot;
  bottomTab: QbBottomTab;
  canvasCollapsed: boolean;
}

/** Alias derived from a table name: `film_actor` → `fa`, `actor` → `a`. */
export function defaultAliasBase(table: string): string {
  const words = table.split(/[^A-Za-z0-9]+/).filter(Boolean);
  if (words.length === 0) return 't';
  if (words.length === 1) return words[0]!.slice(0, 1).toLowerCase();
  return words
    .map((w) => w[0]!)
    .join('')
    .slice(0, 4)
    .toLowerCase();
}

/** First alias not already taken by another table. */
export function uniqueTableAlias(table: string, aliases: Record<string, string>): string {
  const base = defaultAliasBase(table);
  const taken = new Set(
    Object.entries(aliases)
      .filter(([name]) => name !== table)
      .map(([, alias]) => alias)
      .filter(Boolean),
  );
  if (!taken.has(base)) return base;
  let suffix = 2;
  while (taken.has(`${base}${suffix}`)) suffix += 1;
  return `${base}${suffix}`;
}

/** Deep-clone a plain-JSON sub-state into an immutable snapshot. */
export function snapshotCanvas(s: QueryBuilderState): QbCanvasSnapshot {
  return JSON.parse(
    JSON.stringify({
      selectedTables: s.selectedTables,
      selectedColumns: s.selectedColumns,
      joins: s.joins,
      tableAliases: s.tableAliases,
      tablePositions: s.tablePositions,
      where: s.where,
      having: s.having,
      orderBy: s.orderBy,
      groupBy: s.groupBy,
      distinct: s.distinct,
      limit: s.limit,
      offset: s.offset,
    }),
  ) as QbCanvasSnapshot;
}

/** Stable serialisation used for the dirty comparison. */
function serialiseSnapshot(s: QbCanvasSnapshot): string {
  return JSON.stringify(s);
}

/** True when the live canvas differs from the snapshot captured on open. */
export function canvasChangedSince(state: QueryBuilderState): boolean {
  if (!state.entrySnapshot) return false;
  return serialiseSnapshot(snapshotCanvas(state)) !== serialiseSnapshot(state.entrySnapshot);
}

/** Complete state of the visual query builder. */
export interface QueryBuilderState {
  /** Currently selected table names. */
  selectedTables: string[];
  /** Currently selected columns (with optional alias / aggregate / sort / groupBy / where). */
  selectedColumns: QbColumnSelection[];
  /** WHERE clause root group. */
  where: QbConditionGroup;
  /** HAVING clause root group — filters on aggregates, emitted after GROUP BY. */
  having: QbConditionGroup;
  /** ORDER BY items. */
  orderBy: QbSortItem[];
  /** GROUP BY items. */
  groupBy: QbGroupByItem[];
  /** Whether DISTINCT is applied. */
  distinct: boolean;
  /** Whether the builder panel is open. */
  isOpen: boolean;

  // ── View state ──
  /** Query panel that currently hosts the builder (panel-scoped openness). */
  openPanelId: string | null;
  /** Active bottom tab. */
  bottomTab: QbBottomTab;
  /** Whether the diagram canvas is collapsed to give the bottom region room. */
  canvasCollapsed: boolean;
  /** Canvas snapshot captured when the builder opened (dirty check + cancel rollback). */
  entrySnapshot: QbCanvasSnapshot | null;
  /** Saved builder state per query panel id, restored when that panel re-opens the builder. */
  sessions: Record<string, QbPanelSession>;

  // ── JOIN state ──
  /** Manually confirmed JOINs. */
  joins: QbJoin[];
  /** Auto-detected FK relationship JOINs (dashed lines in UI). */
  autoJoins: QbJoin[];

  // ── Table metadata ──
  /** Table alias mapping (tableName → alias). */
  tableAliases: Record<string, string>;
  /** Canvas position of each table card. */
  tablePositions: Record<string, { x: number; y: number }>;

  // ── Canvas state ──
  /** Canvas zoom level (panning is native scrolling). */
  zoom: number;

  // ── Pagination ──
  /** LIMIT clause value (null = no limit). */
  limit: number | null;
  /** OFFSET clause value (null = no offset). */
  offset: number | null;
}

/** Actions mutating the query builder state. */
export interface QueryBuilderActions {
  toggleTable: (tableName: string) => void;
  /** Remove a table and every artifact that references it (columns, conditions, joins). */
  removeTable: (tableName: string) => void;
  toggleColumn: (table: string, column: string) => void;
  /** Check/uncheck every column of a table card at once. */
  setAllColumns: (table: string, columns: string[], selected: boolean) => void;
  setColumnAlias: (table: string, column: string, alias: string) => void;
  setColumnAggregate: (table: string, column: string, agg: QbAggregate | undefined) => void;
  /** Generic patch for any QbColumnSelection fields (alias, aggregate, sort, groupBy, where). */
  updateColumnConfig: (table: string, column: string, patch: Partial<QbColumnSelection>) => void;
  addCondition: (groupId: string, condition: Omit<QbCondition, 'id'>) => void;
  updateCondition: (id: string, patch: Partial<QbCondition>) => void;
  removeCondition: (id: string) => void;
  addConditionGroup: (parentId: string, logic: 'AND' | 'OR') => void;
  /** Rewrite the AND/OR of an existing group (root or nested). */
  setGroupLogic: (groupId: string, logic: 'AND' | 'OR') => void;
  /** Same five operations, applied to the HAVING tree instead of WHERE. */
  addHavingCondition: (groupId: string, condition: Omit<QbCondition, 'id'>) => void;
  updateHavingCondition: (id: string, patch: Partial<QbCondition>) => void;
  removeHavingCondition: (id: string) => void;
  addHavingGroup: (parentId: string, logic: 'AND' | 'OR') => void;
  setHavingGroupLogic: (groupId: string, logic: 'AND' | 'OR') => void;
  addSort: (item: QbSortItem) => void;
  /** Patch one ORDER BY entry in place (used to flip ASC/DESC from the clause list). */
  updateSort: (index: number, patch: Partial<QbSortItem>) => void;
  removeSort: (index: number) => void;
  addGroupBy: (item: QbGroupByItem) => void;
  removeGroupBy: (index: number) => void;
  setDistinct: (v: boolean) => void;
  /**
   * Legacy visibility toggle. Prefer `openFor` / `closeFor` which are
   * panel-scoped and snapshot-aware.
   */
  toggleOpen: () => void;

  // ── View actions ──
  /** Open the builder for `panelId`, capturing the entry snapshot. */
  openFor: (panelId: string) => void;
  /**
   * Close the builder.
   * - `ok`     → keep the canvas as-is (the generated SQL was committed).
   * - `cancel` → roll the canvas back to the entry snapshot.
   */
  closeFor: (mode: 'ok' | 'cancel') => void;
  /** Hide the builder without committing or discarding (non-destructive 收起). */
  hideFor: () => void;
  /**
   * The owning query panel was destroyed: forget its session and blank the live
   * canvas when it was the one hosting the builder.
   */
  destroyFor: (panelId: string) => void;
  setBottomTab: (tab: QbBottomTab) => void;
  setCanvasCollapsed: (v: boolean) => void;
  toggleCanvasCollapsed: () => void;
  /** True when the live canvas differs from the entry snapshot. */
  hasChanges: () => boolean;

  // ── JOIN actions ──
  addJoin: (join: Omit<QbJoin, 'id'>) => void;
  removeJoin: (id: string) => void;
  updateJoinType: (id: string, type: QbJoinType) => void;
  /** Change the JOIN type of unconfirmed FK candidates (by constraint key). */
  updateAutoJoinType: (constraintKey: string, type: QbJoinType) => void;
  /**
   * Promote an auto-detected FK candidate into the SQL (PRD F-03.2).
   * Idempotent: an equivalent confirmed join is never duplicated.
   */
  confirmAutoJoin: (id: string) => void;
  /** Drop an auto-detected candidate the user is not interested in. */
  dismissAutoJoin: (id: string) => void;
  /**
   * Confirm **every** column pair of one constraint at once.
   *
   * A composite FK confirmed pair-by-pair would emit a JOIN missing half of its
   * predicate, so the group is the smallest safe unit.
   */
  confirmConstraintGroup: (constraint: string) => void;
  /** Remove every join belonging to a constraint. */
  removeConstraintGroup: (constraint: string) => void;

  // ── Table metadata actions ──
  setTableAlias: (tableName: string, alias: string) => void;
  updateTablePosition: (table: string, pos: { x: number; y: number }) => void;

  // ── Canvas actions ──
  /** Set the canvas zoom (panning is native scrolling). */
  setZoom: (zoom: number) => void;

  // ── Pagination actions ──
  setLimit: (limit: number | null) => void;
  setOffset: (offset: number | null) => void;

  reset: () => void;
}

// ── Helpers ───────────────────────────────────────────────────

function uid(): string {
  return crypto.randomUUID();
}

function removeConditionById(group: QbConditionGroup, id: string): QbConditionGroup {
  return {
    ...group,
    conditions: group.conditions.filter((c) => c.id !== id),
    groups: group.groups.map((g) => removeConditionById(g, id)),
  };
}

function updateConditionById(
  group: QbConditionGroup,
  id: string,
  patch: Partial<QbCondition>,
): QbConditionGroup {
  return {
    ...group,
    conditions: group.conditions.map((c) => (c.id === id ? { ...c, ...patch } : c)),
    groups: group.groups.map((g) => updateConditionById(g, id, patch)),
  };
}

function addConditionToGroup(
  group: QbConditionGroup,
  groupId: string,
  condition: QbCondition,
): QbConditionGroup {
  if (group.id === groupId) {
    return { ...group, conditions: [...group.conditions, condition] };
  }
  return {
    ...group,
    groups: group.groups.map((g) => addConditionToGroup(g, groupId, condition)),
  };
}

function addSubGroup(
  group: QbConditionGroup,
  parentId: string,
  newGroup: QbConditionGroup,
): QbConditionGroup {
  if (group.id === parentId) {
    return { ...group, groups: [...group.groups, newGroup] };
  }
  return {
    ...group,
    groups: group.groups.map((g) => addSubGroup(g, parentId, newGroup)),
  };
}

function setGroupLogicById(
  group: QbConditionGroup,
  groupId: string,
  logic: 'AND' | 'OR',
): QbConditionGroup {
  if (group.id === groupId) {
    return {
      ...group,
      logic,
      // The generated expression joins rows with each row's *own* conjunction
      // (see `buildGroupExpr`), so switching the group to OR has to move its
      // rows too — otherwise the control would appear to do nothing.
      conditions: group.conditions.map((cond, index) =>
        index === 0 ? cond : { ...cond, conjunction: logic },
      ),
    };
  }
  return {
    ...group,
    groups: group.groups.map((g) => setGroupLogicById(g, groupId, logic)),
  };
}

/**
 * Remove a table plus everything that references it: selected columns,
 * WHERE conditions (including nested groups), confirmed joins, its alias and
 * its canvas position. Leaving any of these behind produces SQL that mentions
 * a table the user already deleted.
 */
function removeTableFrom(s: QueryBuilderState, tableName: string): Partial<QueryBuilderState> {
  const removed = new Set([tableName]);
  const { [tableName]: _alias, ...tableAliases } = s.tableAliases;
  const { [tableName]: _position, ...tablePositions } = s.tablePositions;
  return {
    selectedTables: s.selectedTables.filter((t) => t !== tableName),
    selectedColumns: s.selectedColumns.filter((c) => c.table !== tableName),
    where: pruneConditions(s.where, removed),
    having: pruneConditions(s.having, removed),
    joins: s.joins.filter((j) => j.leftTable !== tableName && j.rightTable !== tableName),
    autoJoins: s.autoJoins.filter((j) => j.leftTable !== tableName && j.rightTable !== tableName),
    tableAliases,
    tablePositions,
  };
}

/** Extract a restorable session from the live state. */
function captureSession(s: QueryBuilderState): QbPanelSession {
  return {
    canvas: snapshotCanvas(s),
    bottomTab: s.bottomTab,
    canvasCollapsed: s.canvasCollapsed,
  };
}

/** A blank canvas: what a query panel starts with before it owns any state. */
function freshCanvas(): QbCanvasSnapshot {
  return {
    selectedTables: [],
    selectedColumns: [],
    joins: [],
    tableAliases: {},
    tablePositions: {},
    where: emptyConditionGroup(),
    having: emptyConditionGroup(),
    orderBy: [],
    groupBy: [],
    distinct: false,
    limit: null,
    offset: null,
  };
}

/** Drop every condition that references one of `tables` (root groups included). */
function pruneConditions(group: QbConditionGroup, tables: Set<string>): QbConditionGroup {
  return {
    ...group,
    conditions: group.conditions.filter((c) => !tables.has(c.table)),
    groups: group.groups.map((g) => pruneConditions(g, tables)),
  };
}

function emptyConditionGroup(): QbConditionGroup {
  return { id: uid(), logic: 'AND', conditions: [], groups: [] };
}

// ── Initial state ─────────────────────────────────────────────

const INITIAL_STATE: QueryBuilderState = {
  selectedTables: [],
  selectedColumns: [],
  where: emptyConditionGroup(),
  having: emptyConditionGroup(),
  orderBy: [],
  groupBy: [],
  distinct: false,
  isOpen: false,
  openPanelId: null,
  bottomTab: 'build',
  canvasCollapsed: false,
  entrySnapshot: null,
  sessions: {},
  joins: [],
  autoJoins: [],
  tableAliases: {},
  tablePositions: {},
  zoom: 1,
  limit: null,
  offset: null,
};

// ── Store ─────────────────────────────────────────────────────

export const useQueryBuilderStore = create<QueryBuilderState & QueryBuilderActions>()(
  (set, get) => ({
    ...INITIAL_STATE,

    toggleTable: (tableName) =>
      set((s) => {
        if (s.selectedTables.includes(tableName)) {
          // Removal goes through the same cleanup as the card's × button.
          return removeTableFrom(s, tableName);
        }
        // A fresh table gets a default alias, so every generated reference is
        // qualified from the very first column — an alias the user has to add
        // by hand before it has any effect is a trap.
        //
        // It also gets a canvas slot: without a position every added card would
        // fall back to {0,0} and stack on top of the previous ones. The first
        // card lands in the canvas' top-left corner; later ones cascade right.
        const lastTable = s.selectedTables[s.selectedTables.length - 1];
        const lastPos = lastTable ? s.tablePositions[lastTable] : undefined;
        const pos = lastPos
          ? alignDroppedCard({ x: lastPos.x + CARD_STRIDE, y: lastPos.y }, s.tablePositions)
          : { x: 0, y: 0 };
        return {
          selectedTables: [...s.selectedTables, tableName],
          tableAliases: s.tableAliases[tableName]
            ? s.tableAliases
            : { ...s.tableAliases, [tableName]: uniqueTableAlias(tableName, s.tableAliases) },
          tablePositions: { ...s.tablePositions, [tableName]: pos },
        };
      }),

    removeTable: (tableName) => set((s) => removeTableFrom(s, tableName)),

    setAllColumns: (table, columns, selected) =>
      set((s) => {
        const others = s.selectedColumns.filter((c) => c.table !== table);
        if (!selected) return { selectedColumns: others };
        // Keep the columns already selected for this table — that preserves
        // their alias / aggregate / sort / groupBy configuration.
        const existing = s.selectedColumns.filter((c) => c.table === table);
        const existingNames = new Set(existing.map((c) => c.column));
        const added = columns
          .filter((column) => !existingNames.has(column))
          .map((column) => ({ table, column }));
        return { selectedColumns: [...others, ...existing, ...added] };
      }),

    toggleColumn: (table, column) =>
      set((s) => {
        const exists = s.selectedColumns.some((c) => c.table === table && c.column === column);
        if (exists) {
          return {
            selectedColumns: s.selectedColumns.filter(
              (c) => !(c.table === table && c.column === column),
            ),
          };
        }
        return { selectedColumns: [...s.selectedColumns, { table, column }] };
      }),

    setColumnAlias: (table, column, alias) =>
      set((s) => ({
        selectedColumns: s.selectedColumns.map((c) =>
          c.table === table && c.column === column ? { ...c, alias: alias || undefined } : c,
        ),
      })),

    setColumnAggregate: (table, column, agg) =>
      set((s) => ({
        selectedColumns: s.selectedColumns.map((c) =>
          c.table === table && c.column === column ? { ...c, aggregate: agg } : c,
        ),
      })),

    updateColumnConfig: (table, column, patch) =>
      set((s) => ({
        selectedColumns: s.selectedColumns.map((c) => {
          if (c.table !== table || c.column !== column) return c;
          const updated = { ...c, ...patch };
          // Normalise: empty alias → undefined, empty string where → remove
          if (updated.alias === '') updated.alias = undefined;
          return updated;
        }),
      })),

    addCondition: (groupId, condition) =>
      set((s) => ({
        where: addConditionToGroup(s.where, groupId, { ...condition, id: uid() }),
      })),

    updateCondition: (id, patch) =>
      set((s) => ({
        where: updateConditionById(s.where, id, patch),
      })),

    removeCondition: (id) =>
      set((s) => ({
        where: removeConditionById(s.where, id),
      })),

    addConditionGroup: (parentId, logic) =>
      set((s) => ({
        where: addSubGroup(s.where, parentId, {
          id: uid(),
          logic,
          conditions: [],
          groups: [],
        }),
      })),

    setGroupLogic: (groupId, logic) =>
      set((s) => ({
        where: setGroupLogicById(s.where, groupId, logic),
      })),

    addHavingCondition: (groupId, condition) =>
      set((s) => ({
        having: addConditionToGroup(s.having, groupId, { ...condition, id: uid() }),
      })),

    updateHavingCondition: (id, patch) =>
      set((s) => ({
        having: updateConditionById(s.having, id, patch),
      })),

    removeHavingCondition: (id) =>
      set((s) => ({
        having: removeConditionById(s.having, id),
      })),

    addHavingGroup: (parentId, logic) =>
      set((s) => ({
        having: addSubGroup(s.having, parentId, {
          id: uid(),
          logic,
          conditions: [],
          groups: [],
        }),
      })),

    setHavingGroupLogic: (groupId, logic) =>
      set((s) => ({
        having: setGroupLogicById(s.having, groupId, logic),
      })),

    addSort: (item) =>
      set((s) => ({
        orderBy: [...s.orderBy, item],
      })),

    updateSort: (index, patch) =>
      set((s) => ({
        orderBy: s.orderBy.map((item, i) => (i === index ? { ...item, ...patch } : item)),
      })),

    removeSort: (index) =>
      set((s) => ({
        orderBy: s.orderBy.filter((_, i) => i !== index),
      })),

    addGroupBy: (item) =>
      set((s) => ({
        groupBy: [...s.groupBy, item],
      })),

    removeGroupBy: (index) =>
      set((s) => ({
        groupBy: s.groupBy.filter((_, i) => i !== index),
      })),

    setDistinct: (v) => set(() => ({ distinct: v })),

    toggleOpen: () =>
      // Legacy visibility toggle (kept for tests / older callers). The UI uses
      // `openFor` / `closeFor`, which are panel-scoped.
      set((s) => {
        if (s.isOpen) {
          const canvas = snapshotCanvas(s);
          const sessions = { ...s.sessions };
          if (s.openPanelId) {
            sessions[s.openPanelId] = {
              canvas,
              bottomTab: s.bottomTab,
              canvasCollapsed: s.canvasCollapsed,
            };
          }
          return { sessions, isOpen: false, openPanelId: null, entrySnapshot: null };
        }
        return { isOpen: true, entrySnapshot: snapshotCanvas(s) };
      }),

    // ── View actions ──

    openFor: (panelId) =>
      set((s) => {
        // Already hosting for this panel — keep the existing snapshot so a
        // re-render does not reset the dirty baseline.
        if (s.openPanelId === panelId) return { isOpen: true };

        // Stash the outgoing panel's builder, then restore the target's own
        // session. The builder belongs to a query panel: two panels must never
        // share one canvas, and a panel that never opened it starts blank.
        const sessions = { ...s.sessions };
        if (s.openPanelId) sessions[s.openPanelId] = captureSession(s);

        const restored = s.sessions[panelId];
        const canvas = restored?.canvas ?? freshCanvas();
        return {
          ...canvas,
          bottomTab: restored?.bottomTab ?? 'build',
          canvasCollapsed: restored?.canvasCollapsed ?? false,
          sessions,
          isOpen: true,
          openPanelId: panelId,
          entrySnapshot: canvas,
        };
      }),

    closeFor: (mode) =>
      set((s) => {
        const canvas = mode === 'cancel' && s.entrySnapshot ? s.entrySnapshot : snapshotCanvas(s);
        const sessions = { ...s.sessions };
        // Keep the session so re-opening the *same* panel restores its canvas.
        if (s.openPanelId) {
          sessions[s.openPanelId] = {
            canvas,
            bottomTab: s.bottomTab,
            canvasCollapsed: s.canvasCollapsed,
          };
        }
        return {
          ...canvas,
          sessions,
          isOpen: false,
          openPanelId: null,
          entrySnapshot: null,
        };
      }),

    hideFor: () =>
      set((s) => {
        const canvas = snapshotCanvas(s);
        const sessions = { ...s.sessions };
        if (s.openPanelId) {
          sessions[s.openPanelId] = {
            canvas,
            bottomTab: s.bottomTab,
            canvasCollapsed: s.canvasCollapsed,
          };
        }
        return { sessions, isOpen: false, openPanelId: null, entrySnapshot: null };
      }),

    destroyFor: (panelId) =>
      set((s) => {
        const sessions = { ...s.sessions };
        delete sessions[panelId];
        // A destroyed panel must not leave its builder behind: the next panel
        // to open the builder has to start from a blank canvas.
        if (s.openPanelId !== panelId) return { sessions };
        return {
          ...freshCanvas(),
          sessions,
          bottomTab: 'build' as const,
          canvasCollapsed: false,
          isOpen: false,
          openPanelId: null,
          entrySnapshot: null,
        };
      }),

    setBottomTab: (tab) => set(() => ({ bottomTab: tab })),

    setCanvasCollapsed: (v) => set(() => ({ canvasCollapsed: v })),

    toggleCanvasCollapsed: () => set((s) => ({ canvasCollapsed: !s.canvasCollapsed })),

    hasChanges: () => canvasChangedSince(get()),

    // ── JOIN actions ──

    addJoin: (join) =>
      set((s) => ({
        joins: [...s.joins, { ...join, id: uid() }],
      })),

    removeJoin: (id) =>
      set((s) => ({
        joins: s.joins.filter((j) => j.id !== id),
      })),

    updateJoinType: (id, type) =>
      set((s) => ({
        joins: s.joins.map((j) => (j.id === id ? { ...j, type } : j)),
      })),

    updateAutoJoinType: (constraintKey, type) =>
      set((s) => ({
        autoJoins: s.autoJoins.map((j) => (j.constraint === constraintKey ? { ...j, type } : j)),
      })),

    confirmAutoJoin: (id) =>
      set((s) => {
        const candidate = s.autoJoins.find((j) => j.id === id);
        if (!candidate) return {};
        const alreadyJoined = s.joins.some(
          (j) =>
            j.leftTable === candidate.leftTable &&
            j.leftColumn === candidate.leftColumn &&
            j.rightTable === candidate.rightTable &&
            j.rightColumn === candidate.rightColumn,
        );
        if (alreadyJoined) return { autoJoins: s.autoJoins.filter((j) => j.id !== id) };
        return {
          joins: [...s.joins, { ...candidate, id: uid(), isManual: false }],
          autoJoins: s.autoJoins.filter((j) => j.id !== id),
        };
      }),

    dismissAutoJoin: (id) =>
      set((s) => ({
        autoJoins: s.autoJoins.filter((j) => j.id !== id),
      })),

    confirmConstraintGroup: (constraint) =>
      set((s) => {
        const candidates = s.autoJoins.filter((j) => j.constraint === constraint);
        if (candidates.length === 0) return {};
        const pairId = (j: {
          leftTable: string;
          leftColumn: string;
          rightTable: string;
          rightColumn: string;
        }) => `${j.leftTable}.${j.leftColumn}->${j.rightTable}.${j.rightColumn}`;
        const already = new Set(s.joins.map(pairId));
        const added = candidates
          .filter((candidate) => !already.has(pairId(candidate)))
          .map((candidate) => ({ ...candidate, id: uid(), isManual: false as const }));
        return {
          joins: [...s.joins, ...added],
          autoJoins: s.autoJoins.filter((j) => j.constraint !== constraint),
        };
      }),

    removeConstraintGroup: (constraint) =>
      set((s) => ({
        joins: s.joins.filter((j) => j.constraint !== constraint),
      })),

    // ── Table metadata actions ──

    setTableAlias: (tableName, alias) =>
      set((s) => ({
        tableAliases: { ...s.tableAliases, [tableName]: alias },
      })),

    updateTablePosition: (table, pos) =>
      set((s) => ({
        tablePositions: { ...s.tablePositions, [table]: pos },
      })),

    // ── Canvas actions ──

    setZoom: (zoom) => set(() => ({ zoom })),

    // ── Pagination actions ──

    setLimit: (limit) => set(() => ({ limit })),

    setOffset: (offset) => set(() => ({ offset })),

    reset: () =>
      set((s) => ({
        ...INITIAL_STATE,
        // View state survives a canvas reset.
        isOpen: s.isOpen,
        openPanelId: s.openPanelId,
        bottomTab: s.bottomTab,
        canvasCollapsed: s.canvasCollapsed,
        entrySnapshot: s.entrySnapshot,
        where: emptyConditionGroup(),
      })),
  }),
);

// Expose store for E2E tests (WebKit DragEvent doesn't fire React synthetic handlers)
if (typeof window !== 'undefined') {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (window as any).__qbStore = useQueryBuilderStore;
}
