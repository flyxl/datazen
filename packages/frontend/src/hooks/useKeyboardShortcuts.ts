import { useEffect, useRef } from 'react';

export interface ShortcutDef {
  key: string;
  scope: 'global' | 'editor' | 'table';
  action: () => void;
  description: string;
}

function matchShortcut(
  def: string,
  ctx: { mod: boolean; shift: boolean; alt: boolean; key: string },
): boolean {
  const parts = def.toLowerCase().split('+').map((p) => p.trim());
  let needsMod = false;
  let needsShift = false;
  let needsAlt = false;
  let key = '';

  for (const p of parts) {
    if (p === 'mod') needsMod = true;
    else if (p === 'shift') needsShift = true;
    else if (p === 'alt') needsAlt = true;
    else key = p;
  }

  if (needsMod !== ctx.mod) return false;
  if (needsShift !== ctx.shift) return false;
  if (needsAlt !== ctx.alt) return false;

  if (key === 'escape') return ctx.key === 'escape';
  if (key === 'delete') return ctx.key === 'delete' || ctx.key === 'backspace';
  if (key === 'enter') return ctx.key === 'enter';
  if (key === 'space') return ctx.key === ' ';

  return ctx.key === key;
}

export function useKeyboardShortcuts(shortcuts: ShortcutDef[]) {
  const shortcutsRef = useRef(shortcuts);
  shortcutsRef.current = shortcuts;

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName?.toLowerCase();
      const inField =
        tag === 'input' || tag === 'textarea' || target?.isContentEditable === true;

      const mod = e.metaKey || e.ctrlKey;
      const shift = e.shiftKey;
      const alt = e.altKey;
      const key = e.key.toLowerCase();

      for (const shortcut of shortcutsRef.current) {
        if (shortcut.scope !== 'global' && inField && shortcut.scope === 'table') continue;
        if (shortcut.scope === 'editor' && inField === false) continue;

        if (matchShortcut(shortcut.key, { mod, shift, alt, key })) {
          e.preventDefault();
          shortcut.action();
          return;
        }
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);
}
