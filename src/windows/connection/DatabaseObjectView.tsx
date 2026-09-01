import { useCallback, useEffect, useRef, useState } from 'react';
import { Loader2, Play, RefreshCw } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { SqlEditor, type SqlEditorHandle } from '../../components/SqlEditor';
import { useI18n } from '../../hooks/useI18n';
import { databaseCommands } from '../../commands/database';
import { queryCommands } from '../../commands/query';
import type { DatabaseObjectKind } from '../../types';

interface DatabaseObjectViewProps {
  dbSessionId: string;
  databaseType?: string;
  database?: string | null;
  objectKind: DatabaseObjectKind;
  objectName: string;
  objectSchema?: string;
}

export function DatabaseObjectView({
  dbSessionId,
  databaseType,
  database = null,
  objectKind,
  objectName,
  objectSchema,
}: DatabaseObjectViewProps) {
  const { t } = useI18n();
  const [ddl, setDdl] = useState('');
  const [loading, setLoading] = useState(true);
  const [running, setRunning] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const editorRef = useRef<SqlEditorHandle>(null);

  const loadDdl = useCallback(async () => {
    setLoading(true);
    setMessage(null);
    try {
      const text = await databaseCommands.getObjectDdl(
        dbSessionId,
        objectKind,
        objectName,
        objectSchema,
      );
      setDdl(text);
    } catch (e) {
      setDdl(`-- ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLoading(false);
    }
  }, [dbSessionId, objectKind, objectName, objectSchema]);

  useEffect(() => {
    void loadDdl();
  }, [loadDdl]);

  const handleExecute = useCallback(async () => {
    if (!ddl.trim()) return;
    setRunning(true);
    setMessage(null);
    try {
      await queryCommands.executeQuery(dbSessionId, ddl, undefined, database, objectSchema ?? null);
      setMessage(t('objects.executeOk'));
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  }, [database, dbSessionId, ddl, objectSchema, t]);

  const kindLabel =
    objectKind === 'function'
      ? t('objects.function')
      : objectKind === 'procedure'
        ? t('objects.procedure')
        : t('objects.trigger');

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge px-3">
        <span className="text-xs text-fg-muted">
          {kindLabel} · {objectSchema ? `${objectSchema}.${objectName}` : objectName}
        </span>
        <div className="flex-1" />
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          onClick={() => void loadDdl()}
        >
          <RefreshCw className="h-3.5 w-3.5" />
        </Button>
        <Button
          variant="run"
          className="h-7 gap-1 px-2 text-xs"
          disabled={running || !ddl.trim() || loading}
          onClick={() => void handleExecute()}
        >
          <Play className="h-3.5 w-3.5" />
          {t('query.execute')}
        </Button>
      </div>
      {loading ? (
        <div className="flex flex-1 items-center justify-center text-fg-muted">
          <Loader2 className="h-5 w-5 animate-spin" />
        </div>
      ) : (
        <div className="min-h-0 flex-1">
          <SqlEditor ref={editorRef} value={ddl} onChange={setDdl} databaseType={databaseType} />
        </div>
      )}
      {message && (
        <div className="border-t border-edge px-3 py-1.5 text-[11px] text-fg-muted">{message}</div>
      )}
    </div>
  );
}
