import { ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';

export interface PaginationProps {
  page: number;
  pageSize: number;
  totalRows: number;
  onPageChange: (page: number) => void;
  onPageSizeChange: (size: number) => void;
}

export function Pagination({ page, pageSize, totalRows, onPageChange, onPageSizeChange }: PaginationProps) {
  const totalPages = Math.max(1, Math.ceil(totalRows / pageSize));
  const current = Math.min(page, totalPages - 1);
  const from = totalRows === 0 ? 0 : current * pageSize + 1;
  const to = Math.min(totalRows, (current + 1) * pageSize);

  return (
    <div className="flex h-10 shrink-0 items-center justify-between gap-3 border-t border-edge bg-surface-alt px-3 text-xs text-fg-secondary">
      <div className="min-w-0 truncate">
        {from}-{to} / {totalRows}
      </div>
      <div className="flex items-center gap-2">
        <div className="hidden items-center gap-2 sm:flex">
          <span>每页</span>
          <Select
            className="h-8 w-[92px]"
            value={pageSize}
            options={[25, 50, 100, 200, 500].map((n) => ({ value: String(n), label: String(n) }))}
            onChange={(v) => onPageSizeChange(Number(v))}
          />
        </div>
        <div className="flex items-center gap-1">
          <Button
            variant="secondary"
            className="h-8 w-8 px-0"
            disabled={current <= 0}
            onClick={() => onPageChange(current - 1)}
            aria-label="上一页"
          >
            <ChevronLeft className="h-4 w-4" />
          </Button>
          <div className="w-[120px] truncate text-center tabular-nums">
            第 {current + 1} / {totalPages} 页
          </div>
          <Button
            variant="secondary"
            className="h-8 w-8 px-0"
            disabled={current >= totalPages - 1}
            onClick={() => onPageChange(current + 1)}
            aria-label="下一页"
          >
            <ChevronRight className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}
