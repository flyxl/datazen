import { useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2, RefreshCw, Skull } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { CopyableError } from '../../components/ui/CopyableError';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { useI18n } from '../../hooks/useI18n';
import { driverCommands } from '../../commands/driver';
import { commandResultColumns, commandResultRows } from '../../lib/processListResult';
import type { QueryResult, Value, ColumnInfo } from '../../types';

export interface ProcessListCache {
  rows: (string | number | boolean | null)[][];
  columns?: { name: string; dataType: string; nullable?: boolean }[];
}

interface ProcessListViewProps {
  dbSessionId: string;
  connectionName?: string;
  /** 面板 id（用于把加载的数据写回对应 tab）。 */
  panelId?: string;
  /** 该 tab 已缓存的数据；用于首帧直接展示，随后总是重新拉取以保证最新。 */
  initialData?: ProcessListCache;
  /** 加载完成后把最新数据写回对应面板。 */
  onDataChange?: (data: ProcessListCache) => void;
}

function panelDataToQueryResult(data: ProcessListCache): QueryResult {
  return {
    columns: data.columns?.length
      ? data.columns.map((c) => ({
          name: c.name,
          dataType: c.dataType,
          nullable: c.nullable ?? true,
        }))
      : [],
    rows: data.rows as Value[][],
    executionTimeMs: 0,
  };
}

function queryResultToPanelData(result: QueryResult | null): ProcessListCache | null {
  if (!result) return null;
  const columns: ColumnInfo[] = result.columns ?? [];
  return {
    rows: (result.rows ?? []) as (string | number | boolean | null)[][],
    columns: columns.map((c) => ({
      name: c.name,
      dataType: c.dataType,
      nullable: c.nullable ?? true,
    })),
  };
}

export function ProcessListView({
  dbSessionId,
  connectionName,
  initialData,
  onDataChange,
}: ProcessListViewProps) {
  const { t } = useI18n();
  const [confirmKill, confirmKillDialog] = useConfirmDialog();
  const [result, setResult] = useState<QueryResult | null>(() =>
    initialData ? panelDataToQueryResult(initialData) : null,
  );
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [highlightedRow, setHighlightedRow] = useState<number | null>(null);
  const [killing, setKilling] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await driverCommands.execute({
        dbSessionId,
        command: 'list_processes',
        input: {},
      });
      const next = commandResultRows(response.data);
      setResult(next);
      const cached = queryResultToPanelData(next);
      if (cached) onDataChange?.(cached);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setResult(null);
    } finally {
      setLoading(false);
    }
  }, [dbSessionId, onDataChange]);

  // 按 dbSessionId 拉取；切换进程列表 tab 时若组件被复用，必须重新加载，避免串数据。
  // 故意不依赖 load：onDataChange 每帧可能是新引用，纳入依赖会无限重拉。
  useEffect(() => {
    setResult(initialData ? panelDataToQueryResult(initialData) : null);
    setHighlightedRow(null);
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dbSessionId]);

  const columns: ColumnDef[] = useMemo(() => {
    if (!result) return commandResultColumns([], t);
    return commandResultColumns(result.columns, t);
  }, [result, t]);

  const pidColumnIndex = useMemo(() => {
    if (!result) return -1;
    return result.columns.findIndex((c) => c.name.toLowerCase() === 'pid');
  }, [result]);

  const handleKill = useCallback(async () => {
    if (!result || highlightedRow == null || pidColumnIndex < 0) return;
    const pidValue = result.rows[highlightedRow]?.[pidColumnIndex];
    const pid = typeof pidValue === 'number' ? pidValue : Number(pidValue);
    if (!Number.isFinite(pid)) return;

    const ok = await confirmKill({
      title: t('processList.killTitle'),
      message: t('processList.killConfirm', { pid: String(pid) }),
      confirmLabel: t('processList.kill'),
      kind: 'warning',
    });
    if (!ok) return;

    setKilling(true);
    setError(null);
    try {
      await driverCommands.execute({
        dbSessionId,
        command: 'kill_process',
        input: { pid, force: true },
      });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setKilling(false);
    }
  }, [confirmKill, dbSessionId, highlightedRow, load, pidColumnIndex, result, t]);

  const rows = (result?.rows ?? []) as Value[][];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge px-3">
        <span className="text-xs font-medium text-fg">{t('processList.title')}</span>
        <span className="truncate text-xs text-fg-muted" title={connectionName}>
          {connectionName}
        </span>
        <span className="h-3 w-px bg-edge" />
        <div className="flex-1" />
        <Button
          variant="ghost"
          className="h-7 w-7 !px-0"
          title={t('processList.refresh')}
          disabled={loading}
          onClick={() => void load()}
        >
          {loading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <RefreshCw className="h-3.5 w-3.5" />
          )}
        </Button>
        <Button
          variant="ghost"
          className="h-7 gap-1 !px-2 text-xs"
          title={t('processList.kill')}
          disabled={killing || highlightedRow == null || pidColumnIndex < 0}
          onClick={() => void handleKill()}
        >
          {killing ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Skull className="h-3.5 w-3.5" />
          )}
          {t('processList.kill')}
        </Button>
      </div>
      {error && (
        <div className="border-b border-edge px-3 py-2">
          <CopyableError message={error} />
        </div>
      )}
      <div className="min-h-0 flex-1">
        <DataTable
          columns={columns}
          rows={rows}
          highlightedRow={highlightedRow}
          onRowClick={(row) => setHighlightedRow(row)}
        />
      </div>
      {confirmKillDialog}
    </div>
  );
}
