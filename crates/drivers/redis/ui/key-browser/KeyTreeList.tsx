import { useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from 'react';
import {
  ChevronDown,
  ChevronRight,
  FileKey2,
  Folder,
  FolderOpen,
  Search,
  ShieldAlert,
  Trash2,
  WifiOff,
} from 'lucide-react';
import { VirtualTree, ariaLevelOf, cn, useI18n, Spinner } from '@datazen/ui';
import type { VirtualTreeItemAria, VirtualTreeOverlayContext } from '@datazen/ui';
import type { KeyTreeRow } from './keyTree';
import { keyUnderFolder } from './keyTree';
import { countSelectableRows, isBreadcrumbRow } from './keyTreeFilter';
import { TREE_EMPTY_STATE_KEYS, type TreeEmptyState } from './treeEmptyState';
import { ROW_HEIGHT, isFolderRow, keyTreeRowKey, rowIndent, treeNavAction } from './treeRowSpec';

/**
 * ARIA for one row, and the one place the key browser announces hierarchy.
 *
 * Three outcomes, and the third is the interesting one:
 * - a filter breadcrumb is **decoration** (`null`): it is painted so a surviving
 *   deeper row keeps its path, but it is not a row of the tree, so it gets no
 *   `treeitem` role and stays `aria-hidden` exactly as before;
 * - a folder is a branch and therefore says `aria-expanded`;
 * - a key is a leaf and therefore **does not** — announcing expansion on a row
 *   that can never expand is a lie to a screen reader, and the host navigator
 *   already made the same call for its `table` / `object` rows.
 *
 * The level comes from the shared `ariaLevelOf`, never from `depth + 1` typed
 * here, so the key browser and the connection navigator cannot drift apart on
 * what a level means. Indentation stays its own business: `rowIndent(depth)`
 * paints the absolute hierarchy, and the two are deliberately decoupled.
 */
function keyTreeRowAria(row: KeyTreeRow, expandedFolders: Set<string>): VirtualTreeItemAria | null {
  if (isBreadcrumbRow(row)) return null;
  const level = ariaLevelOf(row);
  if (row.kind !== 'folder') return { role: 'treeitem', 'aria-level': level };
  return { role: 'treeitem', 'aria-level': level, 'aria-expanded': expandedFolders.has(row.path) };
}

/**
 * Delete target for a hover action on a tree row: a single leaf key, or a
 * folder prefix (delete every key under its subtree).
 */
export type KeyTreeDeleteTarget =
  | { kind: 'key'; key: string }
  | { kind: 'folder'; prefix: string; label: string; count: number };

/** Outlined pill colors per Redis type, keyed by the raw `keyType` string. */
const TYPE_BADGE: Record<string, string> = {
  string: 'border-success/40 text-success',
  hash: 'border-accent/40 text-accent',
  list: 'border-warning/40 text-warning',
  set: 'border-fg-secondary/40 text-fg-secondary',
  zset: 'border-danger/40 text-danger',
  stream: 'border-fg-muted/40 text-fg-muted',
};

/** Icon per named empty state (I-11) — the copy itself always comes from i18n. */
const EMPTY_ICON: Record<TreeEmptyState, typeof Search> = {
  none: Folder,
  'no-match': Search,
  interrupted: WifiOff,
  'no-permission': ShieldAlert,
};

export interface KeyTreeListProps {
  treeRows: KeyTreeRow[];
  /**
   * The keys **visible** under the applied pattern (BUG-001 single source, owned
   * by `useKeyTreeView`): the prefix cascade on folder checkboxes and R1's
   * `{loaded}` both read this, so no surface can select or count a key the
   * pattern rejected.
   */
  allKeys: string[];
  expandedFolders: Set<string>;
  onToggleFolder: (path: string) => void;
  selectedKey: string | null;
  selectedKeys: Set<string>;
  onSelectKey: (key: string) => void;
  onToggleKey: (key: string, checked: boolean) => void;
  onToggleKeys: (keys: string[], checked: boolean) => void;
  onKeyContextMenu: (e: ReactMouseEvent, key: string) => void;
  onDeleteRow: (target: KeyTreeDeleteTarget) => void;
  loading: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
  /** R3 separator — decides which folder/leaf boundary counts as a match. */
  separator: string;
  /** I-11: named empty state to render, or `null` when the list is not empty. */
  emptyState: TreeEmptyState | null;
  /** Pattern quoted back by the `no-match` state. */
  pattern: string;
  /**
   * A pattern is *applied* (not blank / `*`), so a folder whose level scan is
   * still open must admit that part of its `(n+)` remainder is unfiltered
   * (D-2 / BUG-001; the wording is `redis.tree.filterUnloaded`).
   */
  filterActive: boolean;
  /** I-9 chords the tree owns: select-all-loaded / refresh / leave selection. */
  onSelectAllLoaded: () => void;
  onRefresh: () => void;
  onClearSelection: () => void;
}

/**
 * Compact hierarchical key tree (server-driven `list_children` levels):
 * one 30 px row per folder/leaf with a leading checkbox, folder icon + child
 * count, inline type / TTL badges, and a hover delete action (folder rows
 * delete their whole subtree).
 *
 * Row geometry and the pinned folder stack come from `treeRowSpec` (D-4), and so
 * does the keyboard map (D-7 / I-9): the tree is focusable, tracks one
 * `activeIndex`, and clamps at both ends rather than wrapping.
 */
export function KeyTreeList({
  treeRows,
  allKeys,
  expandedFolders,
  onToggleFolder,
  selectedKey,
  selectedKeys,
  onSelectKey,
  onToggleKey,
  onToggleKeys,
  onKeyContextMenu,
  onDeleteRow,
  loading,
  hasMore,
  onLoadMore,
  separator,
  emptyState,
  pattern,
  filterActive,
  onSelectAllLoaded,
  onRefresh,
  onClearSelection,
}: KeyTreeListProps) {
  const { t } = useI18n();
  const scrollRef = useRef<HTMLDivElement>(null);
  /*
   * `paintedRows` is the grid the virtualizer and the sticky stack lay out over;
   * `rowCount` is what `data-row-count`, R1's counter and every selection surface
   * agree on (BUG-001 single source). They differ exactly by the breadcrumb rows
   * the pattern filter back-fills as path context: painted, but not rows — not
   * clickable, not checkable, not navigable (see `keyTreeFilter.ts`).
   */
  const rowCount = countSelectableRows(treeRows);
  const [activeIndex, setActiveIndex] = useState(-1);

  /** Rows a keyboard user can land on: everything a breadcrumb is not. */
  const isNavigable = (index: number): boolean => {
    const row = treeRows[index];
    return !!row && !isBreadcrumbRow(row);
  };

  // Folder checkboxes select every key under the prefix from the *visible* key
  // set (D-2 / BUG-001), so collapsed (unexpanded) folders are selectable too and
  // a filter can never select a key the pattern rejected.
  const folderSelection = useMemo(() => {
    const map = new Map<string, { keys: string[]; all: boolean }>();
    for (const row of treeRows) {
      if (row.kind !== 'folder' || isBreadcrumbRow(row)) continue;
      const keys = allKeys.filter((k) => keyUnderFolder(k, row.path, separator));
      const all = keys.length > 0 && keys.every((k) => selectedKeys.has(k));
      map.set(row.path, { keys, all });
    }
    return map;
  }, [treeRows, allKeys, selectedKeys, separator]);

  /**
   * `(n)` for a complete level, `(n+)` while its scan is still open (I-4). Under an
   * applied pattern the `+` also carries the unloaded-tail hint: a level whose scan
   * has not finished holds keys the client-side filter never saw, so the visible
   * subset cannot be presented as the whole remainder (D-2 known limitation /
   * redis-tree-ui-BUG-001). Only `level.partial` (⇒ `level.done === false`) adds
   * it — a drained level is fully filtered and needs no caveat.
   */
  const countLabel = (count: number, partial: boolean | undefined) => {
    if (!partial) return String(count);
    const shown = t('redis.tree.folderPartial').replace('{count}', String(count));
    return filterActive ? `${shown} ${t('redis.tree.filterUnloaded')}` : shown;
  };

  /*
   * I-9 (D-7) state machine, wired into the shared shell instead of hand-rolled
   * here. Entry: the tree is focusable, so a key press has one owner. State: one
   * `activeIndex`, clamped at both ends — ↑ on the first row and ↓ on the last
   * stay put instead of flinging the viewport to the other end of a 10k-key
   * tree. Exit: `←` on a root row and `Esc` both leave selection mode (`Esc`
   * clears the checks, the visible half of the exit).
   *
   * What this object *is* is the key browser's whole I-9 policy expressed in the
   * shell's vocabulary; what it is not is the mechanism. Clamping the stale
   * index, stepping, skipping breadcrumbs, the expand-or-descend and
   * fold-or-ascend rules, and "a step off the end is not a move" are all decided
   * by `planTreeNavigation`, which the host navigator uses too — so a fix to
   * redis-tree-ui-BUG-004 (a step that lands on a breadcrumb) is now a fix to
   * one function instead of two parallel hand-written copies.
   *
   * Note the breadcrumb rule survives verbatim: navigation runs over the painted
   * grid but only ever *lands* on a real row, so a pattern breadcrumb is path
   * context that ↑/↓ pass through and that no intent can select, toggle or
   * delete.
   */
  const navigation = {
    mapKey: treeNavAction,
    activeIndex,
    onActiveIndexChange: setActiveIndex,
    isNavigable,
    isExpanded: (index: number) => {
      const row = treeRows[index];
      return row?.kind === 'folder' && expandedFolders.has(row.path);
    },
    onToggle: (row: KeyTreeRow) => {
      if (row.kind === 'folder') onToggleFolder(row.path);
    },
    onActivate: (row: KeyTreeRow) => {
      if (row.kind === 'key') onSelectKey(row.entry.key);
      else onToggleFolder(row.path);
    },
    onCommand: (action: 'select-all' | 'refresh' | 'clear') => {
      if (action === 'select-all') onSelectAllLoaded();
      else if (action === 'refresh') onRefresh();
      else onClearSelection();
    },
  };

  /** The pinned folder stack (D-4), rebuilt whenever the shell's scroll offset moves. */
  const renderStickyHeaders = ({ stickyIndexes, scrollTop }: VirtualTreeOverlayContext) => {
    if (stickyIndexes.length === 0) return null;
    return (
      <div
        /*
         * Pinned to the viewport top. The shell hands the offset back because it
         * is the component that tracks it — pinning is arithmetic against the
         * same `scrollTop` the virtual window is built from, so the stack and
         * the rows can never disagree about which ancestors are current.
         */
        className="absolute left-0 z-10 w-full bg-surface"
        style={{ top: scrollTop, height: stickyIndexes.length * ROW_HEIGHT }}
        data-testid="redis-tree-sticky-headers"
      >
        {stickyIndexes.map((index, i) => {
          const row = treeRows[index]!;
          if (row.kind !== 'folder') return null;
          return (
            <div
              key={`sticky:${row.path}`}
              role="button"
              tabIndex={-1}
              className={cn(
                'absolute left-0 flex w-full items-center gap-1.5 border-b border-edge bg-surface pr-3',
                // A filter breadcrumb stays decoration even while pinned.
                isBreadcrumbRow(row) ? 'opacity-70' : 'cursor-pointer hover:bg-accent/5',
              )}
              style={{ top: i * ROW_HEIGHT, height: ROW_HEIGHT, paddingLeft: rowIndent(row.depth) }}
              onClick={isBreadcrumbRow(row) ? undefined : () => onToggleFolder(row.path)}
              data-testid={`redis-tree-sticky-folder-${row.path}`}
              data-sticky-depth={row.depth}
              data-sticky-order={i}
              data-partial={row.partial === true ? 'true' : 'false'}
              data-breadcrumb={isBreadcrumbRow(row) ? 'true' : 'false'}
            >
              <ChevronDown className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
              <FolderOpen className="h-4 w-4 shrink-0 text-warning" />
              <span className="truncate font-mono text-xs font-medium text-fg">{row.label}</span>
              <span className="shrink-0 text-[11px] text-fg-muted">
                ({countLabel(row.count, row.partial)})
              </span>
            </div>
          );
        })}
      </div>
    );
  };

  /** The scan-continuation row. Outside `role="tree"`: it is not a key. */
  const renderLoadMore = hasMore ? (
    <div className="flex w-full items-center justify-center border-b border-edge">
      <button
        type="button"
        className="text-xs text-accent hover:underline"
        onClick={onLoadMore}
        disabled={loading}
        data-testid="redis-tree-load-more"
        data-loading={loading ? 'true' : 'false'}
      >
        {loading ? <Spinner size="md" className="inline" /> : t('redis.loadMore')}
      </button>
    </div>
  ) : null;

  return (
    <VirtualTree
      rows={treeRows}
      getKey={keyTreeRowKey}
      getRowAria={(row) => keyTreeRowAria(row, expandedFolders)}
      isBranch={isFolderRow}
      navigation={navigation}
      rowHeight={ROW_HEIGHT}
      overscan={20}
      scrollRef={scrollRef}
      testId="redis-key-tree"
      className="min-h-0 flex-1 overflow-auto outline-none focus-visible:ring-1 focus-visible:ring-accent"
      // The shell added `role="tree"`, and a tree without an accessible name is
      // announced as an unlabelled tree — two of them on a page are then
      // indistinguishable. This row list is the only `role="tree"` in the
      // driver, so it owns the name outright.
      ariaLabel={t('redis.keyBrowser.tree')}
      // I-9: the tree is a focusable list surface, so ⌘A / ⌘R / arrows have one
      // owner instead of racing the browser default.
      containerProps={({ stickyIndexes }) => ({
        tabIndex: 0,
        'data-row-count': rowCount,
        // Live, because the shell re-renders while the pinned layer needs it.
        'data-sticky-depth': stickyIndexes.length,
        'data-active-index': activeIndex,
        'data-separator': separator,
        // D-2 / BUG-001: is a pattern in force? Published as data so tests never
        // read copy to find out (and so the `(n+)` caveat has one owner).
        'data-filter-active': filterActive ? 'true' : 'false',
        'data-filter-pattern': filterActive ? pattern : '',
      })}
      renderOverlay={renderStickyHeaders}
      trailing={renderLoadMore}
      renderRow={({ row, index, itemProps }) => {
        const indent = rowIndent(row.depth);

        if (row.kind === 'folder') {
          /*
           * Breadcrumb arm (D-2 / BUG-001): the folder does not match the
           * pattern itself — it is painted only so a surviving deeper row keeps
           * its path. It therefore has no checkbox, no delete, no expansion
           * click, and the renderer marks it (`data-breadcrumb`) so tests never
           * have to guess from styling.
           */
          const breadcrumb = isBreadcrumbRow(row);
          const open = expandedFolders.has(row.path);
          const sel = folderSelection.get(row.path);
          const descendants = sel?.keys ?? [];
          const allChecked = sel?.all ?? false;
          if (breadcrumb) {
            return (
              <div
                {...itemProps}
                aria-hidden="true"
                className={cn(
                  'flex w-full items-center gap-1.5 border-b border-edge pr-3 opacity-70',
                  index % 2 === 0 ? 'bg-surface' : 'bg-surface-raised/40',
                )}
                style={{ height: ROW_HEIGHT, paddingLeft: indent }}
                data-testid={`redis-tree-folder-${row.path}`}
                data-row-index={index}
                data-row-kind="folder"
                data-row-path={row.path}
                data-depth={row.depth}
                data-indent={indent}
                data-expanded={open ? 'true' : 'false'}
                data-active="false"
                data-breadcrumb="true"
              >
                {open ? (
                  <ChevronDown className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
                ) : (
                  <ChevronRight className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
                )}
                <Folder className="h-4 w-4 shrink-0 text-warning" />
                <span className="truncate font-mono text-xs font-medium text-fg-muted">
                  {row.label}
                </span>
              </div>
            );
          }
          return (
            <div
              {...itemProps}
              className={cn(
                'group flex w-full items-center gap-1.5 border-b border-edge pr-3',
                index % 2 === 0 ? 'bg-surface' : 'bg-surface-raised/40',
                'hover:bg-accent/5',
                index === activeIndex && 'bg-accent/10 ring-1 ring-inset ring-accent',
              )}
              style={{ height: ROW_HEIGHT, paddingLeft: indent }}
              onClick={() => onToggleFolder(row.path)}
              data-testid={`redis-tree-folder-${row.path}`}
              data-row-index={index}
              data-row-kind="folder"
              data-row-path={row.path}
              data-depth={row.depth}
              data-indent={indent}
              data-expanded={open ? 'true' : 'false'}
              data-active={index === activeIndex ? 'true' : 'false'}
              data-breadcrumb="false"
            >
              <span
                className="flex shrink-0 items-center justify-center"
                style={{ width: 16 }}
                onClick={(e) => e.stopPropagation()}
              >
                <input
                  type="checkbox"
                  checked={allChecked}
                  onChange={(e) => onToggleKeys(descendants, e.target.checked)}
                  aria-label={row.label}
                  data-testid={`redis-tree-folder-check-${row.path}`}
                />
              </span>
              {open ? (
                <ChevronDown className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
              ) : (
                <ChevronRight className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
              )}
              {open ? (
                <FolderOpen className="h-4 w-4 shrink-0 text-warning" />
              ) : (
                <Folder className="h-4 w-4 shrink-0 text-warning" />
              )}
              <span className="truncate font-mono text-xs font-medium text-fg">{row.label}</span>
              <span
                className="shrink-0 text-[11px] text-fg-muted"
                data-testid={`redis-tree-folder-count-${row.path}`}
                data-count={row.count}
                data-partial={row.partial === true ? 'true' : 'false'}
              >
                ({countLabel(row.count, row.partial)})
              </span>
              <button
                type="button"
                className="ml-auto shrink-0 rounded p-1 text-fg-muted opacity-0 hover:bg-danger/10 hover:text-danger group-hover:opacity-100"
                title={t('redis.delete')}
                aria-label={t('redis.delete')}
                data-testid={`redis-tree-folder-delete-${row.path}`}
                onClick={(e) => {
                  e.stopPropagation();
                  onDeleteRow({
                    kind: 'folder',
                    prefix: row.path,
                    label: row.label,
                    count: row.count,
                  });
                }}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </div>
          );
        }

        const { entry } = row;
        const isSelected = selectedKey === entry.key;
        const isChecked = selectedKeys.has(entry.key);
        return (
          <div
            {...itemProps}
            className={cn(
              'group flex w-full items-center gap-1.5 border-b border-edge pr-3',
              isSelected ? 'bg-accent/10' : index % 2 === 0 ? 'bg-surface' : 'bg-surface-raised/40',
              'hover:bg-accent/5',
              index === activeIndex && 'bg-accent/10 ring-1 ring-inset ring-accent',
            )}
            style={{ height: ROW_HEIGHT, paddingLeft: indent }}
            onClick={() => onSelectKey(entry.key)}
            onContextMenu={(e) => onKeyContextMenu(e, entry.key)}
            data-testid={`redis-key-row-${entry.key}`}
            data-row-index={index}
            data-row-kind="key"
            data-row-path={entry.key}
            data-depth={row.depth}
            data-indent={indent}
            data-selected={isSelected ? 'true' : 'false'}
            data-active={index === activeIndex ? 'true' : 'false'}
          >
            {/*
                Leaf affordance swap (D-4): a key row shows its key icon and only
                reveals the checkbox on hover — unless it *is* checked, in which
                case the checkbox stays so the selection set never hides itself.
                Folders keep a resident checkbox because their subtree state is
                meaningful without hovering.
              */}
            <span
              className="flex shrink-0 items-center justify-center"
              style={{ width: 16 }}
              onClick={(e) => e.stopPropagation()}
            >
              <FileKey2
                className={cn(
                  'h-3.5 w-3.5 shrink-0 text-fg-muted',
                  isChecked ? 'hidden' : 'block group-hover:hidden',
                )}
                data-testid={`redis-tree-key-icon-${entry.key}`}
                aria-hidden="true"
              />
              <input
                type="checkbox"
                checked={isChecked}
                onChange={(e) => onToggleKey(entry.key, e.target.checked)}
                aria-label={entry.key}
                className={isChecked ? 'block' : 'hidden group-hover:block'}
                data-testid={`redis-tree-key-check-${entry.key}`}
                data-key={entry.key}
                data-checked={isChecked ? 'true' : 'false'}
              />
            </span>
            <span className="min-w-0 flex-1 truncate font-mono text-xs text-fg-secondary">
              {row.label}
            </span>
            <span
              className={cn(
                'shrink-0 rounded-full border px-1.5 py-px text-[10px] font-medium leading-tight',
                TYPE_BADGE[entry.keyType] ?? 'border-edge text-fg-muted',
              )}
            >
              {entry.keyType}
            </span>
            <span className="shrink-0 rounded-full border border-warning/40 px-1.5 py-px text-[10px] font-medium leading-tight text-warning">
              {entry.ttl < 0 ? t('redis.noExpiry') : `${entry.ttl}${t('redis.seconds')}`}
            </span>
            <button
              type="button"
              className="ml-auto shrink-0 rounded p-1 text-fg-muted opacity-0 hover:bg-danger/10 hover:text-danger group-hover:opacity-100"
              title={t('redis.delete')}
              aria-label={t('redis.delete')}
              data-testid={`redis-tree-key-delete-${entry.key}`}
              onClick={(e) => {
                e.stopPropagation();
                onDeleteRow({ kind: 'key', key: entry.key });
              }}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          </div>
        );
      }}
      afterRows={
        <>
          {loading && rowCount === 0 && (
            <div className="flex items-center justify-center gap-2 py-8 text-xs text-fg-muted">
              <Spinner size="lg" />
              {t('common.loading')}
            </div>
          )}

          {/*
        I-11 (D-8): "no rows" is four different facts, each named by its own
        i18n key — never a shared blank. `resolveTreeEmptyState` picks which one,
        and the state is also published as a data attribute so the assertion does
        not depend on English copy.
      */}
          {emptyState && !loading && (
            <div
              className="flex flex-col items-center justify-center gap-1.5 px-4 py-10 text-center"
              data-testid="redis-tree-empty"
              data-empty-state={emptyState}
            >
              {(() => {
                const Icon = EMPTY_ICON[emptyState];
                return <Icon className="h-6 w-6 text-fg-muted opacity-40" aria-hidden="true" />;
              })()}
              <div className="text-xs text-fg-muted" data-testid="redis-tree-empty-label">
                {t(TREE_EMPTY_STATE_KEYS[emptyState])
                  .replace('{pattern}', pattern)
                  .replace('{loaded}', String(allKeys.length))}
              </div>
            </div>
          )}
        </>
      }
    />
  );
}
