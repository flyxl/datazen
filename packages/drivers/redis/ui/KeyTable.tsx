import {
  useMemo,
  useRef,
  type MouseEvent as ReactMouseEvent,
} from 'react';
import { ChevronDown, ChevronRight, Key, Loader2 } from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useColumnResize } from '../../../../src/hooks/useColumnResize';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';
import type { KeyEntry } from '../../../../src/types';
import { buildKeyTreeRows } from './keyTree';
import type { KeyBrowserViewMode } from './KeyBrowserControls';

const ROW_HEIGHT = 32;

function formatSize(size: number): string {
  if (!size || size < 0) return '—';
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

export function KeyTable({
  keys,
  viewMode = 'flat',
  expandedFolders = new Set<string>(),
  onToggleFolder,
  selectedKey,
  selectedKeys,
  onSelectKey,
  onToggleKey,
  onToggleSelectAll,
  onKeyContextMenu,
  loading,
  hasMore,
  onLoadMore,
}: {
  keys: KeyEntry[];
  viewMode?: KeyBrowserViewMode;
  expandedFolders?: Set<string>;
  onToggleFolder?: (path: string) => void;
  selectedKey: string | null;
  selectedKeys: Set<string>;
  onSelectKey: (key: string) => void;
  onToggleKey: (key: string, checked: boolean) => void;
  onToggleSelectAll: () => void;
  onKeyContextMenu: (e: ReactMouseEvent, key: string) => void;
  loading: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
}) {
  const { t } = useI18n();
  const scrollRef = useRef<HTMLDivElement>(null);
  const { columnWidths, onResizeStart } = useColumnResize({ count: 6 });

  const treeRows = useMemo(
    () => (viewMode === 'tree' ? buildKeyTreeRows(keys, expandedFolders) : null),
    [viewMode, keys, expandedFolders],
  );
  const rowCount = treeRows ? treeRows.length : keys.length;

  const virtualizer = useVirtualizer({
    count: rowCount + (hasMore ? 1 : 0),
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });

  const TYPE_COLORS: Record<string, string> = {
    string: 'text-success',
    hash: 'text-accent',
    list: 'text-warning',
    set: 'text-fg-secondary',
    zset: 'text-danger',
    stream: 'text-fg-muted',
  };

  const columns = [
    '',
    t('redis.key'),
    t('redis.type'),
    t('redis.ttl'),
    t('redis.size'),
    t('redis.preview'),
  ];

  const allSelected = keys.length > 0 && selectedKeys.size === keys.length;

  return (
    <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
      <div className="min-w-max text-[13px]">
        <div className="sticky top-0 z-10 flex bg-surface-alt">
          {columns.map((col, ci) => (
            <div
              key={col || 'check'}
              className="relative shrink-0 border-b border-r border-edge px-3 py-2 text-left text-xs font-medium text-fg-secondary"
              style={{
                width: columnWidths[ci],
                ...(ci === 0 ? { width: 36, minWidth: 36 } : {}),
                ...(ci === 1 || ci === 5 ? { flex: '1 1 0', minWidth: 100 } : {}),
              }}
            >
              {ci === 0 ? (
                <input
                  type="checkbox"
                  checked={allSelected}
                  onChange={onToggleSelectAll}
                  aria-label={t('common.selectAll')}
                />
              ) : (
                col
              )}
              {ci > 0 && (
                <div
                  className="absolute right-0 top-0 z-20 h-full w-[5px] cursor-col-resize hover:bg-accent/40 active:bg-accent/60"
                  onPointerDown={(e) => {
                    e.preventDefault();
                    onResizeStart(ci, e.clientX);
                  }}
                />
              )}
            </div>
          ))}
        </div>

        <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
          {virtualizer.getVirtualItems().map((vRow) => {
            if (vRow.index >= rowCount) {
              return (
                <div
                  key="load-more"
                  className="absolute left-0 flex w-full items-center justify-center border-b border-edge"
                  style={{ top: vRow.start, height: ROW_HEIGHT }}
                >
                  <button
                    type="button"
                    className="text-xs text-accent hover:underline"
                    onClick={onLoadMore}
                    disabled={loading}
                  >
                    {loading ? (
                      <Loader2 className="inline h-3.5 w-3.5 animate-spin" />
                    ) : (
                      t('redis.loadMore')
                    )}
                  </button>
                </div>
              );
            }

            const treeRow = treeRows?.[vRow.index];
            if (treeRow?.kind === 'folder') {
              const open = expandedFolders.has(treeRow.path);
              return (
                <div
                  key={`folder:${treeRow.path}`}
                  className={cn(
                    'absolute left-0 flex w-full cursor-pointer border-b border-edge',
                    vRow.index % 2 === 0 ? 'bg-surface' : 'bg-surface-raised/50',
                    'hover:bg-accent/5',
                  )}
                  style={{ top: vRow.start, height: ROW_HEIGHT }}
                  onClick={() => onToggleFolder?.(treeRow.path)}
                >
                  <div
                    className="flex shrink-0 items-center justify-center border-r border-edge px-2"
                    style={{ width: 36 }}
                  />
                  <div
                    className="flex min-w-0 flex-1 items-center gap-1 overflow-hidden border-r border-edge px-3 font-mono text-xs"
                    style={{ paddingLeft: 12 + treeRow.depth * 14 }}
                  >
                    {open ? (
                      <ChevronDown className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
                    ) : (
                      <ChevronRight className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
                    )}
                    <span className="truncate font-medium text-fg">{treeRow.label}</span>
                    <span className="text-fg-muted">({treeRow.count})</span>
                  </div>
                </div>
              );
            }

            const entry = treeRow?.kind === 'key' ? treeRow.entry : keys[vRow.index]!;
            const keyLabel = treeRow?.kind === 'key' ? treeRow.label : entry.key;
            const depth = treeRow?.kind === 'key' ? treeRow.depth : 0;
            const isSelected = selectedKey === entry.key;
            const isChecked = selectedKeys.has(entry.key);

            return (
              <div
                key={entry.key}
                className={cn(
                  'absolute left-0 flex w-full cursor-pointer border-b border-edge',
                  isSelected
                    ? 'bg-accent/10'
                    : vRow.index % 2 === 0
                      ? 'bg-surface'
                      : 'bg-surface-raised/50',
                  'hover:bg-accent/5',
                )}
                style={{ top: vRow.start, height: ROW_HEIGHT }}
                onClick={() => onSelectKey(entry.key)}
                onContextMenu={(e) => onKeyContextMenu(e, entry.key)}
              >
                <div
                  className="flex shrink-0 items-center justify-center border-r border-edge px-2"
                  style={{ width: 36 }}
                  onClick={(e) => e.stopPropagation()}
                >
                  <input
                    type="checkbox"
                    checked={isChecked}
                    onChange={(e) => onToggleKey(entry.key, e.target.checked)}
                    aria-label={entry.key}
                  />
                </div>
                <div
                  className="flex min-w-0 flex-1 items-center overflow-hidden border-r border-edge px-3 font-mono"
                  style={{ flex: '1 1 0', minWidth: 100, width: columnWidths[1] }}
                >
                  <Key className="mr-1.5 h-3 w-3 shrink-0 text-fg-muted" />
                  <span className="truncate text-fg-secondary" style={{ paddingLeft: depth * 14 }}>
                    {keyLabel}
                  </span>
                </div>
                <div
                  className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3"
                  style={{ width: columnWidths[2] }}
                >
                  <span className={cn('text-xs font-medium', TYPE_COLORS[entry.keyType] ?? 'text-fg-muted')}>
                    {entry.keyType}
                  </span>
                </div>
                <div
                  className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3 text-fg-secondary"
                  style={{ width: columnWidths[3] }}
                >
                  {entry.ttl < 0 ? t('redis.noExpiry') : `${entry.ttl}${t('redis.seconds')}`}
                </div>
                <div
                  className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3 text-fg-secondary"
                  style={{ width: columnWidths[4] }}
                >
                  {formatSize(entry.size)}
                </div>
                <div
                  className="flex min-w-0 flex-1 items-center overflow-hidden px-3 text-fg-muted"
                  style={{ flex: '1 1 0', minWidth: 100, width: columnWidths[5] }}
                >
                  <span className="truncate font-mono text-xs">{entry.preview}</span>
                </div>
              </div>
            );
          })}
        </div>

        {loading && keys.length === 0 && (
          <div className="flex items-center justify-center gap-2 py-8 text-xs text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('common.loading')}
          </div>
        )}
      </div>
    </div>
  );
}
