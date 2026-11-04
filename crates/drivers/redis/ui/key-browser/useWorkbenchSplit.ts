import { useCallback, useState, type MouseEvent as ReactMouseEvent } from 'react';

/**
 * Draggable tree/detail split of 屏 B (PRD §3.2 left column width).
 *
 * State machine (AGENTS.md 「状态机三要素」):
 *  - enter: `pointerdown` on the separator;
 *  - during: every `pointermove` clamps the new width into [{@link MIN_TREE_WIDTH},
 *    {@link MAX_TREE_WIDTH}];
 *  - exit: `pointerup` removes **both** listeners, so a release anywhere
 *    (including outside the window's tree pane) ends the drag — no one-way latch.
 *
 * The width persists to localStorage so the user's preferred split survives
 * across sessions.
 */

export const DEFAULT_TREE_WIDTH = 360;
export const MIN_TREE_WIDTH = 220;
export const MAX_TREE_WIDTH = 900;
const STORAGE_KEY = 'redis-workbench-split-width';

export function clampTreeWidth(next: number): number {
  if (!Number.isFinite(next)) return DEFAULT_TREE_WIDTH;
  return Math.min(MAX_TREE_WIDTH, Math.max(MIN_TREE_WIDTH, Math.round(next)));
}

function readPersistedWidth(): number {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw !== null) {
      const n = Number(raw);
      if (Number.isFinite(n)) return clampTreeWidth(n);
    }
  } catch {
    // localStorage unavailable (SSR / test env) — ignore.
  }
  return DEFAULT_TREE_WIDTH;
}

function persistWidth(width: number): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(width));
  } catch {
    // localStorage unavailable — ignore.
  }
}

export function useWorkbenchSplit(initial?: number) {
  const [treeWidth, setTreeWidth] = useState(() => clampTreeWidth(initial ?? readPersistedWidth()));

  const updateWidth = useCallback((next: number) => {
    const clamped = clampTreeWidth(next);
    setTreeWidth(clamped);
    persistWidth(clamped);
  }, []);

  const startSplitDrag = useCallback(
    (e: ReactMouseEvent) => {
      e.preventDefault();
      const startX = e.clientX;
      const startWidth = treeWidth;
      const onMove = (ev: globalThis.MouseEvent) => {
        updateWidth(startWidth + ev.clientX - startX);
      };
      const onUp = () => {
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
      };
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
    },
    [treeWidth, updateWidth],
  );

  return { treeWidth, setTreeWidth: updateWidth, startSplitDrag };
}
