import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { Loader2 } from 'lucide-react';
import { Button } from '@datazen/ui';
import { useI18n } from '../../../../src/hooks/useI18n';
import { useSettingsStore } from '../../../../src/stores/settingsStore';
import { cn } from '../../../../src/lib/cn';
import {
  resolveEditorFontFamily,
  HOST_DEFAULT_EDITOR_FONT,
} from '../../../../src/lib/resolveEditorFontFamily';
import { redisCommandInvoke } from './redisInvoke';
import { readBooleanField } from '../../../../src/lib/driverSettings';
import { classifyDangerLevel, dangerBadgeColor, type DangerLevel } from './redisConsoleDanger';
import { useRedisGate } from './useRedisGate';
import { SafeModeBadge } from './SafeModeBadge';
import {
  loadConsoleHistory,
  navigateConsoleHistory,
  pushConsoleHistory,
  type HistoryNavigationState,
} from './consoleHistory';
import { useCompletion } from './consoleCompletion/useCompletion';
import { CompletionPopup } from './consoleCompletion/CompletionPopup';
import { ClusterNodePicker } from './ClusterNodePicker';
import { readClusterRouting, resolvePinnedNodeAddr } from './settingsHelpers';

export interface RedisConsoleProps {
  dbSessionId: string;
  dbIndex?: number;
  keySuggestions?: string[];
  pinnedNodeAddr?: string;
  onPinnedNodeAddrChange?: (addr: string) => void;
}

interface ExecResult {
  command: string;
  ok: boolean;
  value?: string;
  error?: string;
}

interface ExecResponse {
  results: ExecResult[];
}

function dangerLevelLabel(level: DangerLevel, t: (key: string) => string): string {
  switch (level) {
    case 'ultra-danger':
      return t('redis.console.dangerUltra');
    case 'danger':
      return t('redis.console.dangerDanger');
    case 'write':
      return t('redis.console.dangerWrite');
    default:
      return t('redis.console.dangerSafe');
  }
}

function dangerBorderClass(level: DangerLevel): string {
  switch (level) {
    case 'ultra-danger':
      return 'border-l-red-500';
    case 'danger':
      return 'border-l-orange-500';
    case 'write':
      return 'border-l-yellow-500';
    default:
      return 'border-l-transparent';
  }
}

function applyCompletion(
  text: string,
  tokenStart: number,
  tokenEnd: number,
  completion: string,
): { text: string; cursor: number } {
  const nextText = `${text.slice(0, tokenStart)}${completion} ${text.slice(tokenEnd)}`;
  const nextCursor = tokenStart + completion.length + 1;
  return { text: nextText, cursor: nextCursor };
}

