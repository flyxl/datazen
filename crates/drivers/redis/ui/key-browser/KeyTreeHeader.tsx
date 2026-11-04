import type { ReactNode } from 'react';
import { CheckSquare, ListChecks, Plus, RefreshCw, Trash2 } from 'lucide-react';
import { Button, cn, useI18n } from '@datazen/ui';
import { SearchModeTabs, type SearchMode } from './SearchModeTabs';

/**
 * Row R1 of the key-tree column header (PRD §3.2 屏 B 左列).
 *
 * This row is *resident*: the search-scope segment control used to sit in the
 * database sidebar, which the host hides whenever it renders its own navigator
 * tree (`hideSidebar`) — so in the normal layout the user could never switch
 * between key / value / all search. R1 owns the segment control, the loaded-vs-total
 * counter and the action group that the tree's selection feeds.
 *
 * Counter semantics (PRD §3.2 R1 + I-4): `已加载 N / 共 M` while the scan cursor is
 * exhausted, `N+` while it is not — an unfinished scan never presents a partial
 * subset as a total.
 *
 * R2 search and R3 grouping rows mount as `children` so the header stays one
 * bordered block instead of sibling divs in the workbench.
 */

export interface KeyTreeHeaderProps {
  searchMode: SearchMode;
  onSearchModeChange: (mode: SearchMode) => void;
  /** Keys currently materialised in the tree. */
  loadedCount: number;
  /** `DBSIZE` of the selected database. */
  totalCount: number;
  /** Scan cursor still open ⇒ the loaded set is partial (`N+`). */
  scanning: boolean;
  onSelectAll: () => void;
  /** How many keys the tree's checkboxes currently hold. `0` ⇒ no selection. */
  selectionCount: number;
  /** R1's select-all slot becomes the delete action once anything is ticked. */
  onDeleteSelected: () => void;
  onRefresh: () => void;
  onCreateKey: () => void;
  children?: ReactNode;
}

export function KeyTreeHeader({
  searchMode,
  onSearchModeChange,
  loadedCount,
  totalCount,
  scanning,
  onSelectAll,
  selectionCount,
  onDeleteSelected,
  onRefresh,
  onCreateKey,
  children,
}: KeyTreeHeaderProps) {
  const { t } = useI18n();
  const isKeyMode = searchMode === 'key';
  const loadedLabel = scanning ? `${loadedCount}+` : String(loadedCount);
  /*
   * One slot, two actions. R1 has no room for a second button without widening
   * the row that was just lined up with the right panel's tab bar, so the
   * select-all button *becomes* the delete button the moment anything is
   * ticked — the tree needs an exit for its own selection, and a second
   * permanently-red button next to refresh is the way to offer one.
   *
   * Keyed off the selection rather than off "the select-all button was
   * clicked": ticking three boxes by hand is the same state as clicking
   * select-all, and leaving those users without a delete would be arbitrary.
   *
   * Exit from the delete state is `Esc` (treeNavAction's `clear` chord) or
   * unticking; the button's own title says so, because a button that changed
   * identity is the one place the affordance needs to be spelled out.
   */
  const hasSelection = selectionCount > 0;

  return (
    <div
      className="flex shrink-0 flex-col gap-1 border-b border-edge bg-surface-alt px-2 pb-1.5"
      data-testid="redis-tree-header"
      data-search-mode={searchMode}
    >
      {/*
        R1 is the column's toolbar row. `h-10` is shared with the right panel's
        tab bar so the two columns' first rows line up; the top padding that
        used to add to it is dropped rather than kept, because the height would
        then be the sum of two numbers living in two files. `pb-1.5` above
        belongs to the search row below, not to this row.
      */}
      <div className="flex h-10 shrink-0 items-center gap-2" data-testid="redis-tree-toolbar-row">
        <SearchModeTabs mode={searchMode} onChange={onSearchModeChange} />
        <span className="h-4 w-px shrink-0 bg-edge" />
        {/*
          The counter counts *scanned keys*, so it only has a meaning while the
          key scope owns the column. In the value / all scopes the column is
          `ValueSearchResults`, which renders its own status bar directly below
          this row (scanned / maxKeys + hit count). Leaving this one up would put
          a frozen key-scan number next to the live value-search numbers — two
          rows of counters disagreeing about what the column is showing. Hiding it
          is also what keeps it from duplicating the hit count the bar below
          already owns.
        */}
        {searchMode === 'key' && (
          <span
            className="min-w-0 truncate text-[11px] text-fg-muted"
            data-testid="redis-tree-count"
            data-loaded={loadedCount}
            data-total={totalCount}
            data-partial={scanning ? 'true' : 'false'}
          >
            {t('redis.tree.loadedOfTotal')
              .replace('{loaded}', loadedLabel)
              .replace('{total}', String(totalCount))}
          </span>
        )}
        <div className="flex-1" />
        <div className="flex shrink-0 items-center gap-1">
          <HeaderIcon
            testId={hasSelection ? 'redis-tree-delete-selected' : 'redis-tree-select-all'}
            labelKey={hasSelection ? 'redis.deleteSelected' : 'redis.tree.selectAll'}
            Icon={hasSelection ? Trash2 : CheckSquare}
            /*
             * The delete state is keyed off the selection, so it is enabled
             * whenever anything is ticked; the select-all state still needs a
             * loaded set to take and the key scope to exist at all.
             */
            disabled={!isKeyMode || (!hasSelection && loadedCount === 0)}
            onClick={hasSelection ? onDeleteSelected : onSelectAll}
            tone={hasSelection ? 'danger' : 'default'}
          />
          <HeaderIcon
            testId="redis-tree-refresh"
            labelKey="connWin.refresh"
            Icon={RefreshCw}
            onClick={onRefresh}
          />
          <HeaderIcon
            testId="redis-tree-create-key"
            labelKey="redis.createKey"
            Icon={Plus}
            onClick={onCreateKey}
          />
        </div>
      </div>
      {children}
    </div>
  );
}

interface HeaderIconProps {
  testId: string;
  labelKey: string;
  Icon: typeof ListChecks;
  disabled?: boolean;
  onClick: () => void;
  /**
   * `danger` recolours a ghost button rather than switching it to the design
   * system's solid `danger` variant: a 28px block of `bg-danger` in a toolbar
   * that also holds refresh and create would out-shout the row it lives in, and
   * it appears the moment anything is ticked rather than after a deliberate
   * click on it.
   */
  tone?: 'default' | 'danger';
  children?: ReactNode;
}

function HeaderIcon({
  testId,
  labelKey,
  Icon,
  disabled,
  onClick,
  tone = 'default',
  children,
}: HeaderIconProps) {
  const { t } = useI18n();
  return (
    <Button
      variant="ghost"
      className={cn(
        'relative h-7 w-7 shrink-0 p-0',
        tone === 'danger' && 'text-danger hover:bg-danger/10 hover:text-danger',
      )}
      title={t(labelKey)}
      aria-label={t(labelKey)}
      data-testid={testId}
      data-action-label-key={labelKey}
      disabled={disabled}
      onClick={onClick}
    >
      <Icon className="h-3.5 w-3.5" />
      {children}
    </Button>
  );
}
