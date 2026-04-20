import { useCallback, useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { cn } from '../../lib/cn';

export interface ContextMenuItem {
  id: string;
  label: string;
  icon?: ReactNode;
  disabled?: boolean;
  danger?: boolean;
  separator?: false;
}

export interface ContextMenuSeparator {
  id: string;
  separator: true;
}

export type ContextMenuEntry = ContextMenuItem | ContextMenuSeparator;

interface ContextMenuProps {
  items: ContextMenuEntry[];
  onAction: (id: string) => void;
  children: ReactNode;
}

export function ContextMenu({ items, onAction, children }: ContextMenuProps) {
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    if (e.ctrlKey && e.button === 0) return;
    setPos({ x: e.clientX, y: e.clientY });
  }, []);

  const close = useCallback(() => setPos(null), []);

  useEffect(() => {
    if (!pos) return;
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        close();
      }
    };
    const escHandler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
    };
    window.addEventListener('mousedown', handler);
    window.addEventListener('keydown', escHandler);
    return () => {
      window.removeEventListener('mousedown', handler);
      window.removeEventListener('keydown', escHandler);
    };
  }, [pos, close]);

  useEffect(() => {
    if (!pos || !menuRef.current) return;
    const rect = menuRef.current.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let { x, y } = pos;
    if (x + rect.width > vw) x = vw - rect.width - 4;
    if (y + rect.height > vh) y = vh - rect.height - 4;
    if (x !== pos.x || y !== pos.y) setPos({ x, y });
  }, [pos]);

  return (
    <>
      <div onContextMenu={handleContextMenu} className="flex min-h-0 min-w-0 flex-1 flex-col">{children}</div>
      {pos &&
        createPortal(
          <div
            ref={menuRef}
            className="fixed z-[9999] min-w-[180px] rounded-lg border border-edge bg-surface-alt py-1 shadow-xl"
            style={{ left: pos.x, top: pos.y }}
          >
            {items.map((item) => {
              if (item.separator) {
                return <div key={item.id} className="my-1 h-px bg-edge" />;
              }
              return (
                <button
                  key={item.id}
                  type="button"
                  disabled={item.disabled}
                  className={cn(
                    'flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] transition-colors',
                    item.disabled
                      ? 'cursor-not-allowed text-fg-muted'
                      : item.danger
                        ? 'text-red-400 hover:bg-red-500/10'
                        : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                  )}
                  onClick={() => {
                    if (!item.disabled) {
                      onAction(item.id);
                      close();
                    }
                  }}
                >
                  {item.icon && <span className="flex h-4 w-4 shrink-0 items-center justify-center">{item.icon}</span>}
                  {item.label}
                </button>
              );
            })}
          </div>,
          document.body,
        )}
    </>
  );
}
