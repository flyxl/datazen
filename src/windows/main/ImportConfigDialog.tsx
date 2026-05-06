import { useState, useMemo, useCallback } from 'react';
import { AlertTriangle, Check, Plus } from 'lucide-react';
import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';
import { connectionCommands } from '../../commands/connection';
import { useConnectionStore } from '../../stores/connectionStore';
import type { ConnectionConfig } from '../../types';
import { DB_REGISTRY } from '../../lib/databaseTypes';

type ConflictAction = 'skip' | 'overwrite' | 'keepBoth';

interface ConflictItem {
  incoming: ConnectionConfig;
  existing: ConnectionConfig;
  action: ConflictAction;
}

interface ImportConfigDialogProps {
  open: boolean;
  onClose: () => void;
  importData: { connections: ConnectionConfig[]; groups: string[] } | null;
  onResult: (message: string, isError?: boolean) => void;
}

export function ImportConfigDialog({ open, onClose, importData, onResult }: ImportConfigDialogProps) {
  const { t } = useI18n();
  const existingConnections = useConnectionStore((s) => s.connections);
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const fetchGroups = useConnectionStore((s) => s.fetchGroups);
  const [importing, setImporting] = useState(false);

  const { conflicts, newItems } = useMemo(() => {
    if (!importData) return { conflicts: [] as ConflictItem[], newItems: [] as ConnectionConfig[] };

    const existingById = new Map(existingConnections.map((c) => [c.id, c]));
    const existingByName = new Map(existingConnections.map((c) => [c.name, c]));

    const conflictList: ConflictItem[] = [];
    const newList: ConnectionConfig[] = [];

    for (const incoming of importData.connections) {
      const byId = existingById.get(incoming.id);
      const byName = !byId ? existingByName.get(incoming.name) : undefined;
      const existing = byId || byName;

      if (existing) {
        conflictList.push({ incoming, existing, action: 'overwrite' });
      } else {
        newList.push(incoming);
      }
    }

    return { conflicts: conflictList, newItems: newList };
  }, [importData, existingConnections]);

  const [conflictActions, setConflictActions] = useState<ConflictAction[]>([]);

  // Reset actions when conflicts change
  useMemo(() => {
    setConflictActions(conflicts.map((c) => c.action));
  }, [conflicts]);

  const setAction = useCallback((index: number, action: ConflictAction) => {
    setConflictActions((prev) => {
      const next = [...prev];
      next[index] = action;
      return next;
    });
  }, []);

  const setAllActions = useCallback((action: ConflictAction) => {
    setConflictActions((prev) => prev.map(() => action));
  }, []);

  const handleImport = useCallback(async () => {
    if (!importData) return;
    setImporting(true);
    try {
      let imported = 0;

      // Import new items directly
      for (const conn of newItems) {
        await connectionCommands.saveConnection(conn);
        imported++;
      }

      // Handle conflicts
      for (let i = 0; i < conflicts.length; i++) {
        const { incoming, existing } = conflicts[i];
        const action = conflictActions[i];

        if (action === 'skip') continue;

        if (action === 'overwrite') {
          const merged = { ...incoming, id: existing.id };
          await connectionCommands.saveConnection(merged);
          imported++;
        } else if (action === 'keepBoth') {
          const newId = `${incoming.id}_imported_${Date.now()}`;
          const newConn = { ...incoming, id: newId, name: `${incoming.name} (${t('configImport.incoming')})` };
          await connectionCommands.saveConnection(newConn);
          imported++;
        }
      }

      // Import new groups
      if (importData.groups?.length) {
        const existingGroups = await connectionCommands.getGroups();
        const allGroups = [...new Set([...existingGroups, ...importData.groups])];
        await connectionCommands.saveGroups(allGroups);
      }

      await fetchConnections();
      await fetchGroups();
      onResult(t('configImport.success', { count: imported }));
      onClose();
    } catch (e) {
      onResult(e instanceof Error ? e.message : String(e), true);
    } finally {
      setImporting(false);
    }
  }, [importData, newItems, conflicts, conflictActions, fetchConnections, fetchGroups, onResult, onClose, t]);

  const dbIcon = (type: string) => {
    const entry = DB_REGISTRY[type as keyof typeof DB_REGISTRY];
    return entry?.shortLabel ?? type;
  };

  const actionButtons = (index: number) => {
    const current = conflictActions[index];
    const actions: { key: ConflictAction; label: string }[] = [
      { key: 'skip', label: t('configImport.skip') },
      { key: 'overwrite', label: t('configImport.overwrite') },
      { key: 'keepBoth', label: t('configImport.keepBoth') },
    ];
    return (
      <div className="flex gap-1">
        {actions.map(({ key, label }) => (
          <button
            key={key}
            type="button"
            onClick={() => setAction(index, key)}
            className={cn(
              'rounded px-2 py-0.5 text-[11px] font-medium transition-colors',
              current === key
                ? key === 'skip' ? 'bg-zinc-500/20 text-zinc-400'
                  : key === 'overwrite' ? 'bg-amber-500/20 text-amber-400'
                  : 'bg-blue-500/20 text-blue-400'
                : 'bg-surface-raised text-fg-muted hover:text-fg',
            )}
          >
            {label}
          </button>
        ))}
      </div>
    );
  };

  const totalToImport = newItems.length + conflictActions.filter((a) => a !== 'skip').length;

  return (
    <Dialog
      open={open}
      title={t('configImport.title')}
      description={conflicts.length > 0 ? t('configImport.description') : undefined}
      onClose={onClose}
      className="max-w-2xl"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={importing}>
            {t('common.cancel')}
          </Button>
          <Button variant="primary" onClick={handleImport} disabled={importing || totalToImport === 0}>
            {importing ? t('configImport.importing') : `${t('common.confirm')} (${totalToImport})`}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {/* New connections (no conflict) */}
        {newItems.length > 0 && (
          <div>
            <div className="mb-2 flex items-center gap-2 text-xs font-medium text-green-400">
              <Plus className="h-3.5 w-3.5" />
              {t('configImport.new')} ({newItems.length})
            </div>
            <div className="space-y-1">
              {newItems.map((conn) => (
                <div key={conn.id} className="flex items-center gap-2 rounded-lg bg-surface px-3 py-2 text-[13px]">
                  <span className="rounded bg-green-500/20 px-1.5 py-0.5 text-[10px] font-bold text-green-400">
                    {dbIcon(conn.databaseType)}
                  </span>
                  <span className="font-medium text-fg">{conn.name}</span>
                  {conn.host && <span className="text-fg-muted">{conn.host}:{conn.port}</span>}
                  {conn.group && <span className="ml-auto rounded bg-surface-raised px-1.5 py-0.5 text-[10px] text-fg-muted">{conn.group}</span>}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Conflicts */}
        {conflicts.length > 0 && (
          <div>
            <div className="mb-2 flex items-center justify-between">
              <div className="flex items-center gap-2 text-xs font-medium text-amber-400">
                <AlertTriangle className="h-3.5 w-3.5" />
                {t('configImport.conflict')} ({conflicts.length})
              </div>
              <div className="flex items-center gap-1 text-[11px] text-fg-muted">
                <span>{t('configImport.applyAll')}:</span>
                <button type="button" onClick={() => setAllActions('skip')} className="rounded px-1.5 py-0.5 hover:bg-surface-raised">{t('configImport.skip')}</button>
                <button type="button" onClick={() => setAllActions('overwrite')} className="rounded px-1.5 py-0.5 hover:bg-surface-raised">{t('configImport.overwrite')}</button>
                <button type="button" onClick={() => setAllActions('keepBoth')} className="rounded px-1.5 py-0.5 hover:bg-surface-raised">{t('configImport.keepBoth')}</button>
              </div>
            </div>
            <div className="space-y-2">
              {conflicts.map((conflict, i) => (
                <div key={conflict.incoming.id} className="rounded-lg border border-edge bg-surface p-3">
                  <div className="mb-2 flex items-center justify-between">
                    <div className="flex items-center gap-2 text-[13px]">
                      <span className="rounded bg-amber-500/20 px-1.5 py-0.5 text-[10px] font-bold text-amber-400">
                        {dbIcon(conflict.incoming.databaseType)}
                      </span>
                      <span className="font-medium text-fg">{conflict.incoming.name}</span>
                    </div>
                    {actionButtons(i)}
                  </div>
                  <div className="grid grid-cols-2 gap-2 text-[11px]">
                    <div className="rounded bg-surface-alt px-2 py-1.5">
                      <div className="mb-0.5 font-medium text-fg-muted">{t('configImport.existing')}</div>
                      <div className="text-fg">{conflict.existing.name}</div>
                      {conflict.existing.host && <div className="text-fg-muted">{conflict.existing.host}:{conflict.existing.port}</div>}
                    </div>
                    <div className="rounded bg-surface-alt px-2 py-1.5">
                      <div className="mb-0.5 font-medium text-fg-muted">{t('configImport.incoming')}</div>
                      <div className="text-fg">{conflict.incoming.name}</div>
                      {conflict.incoming.host && <div className="text-fg-muted">{conflict.incoming.host}:{conflict.incoming.port}</div>}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* No conflicts summary */}
        {conflicts.length === 0 && newItems.length > 0 && (
          <div className="flex items-center gap-2 rounded-lg bg-green-500/10 px-4 py-3 text-sm text-green-400">
            <Check className="h-4 w-4" />
            {t('configImport.noConflicts', { count: newItems.length })}
          </div>
        )}
      </div>
    </Dialog>
  );
}
