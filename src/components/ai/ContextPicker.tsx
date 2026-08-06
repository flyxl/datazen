import { useCallback, useEffect, useRef, useState } from 'react';
import { File, Folder, Loader2 } from 'lucide-react';
import { cn } from '../../lib/cn';
import { contextCommands } from '../../commands/context';
import { useI18n } from '../../hooks/useI18n';
import type { ContextEntry } from '../../types';

interface ContextPickerProps {
  query: string;
  onSelect: (entry: ContextEntry) => void;
  onClose: () => void;
  anchorRef: React.RefObject<HTMLElement | null>;
}

function flattenEntries(entries: ContextEntry[], depth = 0): { entry: ContextEntry; depth: number }[] {
  const result: { entry: ContextEntry; depth: number }[] = [];
  for (const e of entries) {
    result.push({ entry: e, depth });
    if (e.isDir && e.children) {
      result.push(...flattenEntries(e.children, depth + 1));
    }
  }
  return result;
}

export function ContextPicker({ query, onSelect, onClose, anchorRef }: ContextPickerProps) {
  const { t } = useI18n();
  const [entries, setEntries] = useState<ContextEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [activeIndex, setActiveIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    contextCommands
      .listFiles(query || undefined)
      .then((data) => {
        if (!cancelled) {
          setEntries(data);
          setActiveIndex(0);
          setLoading(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setEntries([]);
          setLoading(false);
        }
      });
    return () => { cancelled = true; };
  }, [query]);

  const flat = flattenEntries(entries);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIndex((prev) => Math.min(prev + 1, flat.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIndex((prev) => Math.max(prev - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        if (flat[activeIndex]) {
          onSelect(flat[activeIndex].entry);
        }
      } else if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    },
    [flat, activeIndex, onSelect, onClose],
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

  return (
    <div
      ref={listRef}
      className={cn(
        'absolute bottom-full left-0 z-50 mb-1 w-72',
        'max-h-60 overflow-y-auto rounded-lg border border-edge bg-surface shadow-lg',
      )}
    >
      {loading && (
        <div className="flex items-center gap-2 px-3 py-3 text-xs text-fg-muted">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t('common.loading')}
        </div>
      )}

      {!loading && flat.length === 0 && (
        <div className="px-3 py-3 text-xs text-fg-muted">
          {query ? t('context.noResults') : t('context.noFiles')}
        </div>
      )}

      {!loading &&
        flat.map(({ entry, depth }, idx) => (
          <button
            key={entry.path}
            type="button"
            data-active={idx === activeIndex}
            className={cn(
              'flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors',
              idx === activeIndex
                ? 'bg-accent/15 text-accent'
                : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
            )}
            style={{ paddingLeft: `${12 + depth * 16}px` }}
            onMouseEnter={() => setActiveIndex(idx)}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => onSelect(entry)}
          >
            {entry.isDir ? (
              <Folder className="h-3.5 w-3.5 shrink-0 text-yellow-500" />
            ) : (
              <File className="h-3.5 w-3.5 shrink-0 text-blue-400" />
            )}
            <span className="truncate">{entry.name}</span>
            {entry.isDir && (
              <span className="ml-auto shrink-0 text-[10px] text-fg-muted">
                {t('context.dir')}
              </span>
            )}
            {!entry.isDir && entry.size !== undefined && (
              <span className="ml-auto shrink-0 text-[10px] text-fg-muted">
                {formatSize(entry.size)}
              </span>
            )}
          </button>
        ))}
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
}
