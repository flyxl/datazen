import { useEffect, useMemo, useState } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { DbTypeBadge } from '../../components/DbTypeBadge';
import { useConnectionStore } from '../../stores/connectionStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { getUrlParam } from '../../lib/windowKind';
import { DB_REGISTRY, sortDbTypesByPopularity } from '../../lib/databaseTypes';
import { formatGroupLabel } from '../../lib/connectionGroups';
import { filterDbTypesByQuery } from '../../lib/filterDbTypes';
import { connectionCommands } from '../../commands/connection';
import { ConnectionFormBody } from '../../components/connection/ConnectionFormBody';
import { useConnectionForm } from '../../components/connection/useConnectionForm';
import { useConnectionClipboardFill } from '../../components/connection/useConnectionClipboardFill';
import type { DatabaseType } from '../../types';

const ALL_DB_TYPES: { value: DatabaseType; label: string }[] = sortDbTypesByPopularity(
  (Object.entries(DB_REGISTRY) as [DatabaseType, (typeof DB_REGISTRY)[DatabaseType]][]).map(
    ([value, meta]) => ({
      value,
      label: meta.label,
    }),
  ),
);

function closeWindow() {
  if ('__TAURI_INTERNALS__' in window) {
    import('@tauri-apps/api/window').then(({ getCurrentWindow }) => {
      void getCurrentWindow().close();
    });
  } else {
    window.close();
  }
}

export function NewConnectionWindow() {
  useThemeListener();
  const { t } = useI18n();

  const loadSettings = useSettingsStore((s) => s.loadSettings);
  useEffect(() => { void loadSettings(); }, [loadSettings]);

  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const connections = useConnectionStore((s) => s.connections);
  const fetchGroups = useConnectionStore((s) => s.fetchGroups);
  const groups = useConnectionStore((s) => s.groups);

  const [editId] = useState(() => getUrlParam('editId'));
  const [availableDrivers, setAvailableDrivers] = useState<string[] | null>(null);
  const [driverQuery, setDriverQuery] = useState('');

  useEffect(() => {
    void fetchConnections();
    void fetchGroups();
    connectionCommands.getAvailableDrivers()
      .then(setAvailableDrivers)
      .catch(() => setAvailableDrivers(null));
  }, [fetchConnections, fetchGroups]);

  const dbTypes = useMemo(() => {
    const available = !availableDrivers
      ? ALL_DB_TYPES
      : ALL_DB_TYPES.filter((db) => availableDrivers.includes(db.value));
    return filterDbTypesByQuery(available, driverQuery);
  }, [availableDrivers, driverQuery]);

  const form = useConnectionForm({
    editId,
    existingConnections: connections,
    onAfterSave: closeWindow,
  });

  useConnectionClipboardFill(form, {
    enabled: !editId,
    availableTypes: availableDrivers,
    onApplied: (databaseType) => {
      setDriverQuery('');
      window.setTimeout(() => {
        document
          .querySelector(`[data-testid="new-conn-driver-${databaseType}"]`)
          ?.scrollIntoView({ block: 'nearest' });
      }, 0);
    },
  });

  const groupOptions = useMemo(
    () => [
      { value: '', label: t('newConn.noGroup') },
      ...groups.map((g) => ({ value: g, label: formatGroupLabel(g, t) })),
    ],
    [groups, t],
  );

  return (
    <div className="flex h-screen min-h-0 flex-col overflow-hidden bg-surface-alt text-fg">
      <TitleBar title={editId ? t('newConn.editTitle') : t('newConn.title')} />

      <div className="flex min-h-0 flex-1">
        <aside className="flex w-[220px] shrink-0 min-h-0 flex-col border-r border-edge bg-surface">
          <div className="shrink-0 space-y-2 p-4 pb-2">
            <div className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              {t('newConn.selectDbType')}
            </div>
            <Input
              value={driverQuery}
              onChange={(e) => setDriverQuery(e.target.value)}
              placeholder={t('newConn.searchDrivers')}
              aria-label={t('newConn.searchDrivers')}
              className="h-8 text-sm"
            />
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
            <div className="flex flex-col gap-0.5">
              {dbTypes.length === 0 ? (
                <div className="px-2.5 py-2 text-sm text-fg-muted">
                  {t('newConn.noDriversMatch')}
                </div>
              ) : (
                dbTypes.map((db) => (
                  <button
                    key={db.value}
                    type="button"
                    data-testid={`new-conn-driver-${db.value}`}
                    onClick={() => form.handleDatabaseTypeChange(db.value)}
                    className={cn(
                      'flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-left text-sm transition-colors select-none',
                      form.databaseType === db.value
                        ? 'bg-surface-raised text-fg'
                        : 'text-fg-secondary hover:bg-surface-alt hover:text-fg',
                    )}
                  >
                    <DbTypeBadge databaseType={db.value} size={24} />
                    <div className="font-medium">{db.label}</div>
                  </button>
                ))
              )}
            </div>
          </div>
        </aside>

        <main className="flex min-h-0 min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-1 overflow-y-auto p-6">
            <div className="mb-4 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              {t('newConn.connectionConfig')}
            </div>
            <ConnectionFormBody form={form} groupOptions={groupOptions} variant="window" />
          </div>

          <footer className="flex shrink-0 items-center justify-end gap-3 border-t border-edge bg-surface-alt px-6 py-3">
            <Button variant="secondary" onClick={() => void form.onTest()} disabled={form.testing}>
              {form.testing ? t('newConn.testing') : t('newConn.testConnection')}
            </Button>
            <Button variant="secondary" onClick={closeWindow}>
              {t('common.cancel')}
            </Button>
            <Button variant="primary" onClick={() => void form.onSave()}>
              {t('common.save')}
            </Button>
          </footer>
        </main>
      </div>
    </div>
  );
}
