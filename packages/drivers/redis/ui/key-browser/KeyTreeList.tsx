import { useMemo, useRef, type MouseEvent as ReactMouseEvent } from 'react';
import { ChevronDown, ChevronRight, Folder, FolderOpen, Loader2, Trash2 } from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useI18n } from '@datazen/ui';
import { cn } from '@datazen/ui';
import { formatSize } from '../shared/formatSize';
import type { KeyTreeRow } from './keyTree';

/**
 * Delete target for a hover action on a tree row: a single leaf key, or a
 * folder prefix (delete every key under its subtree).
 */
export type KeyTreeDeleteTarget =
  | { kind: 'key'; key: string }
  | { kind: 'folder'; prefix: string; label: string; count: number };

const ROW_HEIGHT = 30;
const INDENT = 16;

/** Outlined pill colors per Redis type, keyed by the raw `keyType` string. */
const TYPE_BADGE: Record<string, string> = {
  string: 'border-success/40 text-success',
  hash: 'border-accent/40 text-accent',
  list: 'border-warning/40 text-warning',
  set: 'border-fg-secondary/40 text-fg-secondary',
  zset: 'border-danger/40 text-danger',
  stream: 'border-fg-muted/40 text-fg-muted',
};

/**
 * True when `key` lives under `folderPrefix`. Server tree prefixes carry a
 * trailing separator (`app:`); the client fallback tree does not (`app`), so
 * require the boundary char to be a separator to avoid `app` matching `apple`.
 */
function keyUnderFolder(key: string, folderPrefix: string): boolean {
  if (!key.startsWith(folderPrefix)) return false;
  const rest = key.slice(folderPrefix.length);
  if (rest === '') return false;
  if (/[:.]$/.test(folderPrefix)) return true;
  return rest[0] === ':' || rest[0] === '.';
}

export interface KeyTreeListProps {
  treeRows: KeyTreeRow[];
  /** All loaded key names — used for prefix cascade on folder checkboxes. */
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
}

/**
 * Compact hierarchical key tree (server-driven `list_children` levels):
 * one row per folder/leaf with a leading checkbox, folder icon + child count,
 * inline type / TTL / size badges, and a hover delete action (folder rows
 * delete their whole subtree).
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
}: KeyTreeListProps) {
  const { t } = useI18n();
  const scrollRef = useRef<HTMLDivElement>(null);
  const rowCount = treeRows.length;

  const virtualizer = useVirtualizer({
    count: rowCount + (hasMore ? 1 : 0),
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });

  // Folder checkboxes select every key under the prefix from the full loaded
  // key set, so collapsed (unexpanded) folders are selectable too.
  const folderSelection = useMemo(() => {
    const map = new Map<string, { keys: string[]; all: boolean }>();
    for (const row of treeRows) {
      if (row.kind !== 'folder') continue;
      const keys = allKeys.filter((k) => keyUnderFolder(k, row.path));
      const all = keys.length > 0 && keys.every((k) => selectedKeys.has(k));
      map.set(row.path, { keys, all });
    }
    return map;
  }, [treeRows, allKeys, selectedKeys]);

  return (
    <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
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

          const row = treeRows[vRow.index]!;
          const indent = 8 + row.depth * INDENT;

          if (row.kind === 'folder') {
            const open = expandedFolders.has(row.path);
            const sel = folderSelection.get(row.path);
            const descendants = sel?.keys ?? [];
            const allChecked = sel?.all ?? false;
            return (
              <div
                key={`folder:${row.path}`}
                className={cn(
                  'group absolute left-0 flex w-full items-center gap-1.5 border-b border-edge pr-3',
                  vRow.index % 2 === 0 ? 'bg-surface' : 'bg-surface-raised/40',
                  'hover:bg-accent/5',
                )}
                style={{ top: vRow.start, height: ROW_HEIGHT, paddingLeft: indent }}
                onClick={() => onToggleFolder(row.path)}
                data-testid={`redis-tree-folder-${row.path}`}
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
                <span className="shrink-0 text-[11px] text-fg-muted">({row.count})</span>
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
              key={entry.key}
              className={cn(
                'group absolute left-0 flex w-full items-center gap-1.5 border-b border-edge pr-3',
                isSelected
                  ? 'bg-accent/10'
                  : vRow.index % 2 === 0
                    ? 'bg-surface'
                    : 'bg-surface-raised/40',
                'hover:bg-accent/5',
              )}
              style={{ top: vRow.start, height: ROW_HEIGHT, paddingLeft: indent }}
              onClick={() => onSelectKey(entry.key)}
              onContextMenu={(e) => onKeyContextMenu(e, entry.key)}
              data-testid={`redis-key-row-${entry.key}`}
            >
              <span
                className="flex shrink-0 items-center justify-center"
                style={{ width: 16 }}
                onClick={(e) => e.stopPropagation()}
              >
                <input
                  type="checkbox"
                  checked={isChecked}
                  onChange={(e) => onToggleKey(entry.key, e.target.checked)}
                  aria-label={entry.key}
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
              <span className="shrink-0 rounded-full border border-edge px-1.5 py-px text-[10px] font-medium leading-tight text-fg-muted">
                {formatSize(entry.size)}
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
        })}
      </div>

      {loading && rowCount === 0 && (
        <div className="flex items-center justify-center gap-2 py-8 text-xs text-fg-muted">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t('common.loading')}
        </div>
      )}
    </div>
  );
}
