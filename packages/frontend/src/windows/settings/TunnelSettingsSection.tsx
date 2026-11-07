import { useCallback, useEffect, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { ErrorBanner } from '../../components/ui/ErrorBanner';
import { connectionCommands } from '../../commands/connection';
import { tunnelCommands } from '../../commands/tunnel';
import { useI18n } from '../../hooks/useI18n';
import { deleteTunnelAndUnbind, type DeleteTunnelOutcome } from '../../lib/tunnelDeletion';
import { copyTunnelName } from '../../lib/tunnelDraft';
import { useConnectionStore } from '../../stores/connectionStore';
import { newTunnelId, useTunnelStore } from '../../stores/tunnelStore';
import type { SavedTunnelSummary, TunnelUsage } from '../../types';
import { SectionTitle } from './settingsUi';
import { TunnelDeleteDialog } from './TunnelDeleteDialog';
import { TunnelEditDialog } from './TunnelEditDialog';
import { TunnelTestDialog } from './TunnelTestDialog';

function messageOf(e: unknown, fallback: string): string {
  if (typeof e === 'string' && e) return e;
  if (e instanceof Error && e.message) return e.message;
  return fallback;
}

/**
 * Settings → Tunnels: the management surface for saved tunnels (G2).
 *
 * The list renders exclusively from `useTunnelStore().summaries` — the
 * secret-free `get_tunnel_summaries` projection. `getTunnels` (which returns
 * decrypted credentials) is never called from a render path; the only full-entity
 * fetch is `getTunnel(id)` when the edit dialog opens (G9).
 */
export function TunnelSettingsSection() {
  const { t } = useI18n();

  const summaries = useTunnelStore((s) => s.summaries);
  const loaded = useTunnelStore((s) => s.loaded);
  const loading = useTunnelStore((s) => s.loading);
  const storeError = useTunnelStore((s) => s.error);
  const load = useTunnelStore((s) => s.load);
  const createTunnel = useTunnelStore((s) => s.create);

  const [usageById, setUsageById] = useState<Record<string, TunnelUsage>>({});
  const [usagePending, setUsagePending] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  const [editTarget, setEditTarget] = useState<{ id: string | null } | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<SavedTunnelSummary | null>(null);
  const [testTarget, setTestTarget] = useState<SavedTunnelSummary | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  /**
   * Reference counts are fetched as one concurrent batch rather than per row:
   * a serial per-row fetch would be an N+1 stall on a settings page, and the
   * collection is small enough that `allSettled` keeps a single failing probe
   * from hiding the counts of every other tunnel.
   */
  useEffect(() => {
    if (summaries.length === 0) {
      setUsageById({});
      setUsagePending(false);
      return;
    }
    let cancelled = false;
    setUsagePending(true);
    void Promise.allSettled(
      summaries.map(
        async (summary) => [summary.id, await tunnelCommands.getTunnelUsage(summary.id)] as const,
      ),
    )
      .then((results) => {
        if (cancelled) return;
        const next: Record<string, TunnelUsage> = {};
        for (const result of results) {
          if (result.status === 'fulfilled') next[result.value[0]] = result.value[1];
        }
        setUsageById(next);
      })
      .finally(() => {
        if (!cancelled) setUsagePending(false);
      });
    return () => {
      cancelled = true;
    };
  }, [summaries]);

  const handleCopy = useCallback(
    async (summary: SavedTunnelSummary) => {
      setBusyId(summary.id);
      setActionError(null);
      try {
        const entity = await tunnelCommands.getTunnel(summary.id);
        if (!entity) {
          setActionError(t('settings.tunnels.copyFailed'));
          await load(true);
          return;
        }
        await createTunnel({
          ...entity,
          id: newTunnelId(),
          name: copyTunnelName(entity.name, t('settings.tunnels.copySuffix')),
        });
      } catch (e) {
        setActionError(messageOf(e, t('settings.tunnels.copyFailed')));
      } finally {
        setBusyId(null);
      }
    },
    [createTunnel, load, t],
  );

  /**
   * Unbind first, delete second (`deleteTunnelAndUnbind`). The backend delete
   * does not clean up references, so this ordering is what keeps a delete from
   * leaving dangling `tunnelId`s behind.
   */
  const performDelete = useCallback(
    async (id: string): Promise<DeleteTunnelOutcome> => {
      const outcome = await deleteTunnelAndUnbind(id, {
        getConnections: () => connectionCommands.getConnections(),
        saveConnection: (config) => connectionCommands.saveConnection(config),
        deleteTunnel: (tunnelId) => useTunnelStore.getState().remove(tunnelId),
      });
      if (outcome.kind !== 'unbind-failed') {
        // The unbound connections changed on disk: refresh the navigator's view.
        await useConnectionStore.getState().fetchConnections();
      }
      await load(true);
      return outcome;
    },
    [load],
  );

  const usageText = (id: string): string => {
    const usage = usageById[id];
    if (!usage)
      return t(usagePending ? 'settings.tunnels.usagePending' : 'settings.tunnels.usageNone');
    if (usage.connectionNames.length === 0) return t('settings.tunnels.usageNone');
    return t('settings.tunnels.usageCount', { count: usage.connectionNames.length });
  };

  const usageTitle = (id: string): string | undefined => {
    const names = usageById[id]?.connectionNames;
    return names && names.length > 0 ? names.join(', ') : undefined;
  };

  const showEmpty = loaded && summaries.length === 0;

  return (
    <div className="space-y-4" data-testid="settings-tunnels-section">
      <div className="flex items-start justify-between gap-4">
        <SectionTitle hint={t('settings.tunnels.description')}>
          {t('settings.tunnels.title')}
        </SectionTitle>
        <Button
          size="sm"
          variant="secondary"
          onClick={() => setEditTarget({ id: null })}
          data-testid="tunnel-create"
        >
          {t('settings.tunnels.create')}
        </Button>
      </div>

      {storeError ? (
        <div className="flex items-center gap-3" data-testid="tunnel-load-error">
          <ErrorBanner as="p">{storeError}</ErrorBanner>
          <Button size="sm" variant="ghost" onClick={() => void load(true)}>
            {t('common.retry')}
          </Button>
        </div>
      ) : null}

      {actionError ? (
        <ErrorBanner as="p" data-testid="tunnel-action-error">
          {actionError}
        </ErrorBanner>
      ) : null}

      {loading && !loaded ? (
        <p className="text-xs text-fg-muted" data-testid="tunnel-loading">
          {t('common.loading')}
        </p>
      ) : null}

      {showEmpty ? (
        <div
          className="space-y-2 rounded-md border border-dashed border-edge p-4"
          data-testid="tunnel-list-empty"
        >
          <p className="text-sm text-fg">{t('settings.tunnels.empty')}</p>
          <p className="text-xs text-fg-muted">{t('settings.tunnels.emptyHint')}</p>
        </div>
      ) : null}

      {summaries.length > 0 ? (
        <ul className="space-y-2" data-testid="tunnel-list">
          {summaries.map((summary) => (
            <li
              key={summary.id}
              data-testid="tunnel-row"
              data-tunnel-id={summary.id}
              className="flex items-center gap-3 rounded-md border border-edge bg-surface px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm text-fg" data-testid="tunnel-row-name">
                  {summary.name}
                </div>
                <div className="mt-0.5 flex items-center gap-2 text-[11px] text-fg-muted">
                  <span
                    className="rounded bg-accent/20 px-1.5 py-0.5 text-accent"
                    data-testid="tunnel-row-kind"
                  >
                    {t(`settings.tunnels.kind.${summary.kind}`)}
                  </span>
                  <span data-testid="tunnel-row-usage" title={usageTitle(summary.id)}>
                    {usageText(summary.id)}
                  </span>
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-1">
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setEditTarget({ id: summary.id })}
                  data-testid={`tunnel-edit-${summary.id}`}
                  data-tunnel-action="edit"
                >
                  {t('settings.tunnels.edit')}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={busyId === summary.id}
                  onClick={() => void handleCopy(summary)}
                  data-testid={`tunnel-copy-${summary.id}`}
                  data-tunnel-action="copy"
                >
                  {t('settings.tunnels.copy')}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setTestTarget(summary)}
                  data-testid={`tunnel-test-${summary.id}`}
                  data-tunnel-action="test"
                >
                  {t('settings.tunnels.test')}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  className="text-red-400 hover:text-red-300"
                  onClick={() => setDeleteTarget(summary)}
                  data-testid={`tunnel-delete-${summary.id}`}
                  data-tunnel-action="delete"
                >
                  {t('settings.tunnels.delete')}
                </Button>
              </div>
            </li>
          ))}
        </ul>
      ) : null}

      <TunnelEditDialog
        open={editTarget !== null}
        tunnelId={editTarget?.id ?? null}
        onClose={() => setEditTarget(null)}
        onSaved={() => setActionError(null)}
      />

      <TunnelDeleteDialog
        open={deleteTarget !== null}
        tunnel={deleteTarget}
        onClose={() => setDeleteTarget(null)}
        onConfirm={performDelete}
      />

      <TunnelTestDialog
        open={testTarget !== null}
        tunnel={testTarget}
        onClose={() => setTestTarget(null)}
      />
    </div>
  );
}
