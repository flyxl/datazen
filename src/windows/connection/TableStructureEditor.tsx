import { useCallback, useEffect, useMemo, useState } from 'react';
import { ArrowLeft, Download, Loader2, Plus } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { databaseCommands } from '../../commands/database';
import { queryCommands } from '../../commands/query';
import { structureCommands } from '../../commands/structure';
import { driverCommands } from '../../commands/driver';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { exportTableStructureToFile } from '../../lib/exportTableStructure';
import { buildStructureChangeRequest } from '../../lib/structureEditor/buildStructureChangeRequest';
import { capEnabled } from '../../lib/structureEditor/controlHints';
import {
  buildAlterApplyWarningMessage,
  shouldConfirmAlterApply,
} from '../../lib/structureEditor/ddlApplyWarnings';
import {
  defaultCreateColumns,
  emptyColumnDraft,
  emptyIndexDraft,
} from '../../lib/structureEditor/draftDefaults';
import { schemaToDraft } from '../../lib/structureEditor/schemaToDraft';
import { ESTIMATE_TABLE_ROWS_COMMAND } from '../../lib/driverCommandIds';
import { hasCommand } from '../../lib/commandSchema';
import type {
  StructureCapabilities,
  StructureChangePlan,
  StructureColumnDraft,
  StructureEditorUiConfig,
  StructureIndexDraft,
} from '../../lib/structureEditor/types';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import type { DatabaseType } from '../../types';
import { StructureColumnTable } from './structure/StructureColumnTable';
import { StructureIndexTable, suggestedIndexName } from './structure/StructureIndexTable';
import { StructurePlanPreview } from './structure/StructurePlanPreview';

interface TableStructureEditorProps {
  dbSessionId: string;
  databaseType: DatabaseType;
  /** Logical database for multi-db drivers; ensures session before plan/execute. */
  database?: string | null;
  /** SQL schema namespace for plan IPC (PG schema, MySQL database, etc.). */
  schema?: string | null;
  mode: 'create' | 'alter';
  tableName?: string;
  /**
   * When true (inline alter inside Structure sub-tab), show a Back control.
   * Primary-tab flows (create table) must not pass this — they use Cancel / close tab.
   */
  showBackButton?: boolean;
  onSuccess: () => void;
  onCancel: () => void;
}

function resolveUiConfig(databaseType: DatabaseType): StructureEditorUiConfig | null {
  const meta = DB_REGISTRY[databaseType];
  if (!meta?.supportsSQL) return null;
  const cfg = meta.structureEditor;
  if (!cfg || cfg.enabled === false) return null;
  return cfg;
}

function resolveIndexMethods(
  uiConfig: StructureEditorUiConfig,
  caps: StructureCapabilities | null,
): string[] {
  if (caps?.indexMethods?.length) return caps.indexMethods;
  return uiConfig.indexMethods;
}

async function executePlanStatements(
  dbSessionId: string,
  plan: StructureChangePlan,
  database?: string | null,
  schema?: string | null,
): Promise<{ executed: number; error?: string }> {
  let executed = 0;
  for (const stmt of plan.statements) {
    try {
      await queryCommands.executeQuery(
        dbSessionId,
        stmt.sql,
        undefined,
        database ?? null,
        schema ?? null,
      );
      executed += 1;
    } catch (e) {
      const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : 'Execution failed';
      return { executed, error: msg };
    }
  }
  return { executed };
}

async function fetchEstimatedTableRows(args: {
  dbSessionId: string;
  table: string;
  schema?: string | null;
  database?: string | null;
}): Promise<number | null> {
  try {
    const definitions = await driverCommands.getConnectionCommands(args.dbSessionId);
    if (!hasCommand(definitions, ESTIMATE_TABLE_ROWS_COMMAND)) {
      return null;
    }
    const input: Record<string, unknown> = { table: args.table };
    if (args.schema) input.schema = args.schema;
    const result = await driverCommands.execute({
      dbSessionId: args.dbSessionId,
      command: ESTIMATE_TABLE_ROWS_COMMAND,
      input,
      database: args.database ?? null,
      schema: args.schema ?? null,
    });
    const data = result.data;
    if (!data || typeof data !== 'object' || Array.isArray(data)) return null;
    const rows = (data as { estimatedRows?: unknown }).estimatedRows;
    return typeof rows === 'number' ? rows : null;
  } catch {
    return null;
  }
}

