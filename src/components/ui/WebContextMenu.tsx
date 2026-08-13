import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type Ref,
} from 'react';
import { createPortal } from 'react-dom';
import { ChevronRight } from 'lucide-react';
import { cn } from '../../lib/cn';
import { positionRootMenu, positionSubmenu } from '../../lib/contextMenuPosition';
import { runPredefinedEdit } from '../../lib/runPredefinedEdit';
import type { NativeMenuItemDef } from '../../lib/nativeContextMenu';
import { useContextMenuStore } from '../../stores/contextMenuStore';

function isSeparator(item: NativeMenuItemDef): boolean {
  return item.kind === 'separator' || (item.kind === 'predefined' && item.item === 'Separator');
}

function itemLabel(item: NativeMenuItemDef): string {
  if (item.kind === 'item') return item.label;
  if (item.kind === 'submenu') return item.label;
  if (item.kind === 'predefined') return item.text ?? item.item;
  return '';
}

function MenuPanel({
  items,
  testId,
  style,
  panelRef,
  onItemEnter,
  openSubmenuId,
}: {
  items: NativeMenuItemDef[];
  testId: string;
  style: CSSProperties;
  panelRef?: Ref<HTMLDivElement>;
  onItemEnter?: (id: string, el: HTMLElement) => void;
  openSubmenuId?: string | null;
}) {
  const hide = useContextMenuStore((s) => s.hide);

  return (
    <div
      ref={panelRef}
      data-testid={testId}
      role="menu"
      className="fixed z-[10000] min-w-[180px] rounded-lg border border-edge bg-surface-alt py-1 shadow-xl"
      style={style}
    >
      {items.map((item, idx) => {
        if (isSeparator(item)) {
          return (
            <div
              key={item.kind === 'separator' ? item.id ?? `sep-${idx}` : `pre-sep-${idx}`}
              role="separator"
              className="my-1 h-px bg-edge"
            />
          );
        }
        if (item.kind === 'submenu') {
          const id = item.id ?? `sub-${idx}`;
          return (
            <button
              key={id}
              type="button"
              role="menuitem"
              data-testid={`web-context-submenu-trigger-${id}`}
              className={cn(
                'flex w-full items-center justify-between gap-3 px-3 py-1.5 text-left text-sm text-fg',
                'hover:bg-surface-raised',
                openSubmenuId === id && 'bg-surface-raised',
              )}
              onMouseEnter={(e) => onItemEnter?.(id, e.currentTarget)}
              onFocus={(e) => onItemEnter?.(id, e.currentTarget)}
            >
              <span className="min-w-0 truncate">{item.label}</span>
              <ChevronRight className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
            </button>
          );
        }
        const enabled = item.kind === 'item' ? item.enabled !== false : true;
        const label = itemLabel(item);
        return (
          <button
            key={item.kind === 'item' ? item.id : `pre-${item.item}-${idx}`}
            type="button"
            role="menuitem"
            disabled={!enabled}
            data-testid={item.kind === 'item' ? `web-context-item-${item.id}` : `web-context-pre-${item.item}`}
            className={cn(
              'flex w-full items-center px-3 py-1.5 text-left text-sm',
              enabled ? 'text-fg hover:bg-surface-raised' : 'cursor-not-allowed text-fg-muted opacity-50',
            )}
            onMouseEnter={() => onItemEnter?.('', document.body)}
            onClick={() => {
              if (!enabled) return;
              hide();
              if (item.kind === 'item') void item.action();
              else runPredefinedEdit(item.item);
            }}
          >
            {label}
          </button>
        );
      })}
    </div>
  );
}

export function WebContextMenuHost() {
  const { open, x, y, items, hide } = useContextMenuStore();
  const rootRef = useRef<HTMLDivElement>(null);
  const subRef = useRef<HTMLDivElement>(null);
  const [rootPos, setRootPos] = useState({ left: x, top: y });
  const [sub, setSub] = useState<{
    id: string;
    items: NativeMenuItemDef[];
    itemRect: { left: number; top: number; width: number; height: number };
    pos: { left: number; top: number };
  } | null>(null);

  useLayoutEffect(() => {
    if (!open) {
      setSub(null);
      return;
    }
    const el = rootRef.current;
    const width = el?.offsetWidth ?? 180;
    const height = el?.offsetHeight ?? 40;
    setRootPos(
      positionRootMenu(
        { x, y },
        { width, height },
        { width: window.innerWidth, height: window.innerHeight },
      ),
    );
  }, [open, x, y, items]);

  useLayoutEffect(() => {
    if (!sub) return;
    const el = subRef.current;
    const width = el?.offsetWidth ?? 160;
    const height = el?.offsetHeight ?? 40;
    const pos = positionSubmenu(sub.itemRect, { width, height }, {
      width: window.innerWidth,
      height: window.innerHeight,
    });
    if (pos.left !== sub.pos.left || pos.top !== sub.pos.top) {
      setSub((prev) => (prev ? { ...prev, pos: { left: pos.left, top: pos.top } } : prev));
    }
  }, [sub]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') hide();
    };
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node | null;
      if (rootRef.current?.contains(t) || subRef.current?.contains(t)) return;
      hide();
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('mousedown', onDown);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('mousedown', onDown);
    };
  }, [open, hide]);

  const onItemEnter = useCallback(
    (id: string, el: HTMLElement) => {
      if (!id) {
        setSub(null);
        return;
      }
      const def = items.find((it, idx) => it.kind === 'submenu' && (it.id ?? `sub-${idx}`) === id);
      if (!def || def.kind !== 'submenu') {
        setSub(null);
        return;
      }
      const rect = el.getBoundingClientRect();
      setSub({
        id,
        items: def.items,
        itemRect: { left: rect.left, top: rect.top, width: rect.width, height: rect.height },
        pos: { left: rect.right, top: rect.top },
      });
    },
    [items],
  );

  if (!open || typeof document === 'undefined') return null;

  return createPortal(
    <>
      <MenuPanel
        items={items}
        testId="web-context-menu"
        panelRef={rootRef}
        style={{ left: rootPos.left, top: rootPos.top }}
        onItemEnter={onItemEnter}
        openSubmenuId={sub?.id ?? null}
      />
      {sub && (
        <MenuPanel
          items={sub.items}
          testId="web-context-submenu"
          panelRef={subRef}
          style={{ left: sub.pos.left, top: sub.pos.top }}
        />
      )}
    </>,
    document.body,
  );
}
