const STORAGE_PREFIX = 'datazen:redis-console-history:';
const MAX_ENTRIES = 200;

function storageKey(scope: string): string {
  return `${STORAGE_PREFIX}${scope}`;
}

/** Load persisted command history for one tab scope (newest first). */
export function loadConsoleHistory(scope: string): string[] {
  try {
    const raw = localStorage.getItem(storageKey(scope));
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((entry): entry is string => typeof entry === 'string');
  } catch {
    return [];
  }
}

/** Persist command history for one tab scope. */
export function saveConsoleHistory(scope: string, entries: string[]): void {
  try {
    localStorage.setItem(storageKey(scope), JSON.stringify(entries.slice(0, MAX_ENTRIES)));
  } catch {
    // localStorage may be unavailable in tests or private mode
  }
}

/** Append a command to history (dedupe, newest first). Returns updated list. */
export function pushConsoleHistory(scope: string, command: string): string[] {
  const trimmed = command.trim();
  if (!trimmed) return loadConsoleHistory(scope);

  const existing = loadConsoleHistory(scope).filter((entry) => entry !== trimmed);
  const next = [trimmed, ...existing].slice(0, MAX_ENTRIES);
  saveConsoleHistory(scope, next);
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
