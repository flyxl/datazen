import { useCallback, useEffect, useState } from 'react';
import { Loader2, RefreshCw } from 'lucide-react';
import { Button } from '../../../../src/components/ui/Button';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';
import { redisCommandInvoke } from './redisInvoke';

export interface StreamOverviewProps {
  dbSessionId: string;
  dbIndex?: number;
  limit?: number;
}

interface StreamOverviewRow {
  key: string;
  length: number;
  groupCount: number;
  pendingTotal: number;
}

interface StreamOverviewResult {
  rows: StreamOverviewRow[];
  truncated: boolean;
}

const DEFAULT_LIMIT = 100;

export async function invokeStreamOverview(
  dbSessionId: string,
  dbIndex: number,
  limit?: number,
): Promise<StreamOverviewResult> {
  return redisCommandInvoke('redis', 'stream_overview', {
    dbSessionId,
    dbIndex,
    limit: limit ?? null,
  });
}

export function StreamOverview({
  dbSessionId,
  dbIndex = 0,
  limit = DEFAULT_LIMIT,
}: StreamOverviewProps) {
  const { t } = useI18n();
  const [rows, setRows] = useState<StreamOverviewRow[]>([]);
  const [truncated, setTruncated] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invokeStreamOverview(dbSessionId, dbIndex, limit);
      setRows(result.rows);
      setTruncated(result.truncated);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setRows([]);
      setTruncated(false);
    } finally {
      setLoading(false);
    }
  }, [dbSessionId, dbIndex, limit]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3 py-2">
        <span className="text-xs font-medium text-fg">{t('redis.streamOverview')}</span>
        <Button
          variant="secondary"
          className="ml-auto h-7 gap-1 px-2 text-xs"
          disabled={loading}
          onClick={() => void load()}
        >
          <RefreshCw className={cn('h-3 w-3', loading && 'animate-spin')} />
          {t('redis.refresh')}
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-3">
        {error && (
          <div className="mb-3 rounded-md border border-danger/20 bg-danger/10 px-2 py-1.5 text-xs text-danger">
            {error}
          </div>
        )}

        {loading && rows.length === 0 ? (
          <div className="flex items-center gap-2 text-xs text-fg-muted">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {t('redis.monitorLoading')}
          </div>
        ) : rows.length === 0 ? (
          <p className="text-xs text-fg-muted">{t('redis.streamOverviewEmpty')}</p>
        ) : (
          <>
            <table className="w-full border-collapse text-xs">
              <thead>
                <tr className="border-b border-edge bg-surface-alt text-left">
                  <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.keyName')}</th>
                  <th className="px-2 py-1.5 font-medium text-fg-muted">
                    {t('redis.streamLength')}
                  </th>
                  <th className="px-2 py-1.5 font-medium text-fg-muted">
                    {t('redis.streamGroupCount')}
                  </th>
                  <th className="px-2 py-1.5 font-medium text-fg-muted">
                    {t('redis.streamPendingTotal')}
                  </th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={row.key} className="border-b border-edge">
                    <td className="px-2 py-1.5 font-mono text-fg-secondary">{row.key}</td>
                    <td className="px-2 py-1.5 text-fg-secondary">{row.length}</td>
                    <td className="px-2 py-1.5 text-fg-secondary">{row.groupCount}</td>
                    <td className="px-2 py-1.5 text-fg-secondary">{row.pendingTotal}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {truncated && (
              <p className="mt-3 text-xs text-fg-muted">{t('redis.streamOverviewTruncated')}</p>
            )}
          </>
        )}
      </div>
    </div>
  );
}
