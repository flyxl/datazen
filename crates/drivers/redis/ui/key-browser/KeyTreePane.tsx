/**
 * The whole left column of 屏 B: the two-row column header (R1 actions / R2
 * search) above the key list.
 *
 * Extracted from `RedisWorkbench` (D-3) because from here on the header grows a
 * row per PRD §3.2 requirement, and the composition root has a hard size budget.
 * This pane is *presentation only*: every object it reads is owned elsewhere
 * (`useKeyTreeView` for the view + rows, `useRedisKeyScan` for the flat list,
 * `useWorkbenchSearch` for the scope, `useKeySelection` for the checks,
 * `useKeyDetailState` for the mounted key, `useKeyRowActions` for the writes), so
 * no state and no I/O moved here — only the wiring of one to the other.
 */
import { KeyTreeColumn } from './KeyTreeColumn';
import { KeyTreeHeader } from './KeyTreeHeader';
import { KeyTreeSearchRow } from './KeyTreeSearchRow';
import type { KeySelection } from './useKeySelection';
import type { KeyTreeView } from './useKeyTreeView';
import type { KeyScanApi } from './useRedisKeyScan';
import type { WorkbenchSearch } from './useWorkbenchSearch';
import type { KeyDetailState } from './useKeyDetailState';
import type { KeyTreeDeleteTarget } from './KeyTreeList';
import type { MouseEvent as ReactMouseEvent } from 'react';

export interface KeyTreePaneProps {
  view: KeyTreeView;
  scan: KeyScanApi;
  search: WorkbenchSearch;
  selection: KeySelection;
  detail: KeyDetailState;
  onKeyContextMenu: (e: ReactMouseEvent, key: string) => void;
  onDeleteRow: (target: KeyTreeDeleteTarget) => void;
  /** R1's select-all slot flips to delete over the ticked keys. */
  onDeleteSelected: (keys: string[]) => void;
  /** `DBSIZE` — the denominator of R1's `已加载 N / 共 M`. */
  totalCount: number;
  /** R2's fuzzy chip: literal → `*literal*` on apply. */
  fuzzy: boolean;
  onFuzzyChange: (fuzzy: boolean) => void;
  onCreateKey: () => void;
  /** ⌘R / header refresh (also the I-9 chord target inside the tree). */
  onRefresh: () => void;
}

export function KeyTreePane({
  view,
  scan,
  search,
  selection,
  detail,
  onKeyContextMenu,
  onDeleteRow,
  onDeleteSelected,
  totalCount,
  fuzzy,
  onFuzzyChange,
  onCreateKey,
  onRefresh,
}: KeyTreePaneProps) {
  const { tree: treeState, treeRows, visibleKeys, emptyState, separator } = view;
  /*
   * BUG-001 single source: the counter in R1, 「全选已加载」 and the folder
   * checkbox cascade all read `view.visibleKeys` — the applied pattern's
   * surviving key set — instead of `scan.keys`. They used to read the unfiltered
   * flat list while the column painted unfiltered rows, which is how the tree
   * could show 2 rows, report "0 loaded" and offer a select-all over nothing.
   */

  const selectAllLoaded = () => selection.selectMany(visibleKeys);

  return (
    <>
      <KeyTreeHeader
        searchMode={search.searchMode}
        onSearchModeChange={search.setSearchMode}
        loadedCount={visibleKeys.length}
        totalCount={totalCount}
        scanning={scan.cursor !== 0}
        onSelectAll={selectAllLoaded}
        selectionCount={selection.selectedKeys.size}
        onDeleteSelected={() => onDeleteSelected([...selection.selectedKeys])}
        onRefresh={onRefresh}
        onCreateKey={onCreateKey}
      >
        <KeyTreeSearchRow
          scope={search.searchMode}
          pattern={scan.searchPattern}
          onPatternChange={scan.setSearchPattern}
          onApply={() => search.applySearch(scan.searchPattern, fuzzy)}
          onClearFilter={() => {
            scan.setSearchPattern('');
            // `''` resolves to `*` (toScanPattern), i.e. "no filter": the scan
            // and the tree are re-run unfiltered, which is what makes the cleared
            // row's exit observable rather than cosmetic.
            search.applySearch('', fuzzy);
          }}
          fuzzy={fuzzy}
          onFuzzyChange={onFuzzyChange}
          noTtlOnly={scan.noTtlOnly}
          onNoTtlOnlyChange={scan.setNoTtlOnly}
        />
      </KeyTreeHeader>
      <KeyTreeColumn
        searchMode={search.searchMode}
        treeRows={treeRows}
        allKeys={visibleKeys}
        expandedFolders={treeState.expanded}
        onToggleFolder={treeState.toggleFolder}
        selectedKey={detail.selectedKey}
        selectedKeys={selection.selectedKeys}
        onSelectKey={detail.selectKey}
        onToggleKey={selection.toggleKey}
        onToggleKeys={selection.toggleKeys}
        onKeyContextMenu={onKeyContextMenu}
        onDeleteRow={onDeleteRow}
        loading={scan.keysLoading || treeState.rootLoading}
        hasMore={scan.cursor !== 0}
        onLoadMore={scan.loadMore}
        separator={separator}
        emptyState={emptyState}
        /*
         * The pattern the empty state has to quote is the one that actually cut
         * the rows (`view.filterPattern`), not necessarily what the input still
         * shows: typing a pattern and then clearing the box must not produce a
         * `no-match` message about an empty pattern.
         */
        pattern={view.filterPattern}
        filterActive={view.filterActive}
        onSelectAllLoaded={selectAllLoaded}
        onRefresh={onRefresh}
        onClearSelection={selection.clearSelection}
        valueSearchState={search.valueSearchState}
        onCancelValueSearch={search.cancelValueSearch}
      />
    </>
  );
}
