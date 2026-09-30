import { useEffect, useMemo, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { Checkbox } from '../../components/ui/Checkbox';
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
  onSelectAllOperation?: (operation: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>) => void;
  onClearAllOperation?: (operation: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>) => void;
  isOperationSelected?: (
    operation: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>,
  ) => boolean;
  hasTableSelection?: boolean;
  pageLoading?: boolean;
  pageIndex?: number;
  hasPreviousPage?: boolean;
  hasNextPage?: boolean;
  onPageChange?: (direction: 'previous' | 'next') => void;
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

export function DiffDetail({
  table,
  options,
  onUpdateRows,
  onSelectAllOperation,
  onClearAllOperation,
  isOperationSelected,
  hasTableSelection = false,
  pageLoading = false,
  pageIndex = 0,
  hasPreviousPage = false,
  hasNextPage = false,
  onPageChange,
}: DiffDetailProps) {
  const { t } = useI18n();
  const [page, setPage] = useState(0);

  useEffect(() => {
    setPage(0);
  }, [table.sourceTable]);

  const serverPaged = table.pageSize !== undefined || table.rowCount !== undefined;
  const diffRows = useMemo(
    () => (table.rows ?? []).filter((r) => r.operation !== 'UNCHANGED'),
    [table.rows],
  );

  const pageCount = serverPaged
    ? Math.max(1, Math.ceil((table.rowCount ?? diffRows.length) / (table.pageSize ?? PAGE_SIZE)))
    : Math.max(1, Math.ceil(diffRows.length / PAGE_SIZE));
  const safePage = serverPaged ? pageIndex : Math.min(page, pageCount - 1);
  const pageRows = serverPaged
    ? diffRows
    : diffRows.slice(safePage * PAGE_SIZE, (safePage + 1) * PAGE_SIZE);

  const toggleRow = (idx: number, checked: boolean) => {
    const next = [...(table.rows ?? [])];
    const target = pageRows[idx];
    const fullIdx = next.findIndex((r) => rowKeyString(r.key) === rowKeyString(target.key));
    if (fullIdx < 0) return;
    next[fullIdx] = { ...next[fullIdx], selected: checked };
    onUpdateRows(next);
  };

  const selectAllOp = (op: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>) => {
    const next = (table.rows ?? []).map((r) => {
      if (r.operation !== op) return r;
      if (!operationAllowed(op, options)) return { ...r, selected: false };
      return { ...r, selected: true };
    });
    onUpdateRows(next);
  };

  const handleSelectAllOperation = (op: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>) => {
    if (onSelectAllOperation) {
      onSelectAllOperation(op);
      return;
    }
    selectAllOp(op);
  };

  const handleClearAllOperation = (op: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>) => {
    if (onClearAllOperation) {
      onClearAllOperation(op);
      return;
    }
    onUpdateRows(
      (table.rows ?? []).map((row) => (row.operation === op ? { ...row, selected: false } : row)),
    );
  };

  const maxCols = useMemo(() => {
    let max = 0;
    for (const r of pageRows) {
      max = Math.max(max, r.sourceRow?.length ?? 0, r.targetRow?.length ?? 0);
    }
    return max;
  }, [pageRows]);

  if (diffRows.length === 0 && (!serverPaged || !hasNextPage)) {
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
          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              size="sm"
              className="text-[10px]"
              data-testid="data-sync-select-all-INSERT"
              onClick={() => handleSelectAllOperation('INSERT')}
            >
              {t('sync.selectAllInsert')}
            </Button>
            {isOperationSelected?.('INSERT') && (
              <Button
                variant="ghost"
                size="sm"
                className="text-[10px]"
                data-testid="data-sync-clear-all-INSERT"
                onClick={() => handleClearAllOperation('INSERT')}
              >
                {t('sync.clearAllInsert')}
              </Button>
            )}
          </div>
        )}
        {options.update && (
          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              size="sm"
              className="text-[10px]"
              data-testid="data-sync-select-all-UPDATE"
              onClick={() => handleSelectAllOperation('UPDATE')}
            >
              {t('sync.selectAllUpdate')}
            </Button>
            {isOperationSelected?.('UPDATE') && (
              <Button
                variant="ghost"
                size="sm"
                className="text-[10px]"
                data-testid="data-sync-clear-all-UPDATE"
                onClick={() => handleClearAllOperation('UPDATE')}
              >
                {t('sync.clearAllUpdate')}
              </Button>
            )}
          </div>
        )}
        {options.delete && (
          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              size="sm"
              className="text-[10px]"
              data-testid="data-sync-select-all-DELETE"
              onClick={() => handleSelectAllOperation('DELETE')}
            >
              {t('sync.selectAllDelete')}
            </Button>
            {isOperationSelected?.('DELETE') && (
              <Button
                variant="ghost"
                size="sm"
                className="text-[10px]"
                data-testid="data-sync-clear-all-DELETE"
                onClick={() => handleClearAllOperation('DELETE')}
              >
                {t('sync.clearAllDelete')}
              </Button>
            )}
          </div>
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
                  title={table.columns?.[i] ?? t('sync.colIndexHint', { n: i + 1 })}
                >
                  {table.columns?.[i] ?? t('sync.colN', { n: i + 1 })}
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
                    <Checkbox
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
                      row.operation === 'UPDATE' && changed.has(table.columns?.[colIdx] ?? '');
                    return (
                      <td
                        key={colIdx}
                        data-column={table.columns?.[colIdx]}
                        data-changed={isChanged}
                        className="p-2 align-top"
                      >
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
                            <div className={cn(isChanged && 'text-accent')}>
                              {t('sync.sourceShort')}: {formatCell(s)}
                            </div>
                            <div className={cn(isChanged && 'text-fg-muted line-through')}>
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

      {(pageCount > 1 || serverPaged) && (
        <div className="flex shrink-0 items-center justify-between border-t border-edge px-3 py-2 text-xs text-fg-muted">
          <span>
            {serverPaged
              ? `${t('sync.pageOf', { page: safePage + 1, total: pageCount })} · ${t(hasTableSelection ? 'sync.pageScopeAll' : 'sync.pageScope')}`
              : t('sync.pageOf', { page: safePage + 1, total: pageCount })}
          </span>
          <div className="flex gap-1">
            <Button
              variant="ghost"
              size="sm"
              disabled={serverPaged ? pageLoading || !hasPreviousPage : safePage <= 0}
              onClick={() => {
                if (serverPaged) onPageChange?.('previous');
                else setPage((p) => Math.max(0, p - 1));
              }}
            >
              {t('sync.pagePrev')}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={serverPaged ? pageLoading || !hasNextPage : safePage >= pageCount - 1}
              onClick={() => {
                if (serverPaged) onPageChange?.('next');
                else setPage((p) => Math.min(pageCount - 1, p + 1));
              }}
            >
              {t('sync.pageNext')}
            </Button>
            {pageLoading && <span className="px-1">{t('sync.loadingPage')}</span>}
          </div>
        </div>
      )}
    </div>
  );
}
