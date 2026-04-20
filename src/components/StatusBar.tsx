import type { ReactNode } from 'react';
import { cn } from '../lib/cn';

export interface StatusBarProps {
  left?: ReactNode;
  right?: ReactNode;
  className?: string;
}

export function StatusBar({ left, right, className }: StatusBarProps) {
  return (
    <footer
      className={cn(
        'flex h-10 min-h-[40px] shrink-0 items-center justify-between gap-3 border-t border-edge bg-surface-alt px-4 text-xs text-fg-secondary',
        className,
      )}
    >
      <div className="min-w-0 truncate">{left}</div>
      <div className="shrink-0">{right}</div>
    </footer>
  );
}
