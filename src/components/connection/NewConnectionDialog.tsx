import { useCallback, useEffect, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import { X } from 'lucide-react';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { DbTypeBadge } from '../DbTypeBadge';
import { useConnectionStore } from '../../stores/connectionStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useSettings } from '../../hooks/useSettings';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { closeNewConnectionDialog, useConnectionEditorStore } from '../../lib/connectionEditor';
import { DB_REGISTRY, sortDbTypesByPopularity } from '../../lib/databaseTypes';
import { formatGroupLabel } from '../../lib/connectionGroups';
import { filterDbTypesByQuery } from '../../lib/filterDbTypes';
import { connectionCommands } from '../../commands/connection';
import { ConnectionFormBody } from './ConnectionFormBody';
import { useConnectionForm } from './useConnectionForm';
import { useConnectionClipboardFill } from './useConnectionClipboardFill';
import type { DatabaseType } from '../../types';

const ALL_DB_TYPES: { value: DatabaseType; label: string }[] = sortDbTypesByPopularity(
  (Object.entries(DB_REGISTRY) as [DatabaseType, (typeof DB_REGISTRY)[DatabaseType]][]).map(
    ([value, meta]) => ({
      value,
      label: meta.label,
    }),
  ),
);

export interface NewConnectionDialogProps {
  open: boolean;
  editId?: string | null;
  onClose?: () => void;
  onSaved?: () => void;
}

export function NewConnectionDialog({
  open,
  editId = null,
  onClose,
  onSaved,
}: NewConnectionDialogProps) {
  useSettings();
  const { t } = useI18n();

  const loadSettings = useSettingsStore((s) => s.loadSettings);
  useEffect(() => {
    if (!open) return;
    void loadSettings();
  }, [loadSettings, open]);

  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const connections = useConnectionStore((s) => s.connections);
  const fetchGroups = useConnectionStore((s) => s.fetchGroups);
  const groups = useConnectionStore((s) => s.groups);

  const [availableDrivers, setAvailableDrivers] = useState<string[] | null>(null);
  const [driverQuery, setDriverQuery] = useState('');

  useEffect(() => {
    if (!open) return;
    void fetchConnections();
    void fetchGroups();
    connectionCommands
      .getAvailableDrivers()
      .then(setAvailableDrivers)
      .catch(() => setAvailableDrivers(null));
  }, [fetchConnections, fetchGroups, open]);

  const handleClose = useCallback(() => {
    onClose?.();
    closeNewConnectionDialog();
  }, [onClose]);

  const form = useConnectionForm({
    editId,
    existingConnections: connections,
    onAfterSave: () => {
      onSaved?.();
      handleClose();
    },
  });

  useConnectionClipboardFill(form, {
    enabled: open && !editId,
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

  const dbTypes = useMemo(() => {
    const available = !availableDrivers
      ? ALL_DB_TYPES
      : ALL_DB_TYPES.filter((db) => availableDrivers.includes(db.value));
    return filterDbTypesByQuery(available, driverQuery);
  }, [availableDrivers, driverQuery]);

  const groupOptions = useMemo(
    () => [
      { value: '', label: t('newConn.noGroup') },
      ...groups.map((g) => ({ value: g, label: formatGroupLabel(g, t) })),
    ],
    [groups, t],
  );

  useEffect(() => {
    if (!open) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') handleClose();
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open, handleClose]);

  if (!open) return null;

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <button
        type="button"
        aria-label={t('common.close')}
        className="absolute inset-0 bg-black/50"
        onClick={handleClose}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={editId ? t('newConn.editTitle') : t('newConn.title')}
        data-testid="new-connection-dialog"
        className="relative z-10 flex h-[min(680px,85vh)] w-full max-w-4xl flex-col overflow-hidden rounded-xl border border-edge bg-surface-alt text-fg shadow-xl"
      >
        <header className="flex shrink-0 items-center justify-between gap-3 border-b border-edge px-5 py-3">
          <div className="truncate text-sm font-semibold text-fg">
            {editId ? t('newConn.editTitle') : t('newConn.title')}
          </div>
          <button
            type="button"
            onClick={handleClose}
            className="rounded-md p-1 text-fg-muted hover:bg-surface-raised hover:text-fg"
            aria-label={t('common.close')}
          >
            <X className="h-4 w-4" />
          </button>
        </header>

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
              <Button
                variant="secondary"
                onClick={() => void form.onTest()}
                disabled={form.testing}
              >
                {form.testing ? t('newConn.testing') : t('newConn.testConnection')}
              </Button>
              <Button variant="secondary" onClick={handleClose}>
                {t('common.cancel')}
              </Button>
              <Button variant="primary" onClick={() => void form.onSave()}>
                {t('common.save')}
              </Button>
            </footer>
          </main>
        </div>
      </div>
    </div>,
    document.body,
  );
}

/** Global dialog wired to {@link useConnectionEditorStore}. Mount once in MainPage. */
export function ConnectionEditorDialogHost() {
  const open = useConnectionEditorStore((s) => s.open);
  const editId = useConnectionEditorStore((s) => s.editId);
  return <NewConnectionDialog open={open} editId={editId} />;
}
