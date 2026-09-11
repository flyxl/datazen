import type { InputHTMLAttributes } from 'react';
import { cn } from './cn';

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {}

export function Input({ className, ...props }: InputProps) {
  return (
    <input
      autoCapitalize="off"
      autoCorrect="off"
      spellCheck={false}
      className={cn(
        'h-10 w-full rounded-[9px] border border-edge bg-surface-inset px-3 font-mono text-[13px] text-fg outline-none',
        'placeholder:text-fg-muted',
        'focus:border-accent focus:ring-[3px] focus:ring-accent-ring focus:bg-surface-inset',
        'disabled:cursor-not-allowed disabled:opacity-50',
        className,
      )}
      {...props}
    />
  );
}
