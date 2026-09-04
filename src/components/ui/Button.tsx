import type { ButtonHTMLAttributes } from 'react';
import { cn } from '../../lib/cn';

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'run';

type Size = 'md' | 'sm';

const variants: Record<Variant, string> = {
  primary:
    'bg-accent text-white hover:bg-accent/90 disabled:opacity-50 disabled:pointer-events-none shadow-sm',
  /** Exclusive to SQL execution and Workflow execution */
  run: 'bg-query-run text-white hover:bg-query-run/90 disabled:opacity-50 disabled:pointer-events-none shadow-sm',
  secondary:
    'border border-edge bg-transparent text-fg hover:bg-surface-raised disabled:opacity-50',
  ghost: 'bg-transparent text-fg-secondary hover:bg-surface-raised disabled:opacity-50',
  danger: 'bg-red-500/90 text-white hover:bg-red-500 disabled:opacity-50',
};

const sizes: Record<Size, string> = {
  md: 'px-3 h-8 text-sm',
  sm: 'px-2 h-7 text-xs',
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
}

export function Button({
  className,
  variant = 'primary',
  size = 'md',
  type = 'button',
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        'inline-flex items-center justify-center gap-2 rounded-md font-medium transition-colors',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
        sizes[size],
        variants[variant],
        className,
      )}
      {...props}
    />
  );
}
