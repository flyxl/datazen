import { useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2, RefreshCw } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { driverCommands } from '../../commands/driver';
import { useI18n } from '../../hooks/useI18n';
import { SERVER_STATUS_SNAPSHOT_COMMAND } from '../../lib/driverCommandIds';

import type { TranslationKey } from '../../locales';

interface ServerStatusViewProps {
  connectionId: string;
}

type StatusRecord = Record<string, string | number | boolean | null>;

function asStatusRecord(data: unknown): StatusRecord | null {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return null;
  const out: StatusRecord = {};
  for (const [key, value] of Object.entries(data as Record<string, unknown>)) {
    if (
      typeof value === 'string' ||
      typeof value === 'number' ||
      typeof value === 'boolean' ||
      value === null
    ) {
      out[key] = value;
    } else {
      out[key] = JSON.stringify(value);
    }
  }
  return out;
}

function formatValue(
  key: string,
  value: string | number | boolean | null,
  t: (key: TranslationKey) => string,
): string {
  if (value == null) return '—';
  if (key === 'uptimeSeconds' && typeof value === 'number') {
    const hours = Math.floor(value / 3600);
    const minutes = Math.floor((value % 3600) / 60);
    const seconds = value % 60;
    return t('serverStatus.uptimeFormat')
      .replace('{hours}', String(hours))
      .replace('{minutes}', String(minutes))
      .replace('{seconds}', String(seconds));
  }
  return String(value);
}

function labelForKey(key: string, t: (key: TranslationKey) => string): string {
  const map: Record<string, TranslationKey> = {
    version: 'serverStatus.version',
    database: 'serverStatus.database',
    uptimeSeconds: 'serverStatus.uptime',
    connections: 'serverStatus.connections',
    activeQueries: 'serverStatus.activeQueries',
    databaseSize: 'serverStatus.databaseSize',
  };
  return map[key] ? t(map[key]) : key;
}

export function ServerStatusView({ connectionId }: ServerStatusViewProps) {
  const { t } = useI18n();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<StatusRecord | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await driverCommands.execute({
        connectionId,
        command: SERVER_STATUS_SNAPSHOT_COMMAND,
        input: {},
      });
      const record = asStatusRecord(result.data);
      if (!record) {
        setError(t('serverStatus.invalidResponse'));
        setStatus(null);
        return;
      }
      setStatus(record);
    } catch (e) {
      const msg =
        typeof e === 'string' ? e : e instanceof Error ? e.message : t('serverStatus.loadFailed');
      setError(msg);
      setStatus(null);
    } finally {
      setLoading(false);
    }
  }, [connectionId, t]);

  useEffect(() => {
    void load();
  }, [load]);

  const rows = useMemo(() => {
    if (!status) return [];
    const order = [
      'version',
      'database',
      'uptimeSeconds',
      'connections',
      'activeQueries',
      'databaseSize',
    ];
    const keys = [
      ...order.filter((k) => k in status),
      ...Object.keys(status)
        .filter((k) => !order.includes(k))
        .sort(),
    ];
    return keys.map((key) => ({
      key,
      label: labelForKey(key, t),
      value: formatValue(key, status[key], t),
    }));
  }, [status, t]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b border-edge bg-surface-alt px-4 py-3">
        <span className="text-sm font-medium text-fg">{t('serverStatus.title')}</span>
        <div className="flex-1" />
        <Button
          variant="secondary"
          className="h-8 gap-1 text-xs"
          onClick={() => void load()}
          disabled={loading}
        >
          {loading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <RefreshCw className="h-3.5 w-3.5" />
          )}
          {t('serverStatus.refresh')}
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {loading && !status ? (
          <div className="flex items-center gap-2 text-sm text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('serverStatus.loading')}
          </div>
        ) : error ? (
          <div className="text-sm text-danger">{error}</div>
        ) : rows.length === 0 ? (
          <div className="text-sm text-fg-muted">{t('serverStatus.empty')}</div>
        ) : (
          <table className="w-full max-w-2xl text-sm">
            <tbody>
              {rows.map((row) => (
                <tr key={row.key} className="border-b border-edge/60">
                  <th className="w-48 py-2 pr-4 text-left font-medium text-fg-secondary">
                    {row.label}
                  </th>
                  <td className="py-2 text-fg">{row.value}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
