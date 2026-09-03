import { Loader2 } from 'lucide-react';
import { cn } from '../lib/cn';

export interface LocaleDomainLoadingProps {
  /**
   * `page` (default): fill the whole window height with a centered spinner —
   * used by full-screen feature windows (Data Sync / Transfer / Schema Diff / Workflow).
   * `section`: a bounded-height region spinner — used by in-page panels
   * (Settings MCP, Dashboard), so it never blanks the whole page.
   */
  variant?: 'page' | 'section';
  testId: string;
  className?: string;
}

/**
 * Loading skeleton shown while a lazy i18n domain is being imported.
 *
 * Renders before the feature body can safely call `t()` for that domain, so it
 * must NOT depend on any (possibly not-yet-loaded) lazy translation key. It only
 * uses the icon + local fallback text, and exposes stable test hooks.
 */
export function LocaleDomainLoading({ variant = 'page', testId, className }: LocaleDomainLoadingProps) {
  return (
    <div
      data-testid={testId}
      role="status"
      aria-busy="true"
      aria-label="Loading"
      className={cn(
        'flex items-center justify-center gap-2 text-fg-muted',
        variant === 'page' ? 'h-screen w-full' : 'min-h-[8rem] w-full',
        className,
      )}
    >
      <Loader2 className="h-5 w-5 animate-spin" aria-hidden="true" />
      <span className="text-xs">Loading…</span>
    </div>
  );
}