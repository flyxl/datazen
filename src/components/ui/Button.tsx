import type { ButtonHTMLAttributes } from 'react';
import { cn } from '../../lib/cn';

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';

const variants: Record<Variant, string> = {
  primary:
    'bg-blue-500 text-white hover:bg-blue-600 disabled:opacity-50 disabled:pointer-events-none shadow-sm',
  secondary:
    'border border-edge bg-transparent text-fg hover:bg-surface-raised disabled:opacity-50',
  ghost: 'bg-transparent text-fg-secondary hover:bg-surface-raised disabled:opacity-50',
  danger: 'bg-red-500/90 text-white hover:bg-red-500 disabled:opacity-50',
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
}

export function Button({ className, variant = 'primary', type = 'button', ...props }: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        'inline-flex items-center justify-center gap-2 rounded-md px-3 h-8 text-sm font-medium transition-colors',
        variants[variant],
        className,
      )}
      {...props}
    />
  );
}
