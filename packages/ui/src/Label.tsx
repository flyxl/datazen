import type { ReactNode } from 'react';

export interface LabelProps {
  children: ReactNode;
  required?: boolean;
}

export function Label({ children, required }: LabelProps) {
  return (
    <div className="mb-1 text-xs text-fg-secondary">
      {children}
      {required && <span className="ml-0.5 text-red-400">*</span>}
    </div>
  );
}
