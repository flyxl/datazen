import { useEffect, useMemo, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import type { SyncOptions } from '../../commands/sync';
import { formatCell } from '../../lib/formatters';
import { cn } from '../../lib/cn';
import {
  operationAllowed,
  rowKeyString,
  type DataSyncRowChange,
  type DataSyncTableResult,
} from './mappingView';

const PAGE_SIZE = 500;

interface DiffDetailProps {
  table: DataSyncTableResult;
  options: SyncOptions;
  onUpdateRows: (rows: DataSyncRowChange[]) => void;
}

function operationBadgeClass(op: string): string {
  switch (op) {
    case 'INSERT':
      return 'bg-green-500/15 text-green-700 dark:text-green-400';
    case 'UPDATE':
      return 'bg-accent/15 text-accent';
    case 'DELETE':
      return 'bg-red-500/15 text-red-700 dark:text-red-400';
    default:
      return 'bg-surface-alt text-fg-muted';
  }
}

export function DiffDetail({ table, options, onUpdateRows }: DiffDetailProps) {
  const { t } = useI18n();
  const [page, setPage] = useState(0);

  useEffect(() => {
    setPage(0);
  }, [table.sourceTable]);

  const diffRows = useMemo(
    () => (table.rows ?? []).filter((r) => r.operation !== 'UNCHANGED'),
    [table.rows],
  );

  const pageCount = Math.max(1, Math.ceil(diffRows.length / PAGE_SIZE));
  const safePage = Math.min(page, pageCount - 1);
  const pageRows = diffRows.slice(safePage * PAGE_SIZE, (safePage + 1) * PAGE_SIZE);

  const toggleRow = (idx: number, checked: boolean) => {
    const globalIdx = safePage * PAGE_SIZE + idx;
    const next = [...(table.rows ?? [])];
    const target = diffRows[globalIdx];
    const fullIdx = next.findIndex((r) => rowKeyString(r.key) === rowKeyString(target.key));
    if (fullIdx < 0) return;
    next[fullIdx] = { ...next[fullIdx], selected: checked };
    onUpdateRows(next);
  };

  const selectAllOp = (op: DataSyncRowChange['operation']) => {
    const next = (table.rows ?? []).map((r) => {
      if (r.operation !== op) return r;
      if (!operationAllowed(op, options)) return { ...r, selected: false };
      return { ...r, selected: true };
    });
    onUpdateRows(next);
  };

  const maxCols = useMemo(() => {
    let max = 0;
    for (const r of pageRows) {
      max = Math.max(max, r.sourceRow?.length ?? 0, r.targetRow?.length ?? 0);
    }
    return max;
  }, [pageRows]);

  if (diffRows.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-fg-muted">
        {t('sync.noRowDiffs')}
      </div>
    );
  }

  return (
    <div data-testid="data-sync-row-diff" className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-edge px-3 py-2">
        <span className="font-mono text-xs font-semibold">{table.sourceTable}</span>
        <div className="flex-1" />
        {options.insert && (
          <Button
            variant="ghost"
            size="sm"
            className="text-[10px]"
            onClick={() => selectAllOp('INSERT')}
          >
            {t('sync.selectAllInsert')}
          </Button>
        )}
        {options.update && (
          <Button
            variant="ghost"
            size="sm"
            className="text-[10px]"
            onClick={() => selectAllOp('UPDATE')}
          >
            {t('sync.selectAllUpdate')}
          </Button>
        )}
        {options.delete && (
          <Button
            variant="ghost"
            size="sm"
            className="text-[10px]"
            onClick={() => selectAllOp('DELETE')}
          >
            {t('sync.selectAllDelete')}
          </Button>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        <table className="w-full border-collapse text-xs">
          <thead className="sticky top-0 bg-surface-alt text-[10px] uppercase tracking-wider text-fg-muted">
            <tr>
              <th className="w-8 border-b border-edge p-2" />
              <th className="border-b border-edge p-2 text-left">{t('sync.op')}</th>
              <th className="border-b border-edge p-2 text-left">{t('sync.rowKey')}</th>
              {Array.from({ length: maxCols }, (_, i) => (
                <th
                  key={i}
                  className="border-b border-edge p-2 text-left"
                  title={t('sync.colIndexHint', { n: i + 1 })}
                >
                  {t('sync.colN', { n: i + 1 })}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {pageRows.map((row, idx) => {
              const selectable =
                row.operation !== 'UNCHANGED' &&
                (row.operation !== 'DELETE' || options.delete) &&
                operationAllowed(row.operation, options);
              const keyLabel = row.key.map((v) => formatCell(v)).join(' · ');
              const src = row.sourceRow ?? [];
              const tgt = row.targetRow ?? [];
              const changed = new Set(row.changedColumns);
              return (
                <tr
                  key={rowKeyString(row.key)}
                  className="border-b border-edge/60 hover:bg-surface-alt/50"
                >
                  <td className="p-2">
                    <input
                      type="checkbox"
                      className="h-3.5 w-3.5"
                      checked={row.selected && selectable}
                      disabled={!selectable}
                      onChange={(e) => toggleRow(idx, e.target.checked)}
                    />
                  </td>
                  <td className="p-2">
                    <span
                      className={cn(
                        'rounded px-1.5 py-0.5 text-[10px] font-semibold',
                        operationBadgeClass(row.operation),
                      )}
                    >
                      {row.operation}
                    </span>
                  </td>
                  <td className="max-w-[8rem] truncate p-2 font-mono" title={keyLabel}>
                    {keyLabel}
                  </td>
                  {Array.from({ length: maxCols }, (_, colIdx) => {
                    const s = src[colIdx] ?? null;
                    const tg = tgt[colIdx] ?? null;
                    const isChanged =
                      row.operation === 'UPDATE' &&
                      (changed.size === 0 ? s !== tg : changed.has(`col${colIdx}`));
                    return (
                      <td key={colIdx} className="p-2 align-top">
                        {row.operation === 'INSERT' && (
                          <span className="font-mono text-green-700 dark:text-green-400">
                            {formatCell(s)}
                          </span>
                        )}
                        {row.operation === 'DELETE' && (
                          <span className="font-mono text-red-700 dark:text-red-400 line-through">
                            {formatCell(tg)}
                          </span>
                        )}
                        {row.operation === 'UPDATE' && (
                          <div className="space-y-0.5 font-mono">
                            <div className={cn(isChanged && 'text-fg-muted line-through')}>
                              {t('sync.sourceShort')}: {formatCell(s)}
                            </div>
                            <div className={cn(isChanged && 'text-accent')}>
                              {t('sync.targetShort')}: {formatCell(tg)}
                            </div>
                          </div>
                        )}
                      </td>
                    );
                  })}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {pageCount > 1 && (
        <div className="flex shrink-0 items-center justify-between border-t border-edge px-3 py-2 text-xs text-fg-muted">
          <span>{t('sync.pageOf', { page: safePage + 1, total: pageCount })}</span>
          <div className="flex gap-1">
            <Button
              variant="ghost"
              size="sm"
              disabled={safePage <= 0}
              onClick={() => setPage((p) => Math.max(0, p - 1))}
            >
              {t('sync.pagePrev')}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={safePage >= pageCount - 1}
              onClick={() => setPage((p) => Math.min(pageCount - 1, p + 1))}
            >
              {t('sync.pageNext')}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
