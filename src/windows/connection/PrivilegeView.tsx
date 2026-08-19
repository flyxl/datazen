import { useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2, Play, RefreshCw, Trash2, X } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { useI18n } from '../../hooks/useI18n';
import { databaseCommands } from '../../commands/database';
import { driverCommands } from '../../commands/driver';
import { queryCommands } from '../../commands/query';
import type { PrivilegeGrant } from '../../types';

interface PrivilegeViewProps {
  connectionId: string;
  databaseType?: string;
}

export function PrivilegeView({ connectionId, databaseType }: PrivilegeViewProps) {
  const { t } = useI18n();
  const [grants, setGrants] = useState<PrivilegeGrant[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sql, setSql] = useState('GRANT SELECT ON TABLE schema.table TO role;');
  const [running, setRunning] = useState(false);
  const [runMessage, setRunMessage] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [confirmAction, confirmDialog] = useConfirmDialog();

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

  const uniqueGrantees = useMemo(() => {
    return [...new Set(grants.map((g) => g.grantee))].sort();
  }, [grants]);

  const handleDropUser = useCallback(
    async (username: string) => {
      const ok = await confirmAction({
        title: t('privileges.dropUser'),
        message: t('privileges.confirmDropUser', { name: username }),
        confirmLabel: t('privileges.dropUser'),
        kind: 'warning',
      });
      if (!ok) return;
      setActionError(null);
      try {
        await driverCommands.execute({
          connectionId,
          command: 'drop_user',
          input: { username },
        });
        void load();
      } catch (e) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    },
    [confirmAction, connectionId, load],
  );

  const handleRevokePrivilege = useCallback(
    async (grantee: string, privilege: string, objectName: string) => {
      const ok = await confirmAction({
        title: t('privileges.revokePrivilege'),
        message: t('privileges.confirmRevoke', { privilege, user: grantee, object: objectName }),
        confirmLabel: t('privileges.revokePrivilege'),
        kind: 'warning',
      });
      if (!ok) return;
      setActionError(null);
      try {
        await driverCommands.execute({
          connectionId,
          command: 'revoke_privileges',
          input: {
            username: grantee,
            database: objectName,
            privileges: [privilege],
          },
        });
        void load();
      } catch (e) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    },
    [confirmAction, connectionId, load],
  );

  const supportsDropUser = databaseType === 'postgresql' || databaseType === 'mysql';

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
          <>
            {supportsDropUser && uniqueGrantees.length > 0 && (
              <div className="border-b border-edge px-3 py-2">
                <div className="text-[11px] font-medium text-fg-muted uppercase tracking-wide mb-1.5">
                  {t('privileges.users')}
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {uniqueGrantees.map((user) => (
                    <span
                      key={user}
                      className="inline-flex items-center gap-1 rounded-md bg-surface-raised px-2 py-0.5 text-xs text-fg"
                    >
                      {user}
                      <button
                        type="button"
                        className="ml-0.5 rounded p-0.5 text-fg-muted hover:bg-red-500/20 hover:text-red-400"
                        title={t('privileges.dropUser')}
                        onClick={() => void handleDropUser(user)}
                      >
                        <X className="h-3 w-3" />
                      </button>
                    </span>
                  ))}
                </div>
              </div>
            )}
            {actionError && (
              <div className="flex items-center gap-2 border-b border-edge bg-red-500/10 px-3 py-2">
                <span className="flex-1 text-xs text-red-400">{actionError}</span>
                <button
                  type="button"
                  className="rounded p-0.5 text-red-400 hover:bg-red-500/20"
                  onClick={() => setActionError(null)}
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            )}
            <table className="w-full text-left text-[12px]">
              <thead className="sticky top-0 bg-surface-alt text-[11px] uppercase text-fg-muted">
                <tr>
                  <th className="px-3 py-1.5 font-medium">{t('privileges.grantee')}</th>
                  <th className="px-3 py-1.5 font-medium">{t('privileges.object')}</th>
                  <th className="px-3 py-1.5 font-medium">{t('privileges.privilege')}</th>
                  {supportsDropUser && <th className="w-8 px-1 py-1.5 font-medium" />}
                </tr>
              </thead>
              <tbody>
                {grants.map((g, i) => (
                  <tr
                    key={`${g.grantee}-${g.objectName}-${g.privilege}-${i}`}
                    className="border-t border-edge group"
                  >
                    <td className="px-3 py-1.5 text-fg">{g.grantee}</td>
                    <td className="px-3 py-1.5 text-fg-secondary">
                      {g.objectSchema ? `${g.objectSchema}.` : ''}
                      {g.objectName}
                    </td>
                    <td className="px-3 py-1.5 text-fg-secondary">{g.privilege}</td>
                    {supportsDropUser && (
                      <td className="px-1 py-1.5">
                        <button
                          type="button"
                          className="invisible group-hover:visible rounded p-0.5 text-fg-muted hover:bg-red-500/20 hover:text-red-400"
                          title={t('privileges.revokePrivilege')}
                          onClick={() =>
                            void handleRevokePrivilege(g.grantee, g.privilege, g.objectName)
                          }
                        >
                          <Trash2 className="h-3 w-3" />
                        </button>
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </>
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
      {confirmDialog}
    </div>
  );
}
