import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ChevronLeft, File, Folder, Layers, Loader2, Table2 } from 'lucide-react';
import { cn } from '../../lib/cn';
import { contextCommands } from '../../commands/context';
import { databaseCommands } from '../../commands/database';
import { useI18n } from '../../hooks/useI18n';
import type { ContextEntry, ContextItem, TableInfo } from '../../types';

const RECENT_KEY = 'datazen.contextRecent';
const MAX_RECENT = 8;

interface ContextPickerProps {
  query: string;
  onSelect: (item: ContextItem) => void;
  onClose: () => void;
  anchorRef: React.RefObject<HTMLElement | null>;
  position?: 'above' | 'below';
  dbSessionId?: string;
  database?: string;
}

type View = 'root' | 'tables' | 'files';

type Row =
  | { type: 'category'; id: 'tables' | 'files'; label: string }
  | { type: 'section'; label: string }
  | { type: 'item'; item: ContextItem };

function loadRecent(): ContextItem[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as ContextItem[];
    return Array.isArray(parsed) ? parsed.slice(0, MAX_RECENT) : [];
  } catch {
    return [];
  }
}

function pushRecent(item: ContextItem): void {
  const prev = loadRecent().filter((r) => !(r.kind === item.kind && r.id === item.id));
  const next = [item, ...prev].slice(0, MAX_RECENT);
  localStorage.setItem(RECENT_KEY, JSON.stringify(next));
}

function entryToItem(entry: ContextEntry): ContextItem {
  return {
    kind: entry.isDir ? 'dir' : 'file',
    id: entry.path,
    name: entry.name,
    path: entry.path,
  };
}

function flattenFileEntries(entries: ContextEntry[]): ContextItem[] {
  const result: ContextItem[] = [];
  for (const entry of entries) {
    result.push(entryToItem(entry));
    if (entry.isDir && entry.children) {
      result.push(...flattenFileEntries(entry.children));
    }
  }
  return result;
}

function tableToItem(table: TableInfo, database?: string): ContextItem {
  const name = table.schema ? `${table.schema}.${table.name}` : table.name;
  return {
    kind: 'table',
    id: name,
    name,
    database,
  };
}

function matchesQuery(item: ContextItem, q: string): boolean {
  const lower = q.toLowerCase();
  return item.name.toLowerCase().includes(lower) || item.id.toLowerCase().includes(lower);
}

