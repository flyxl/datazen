const STORAGE_PREFIX = 'datazen:redis-console-history:';
const MAX_ENTRIES = 200;

function storageKey(dbSessionId: string): string {
  return `${STORAGE_PREFIX}${dbSessionId}`;
}

/** Load persisted command history for a connection (newest first). */
export function loadConsoleHistory(dbSessionId: string): string[] {
  try {
    const raw = localStorage.getItem(storageKey(dbSessionId));
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((entry): entry is string => typeof entry === 'string');
  } catch {
    return [];
  }
}

/** Persist command history for a connection. */
export function saveConsoleHistory(dbSessionId: string, entries: string[]): void {
  try {
    localStorage.setItem(storageKey(dbSessionId), JSON.stringify(entries.slice(0, MAX_ENTRIES)));
  } catch {
    // localStorage may be unavailable in tests or private mode
  }
}

/** Append a command to history (dedupe, newest first). Returns updated list. */
export function pushConsoleHistory(dbSessionId: string, command: string): string[] {
  const trimmed = command.trim();
  if (!trimmed) return loadConsoleHistory(dbSessionId);

  const existing = loadConsoleHistory(dbSessionId).filter((entry) => entry !== trimmed);
  const next = [trimmed, ...existing].slice(0, MAX_ENTRIES);
  saveConsoleHistory(dbSessionId, next);
  return next;
}

export interface HistoryNavigationState {
  index: number | null;
  draft: string;
}

/** Navigate command history with ↑/↓ (index null = editing draft). */
export function navigateConsoleHistory(
  history: readonly string[],
  state: HistoryNavigationState,
  direction: 'up' | 'down',
): HistoryNavigationState & { text: string } {
  if (history.length === 0) {
    return { ...state, text: state.draft };
  }

  if (direction === 'up') {
    const nextIndex = state.index === null ? 0 : Math.min(state.index + 1, history.length - 1);
    return {
      index: nextIndex,
      draft: state.draft,
      text: history[nextIndex] ?? state.draft,
    };
  }

  if (state.index === null || state.index === 0) {
    return { index: null, draft: state.draft, text: state.draft };
  }

  const nextIndex = state.index - 1;
  return {
    index: nextIndex,
    draft: state.draft,
    text: history[nextIndex] ?? state.draft,
  };
}
