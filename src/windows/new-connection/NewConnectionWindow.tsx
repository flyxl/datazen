import { useEffect, useMemo, useState } from 'react';
import { Database } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { Button } from '../../components/ui/Button';
import { useConnectionStore } from '../../stores/connectionStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { getUrlParam } from '../../lib/windowKind';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { ConnectionFormBody } from '../../components/connection/ConnectionFormBody';
import { useConnectionForm } from '../../components/connection/useConnectionForm';
import type { DatabaseType } from '../../types';

const DB_TYPES: { value: DatabaseType; label: string; color: string }[] = (
  Object.entries(DB_REGISTRY) as [DatabaseType, (typeof DB_REGISTRY)[DatabaseType]][]
).map(([value, meta]) => ({
  value,
  label: meta.label,
  color: meta.iconBg,
}));

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

  useEffect(() => {
    void fetchConnections();
    void fetchGroups();
  }, [fetchConnections, fetchGroups]);

  const form = useConnectionForm({
    editId,
    existingConnections: connections,
    onAfterSave: closeWindow,
  });

  const groupOptions = useMemo(
    () => [
      { value: '', label: t('newConn.noGroup') },
      ...groups.map((g) => ({ value: g, label: g })),
    ],
    [groups, t],
  );

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface-alt text-fg">
      <TitleBar title={editId ? t('newConn.editTitle') : t('newConn.title')} />

      <div className="flex min-h-0 flex-1">
        <aside className="flex w-[200px] shrink-0 flex-col border-r border-edge bg-surface p-4">
          <div className="mb-3 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
            {t('newConn.selectDbType')}
          </div>
          <div className="flex flex-col gap-1.5">
            {DB_TYPES.map((db) => (
              <button
                key={db.value}
                type="button"
                onClick={() => form.handleDatabaseTypeChange(db.value)}
                className={cn(
                  'flex items-center gap-3 rounded-lg px-3 py-3 text-left text-sm transition-colors',
                  form.databaseType === db.value
                    ? 'bg-surface-raised text-fg'
                    : 'text-fg-secondary hover:bg-surface-alt hover:text-fg',
                )}
              >
                <div
                  className="flex h-8 w-8 items-center justify-center rounded"
                  style={{ backgroundColor: `${db.color}20` }}
                >
                  <Database className="h-4 w-4" style={{ color: db.color }} />
                </div>
                <div>
                  <div className="font-medium">{db.label}</div>
                </div>
              </button>
            ))}
          </div>
        </aside>

        <main className="flex min-w-0 flex-1 flex-col">
          <div className="flex-1 overflow-y-auto p-6">
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
