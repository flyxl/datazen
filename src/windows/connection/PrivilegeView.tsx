import { useCallback, useEffect, useState } from 'react';
import { Loader2, Play, RefreshCw } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { databaseCommands } from '../../commands/database';
import { queryCommands } from '../../commands/query';
import type { PrivilegeGrant } from '../../types';

interface PrivilegeViewProps {
  connectionId: string;
}

export function PrivilegeView({ connectionId }: PrivilegeViewProps) {
  const { t } = useI18n();
  const [grants, setGrants] = useState<PrivilegeGrant[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sql, setSql] = useState('GRANT SELECT ON TABLE schema.table TO role;');
  const [running, setRunning] = useState(false);
  const [runMessage, setRunMessage] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setGrants(await databaseCommands.getPrivileges(connectionId));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setGrants([]);
    } finally {
      setLoading(false);
    }
  }, [connectionId]);

  useEffect(() => {
    void load();
  }, [load]);

  const handleExecute = useCallback(async () => {
    if (!sql.trim()) return;
    setRunning(true);
    setRunMessage(null);
    try {
      await queryCommands.executeQuery(connectionId, sql);
      setRunMessage(t('privileges.executeOk'));
      void load();
    } catch (e) {
      setRunMessage(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  }, [connectionId, load, sql, t]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge px-3">
        <span className="text-xs font-medium text-fg">{t('privileges.title')}</span>
        <div className="flex-1" />
        <Button variant="ghost" className="h-7 w-7 !px-0" onClick={() => void load()}>
          <RefreshCw className="h-3.5 w-3.5" />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {loading && (
          <div className="flex items-center gap-2 p-3 text-xs text-fg-muted">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {t('common.loading')}
          </div>
        )}
        {error && <div className="p-3 text-xs text-red-400">{error}</div>}
        {!loading && !error && grants.length === 0 && (
          <div className="p-3 text-xs text-fg-muted">{t('privileges.empty')}</div>
        )}
        {grants.length > 0 && (
          <table className="w-full text-left text-[12px]">
            <thead className="sticky top-0 bg-surface-alt text-[11px] uppercase text-fg-muted">
              <tr>
                <th className="px-3 py-1.5 font-medium">{t('privileges.grantee')}</th>
                <th className="px-3 py-1.5 font-medium">{t('privileges.object')}</th>
                <th className="px-3 py-1.5 font-medium">{t('privileges.privilege')}</th>
              </tr>
            </thead>
            <tbody>
              {grants.map((g, i) => (
                <tr key={`${g.grantee}-${g.objectName}-${g.privilege}-${i}`} className="border-t border-edge">
                  <td className="px-3 py-1.5 text-fg">{g.grantee}</td>
                  <td className="px-3 py-1.5 text-fg-secondary">
                    {g.objectSchema ? `${g.objectSchema}.` : ''}{g.objectName}
                  </td>
                  <td className="px-3 py-1.5 text-fg-secondary">{g.privilege}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      <div className="border-t border-edge p-3">
        <div className="mb-1.5 text-[11px] text-fg-muted">{t('privileges.sqlHint')}</div>
        <textarea
          className="h-20 w-full resize-y rounded border border-edge bg-surface-alt px-2.5 py-1.5 font-mono text-xs text-fg outline-none focus:border-accent"
          value={sql}
          onChange={(e) => setSql(e.target.value)}
        />
        <div className="mt-2 flex items-center gap-2">
          <Button
            variant="primary"
            className="h-7 gap-1 px-2 text-xs"
            disabled={running || !sql.trim()}
            onClick={() => void handleExecute()}
          >
            <Play className="h-3.5 w-3.5" />
            {t('query.execute')}
          </Button>
          {runMessage && <span className="text-[11px] text-fg-muted">{runMessage}</span>}
        </div>
      </div>
    </div>
  );
}
