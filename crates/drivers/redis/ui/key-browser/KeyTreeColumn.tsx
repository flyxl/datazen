/**
 * Left column of the Redis key browser: the list of loaded keys.
 *
 * Two mutually exclusive sources render into one column (PRD §3.2):
 * - `searchMode === 'key'` → the hierarchical key tree fed by server
 *   `list_children` levels, with the Load-more footer inside
 *   {@link KeyTreeList}.
 * - any other mode → the guarded value-search hit list.
 *
 * The column is presentational: scan state, selection sets and dialogs stay
 * with `RedisWorkbench`. Carving the column out here is what keeps the
 * workbench a state owner instead of a JSX wall (PRD §7-5 "先拆再改").
 */
import { type MouseEvent as ReactMouseEvent } from 'react';
import type { KeyTreeRow } from './keyTree';
import { KeyTreeList, type KeyTreeDeleteTarget } from './KeyTreeList';
import { ValueSearchResults } from '../value-search/ValueSearchResults';
import type { SearchMode } from './SearchModeTabs';
import type { TreeEmptyState } from './treeEmptyState';
import type { ValueSearchState } from '../value-search/useValueSearch';

export interface KeyTreeColumnProps {
  searchMode: SearchMode;
  /* ── key tree ─────────────────────────────────────────────────────────── */
  treeRows: KeyTreeRow[];
  /** All loaded key names — folder checkbox cascade in {@link KeyTreeList}. */
  allKeys: string[];
  expandedFolders: Set<string>;
  onToggleFolder: (path: string) => void;
  selectedKey: string | null;
  selectedKeys: Set<string>;
  /** Single entry point for "the user picked a key", in either mode. */
  onSelectKey: (key: string) => void;
  onToggleKey: (key: string, checked: boolean) => void;
  onToggleKeys: (keys: string[], checked: boolean) => void;
  onKeyContextMenu: (e: ReactMouseEvent, key: string) => void;
  onDeleteRow: (target: KeyTreeDeleteTarget) => void;
  loading: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
  /** R3 separator (D-3) + I-11 named empty state (D-8). */
  separator: string;
  emptyState: TreeEmptyState | null;
  pattern: string;
  /**
   * A pattern is applied (D-2 / BUG-001): a folder whose scan is still open has
   * to say that part of its `(n+)` remainder was never filtered.
   */
  filterActive: boolean;
  /** I-9 chords that are not row-local: `⌘A`, `⌘R`, `Esc`. */
  onSelectAllLoaded: () => void;
  onRefresh: () => void;
  onClearSelection: () => void;
  /* ── value search ─────────────────────────────────────────────────────── */
  valueSearchState: ValueSearchState;
  onCancelValueSearch: () => void;
}

export function KeyTreeColumn({
  searchMode,
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
  valueSearchState,
  onCancelValueSearch,
}: KeyTreeColumnProps) {
  if (searchMode !== 'key') {
    return (
      <ValueSearchResults
        state={valueSearchState}
        onSelectKey={onSelectKey}
        onCancel={onCancelValueSearch}
      />
    );
  }

  return (
    <KeyTreeList
      treeRows={treeRows}
      allKeys={allKeys}
      expandedFolders={expandedFolders}
      onToggleFolder={onToggleFolder}
      selectedKey={selectedKey}
      selectedKeys={selectedKeys}
      onSelectKey={onSelectKey}
      onToggleKey={onToggleKey}
      onToggleKeys={onToggleKeys}
      onKeyContextMenu={onKeyContextMenu}
      onDeleteRow={onDeleteRow}
      loading={loading}
      hasMore={hasMore}
      onLoadMore={onLoadMore}
      separator={separator}
      emptyState={emptyState}
      pattern={pattern}
      filterActive={filterActive}
      onSelectAllLoaded={onSelectAllLoaded}
      onRefresh={onRefresh}
      onClearSelection={onClearSelection}
    />
  );
}
