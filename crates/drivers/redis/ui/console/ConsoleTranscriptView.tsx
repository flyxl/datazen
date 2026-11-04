/**
 * Scrollback view for the Redis console.
 *
 * Renders the accumulated transcript as a terminal-style session: every command
 * is echoed with its `dbN>` prompt and its result (or error) sits directly
 * beneath it. Unlike the old single-result panel this never replaces what is
 * above it — a failed command stays on screen, in red, as part of the history.
 */
import { useEffect, useRef } from 'react';
import { useI18n } from '@datazen/ui';

import { ConsoleResultView } from './consoleResultRenderer';
import { dangerBorderClass } from './RedisConsoleDangerBadge';
import { assessCommand } from './redisConsoleDanger';
import type { TranscriptEntry } from './consoleTranscript';

export interface ConsoleTranscriptProps {
  entries: TranscriptEntry[];
  /** Scroll to the newest entry when one arrives. */
  followTail?: boolean;
}

export function ConsoleTranscriptView({ entries, followTail = true }: ConsoleTranscriptProps) {
  const { t } = useI18n();
  const scrollRef = useRef<HTMLDivElement>(null);
  // A terminal only auto-scrolls when the user is already parked at the
  // bottom. Yanking the viewport away from someone reading an older result
  // would make the scrollback worse than useless.
  const stickToBottom = useRef(true);

  useEffect(() => {
    if (!followTail || !stickToBottom.current) return;
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [entries, followTail]);

  const handleScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 32;
  };

  if (entries.length === 0) {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-auto" ref={scrollRef}>
        <p className="p-4 text-sm text-fg-muted">{t('redis.console.welcome')}</p>
      </div>
    );
  }

  return (
    <div
      className="min-h-0 flex-1 overflow-auto px-3 py-2"
      ref={scrollRef}
      onScroll={handleScroll}
      data-testid="redis-console-transcript"
    >
      {entries.map((entry) => (
        <TranscriptRow key={entry.id} entry={entry} />
      ))}
    </div>
  );
}

function TranscriptRow({ entry }: { entry: TranscriptEntry }) {
  if (entry.kind === 'command') {
    return (
      <div
        className={`flex items-start gap-2 border-l-2 py-1 pl-2 ${dangerBorderClass(assessCommand(entry.text))}`}
        data-testid="redis-console-cmd"
      >
        <span className="shrink-0 font-mono text-[13px] text-accent select-none">
          db{entry.dbIndex}&gt;
        </span>
        <span className="min-w-0 font-mono text-[13px] text-fg break-all whitespace-pre-wrap">
          {entry.text}
        </span>
      </div>
    );
  }

  if (entry.kind === 'error') {
    return (
      <div
        className="my-1 rounded border border-danger/20 bg-danger/10 px-3 py-1.5 text-sm whitespace-pre-line text-danger font-mono"
        data-testid="redis-console-entry-error"
      >
        {entry.message}
      </div>
    );
  }

  return (
    <div className="px-2 py-1" data-testid="redis-console-entry-result">
      <ConsoleResultView item={entry.item} />
    </div>
  );
}
