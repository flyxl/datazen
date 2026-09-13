import type { ButtonHTMLAttributes } from 'react';
import { cn } from './cn';

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'run';

type Size = 'md' | 'sm';

const variants: Record<Variant, string> = {
  primary:
    'bg-accent text-on-accent hover:bg-accent-2 disabled:opacity-50 disabled:pointer-events-none shadow-sm',
  /** Exclusive to SQL execution and Workflow execution */
  run: 'bg-query-run text-on-accent hover:bg-query-run/90 disabled:opacity-50 disabled:pointer-events-none shadow-sm',
  secondary:
    'border border-edge bg-transparent text-fg hover:border-edge-hi hover:bg-surface-raised disabled:opacity-50',
  ghost:
    'bg-transparent text-fg-secondary hover:bg-surface-raised hover:text-fg disabled:opacity-50',
  danger: 'bg-danger text-on-accent hover:bg-danger/90 disabled:opacity-50',
};

const sizes: Record<Size, string> = {
  md: 'px-[18px] h-9 text-[13px] rounded-[9px]',
  sm: 'px-2 h-7 text-xs rounded-[9px]',
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
        'inline-flex items-center justify-center gap-2 font-semibold transition-all',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
        sizes[size],
        variants[variant],
        className,
      )}
      {...props}
    />
  );
}
