import { forwardRef, type ReactNode } from 'react';
import type { HTMLAttributes } from 'react';
import { cn } from '../../lib/cn';

export interface ToolbarShellProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

export const ToolbarShell = forwardRef<HTMLDivElement, ToolbarShellProps>(function ToolbarShell(
  { children, className, ...props },
  ref,
) {
  return (
    <div
      ref={ref}
      className={cn(
        'flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-4 py-1',
        className,
      )}
      {...props}
    >
      {children}
    </div>
  );
});
