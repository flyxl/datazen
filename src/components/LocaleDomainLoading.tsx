import { Loader2 } from 'lucide-react';
import { cn } from '../lib/cn';

export interface LocaleDomainLoadingProps {
  /** Full-window shell (Data Sync / Transfer / …) vs compact section (Settings MCP). */
  variant?: 'page' | 'section';
  className?: string;
  testId?: string;
}

/**
 * Placeholder while lazy locale packs load. Avoids flashing raw translation keys.
 */
export function LocaleDomainLoading({
  variant = 'page',
  className,
  testId = 'locale-domain-loading',
}: Readonly<LocaleDomainLoadingProps>) {
  return (
    <div
      className={cn(
        'flex items-center justify-center text-sm text-fg-muted',
        variant === 'page' && 'h-screen min-h-0 flex-col bg-surface text-fg',
        variant === 'section' && 'min-h-[8rem] w-full py-8',
        className,
      )}
      data-testid={testId}
      role="status"
      aria-busy="true"
    >
      <Loader2 className="h-4 w-4 animate-spin" aria-hidden />
    </div>
  );
}
