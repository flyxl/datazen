import { useCallback, useEffect, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Copy } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';

export interface ProgressLogProps {
  lines: string[];
}

const ROW_HEIGHT = 20;

export function ProgressLog({ lines }: ProgressLogProps) {
  const { t } = useI18n();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState(false);
  const copiedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const shouldAutoScroll = useRef(true);

  const virtualizer = useVirtualizer({
    count: lines.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const onScroll = () => {
      shouldAutoScroll.current = el.scrollTop + el.clientHeight >= el.scrollHeight - ROW_HEIGHT * 2;
    };
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => el.removeEventListener('scroll', onScroll);
  }, []);

  useEffect(() => {
    if (lines.length === 0 || !shouldAutoScroll.current) return;
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines.length]);

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

  const items = virtualizer.getVirtualItems();
  const paddingTop = items.length > 0 ? items[0].start : 0;
  const paddingBottom =
    items.length > 0 ? virtualizer.getTotalSize() - items[items.length - 1].end : 0;

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
        className="selectable min-h-0 flex-1 overflow-auto rounded border border-edge bg-surface px-2 py-1 font-mono text-[11px] text-fg-secondary"
        data-testid="backup-progress-log"
      >
        <div style={{ paddingTop, paddingBottom }}>
          {items.map((row) => (
            <div
              key={row.index}
              className="select-text overflow-hidden text-ellipsis whitespace-nowrap"
              style={{ height: ROW_HEIGHT, lineHeight: `${ROW_HEIGHT}px` }}
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
