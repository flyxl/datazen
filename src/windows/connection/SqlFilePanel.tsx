import { useCallback, useMemo, useRef, useState } from 'react';
import { CheckCircle2, FileCode, Loader2, Play, Square, XCircle } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { SqlEditor } from '../../components/SqlEditor';
import type { SqlEditorHandle } from '../../components/SqlEditor';
import { buildEditorSchema } from '../../lib/buildEditorSchema';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { usePanelStore } from '../../stores/panelStore';
import { useQueryExec } from '../../hooks/useQueryExec';
import { useSchemaStore } from '../../stores/schemaStore';
import { useI18n } from '../../hooks/useI18n';
import { useResizable } from '../../hooks/useResizable';
import { cn } from '../../lib/cn';
import type { DatabaseType, StatementResult } from '../../types';

interface SqlFilePanelProps {
  panelId: string;
  connectionId: string;
  databaseType: DatabaseType;
  fileName: string;
  sql: string;
}

export function SqlFilePanel({
  panelId,
  connectionId,
  databaseType,
  fileName,
  sql,
}: SqlFilePanelProps) {
  const { t } = useI18n();
  const exec = useQueryExec(panelId);
  const editorRef = useRef<SqlEditorHandle>(null);
  const { executeQuery, cancelQuery } = usePanelStore.getState();

  const tables = useSchemaStore((s) => s.tables);
  const views = useSchemaStore((s) => s.views);
  const columnMap = useSchemaStore((s) => s.columnMap);
  const namespaceTree = useSchemaStore((s) => s.namespaceTree);
  const currentDatabase = useSchemaStore((s) => s.currentDatabase);

  const editorSchema = useMemo(
    () => buildEditorSchema({ namespaceTree, tables, views, columnMap, currentDatabase }),
    [namespaceTree, tables, views, columnMap, currentDatabase],
  );
  const [activeResultIdx, setActiveResultIdx] = useState(0);

  const { size: editorHeight, handleRef } = useResizable({
    direction: 'vertical',
    initialSize: 250,
    minSize: 80,
    maxSize: 600,
    storageKey: 'sql-file-editor-height',
  });

  const handleRun = useCallback(() => {
    void executeQuery(panelId);
  }, [executeQuery, panelId]);

  const handleStop = useCallback(() => {
    void cancelQuery(panelId);
  }, [cancelQuery, panelId]);

  const sqlRef = useRef(false);
  if (!sqlRef.current) {
    sqlRef.current = true;
    usePanelStore.getState().updateSql(panelId, sql);
  }

  const results = exec?.results ?? [];
  const running = exec?.running ?? false;
  const error = exec?.error ?? null;
  const totalTime = exec?.executionTimeMs ?? null;

  const activeResult: StatementResult | undefined = results[activeResultIdx];
  const columns: ColumnDef[] = useMemo(
    () =>
      activeResult?.columns?.map((col) => ({
        id: col.name,
        name: col.name,
        type: col.dataType,
      })) ?? [],
    [activeResult],
  );

  const stmtCount = results.length;
  const totalRows = results.reduce((acc, r) => acc + (r.rowsAffected ?? r.rows?.length ?? 0), 0);

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* Toolbar */}
      <div className="flex shrink-0 items-center gap-2 border-b border-edge px-3 py-1.5">
        <FileCode className="h-4 w-4 text-sky-400" />
        <span className="text-sm font-medium truncate">{fileName}</span>
        <div className="flex-1" />
        {running ? (
          <Button size="sm" variant="danger" onClick={handleStop}>
            <Square className="mr-1 h-3 w-3" />
            {t('query.stop')}
          </Button>
        ) : (
          <Button size="sm" variant="primary" onClick={handleRun}>
            <Play className="mr-1 h-3 w-3" />
            {t('query.run')}
          </Button>
        )}
      </div>

      {/* Editor (read-only) */}
      <div style={{ height: editorHeight }} className="shrink-0 overflow-hidden">
        <SqlEditor
          ref={editorRef}
          value={sql}
          onChange={() => {}}
          schema={editorSchema}
          databaseType={databaseType}
        />
      </div>

      {/* Resize handle */}
      <div
        ref={handleRef}
        className="h-1 shrink-0 cursor-row-resize bg-edge hover:bg-accent/50 transition-colors"
      />

      {/* Results area */}
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {/* Result tabs + status */}
        <div className="flex shrink-0 items-center gap-2 border-b border-edge px-3 py-1">
          {results.length > 1 &&
            results.map((_r, idx) => (
              <button
                key={idx}
                type="button"
                className={cn(
                  'px-2 py-0.5 text-xs rounded transition-colors',
                  idx === activeResultIdx
                    ? 'bg-accent text-accent-fg'
                    : 'text-fg-muted hover:text-fg hover:bg-surface-alt',
                )}
                onClick={() => setActiveResultIdx(idx)}
              >
                #{idx + 1}
              </button>
            ))}
          <div className="flex-1" />
          {running && (
            <span className="flex items-center gap-1 text-xs text-fg-muted">
              <Loader2 className="h-3 w-3 animate-spin" />
              {t('query.executing')}
            </span>
          )}
          {!running && error && (
            <span className="flex items-center gap-1 text-xs text-red-400">
              <XCircle className="h-3 w-3" />
              {error}
            </span>
          )}
          {!running && !error && results.length > 0 && (
            <span className="flex items-center gap-1 text-xs text-green-400">
              <CheckCircle2 className="h-3 w-3" />
              {stmtCount} {stmtCount === 1 ? 'statement' : 'statements'} · {totalRows} rows
              {totalTime != null && ` · ${totalTime}ms`}
            </span>
          )}
        </div>

        {/* Data table */}
        {activeResult?.rows && activeResult.rows.length > 0 ? (
          <DataTable
            columns={columns}
            rows={activeResult.rows}
            connectionId={connectionId}
            databaseType={databaseType}
          />
        ) : (
          !running &&
          !error && (
            <div className="flex flex-1 items-center justify-center text-fg-muted text-sm">
              {results.length > 0
                ? activeResult?.rowsAffected != null
                  ? `${activeResult.rowsAffected} rows affected`
                  : t('sqlFile.noResults')
                : t('sqlFile.clickRun')}
            </div>
          )
        )}
      </div>
    </div>
  );
}
