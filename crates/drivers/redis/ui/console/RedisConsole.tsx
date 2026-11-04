import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { useI18n, Spinner } from '@datazen/ui';
import {
  useBoundSettingsStore,
  resolveEditorFontFamily,
  HOST_DEFAULT_EDITOR_FONT,
  readBooleanField,
} from '@datazen/driver-sdk';
import { redisCommandInvoke } from '../shared/redisInvoke';
import { classifyDangerLevel, isFlushCommand } from './redisConsoleDanger';
import {
  assessCommands,
  badgeAssessment,
  composeBlockedMessage,
  describeCommands,
} from './consoleCommandBatch';
import { RedisConsoleDangerBadge } from './RedisConsoleDangerBadge';
import { inferResultType, type ConsoleResultItem } from './consoleResultRenderer';
import { ConsoleTranscriptView } from './ConsoleTranscriptView';
import {
  appendCommand,
  appendError,
  appendResult,
  setDraft,
  useTranscriptDraft,
  useTranscriptEntries,
} from './consoleTranscript';
import { useRedisGate } from '../shared/useRedisGate';
import { SafeModeBadge } from '../shared/SafeModeBadge';
import {
  loadConsoleHistory,
  navigateConsoleHistory,
  pushConsoleHistory,
  type HistoryNavigationState,
} from './consoleHistory';
import { useCompletion } from './consoleCompletion/useCompletion';
import { CompletionPopup } from './consoleCompletion/CompletionPopup';
import { ClusterNodePicker } from '../connection/ClusterNodePicker';
import { readClusterRouting, resolvePinnedNodeAddr } from '../connection/settingsHelpers';

export interface RedisConsoleProps {
  dbSessionId: string;
  /**
   * Identity of the owning top-level tab (`shared/panelId.ts`). The transcript
   * and the ↑↓ recall list are keyed by this, never by `dbSessionId`, which every
   * db tab of one connection shares — otherwise a sibling db would open showing
   * another db's scrollback. `dbSessionId` stays the IPC session for the wire.
   */
  panelId: string;
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
  /** Server-classified result shape (`scalar` | `array` | `map` | `ok` | `nil` | `error`). */
  resultType?: string;
  /** Server-side danger level, echoed for parity with the client classifier. */
  dangerLevel?: string;
}

