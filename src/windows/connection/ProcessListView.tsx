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
import type { QueryResult, Value } from '../../types';

interface ProcessListViewProps {
  connectionId: string;
}

export function ProcessListView({ connectionId }: ProcessListViewProps) {
  const { t } = useI18n();
  const [confirmKill, confirmKillDialog] = useConfirmDialog();
  const [result, setResult] = useState<QueryResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [highlightedRow, setHighlightedRow] = useState<number | null>(null);
  const [killing, setKilling] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await driverCommands.execute({
        connectionId,
        command: 'list_processes',
        input: {},
      });
      setResult(commandResultRows(response.data));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setResult(null);
    } finally {
      setLoading(false);
    }
  }, [connectionId]);

  useEffect(() => {
    void load();
  }, [load]);

  const columns: ColumnDef[] = useMemo(() => {
    if (!result) return commandResultColumns([]);
    return result.columns.map((c) => ({ id: c.name, name: c.name, type: c.dataType }));
  }, [result]);

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
        connectionId,
        command: 'kill_process',
        input: { pid, force: true },
      });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setKilling(false);
    }
  }, [confirmKill, connectionId, highlightedRow, load, pidColumnIndex, result, t]);

  const rows = (result?.rows ?? []) as Value[][];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge px-3">
        <span className="text-xs font-medium text-fg">{t('processList.title')}</span>
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
