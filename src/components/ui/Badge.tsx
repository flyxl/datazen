import type { HTMLAttributes } from 'react';
import { cn } from '../../lib/cn';

type Tone = 'neutral' | 'success' | 'warning' | 'danger' | 'accent';

const tones: Record<Tone, string> = {
  neutral: 'bg-surface-raised text-fg-secondary border border-edge',
  success: 'bg-green-500/10 text-green-500 border border-green-500/20',
  warning: 'bg-amber-500/10 text-amber-500 border border-amber-500/20',
  danger: 'bg-red-500/10 text-red-500 border border-red-500/20',
  accent: 'bg-blue-500/10 text-blue-500 border border-blue-500/20',
};

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: Tone;
}

export function Badge({ className, tone = 'neutral', ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-md px-2 py-0.5 text-[11px] font-semibold tracking-wide',
        tones[tone],
        className,
      )}
      {...props}
    />
  );
}
