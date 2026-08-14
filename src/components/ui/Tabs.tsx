import type { ReactNode } from 'react';
import { cn } from '../../lib/cn';

export interface TabItem {
  id: string;
  label: ReactNode;
  content: ReactNode;
}

export interface TabsProps {
  items: TabItem[];
  activeId: string;
  onChange: (id: string) => void;
  /** Extra element rendered at the end of the tab bar (e.g. "+" button). */
  trailing?: ReactNode;
  className?: string;
}

export function Tabs({ items, activeId, onChange, trailing, className }: TabsProps) {
  const active = items.find((i) => i.id === activeId) ?? items[0];

  return (
    <div className={cn('flex min-h-0 min-w-0 flex-1 flex-col', className)}>
      <div className="flex h-10 shrink-0 items-center gap-1 overflow-x-auto border-b border-edge bg-surface-alt px-2">
        {items.map((item) => {
          const selected = item.id === active?.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onChange(item.id)}
              className={cn(
                'shrink-0 h-8 rounded-md px-3 text-xs font-medium transition-colors',
                selected
                  ? 'bg-surface text-fg border border-edge'
                  : 'text-fg-muted hover:text-fg hover:bg-surface-raised/40',
              )}
            >
              {item.label}
            </button>
          );
        })}
        {trailing}
      </div>
      <div className="min-h-0 min-w-0 flex-1 overflow-hidden">{active?.content}</div>
    </div>
  );
}
