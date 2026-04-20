import type { ReactNode } from 'react';
import { cn } from '../lib/cn';

export interface ToolbarProps {
  left?: ReactNode;
  right?: ReactNode;
  className?: string;
}

export function Toolbar({ left, right, className }: ToolbarProps) {
  return (
    <div
      className={cn(
        'flex h-12 min-h-[48px] shrink-0 items-center justify-between gap-3 border-b border-edge bg-surface-alt px-4',
        className,
      )}
    >
      <div className="flex min-w-0 flex-1 items-center gap-2">{left}</div>
      <div className="flex shrink-0 items-center gap-2">{right}</div>
    </div>
  );
}