interface ExecResponse {
  results: ExecResult[];
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

const RESULT_TYPES: readonly string[] = ['scalar', 'array', 'map', 'ok', 'nil', 'error'];

/**
 * Map one server `exec` result onto the pure renderer's props. The server's own
 * `resultType` wins; `inferResultType` is only the fallback for servers that do
 * not send the field (P0-3: the Console used to drop it and re-guess shapes).
 */
function toConsoleResultItem(result: ExecResult): ConsoleResultItem {
  const fromServer = result.resultType && RESULT_TYPES.includes(result.resultType);
  return {
    command: result.command,
    ok: result.ok,
    value: result.value,
    error: result.error,
    resultType: (fromServer
      ? result.resultType
      : inferResultType(result.value)) as ConsoleResultItem['resultType'],
    dangerLevel: classifyDangerLevel(result.command),
  };
}

export function RedisConsole({
  dbSessionId,
  panelId,
  dbIndex = 0,
  keySuggestions = [],
  pinnedNodeAddr = '',
  onPinnedNodeAddrChange,
}: RedisConsoleProps) {
  const { t } = useI18n();
  const driverSettings = useBoundSettingsStore((s) => s.settings.driverSettings);
  const clusterRouting = readClusterRouting(driverSettings?.redis);
  const allowFlush = readBooleanField(
    (driverSettings?.redis ?? {}) as Record<string, unknown>,
    'allowFlush',
    false,
  );
  const { gateWrite, gateDialog } = useRedisGate();
  const nodeAddr = resolvePinnedNodeAddr(clusterRouting, pinnedNodeAddr);
  const editorFontFamily = useBoundSettingsStore(
    (s) => s.settings.editorFontFamily || HOST_DEFAULT_EDITOR_FONT,
  );
  const fontFamily = resolveEditorFontFamily(editorFontFamily, '', HOST_DEFAULT_EDITOR_FONT);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // The input draft and the scrollback both live in the module-level transcript
  // store, not in component state: the host unmounts this whole subtree every
  // time the user switches panel, and component state would go with it.
  const commands = useTranscriptDraft(panelId);
  const setCommands = useCallback((next: string) => setDraft(panelId, next), [panelId]);
  const entries = useTranscriptEntries(panelId);
  const [running, setRunning] = useState(false);
  // Caret position stays in component state: it is view state, and losing it on
  // a remount just means the caret returns to where the restored draft begins.
  const [cursor, setCursor] = useState(0);
  const [historyState, setHistoryState] = useState<HistoryNavigationState>({
    index: null,
    draft: '',
  });
  const [history, setHistory] = useState<string[]>([]);
  const [completionActive, setCompletionActive] = useState(0);
  const [completionDismissed, setCompletionDismissed] = useState(false);

  useEffect(() => {
    // Command history is per-session and persisted; the transcript is not
    // reset here because that would wipe the scrollback on every remount.
    setHistory(loadConsoleHistory(panelId));
    setHistoryState({ index: null, draft: '' });
  }, [panelId]);

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

    // PRD I-7: classify the whole batch. A multi-line paste used to be graded by
    // its first line only, which let `GET a\nDEL b` through with no brake at all.
    const batch = assessCommands(trimmed, allowFlush);

    if (batch.blocked.length > 0) {
      // Refuse before the gate: a confirmation dialog is not a release valve for
      // the blocked tier (task book §1.2 "不可仅弹确认放行"). FLUSHDB/FLUSHALL
      // keep their dedicated copy, which explains the Allow Flush opt-in.
      const flushOnly = batch.blocked.every((command) => isFlushCommand(command.name));
      appendError(
        panelId,
        flushOnly ? t('redis.console.flushBlocked') : composeBlockedMessage(batch, t),
      );
      return;
    }

    // R-3 (task book §6.1): one confirmation for the whole batch, listing every
    // danger-tier-and-above command, then a second one for the surviving
    // ultra-danger commands (FLUSHDB/FLUSHALL with the allowFlush opt-in).
    const confirmTargets = [...batch.confirmations, ...batch.doubleConfirmations];
    if (confirmTargets.length > 0) {
      const listing = describeCommands(confirmTargets);
      if (!(await gateWrite('danger', listing))) return;
      for (const command of batch.doubleConfirmations) {
        if (!(await gateWrite('ultra-danger', command.raw))) return;
      }
    } else if (batch.worst !== 'safe') {
      // Pure write / read batch: still route through the gate so Safe Mode can
      // refuse the write path (I-6 semantics, unchanged).
      if (!(await gateWrite(batch.worst, trimmed))) return;
    }

    setRunning(true);
    setHistoryState({ index: null, draft: trimmed });

    // Echo the command into the scrollback *before* the round trip, so the
    // transcript reads like a real session rather than a result slot. A declined
    // confirmation returns above and leaves no echo, matching shell behaviour
    // where Ctrl-C does not print the line.
    appendCommand(panelId, trimmed, dbIndex);

    try {
      const response = await redisCommandInvoke<ExecResponse>('redis', 'exec', {
        dbSessionId,
        dbIndex,
        commands: trimmed,
        nodeAddr,
      });
      for (const result of response.results ?? []) {
        appendResult(panelId, result.command, toConsoleResultItem(result));
      }
      setHistory(pushConsoleHistory(panelId, trimmed));
    } catch (err) {
      appendError(panelId, err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }, [commands, panelId, dbSessionId, dbIndex, nodeAddr, running, allowFlush, gateWrite, t]);

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
        if (event.key === 'Tab') {
          event.preventDefault();
          acceptCompletion(completionActive);
          return;
        }
        if (event.key === 'Escape') {
          event.preventDefault();
          setCompletionDismissed(true);
          return;
        }
        // Any other key (including Enter) falls through to the handlers below;
        // the popup will close itself once the text/cursor changes.
      }

      // Plain Enter (or Cmd/Ctrl+Enter) executes the command.
      if (event.key === 'Enter' && !event.shiftKey) {
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

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* ── top toolbar ──────────────────────────────────────────────── */}
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3">
        <span className="text-[11px] text-fg-muted">{t('redis.console.hint')}</span>
        {commands.trim() && (
          <RedisConsoleDangerBadge assessment={badgeAssessment(commands, allowFlush)} t={t} />
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

      {/* ── scrollback (scrollable, grows downward) ──────────────────── */}
      <div className="flex min-h-0 flex-1 flex-col">
        <ConsoleTranscriptView entries={entries} />
        {running && (
          <div className="flex shrink-0 items-center gap-2 px-4 py-1 text-xs text-fg-muted">
            <Spinner size="md" />
            {t('query.executing')}
          </div>
        )}
      </div>

      {/* ── input bar (terminal-style, bottom) ───────────────────────── */}
      {/* Exactly one line, always. The `dbN>` prompt and the field share `h-9`
          and the same line-height so their text sits on one baseline row.

          The field is a `textarea` rather than an `input` because a pasted
          multi-line batch has to survive intact for `assessCommands` to grade
          the whole thing (PRD I-7). What keeps it to one line is `wrap="off"`
          plus the height clamp: the browser soft-wraps a textarea by default,
          which is what made the field grow to three-plus lines and start
          scrolling. A long command stays fully reachable by scrolling
          horizontally, which is how a real shell line behaves. */}
      <div className="relative shrink-0 border-t border-edge">
        <div className="flex h-9 items-stretch bg-surface">
          <span
            className="flex shrink-0 items-center pl-3 pr-1 font-mono text-[13px] leading-5 text-accent select-none"
            aria-hidden="true"
          >
            db{dbIndex}&gt;
          </span>
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
            rows={1}
            wrap="off"
            className="h-9 w-full resize-none overflow-y-hidden overflow-x-auto bg-transparent py-2 pr-4 text-[13px] leading-5 text-fg outline-none"
            style={{
              fontFamily: `${fontFamily}, ui-monospace, SFMono-Regular, Menlo, monospace`,
            }}
          />
        </div>
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
      {gateDialog}
    </div>
  );
}
