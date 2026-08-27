import type { ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { X } from 'lucide-react';
import { cn } from '../../lib/cn';
import { tid } from '../../lib/tid';

export interface DialogProps {
  open: boolean;
  title: string;
  description?: string;
  children: ReactNode;
  onClose: () => void;
  footer?: ReactNode;
  className?: string;
  /** E2E-only stable locator (see `tid()`); omitted in production builds. */
  testId?: string;
}

export function Dialog({
  open,
  title,
  description,
  children,
  onClose,
  footer,
  className,
  testId,
}: DialogProps) {
  if (!open) return null;

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div aria-hidden="true" className="absolute inset-0 bg-black/50" />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        {...(testId ? tid(testId) : {})}
        className={cn(
          'relative z-10 w-full max-w-xl overflow-hidden rounded-xl border border-edge bg-surface-alt shadow-xl',
          className,
        )}
      >
        <div className="flex items-start justify-between gap-3 border-b border-edge px-5 py-4">
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold text-fg">{title}</div>
            {description ? <div className="mt-1 text-xs text-fg-muted">{description}</div> : null}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-fg-muted hover:bg-surface-raised hover:text-fg"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
        <div className="max-h-[70vh] overflow-auto px-5 py-4">{children}</div>
        {footer ? (
          <div className="flex items-center justify-end gap-2 border-t border-edge px-5 py-3">
            {footer}
          </div>
        ) : null}
      </div>
    </div>,
    document.body,
  );
}
