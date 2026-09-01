import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { Loader2 } from 'lucide-react';
import { Button } from '../../../../src/components/ui/Button';
import { useI18n } from '../../../../src/hooks/useI18n';
import { useSettingsStore } from '../../../../src/stores/settingsStore';
import { cn } from '../../../../src/lib/cn';
import {
  resolveEditorFontFamily,
  HOST_DEFAULT_EDITOR_FONT,
} from '../../../../src/lib/resolveEditorFontFamily';
import { redisCommandInvoke } from './redisInvoke';
import {
  loadConsoleHistory,
  navigateConsoleHistory,
  pushConsoleHistory,
  type HistoryNavigationState,
} from './consoleHistory';
import { filterCompletions, getCompletionPrefix, REDIS_COMMANDS } from './redisCommands';
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

function applyCompletion(
  text: string,
  cursor: number,
  completion: string,
): { text: string; cursor: number } {
  const prefix = getCompletionPrefix(text, cursor);
  if (!prefix) return { text, cursor };

  const lineStart = text.lastIndexOf('\n', Math.max(0, cursor - 1)) + 1;
  const linePrefix = text.slice(lineStart, cursor);
  const tokenStart = lineStart + linePrefix.length - prefix.length;
  const nextText = `${text.slice(0, tokenStart)}${completion}${text.slice(cursor)}`;
  const nextCursor = tokenStart + completion.length;
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
  const pluginSettings = useSettingsStore((s) => s.settings.pluginSettings);
  const clusterRouting = readClusterRouting(pluginSettings?.redis);
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
  const [completionIdx, setCompletionIdx] = useState(0);

  useEffect(() => {
    setHistory(loadConsoleHistory(dbSessionId));
    setCommands('');
    setResults([]);
    setError(null);
    setHistoryState({ index: null, draft: '' });
  }, [dbSessionId]);

  const completionPrefix = useMemo(() => getCompletionPrefix(commands, cursor), [commands, cursor]);

  const completions = useMemo(
    () => filterCompletions(completionPrefix, REDIS_COMMANDS, keySuggestions),
    [completionPrefix, keySuggestions],
  );

  useEffect(() => {
    setCompletionIdx(0);
  }, [completionPrefix, completions.length]);

  const syncCursor = useCallback(() => {
    const el = textareaRef.current;
    if (el) setCursor(el.selectionStart ?? 0);
  }, []);

  const handleExecute = useCallback(async () => {
    const trimmed = commands.trim();
    if (!trimmed || running) return;

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
  }, [commands, dbSessionId, dbIndex, nodeAddr, running]);

  const handleKeyDown = useCallback(
    (event: KeyboardEvent<HTMLTextAreaElement>) => {
      const isMod = event.metaKey || event.ctrlKey;

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

      if (event.key === 'Tab' && completions.length > 0) {
        event.preventDefault();
        const pick = completions[completionIdx] ?? completions[0];
        const applied = applyCompletion(commands, cursor, pick);
        setCommands(applied.text);
        requestAnimationFrame(() => {
          const el = textareaRef.current;
          if (el) {
            el.selectionStart = applied.cursor;
            el.selectionEnd = applied.cursor;
            setCursor(applied.cursor);
          }
        });
        return;
      }

      if (event.key === 'ArrowDown' && completions.length > 0 && event.altKey) {
        event.preventDefault();
        setCompletionIdx((idx) => (idx + 1) % completions.length);
        return;
      }

      if (event.key === 'ArrowUp' && completions.length > 0 && event.altKey) {
        event.preventDefault();
        setCompletionIdx((idx) => (idx - 1 + completions.length) % completions.length);
      }
    },
    [commands, completionIdx, completions, cursor, handleExecute, history, historyState],
  );

  const activeResult = results[activeResultIdx];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3">
        <Button
          variant="run"
          className="h-7 gap-1 px-2 text-xs"
          onClick={() => void handleExecute()}
          disabled={running || !commands.trim()}
        >
          {t('query.execute')}
        </Button>
        <span className="text-[11px] text-fg-muted">{t('redis.console.hint')}</span>
        <div className="flex-1" />
        <ClusterNodePicker
          dbSessionId={dbSessionId}
          compact
          value={pinnedNodeAddr}
          onChange={onPinnedNodeAddrChange}
        />
        {completions.length > 0 && completionPrefix && (
          <span className="max-w-[240px] truncate text-[11px] text-fg-muted">
            {completions[completionIdx] ?? completions[0]}
          </span>
        )}
      </div>

      <div className="relative min-h-[100px] border-b border-edge" style={{ height: '30%' }}>
        <textarea
          ref={textareaRef}
          value={commands}
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
                      'relative max-w-[220px] truncate px-3 py-1.5 text-xs transition-colors',
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
                <div className="flex items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
                  <span className="font-mono">{activeResult.command}</span>
                  <span className="text-edge">|</span>
                  <span className={activeResult.ok ? 'text-success/90' : 'text-danger'}>
                    {activeResult.ok ? t('redis.console.ok') : t('redis.console.failed')}
                  </span>
                </div>
                <div className="min-h-0 flex-1 overflow-auto p-4">
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
    </div>
  );
}
