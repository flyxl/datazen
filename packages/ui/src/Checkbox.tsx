import type { InputHTMLAttributes } from 'react';
import { cn } from './cn';

/** Props for {@link Checkbox}: all native input attributes except `type`. */
export interface CheckboxProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {}

/**
 * The one themed checkbox.
 *
 * Replaces the hand-written `<input type="checkbox" className="accent-accent">`
 * sites across the host, the drivers and the extension UI. Native rendering is
 * kept on purpose: every existing call site wraps the box in its own `<label>`
 * (click-target text + a11y association), so this stays an *atomic* control —
 * just the box, never the row — and callers keep their wrapper, their layout
 * classes and their `data-testid` placement untouched.
 *
 * `accent-accent` activates the theme accent via the CSS `accent-color`
 * property (unchecked chrome and the checkmark itself); the focus ring and
 * disabled treatment mirror `Button`. The native box size is kept — the app's
 * existing raw checkboxes never forced a box size either.
 *
 * `type` is fixed to `checkbox`; every other native input attribute
 * (`checked`, `onChange`, `name`, `value`, `disabled`, `data-*`, …) passes
 * through, so `data-testid` lands on the input just as it did for the raw
 * element.
 */
export function Checkbox({ className, ...props }: CheckboxProps) {
  return (
    <input
      type="checkbox"
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