export function TableStructureEditor({
  dbSessionId,
  databaseType,
  database,
  schema: requestSchema = null,
  mode,
  tableName: initialTableName,
  showBackButton = false,
  onSuccess,
  onCancel,
}: TableStructureEditorProps) {
  const { t } = useI18n();
  const [confirmApply, confirmApplyDialog] = useConfirmDialog();
  const uiConfig = useMemo(() => resolveUiConfig(databaseType), [databaseType]);

  const [tableName, setTableName] = useState(initialTableName ?? '');
  const [columns, setColumns] = useState<StructureColumnDraft[]>([]);
  const [indexes, setIndexes] = useState<StructureIndexDraft[]>([]);
  const [originalColumns, setOriginalColumns] = useState<StructureColumnDraft[]>([]);
  const [originalIndexes, setOriginalIndexes] = useState<StructureIndexDraft[]>([]);
  const [caps, setCaps] = useState<StructureCapabilities | null>(null);
  const [loading, setLoading] = useState(true);
  const [previewing, setPreviewing] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [previewPlan, setPreviewPlan] = useState<StructureChangePlan | null>(null);
  const [dragIdx, setDragIdx] = useState<number | null>(null);
  const [exportingStructure, setExportingStructure] = useState(false);

  useEffect(() => {
    if (!uiConfig) {
      setLoading(false);
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);

    const capsPromise = structureCommands.getStructureCapabilities(dbSessionId);
    const schemaPromise =
      mode === 'alter' && initialTableName
        ? databaseCommands.getTableSchema(dbSessionId, initialTableName)
        : Promise.resolve(null);

    Promise.all([capsPromise, schemaPromise])
      .then(([loadedCaps, schema]) => {
        if (cancelled) return;
        setCaps(loadedCaps);

        if (schema) {
          const draft = schemaToDraft(schema);
          setColumns(draft.columns);
          setIndexes(draft.indexes);
          setOriginalColumns(draft.columns.map((c) => ({ ...c })));
          setOriginalIndexes(draft.indexes.map((i) => ({ ...i })));
        } else {
          setColumns(defaultCreateColumns(uiConfig));
          setIndexes([]);
          setOriginalColumns([]);
          setOriginalIndexes([]);
        }
        setLoading(false);
      })
      .catch((e) => {
        if (cancelled) return;
        let msg = t('common.loadTableStructureFailed');
        if (typeof e === 'string') msg = e;
        else if (e instanceof Error) msg = e.message;
        setError(msg);
        setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [dbSessionId, mode, initialTableName, uiConfig, t]);

  const originalById = useMemo(
    () => new Map(originalColumns.map((c) => [c.id, c])),
    [originalColumns],
  );

  const indexMethods = useMemo(
    () => (uiConfig ? resolveIndexMethods(uiConfig, caps) : []),
    [uiConfig, caps],
  );

  const reorderEnabled = capEnabled(caps, 'reorderColumn');
  const validColumns = columns.filter((c) => c.name.trim());
  const columnNames = validColumns.map((c) => c.name.trim());
  const isValid = tableName.trim().length > 0 && validColumns.length > 0;

  const buildRequest = useCallback(() => {
    if (!isValid) return null;
    return buildStructureChangeRequest({
      mode,
      table: tableName.trim(),
      schema: requestSchema,
      originalColumns,
      currentColumns: columns,
      originalIndexes,
      currentIndexes: indexes,
    });
  }, [mode, tableName, requestSchema, originalColumns, columns, originalIndexes, indexes, isValid]);

  const updateColumn = useCallback((id: string, patch: Partial<StructureColumnDraft>) => {
    setColumns((prev) => prev.map((c) => (c.id === id ? { ...c, ...patch } : c)));
    setPreviewPlan(null);
  }, []);

  const addColumn = useCallback(() => {
    if (!uiConfig) return;
    setColumns((prev) => [...prev, emptyColumnDraft(uiConfig.defaultColumnType)]);
    setPreviewPlan(null);
  }, [uiConfig]);

  const removeColumn = useCallback((id: string) => {
    setColumns((prev) => prev.filter((c) => c.id !== id));
    setPreviewPlan(null);
  }, []);

  const updateIndex = useCallback((id: string, patch: Partial<StructureIndexDraft>) => {
    setIndexes((prev) => prev.map((i) => (i.id === id ? { ...i, ...patch } : i)));
    setPreviewPlan(null);
  }, []);

  const addIndex = useCallback(() => {
    setIndexes((prev) => [
      ...prev,
      {
        ...emptyIndexDraft(indexMethods[0] ?? 'btree'),
        name: suggestedIndexName(tableName, []),
      },
    ]);
    setPreviewPlan(null);
  }, [indexMethods, tableName]);

  const removeIndex = useCallback((id: string) => {
    setIndexes((prev) => prev.filter((i) => i.id !== id));
    setPreviewPlan(null);
  }, []);

  const handleDragStart = useCallback((idx: number) => {
    setDragIdx(idx);
  }, []);

  const handleDragOver = useCallback(
    (e: React.DragEvent, targetIdx: number) => {
      e.preventDefault();
      if (dragIdx === null || dragIdx === targetIdx) return;
      setColumns((prev) => {
        const next = [...prev];
        const [moved] = next.splice(dragIdx, 1);
        next.splice(targetIdx, 0, moved);
        return next;
      });
      setDragIdx(targetIdx);
      setPreviewPlan(null);
    },
    [dragIdx],
  );

  const handlePreview = useCallback(async () => {
    const request = buildRequest();
    if (!request) return;
    setError(null);
    setPreviewing(true);
    try {
      const plan = await structureCommands.planTableStructureChanges(
        dbSessionId,
        request,
        database ?? null,
      );
      setPreviewPlan(plan);
    } catch (e) {
      const msg =
        typeof e === 'string'
          ? e
          : e instanceof Error
            ? e.message
            : t('structEditor.previewFailed');
      setError(msg);
    } finally {
      setPreviewing(false);
    }
  }, [buildRequest, dbSessionId, database, t]);

  const handleExecute = useCallback(async () => {
    const request = buildRequest();
    if (!request) return;
    setError(null);
    setExecuting(true);
    try {
      const plan = await structureCommands.planTableStructureChanges(
        dbSessionId,
        request,
        database ?? null,
      );
      if (plan.statements.length === 0) {
        setError(t('structEditor.noChanges'));
        return;
      }

      let estimatedRows: number | null = null;
      if (mode === 'alter' && initialTableName) {
        estimatedRows = await fetchEstimatedTableRows({
          dbSessionId,
          table: initialTableName,
          schema: requestSchema,
          database,
        });
      }

      if (shouldConfirmAlterApply({ mode, plan, estimatedRows })) {
        const ok = await confirmApply({
          title: t('structEditor.ddlWarn.title'),
          message: buildAlterApplyWarningMessage({ t, plan, estimatedRows }),
          confirmLabel: t('structEditor.ddlWarn.confirm'),
          kind: 'warning',
        });
        if (!ok) return;
      }

      const result = await executePlanStatements(dbSessionId, plan, database, requestSchema);
      if (result.error) {
        if (result.executed > 0) {
          setError(
            t('structEditor.executePartial', {
              executed: result.executed,
              total: plan.statements.length,
              error: result.error,
            }),
          );
        } else {
          setError(result.error);
        }
        return;
      }
      onSuccess();
    } catch (e) {
      const msg =
        typeof e === 'string' ? e : e instanceof Error ? e.message : t('common.executionFailed');
      setError(msg);
    } finally {
      setExecuting(false);
    }
  }, [
    buildRequest,
    dbSessionId,
    confirmApply,
    database,
    initialTableName,
    mode,
    onSuccess,
    requestSchema,
    t,
  ]);

  const handleExportStructure = useCallback(async () => {
    if (mode !== 'alter' || !initialTableName) return;
    setExportingStructure(true);
    setError(null);
    try {
      const result = await exportTableStructureToFile({
        dbSessionId,
        tableName: initialTableName,
        databaseType,
      });
      if (result === 'unsupported') {
        setError(t('structEditor.exportUnsupported'));
      }
    } catch (e) {
      const msg =
        typeof e === 'string' ? e : e instanceof Error ? e.message : t('structEditor.exportFailed');
      setError(msg);
    } finally {
      setExportingStructure(false);
    }
  }, [mode, initialTableName, dbSessionId, databaseType, t]);

  const canAddColumn =
    mode === 'create' ? capEnabled(caps, 'createTable') : capEnabled(caps, 'addColumn');
  const canAddIndex = capEnabled(caps, 'createIndex');

  if (!uiConfig) {
    return (
      <div className="flex flex-1 items-center justify-center px-6 text-sm text-fg-muted">
        {t('structEditor.notSupported')}
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
        <Loader2 className="h-5 w-5 animate-spin" />
        {t('common.loadingTableStructure')}
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-4 border-b border-edge bg-surface-alt px-4 py-3">
        {showBackButton && (
          <Button
            variant="ghost"
            className="h-8 gap-1 px-2 text-xs"
            data-testid="struct-editor-back"
            onClick={onCancel}
            title={t('common.back')}
          >
            <ArrowLeft className="h-3.5 w-3.5" />
            {t('common.back')}
          </Button>
        )}
        <span className="text-base font-semibold text-fg" data-testid="struct-editor-title">
          {mode === 'create'
            ? t('common.newTable')
            : `${t('common.editTableStructure')} · ${initialTableName}`}
        </span>
        <div className="flex-1" />
        {mode === 'alter' && initialTableName && (
          <Button
            variant="secondary"
            className="h-8 gap-1 text-xs"
            data-testid="struct-editor-export-structure"
            title={t('structEditor.exportStructure')}
            onClick={() => void handleExportStructure()}
            disabled={exportingStructure}
          >
            {exportingStructure ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Download className="h-3.5 w-3.5" />
            )}
            {exportingStructure
              ? t('structEditor.exportingStructure')
              : t('structEditor.exportStructure')}
          </Button>
        )}
        <Button
          variant="secondary"
          className="h-8 text-xs"
          onClick={() => void handlePreview()}
          disabled={!isValid || previewing}
        >
          {previewing ? t('structEditor.previewing') : t('structEditor.previewSQL')}
        </Button>
        {!showBackButton && (
          <Button variant="secondary" className="h-8 text-xs" onClick={onCancel}>
            {t('common.cancel')}
          </Button>
        )}
        <Button
          variant="primary"
          className="h-8 text-xs"
          data-testid="struct-editor-execute"
          disabled={!isValid || executing}
          onClick={() => void handleExecute()}
        >
          {executing
            ? t('structEditor.executing')
            : mode === 'create'
              ? t('structEditor.createTable')
              : t('structEditor.saveChanges')}
        </Button>
      </div>

      {mode === 'create' && (
        <div className="flex items-center gap-3 border-b border-edge px-4 py-3">
          <label className="text-sm text-fg-secondary">{t('structEditor.tableName')}</label>
          <Input
            value={tableName}
            onChange={(e) => {
              setTableName(e.target.value);
              setPreviewPlan(null);
            }}
            placeholder="new_table"
            className="h-8 max-w-xs text-sm"
          />
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-auto">
        <StructureColumnTable
          mode={mode}
          caps={caps}
          uiConfig={uiConfig}
          columns={columns}
          originalById={originalById}
          reorderEnabled={reorderEnabled}
          dragIdx={dragIdx}
          onDragStart={handleDragStart}
          onDragOver={handleDragOver}
          onDragEnd={() => setDragIdx(null)}
          onUpdate={updateColumn}
          onRemove={removeColumn}
        />

        <div className="px-4 py-3">
          <Button
            variant="secondary"
            className="h-8 gap-1 text-xs"
            data-testid="struct-editor-add-column"
            onClick={addColumn}
            disabled={!canAddColumn}
            title={!canAddColumn ? t('structEditor.capDisabled') : undefined}
          >
            <Plus className="h-3.5 w-3.5" />
            {t('structEditor.addColumn')}
          </Button>
        </div>

        <div className="border-t border-edge px-4 py-2">
          <div className="flex items-center justify-between py-2">
            <span className="text-xs font-medium text-fg-secondary">
              {t('structEditor.indexes')}
            </span>
            <Button
              variant="secondary"
              className="h-7 gap-1 text-xs"
              onClick={addIndex}
              disabled={!canAddIndex}
              title={!canAddIndex ? t('structEditor.capDisabled') : undefined}
            >
              <Plus className="h-3 w-3" />
              {t('structEditor.addIndex')}
            </Button>
          </div>
          <StructureIndexTable
            caps={caps}
            indexMethods={indexMethods}
            columnNames={columnNames}
            tableName={tableName}
            indexes={indexes}
            onUpdate={updateIndex}
            onRemove={removeIndex}
          />
        </div>
      </div>

      {error && (
        <div className="copyable whitespace-pre-wrap break-words border-t border-danger/20 bg-danger/10 px-4 py-2.5 text-xs text-danger">
          {error}
        </div>
      )}

      {previewPlan && (
        <StructurePlanPreview plan={previewPlan} onClose={() => setPreviewPlan(null)} />
      )}

      {confirmApplyDialog}
    </div>
  );
}
