import type { ButtonHTMLAttributes, ReactNode } from 'react';
import { Button } from './Button';
import { cn } from '../../lib/cn';

export interface ToolbarButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  compact?: boolean;
  label: string;
  icon: ReactNode;
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'run';
}

export function ToolbarButton({
  compact = false,
  label,
  icon,
  variant = 'ghost',
  className,
  title,
  ...props
}: ToolbarButtonProps) {
  return (
    <Button
      {...props}
      variant={variant}
      title={title ?? label}
      aria-label={label}
      className={cn('shrink-0', compact ? 'h-7 px-1.5' : 'h-7 gap-1 px-2 text-xs', className)}
    >
      {icon}
      <span className={cn(compact && 'sr-only')}>{label}</span>
    </Button>
  );
}