export function RedisConsole({
  dbSessionId,
  dbIndex = 0,
  keySuggestions = [],
  pinnedNodeAddr = '',
  onPinnedNodeAddrChange,
}: RedisConsoleProps) {
  const { t } = useI18n();
  const driverSettings = useSettingsStore((s) => s.settings.driverSettings);
  const clusterRouting = readClusterRouting(driverSettings?.redis);
  const allowFlush = readBooleanField(
    (driverSettings?.redis ?? {}) as Record<string, unknown>,
    'allowFlush',
    false,
  );
  const { gateWrite, gateDialog } = useRedisGate();
  const nodeAddr = resolvePinnedNodeAddr(clusterRouting, pinnedNodeAddr);
  const editorFontFamily = useSettingsStore(
    (s) => s.settings.editorFontFamily || HOST_DEFAULT_EDITOR_FONT,
  );
  const fontFamily = resolveEditorFontFamily(editorFontFamily, '', HOST_DEFAULT_EDITOR_FONT);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [commands, setCommands] = useState('');
  const [cursor, setCursor] = useState(0);
  const [running, setRunning] = useState(false);
  const [results, setResults] = useState<ExecResult[]>([]);
  const [activeResultIdx, setActiveResultIdx] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [historyState, setHistoryState] = useState<HistoryNavigationState>({
    index: null,
    draft: '',
  });
  const [history, setHistory] = useState<string[]>([]);
  const [completionActive, setCompletionActive] = useState(0);
  const [completionDismissed, setCompletionDismissed] = useState(false);

  useEffect(() => {
    setHistory(loadConsoleHistory(dbSessionId));
    setCommands('');
    setResults([]);
    setError(null);
    setHistoryState({ index: null, draft: '' });
  }, [dbSessionId]);

  const completion = useCompletion({
    text: commands,
    cursor,
    dbSessionId,
    dbIndex,
    prewarmKeys: keySuggestions,
  });
  const completions = completion.items;
  const completionOpen = completion.open && !completionDismissed;

  useEffect(() => {
    setCompletionActive(0);
  }, [commands, cursor]);

  useEffect(() => {
    setCompletionDismissed(false);
  }, [commands, cursor]);

  const acceptCompletion = useCallback(
    (index: number) => {
      const item = completions[index];
      if (!item) return;
      const applied = applyCompletion(
        commands,
        completion.tokenStart,
        completion.tokenEnd,
        item.insertText,
      );
      setCommands(applied.text);
      requestAnimationFrame(() => {
        const el = textareaRef.current;
        if (el) {
          el.selectionStart = applied.cursor;
          el.selectionEnd = applied.cursor;
        }
      });
      setCursor(applied.cursor);
    },
    [commands, completions, completion.tokenStart, completion.tokenEnd],
  );

  const syncCursor = useCallback(() => {
    const el = textareaRef.current;
    if (el) setCursor(el.selectionStart ?? 0);
  }, []);

  const handleExecute = useCallback(async () => {
    const trimmed = commands.trim();
    if (!trimmed || running) return;

    const level = classifyDangerLevel(trimmed);
    const firstToken = trimmed.split(/\s+/)[0]?.toUpperCase() ?? '';
    if ((firstToken === 'FLUSHDB' || firstToken === 'FLUSHALL') && !allowFlush) {
      setError(t('redis.console.flushBlocked'));
      return;
    }
    const allowed = await gateWrite(level, trimmed);
    if (!allowed) return;

    setRunning(true);
    setError(null);
    setResults([]);
    setActiveResultIdx(0);
    setHistoryState({ index: null, draft: trimmed });

    try {
      const response = await redisCommandInvoke<ExecResponse>('redis', 'exec', {
        dbSessionId,
        dbIndex,
        commands: trimmed,
        nodeAddr,
      });
      setResults(response.results ?? []);
      setHistory(pushConsoleHistory(dbSessionId, trimmed));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }, [commands, dbSessionId, dbIndex, nodeAddr, running, allowFlush, gateWrite, t]);

  const handleKeyDown = useCallback(
    (event: KeyboardEvent<HTMLTextAreaElement>) => {
      const isMod = event.metaKey || event.ctrlKey;

      if (completionOpen) {
        if (event.key === 'ArrowDown') {
          event.preventDefault();
          setCompletionActive((idx) => (idx + 1) % completions.length);
          return;
        }
        if (event.key === 'ArrowUp') {
          event.preventDefault();
          setCompletionActive((idx) => (idx - 1 + completions.length) % completions.length);
          return;
        }
        if (event.key === 'Tab' || (event.key === 'Enter' && !isMod)) {
          event.preventDefault();
          acceptCompletion(completionActive);
          return;
        }
        if (event.key === 'Escape') {
          event.preventDefault();
          setCompletionDismissed(true);
          return;
        }
      }

      if (isMod && event.key === 'Enter') {
        event.preventDefault();
        void handleExecute();
        return;
      }

      if (event.key === 'ArrowUp' && !isMod) {
        const el = event.currentTarget;
        const atLineStart =
          el.selectionStart === el.selectionEnd &&
          el.selectionStart === commands.lastIndexOf('\n', Math.max(0, el.selectionStart - 1)) + 1;
        if (atLineStart || historyState.index !== null) {
          event.preventDefault();
          const next = navigateConsoleHistory(history, historyState, 'up');
          setHistoryState({ index: next.index, draft: next.draft });
          setCommands(next.text);
          return;
        }
      }

      if (event.key === 'ArrowDown' && !isMod) {
        if (historyState.index !== null) {
          event.preventDefault();
          const next = navigateConsoleHistory(history, historyState, 'down');
          setHistoryState({ index: next.index, draft: next.draft });
          setCommands(next.text);
          return;
        }
      }
    },
    [
      completionOpen,
      completions.length,
      completionActive,
      acceptCompletion,
      commands,
      handleExecute,
      history,
      historyState,
    ],
  );

  const activeResult = results[activeResultIdx];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3">
        <Button
          variant="run"
          className="h-7 gap-1 px-2 text-xs"
          data-testid="redis-console-run"
          onClick={() => void handleExecute()}
          disabled={running || !commands.trim()}
        >
          {t('query.execute')}
        </Button>
        <span className="text-[11px] text-fg-muted">{t('redis.console.hint')}</span>
        {commands.trim() && (
          <span
            className={cn(
              'shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium',
              dangerBadgeColor(classifyDangerLevel(commands.trim())),
            )}
            data-testid="redis-console-danger-badge"
          >
            {dangerLevelLabel(classifyDangerLevel(commands.trim()), t)}
          </span>
        )}
        <div className="flex-1" />
        <SafeModeBadge />
        <ClusterNodePicker
          dbSessionId={dbSessionId}
          compact
          value={pinnedNodeAddr}
          onChange={onPinnedNodeAddrChange}
        />
      </div>

      <div className="relative min-h-[100px] border-b border-edge" style={{ height: '30%' }}>
        <textarea
          ref={textareaRef}
          value={commands}
          data-testid="redis-console-input"
          onChange={(e) => {
            setCommands(e.target.value);
            setHistoryState((prev) =>
              prev.index === null ? { ...prev, draft: e.target.value } : prev,
            );
            setCursor(e.target.selectionStart ?? 0);
          }}
          onClick={syncCursor}
          onKeyUp={syncCursor}
          onKeyDown={handleKeyDown}
          spellCheck={false}
          placeholder={t('redis.console.placeholder')}
          className="h-full w-full resize-none bg-surface px-4 py-3 text-[13px] text-fg outline-none"
          style={{ fontFamily: `${fontFamily}, ui-monospace, SFMono-Regular, Menlo, monospace` }}
        />
        {completionOpen && (
          <CompletionPopup
            items={completions}
            activeIndex={completionActive}
            loading={completion.loading}
            onHover={(index) => setCompletionActive(index)}
            onAccept={acceptCompletion}
          />
        )}
      </div>

      <div className="flex min-h-0 flex-1 flex-col">
        {running && (
          <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
            <Loader2 className="h-5 w-5 animate-spin" />
            {t('query.executing')}
          </div>
        )}

        {error && !running && (
          <div className="flex-1 overflow-auto p-4">
            <div className="rounded-md border border-danger/20 bg-danger/10 px-4 py-3 text-sm text-danger">
              {error}
            </div>
          </div>
        )}

        {results.length > 0 && !running && (
          <>
            {results.length > 1 && (
              <div className="flex shrink-0 items-center border-b border-edge bg-surface-alt px-1">
                {results.map((result, idx) => (
                  <button
                    key={`${result.command}-${idx}`}
                    type="button"
                    className={cn(
                      'relative max-w-[220px] truncate border-l-2 px-3 py-1.5 text-xs transition-colors',
                      dangerBorderClass(classifyDangerLevel(result.command)),
                      idx === activeResultIdx
                        ? 'text-fg font-medium'
                        : 'text-fg-muted hover:text-fg-secondary',
                    )}
                    title={result.command}
                    onClick={() => setActiveResultIdx(idx)}
                  >
                    {t('query.result')} {idx + 1}
                    <span
                      className={cn(
                        'ml-1.5 text-[10px]',
                        result.ok ? 'text-success/80' : 'text-danger',
                      )}
                    >
                      {result.ok ? 'OK' : 'ERR'}
                    </span>
                    <span
                      className={cn(
                        'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                        idx === activeResultIdx ? 'opacity-100' : 'opacity-0',
                      )}
                    />
                  </button>
                ))}
              </div>
            )}

            {activeResult && (
              <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                <div
                  className={cn(
                    'flex items-center gap-3 border-b border-l-2 border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary',
                    dangerBorderClass(classifyDangerLevel(activeResult.command)),
                  )}
                >
                  <span className="font-mono">{activeResult.command}</span>
                  <span className="text-edge">|</span>
                  <span className={activeResult.ok ? 'text-success/90' : 'text-danger'}>
                    {activeResult.ok ? t('redis.console.ok') : t('redis.console.failed')}
                  </span>
                </div>
                <div
                  className="min-h-0 flex-1 overflow-auto p-4"
                  data-testid="redis-console-result"
                >
                  {activeResult.ok ? (
                    <pre className="whitespace-pre-wrap break-all font-mono text-[13px] text-fg-secondary">
                      {activeResult.value ?? '(nil)'}
                    </pre>
                  ) : (
                    <div className="rounded-md border border-danger/20 bg-danger/10 px-4 py-3 text-sm text-danger">
                      {activeResult.error}
                    </div>
                  )}
                </div>
              </div>
            )}
          </>
        )}

        {results.length === 0 && !running && !error && (
          <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
            {t('redis.console.empty')}
          </div>
        )}
      </div>
      {gateDialog}
    </div>
  );
}
