import type { InputHTMLAttributes } from 'react';
import { cn } from './cn';

/** Props for {@link Radio}: all native input attributes except `type`. */
export interface RadioProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {}

/**
 * The one themed radio button.
 *
 * Same contract and rationale as {@link Checkbox}: an atomic native
 * `<input type="radio">`, theme-tinted through `accent-color`, focus ring and
 * disabled treatment mirroring `Button`. Callers keep their `<label>` wrapper
 * (which also supplies the radio group semantics via the shared `name`).
 *
 * `type` is fixed to `radio`; `value`, `name`, `checked`, `onChange`,
 * `disabled` and `data-*` all pass through to the native input.
 */
export function Radio({ className, ...props }: RadioProps) {
  return (
    <input
      type="radio"
      className={cn(
        'cursor-pointer accent-accent',
        'disabled:cursor-not-allowed disabled:opacity-50',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
        className,
      )}
      {...props}
    />
  );
}
