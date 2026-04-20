import type { InputHTMLAttributes } from 'react';
import { cn } from '../../lib/cn';

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {}

export function Input({ className, ...props }: InputProps) {
  return (
    <input
      autoCapitalize="off"
      autoCorrect="off"
      spellCheck={false}
      className={cn(
        'h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none',
        'placeholder:text-fg-muted',
        'focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25',
        'disabled:cursor-not-allowed disabled:opacity-50',
        className,
      )}
      {...props}
    />
  );
}
