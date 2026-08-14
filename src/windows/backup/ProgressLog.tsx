import { useCallback, useEffect, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Copy } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';

export interface ProgressLogProps {
  lines: string[];
}

export function ProgressLog({ lines }: ProgressLogProps) {
  const { t } = useI18n();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState(false);
  const copiedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const virtualizer = useVirtualizer({
    count: lines.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 20,
    overscan: 12,
    getItemKey: (index) => index,
  });

  useEffect(() => {
    if (lines.length === 0) return;
    virtualizer.scrollToIndex(lines.length - 1, { align: 'end' });
  }, [lines.length, virtualizer]);

  useEffect(
    () => () => {
      if (copiedTimer.current) clearTimeout(copiedTimer.current);
    },
    [],
  );

  const copyAll = useCallback(async () => {
    const text = lines.join('\n');
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      if (copiedTimer.current) clearTimeout(copiedTimer.current);
      copiedTimer.current = setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      console.warn(e);
    }
  }, [lines]);

  return (
    <div className="mb-3 flex min-h-0 flex-1 flex-col">
      <div className="mb-1 flex items-center justify-between gap-2">
        <div className="text-[11px] font-medium text-fg-muted">{t('backup.progressLog')}</div>
        <button
          type="button"
          className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-fg-muted hover:bg-surface-raised hover:text-fg"
          onClick={() => void copyAll()}
          data-testid="backup-progress-log-copy"
        >
          <Copy className="h-3 w-3" />
          {copied ? t('backup.logCopied') : t('backup.copyLog')}
        </button>
      </div>
      <div
        ref={scrollRef}
        className="selectable min-h-0 flex-1 overflow-auto rounded border border-edge bg-surface px-2 py-1.5 font-mono text-[11px] leading-5 text-fg-secondary"
        data-testid="backup-progress-log"
      >
        <div className="relative w-full select-text" style={{ height: virtualizer.getTotalSize() }}>
          {virtualizer.getVirtualItems().map((row) => (
            <div
              key={row.key}
              ref={virtualizer.measureElement}
              data-index={row.index}
              className="absolute left-0 top-0 w-full select-text overflow-hidden text-ellipsis whitespace-nowrap"
              style={{ transform: `translateY(${row.start}px)` }}
              title={lines[row.index]}
            >
              {lines[row.index]}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
