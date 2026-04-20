import { X } from 'lucide-react';
import type { FilterCondition } from '../types';
import { formatCell } from '../lib/formatters';
import { cn } from '../lib/cn';
import { Badge } from './ui/Badge';

export interface FilterBarProps {
  filters: FilterCondition[];
  onRemove: (index: number) => void;
  onClear: () => void;
  className?: string;
}

function labelFor(f: FilterCondition) {
  return `${f.column} ${f.operator}${f.value === undefined ? '' : ` ${formatCell(f.value)}`}`;
}

export function FilterBar({ filters, onRemove, onClear, className }: FilterBarProps) {
  if (filters.length === 0) return null;

  return (
    <div
      className={cn(
        'flex flex-wrap items-center gap-2 border-b border-edge bg-surface px-3 py-2',
        className,
      )}
    >
      <div className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">筛选</div>
      <div className="flex min-w-0 flex-1 flex-wrap gap-2">
        {filters.map((f, idx) => (
          <Badge key={`${f.column}-${idx}`} tone="accent" className="max-w-full gap-2">
            <span className="truncate" title={labelFor(f)}>
              {labelFor(f)}
            </span>
            <button
              type="button"
              className="rounded-sm p-0.5 text-blue-300 hover:bg-blue-500/10"
              onClick={() => onRemove(idx)}
              aria-label="移除筛选"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </Badge>
        ))}
      </div>
      <button type="button" className="text-xs text-fg-secondary hover:text-fg" onClick={onClear}>
        清空
      </button>
    </div>
  );
}
