import { useCallback, useEffect, useState } from 'react';
import { Loader2, Play, RefreshCw } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { SqlEditor } from '../../components/SqlEditor';
import { useI18n } from '../../hooks/useI18n';
import { databaseCommands } from '../../commands/database';
import { queryCommands } from '../../commands/query';
import { cn } from '../../lib/cn';
import type { DatabaseObject, DatabaseObjectKind } from '../../types';

const KINDS: DatabaseObjectKind[] = ['function', 'procedure', 'trigger'];

interface ObjectBrowserProps {
  connectionId: string;
  databaseType?: string;
}

export function ObjectBrowser({ connectionId, databaseType }: ObjectBrowserProps) {
  const { t } = useI18n();
  const [kind, setKind] = useState<DatabaseObjectKind>('function');
  const [objects, setObjects] = useState<DatabaseObject[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<DatabaseObject | null>(null);
  const [ddl, setDdl] = useState('');
  const [running, setRunning] = useState(false);
  const [runMessage, setRunMessage] = useState<string | null>(null);

  const load = useCallback(async (nextKind: DatabaseObjectKind) => {
    setLoading(true);
    setError(null);
    try {
      const rows = await databaseCommands.getDatabaseObjects(connectionId, nextKind);
      setObjects(rows);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setObjects([]);
    } finally {
      setLoading(false);
    }
  }, [connectionId]);

  useEffect(() => {
    void load(kind);
  }, [kind, load]);

  const openObject = useCallback(async (obj: DatabaseObject) => {
    setSelected(obj);
    setRunMessage(null);
    try {
      const text = await databaseCommands.getObjectDdl(connectionId, obj.kind, obj.name, obj.schema);
      setDdl(text);
    } catch (e) {
      setDdl(`-- ${e instanceof Error ? e.message : String(e)}`);
    }
  }, [connectionId]);

  const handleExecute = useCallback(async () => {
    if (!ddl.trim()) return;
    setRunning(true);
    setRunMessage(null);
    try {
      await queryCommands.executeQuery(connectionId, ddl);
      setRunMessage(t('objects.executeOk'));
      void load(kind);
    } catch (e) {
      setRunMessage(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  }, [connectionId, ddl, kind, load, t]);

  return (
    <div className="flex min-h-0 flex-1">
      <aside className="flex w-64 shrink-0 flex-col border-r border-edge bg-surface-alt">
        <div className="flex gap-1 border-b border-edge p-2">
          {KINDS.map((k) => (
            <button
              key={k}
              type="button"
              className={cn(
                'flex-1 rounded px-1.5 py-1 text-[11px]',
                kind === k ? 'bg-accent/15 text-accent' : 'text-fg-muted hover:text-fg',
              )}
              onClick={() => { setKind(k); setSelected(null); setDdl(''); }}
            >
              {k === 'function' ? t('objects.function') : k === 'procedure' ? t('objects.procedure') : t('objects.trigger')}
            </button>
          ))}
        </div>
        <div className="flex items-center justify-between px-2 py-1.5">
          <span className="text-[11px] text-fg-muted">{objects.length}</span>
          <button type="button" className="text-fg-muted hover:text-fg" onClick={() => void load(kind)}>
            <RefreshCw className="h-3.5 w-3.5" />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {loading && (
            <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t('common.loading')}
            </div>
          )}
          {error && <div className="px-3 py-2 text-xs text-red-400">{error}</div>}
          {!loading && objects.length === 0 && !error && (
            <div className="px-3 py-3 text-xs text-fg-muted">{t('objects.empty')}</div>
          )}
          {objects.map((obj) => (
            <button
              key={`${obj.schema ?? ''}.${obj.name}`}
              type="button"
              className={cn(
                'flex w-full flex-col items-start px-3 py-1.5 text-left text-[13px] hover:bg-surface-raised',
                selected?.name === obj.name && selected?.schema === obj.schema
                  ? 'bg-surface-raised text-fg'
                  : 'text-fg-secondary',
              )}
              onClick={() => void openObject(obj)}
            >
              <span className="truncate">{obj.name}</span>
              {obj.schema && <span className="text-[10px] text-fg-muted">{obj.schema}</span>}
            </button>
          ))}
        </div>
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge px-3">
          <span className="text-xs text-fg-muted">
            {selected ? `${selected.kind} · ${selected.name}` : t('objects.pick')}
          </span>
          <div className="flex-1" />
          <Button
            variant="primary"
            className="h-7 gap-1 px-2 text-xs"
            disabled={running || !ddl.trim()}
            onClick={() => void handleExecute()}
          >
            <Play className="h-3.5 w-3.5" />
            {t('query.execute')}
          </Button>
        </div>
        <div className="min-h-0 flex-1">
          <SqlEditor
            value={ddl}
            onChange={setDdl}
            databaseType={databaseType}
            placeholder={t('objects.placeholder')}
          />
        </div>
        {runMessage && (
          <div className="border-t border-edge px-3 py-1.5 text-[11px] text-fg-muted">{runMessage}</div>
        )}
      </div>
    </div>
  );
}