export function ContextPicker({
  query,
  onSelect,
  onClose,
  anchorRef,
  position = 'above',
  dbSessionId,
  database,
}: ContextPickerProps) {
  const { t } = useI18n();
  const [view, setView] = useState<View>('root');
  const [recent, setRecent] = useState<ContextItem[]>(() => loadRecent());
  const [tables, setTables] = useState<ContextItem[]>([]);
  const [files, setFiles] = useState<ContextItem[]>([]);
  const [loadingTables, setLoadingTables] = useState(false);
  const [loadingFiles, setLoadingFiles] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  // `@` only opens the picker at root (categories). Typing after `@` filters
  // once the user drills into Tables/Files — no second `@` for nested levels.
  const showTablesCategory = Boolean(dbSessionId);
  const needsTables = view === 'tables' && showTablesCategory;
  const needsFiles = view === 'files';

  useEffect(() => {
    setRecent(loadRecent());
  }, [view]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query, view]);

  useEffect(() => {
    if (!needsTables) return;
    if (!dbSessionId || !database) return;

    let cancelled = false;
    setLoadingTables(true);
    databaseCommands
      .getTables(dbSessionId, database)
      .then((data) => {
        if (!cancelled) {
          setTables(data.map((tbl) => tableToItem(tbl, database)));
          setLoadingTables(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setTables([]);
          setLoadingTables(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [needsTables, dbSessionId, database]);

  useEffect(() => {
    if (!needsFiles) return;

    let cancelled = false;
    setLoadingFiles(true);
    contextCommands
      .listFiles(undefined)
      .then((data) => {
        if (!cancelled) {
          setFiles(flattenFileEntries(data));
          setLoadingFiles(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setFiles([]);
          setLoadingFiles(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [needsFiles]);

  const rows: Row[] = useMemo(() => {
    if (view === 'tables') {
      const list = query ? tables.filter((item) => matchesQuery(item, query)) : tables;
      return list.map((item) => ({ type: 'item' as const, item }));
    }

    if (view === 'files') {
      const list = query ? files.filter((item) => matchesQuery(item, query)) : files;
      return list.map((item) => ({ type: 'item' as const, item }));
    }

    // Root: categories + recent (optionally narrowed by the same @query).
    const result: Row[] = [];
    if (showTablesCategory) {
      result.push({ type: 'category', id: 'tables', label: t('context.tables') });
    }
    result.push({ type: 'category', id: 'files', label: t('context.files') });
    const recentRows = query ? recent.filter((item) => matchesQuery(item, query)) : recent;
    if (recentRows.length > 0) {
      result.push({ type: 'section', label: t('context.recent') });
      for (const item of recentRows) {
        result.push({ type: 'item', item });
      }
    }
    return result;
  }, [view, showTablesCategory, tables, files, recent, query, t]);

  useEffect(() => {
    setActiveIndex((prev) => Math.min(prev, Math.max(rows.length - 1, 0)));
  }, [rows.length]);

  const handleSelectItem = useCallback(
    (item: ContextItem) => {
      pushRecent(item);
      setRecent(loadRecent());
      onSelect(item);
    },
    [onSelect],
  );

  const handleCategoryClick = useCallback((id: 'tables' | 'files') => {
    setView(id);
    setActiveIndex(0);
  }, []);

  const handleBack = useCallback(() => {
    setView('root');
    setActiveIndex(0);
  }, []);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIndex((prev) => Math.min(prev + 1, rows.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIndex((prev) => Math.max(prev - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const row = rows[activeIndex];
        if (!row) return;
        if (row.type === 'category') {
          handleCategoryClick(row.id);
        } else if (row.type === 'item') {
          handleSelectItem(row.item);
        }
      } else if (e.key === 'Escape') {
        e.preventDefault();
        if (view !== 'root') {
          handleBack();
        } else {
          onClose();
        }
      }
    },
    [rows, activeIndex, handleCategoryClick, handleSelectItem, view, handleBack, onClose],
  );

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [handleKeyDown]);

  useEffect(() => {
    const active = listRef.current?.querySelector('[data-active="true"]');
    if (active && typeof active.scrollIntoView === 'function') {
      active.scrollIntoView({ block: 'nearest' });
    }
  }, [activeIndex]);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (
        listRef.current &&
        !listRef.current.contains(e.target as Node) &&
        anchorRef.current &&
        !anchorRef.current.contains(e.target as Node)
      ) {
        onClose();
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [onClose, anchorRef]);

  const loading = (needsTables && loadingTables) || (needsFiles && loadingFiles);

  return (
    <div
      ref={listRef}
      data-testid="context-picker"
      className={cn(
        'absolute left-0 z-50 w-72',
        position === 'above' ? 'bottom-full mb-1' : 'top-full mt-1',
        'max-h-60 overflow-y-auto rounded-lg border border-edge bg-surface shadow-lg',
      )}
    >
      {view !== 'root' && (
        <button
          type="button"
          data-testid="context-picker-back"
          className="flex w-full items-center gap-1.5 border-b border-edge px-3 py-1.5 text-left text-xs text-fg-secondary hover:bg-surface-raised hover:text-fg"
          onMouseDown={(e) => e.preventDefault()}
          onClick={handleBack}
        >
          <ChevronLeft className="h-3.5 w-3.5 shrink-0" />
          {t('context.back')}
        </button>
      )}

      {loading && (
        <div className="flex items-center gap-2 px-3 py-3 text-xs text-fg-muted">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t('common.loading')}
        </div>
      )}

      {!loading && rows.length === 0 && (
        <div className="px-3 py-3 text-xs text-fg-muted">
          {view === 'tables'
            ? t('context.noTables')
            : query
              ? t('context.noResults')
              : t('context.noFiles')}
        </div>
      )}

      {!loading &&
        rows.map((row, idx) => {
          if (row.type === 'category') {
            const testId = row.id === 'tables' ? 'context-cat-tables' : 'context-cat-files';
            const Icon = row.id === 'tables' ? Table2 : Folder;
            return (
              <button
                key={row.id}
                type="button"
                data-testid={testId}
                data-active={idx === activeIndex}
                className={cn(
                  'flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors',
                  idx === activeIndex
                    ? 'bg-accent/15 text-accent'
                    : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                )}
                onMouseEnter={() => setActiveIndex(idx)}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => handleCategoryClick(row.id)}
              >
                <Icon className="h-3.5 w-3.5 shrink-0" />
                <span className="truncate">{row.label}</span>
              </button>
            );
          }

          if (row.type === 'section') {
            return (
              <div
                key={`section-${row.label}`}
                className="px-3 pb-0.5 pt-2 text-[10px] font-medium uppercase tracking-wide text-fg-muted"
              >
                {row.label}
              </div>
            );
          }

          const { item } = row;
          const isCtxYaml =
            item.kind === 'file' &&
            (item.name.endsWith('.ctx.yaml') || item.name.endsWith('.ctx.yml'));
          const Icon =
            item.kind === 'table'
              ? Table2
              : item.kind === 'dir'
                ? Folder
                : isCtxYaml
                  ? Layers
                  : File;

          return (
            <button
              key={`${item.kind}-${item.id}`}
              type="button"
              data-testid="context-item"
              data-kind={item.kind}
              data-id={item.id}
              data-active={idx === activeIndex}
              className={cn(
                'flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors',
                idx === activeIndex
                  ? 'bg-accent/15 text-accent'
                  : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
              )}
              onMouseEnter={() => setActiveIndex(idx)}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => handleSelectItem(item)}
            >
              <Icon className="h-3.5 w-3.5 shrink-0" />
              <span className="truncate">{item.name}</span>
            </button>
          );
        })}
    </div>
  );
}
