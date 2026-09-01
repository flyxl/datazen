import { useCallback, useEffect, useRef, useState } from 'react';
import { Loader2, Play, RefreshCw } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { SqlEditor, type SqlEditorHandle } from '../../components/SqlEditor';
import { useI18n } from '../../hooks/useI18n';
import { CopyableError } from '../../components/ui/CopyableError';
import { databaseCommands } from '../../commands/database';
import { queryCommands } from '../../commands/query';
import { cn } from '../../lib/cn';
import { showNativeContextMenu } from '../../lib/nativeContextMenu';
import {
  buildObjectBrowserEditorMenuItems,
  buildObjectBrowserListMenuItems,
} from '../../lib/objectBrowserContextMenu';
import { formatSql } from '../../lib/sqlFormat';
import type { DatabaseObject, DatabaseObjectKind } from '../../types';

const KINDS: DatabaseObjectKind[] = ['function', 'procedure', 'trigger'];

interface ObjectBrowserProps {
  dbSessionId: string;
  databaseType?: string;
  database?: string | null;
}

export function ObjectBrowser({ dbSessionId, databaseType, database = null }: ObjectBrowserProps) {
  const { t } = useI18n();
  const [kind, setKind] = useState<DatabaseObjectKind>('function');
  const [objects, setObjects] = useState<DatabaseObject[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<DatabaseObject | null>(null);
  const [ddl, setDdl] = useState('');
  const [running, setRunning] = useState(false);
  const [runMessage, setRunMessage] = useState<string | null>(null);
  const editorRef = useRef<SqlEditorHandle>(null);

  const load = useCallback(
    async (nextKind: DatabaseObjectKind) => {
      setLoading(true);
      setError(null);
      try {
        const rows = await databaseCommands.getDatabaseObjects(dbSessionId, nextKind);
        setObjects(rows);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        setObjects([]);
      } finally {
        setLoading(false);
      }
    },
    [dbSessionId],
  );

  useEffect(() => {
    void load(kind);
  }, [kind, load]);

  const openObject = useCallback(
    async (obj: DatabaseObject) => {
      setSelected(obj);
      setRunMessage(null);
      try {
        const text = await databaseCommands.getObjectDdl(
          dbSessionId,
          obj.kind,
          obj.name,
          obj.schema,
        );
        setDdl(text);
      } catch (e) {
        setDdl(`-- ${e instanceof Error ? e.message : String(e)}`);
      }
    },
    [dbSessionId],
  );

  const copyObjectDdl = useCallback(
    async (obj: DatabaseObject) => {
      try {
        const text =
          selected?.name === obj.name && selected?.schema === obj.schema && ddl
            ? ddl
            : await databaseCommands.getObjectDdl(dbSessionId, obj.kind, obj.name, obj.schema);
        await navigator.clipboard.writeText(text);
      } catch (e) {
        setRunMessage(e instanceof Error ? e.message : String(e));
      }
    },
    [dbSessionId, ddl, selected],
  );

  const handleExecute = useCallback(async () => {
    if (!ddl.trim()) return;
    setRunning(true);
    setRunMessage(null);
    try {
      await queryCommands.executeQuery(
        dbSessionId,
        ddl,
        undefined,
        database,
        selected?.schema ?? null,
      );
      setRunMessage(t('objects.executeOk'));
      void load(kind);
    } catch (e) {
      setRunMessage(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  }, [database, dbSessionId, ddl, kind, load, selected?.schema, t]);

  const handleListContextMenu = useCallback(
    (e: React.MouseEvent, obj: DatabaseObject) => {
      e.preventDefault();
      e.stopPropagation();
      void showNativeContextMenu(
        buildObjectBrowserListMenuItems({
          labels: {
            open: t('objects.open'),
            copyName: t('common.copyName'),
            copyDdl: t('common.copyDdl'),
            refresh: t('objects.refresh'),
          },
          handlers: {
            onOpen: () => void openObject(obj),
            onCopyName: () => {
              void navigator.clipboard.writeText(obj.name);
            },
            onCopyDdl: () => {
              void copyObjectDdl(obj);
            },
            onRefresh: () => {
              void load(kind);
            },
          },
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [t, openObject, copyObjectDdl, load, kind],
  );

  const handleEditorContextMenu = useCallback(
    (e: MouseEvent, sqlText: string) => {
      const selection = editorRef.current?.getSelection() ?? '';
      void showNativeContextMenu(
        buildObjectBrowserEditorMenuItems({
          labels: {
            execute: t('query.execute'),
            format: t('query.format'),
            comment: t('query.comment'),
          },
          handlers: {
            onExecute: () => {
              void handleExecute();
            },
            onFormat: () => setDdl(formatSql(ddl, databaseType)),
            onComment: () => editorRef.current?.toggleLineComment(),
          },
          sqlText,
          hasSelection: selection.length > 0,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [t, handleExecute, ddl, databaseType],
  );

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
              onClick={() => {
                setKind(k);
                setSelected(null);
                setDdl('');
              }}
            >
              {k === 'function'
                ? t('objects.function')
                : k === 'procedure'
                  ? t('objects.procedure')
                  : t('objects.trigger')}
            </button>
          ))}
        </div>
        <div className="flex items-center justify-between px-2 py-1.5">
          <span className="text-[11px] text-fg-muted">{objects.length}</span>
          <button
            type="button"
            className="text-fg-muted hover:text-fg"
            onClick={() => void load(kind)}
          >
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
          {error && <CopyableError message={error} className="px-3 py-2 text-xs text-red-400" />}
          {!loading && objects.length === 0 && !error && (
            <div className="px-3 py-3 text-xs text-fg-muted">{t('objects.empty')}</div>
          )}
          {objects.map((obj) => (
            <button
              key={`${obj.schema ?? ''}.${obj.name}`}
              type="button"
              data-testid="object-browser-item"
              className={cn(
                'flex w-full flex-col items-start px-3 py-1.5 text-left text-[13px] hover:bg-surface-raised',
                selected?.name === obj.name && selected?.schema === obj.schema
                  ? 'bg-surface-raised text-fg'
                  : 'text-fg-secondary',
              )}
              onClick={() => void openObject(obj)}
              onContextMenu={(e) => handleListContextMenu(e, obj)}
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
            variant="run"
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
            ref={editorRef}
            value={ddl}
            onChange={setDdl}
            databaseType={databaseType}
            placeholder={t('objects.placeholder')}
            onContextMenu={handleEditorContextMenu}
          />
        </div>
        {runMessage && (
          <div className="border-t border-edge px-3 py-1.5 text-[11px] text-fg-muted">
            {runMessage}
          </div>
        )}
      </div>
    </div>
  );
}
