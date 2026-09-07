import { useCallback, useMemo, useRef, useState, type MutableRefObject } from 'react';
import { queryCommands } from '../../../commands/query';
import { dashboardCommands } from '../../../commands/dashboard';
import {
  fetchTableSchemaForSqlGeneration,
  generateTableSqlWithFallbacks,
} from '../../../lib/tableSchemaForSql';
import { formatTableIdentifier } from '../../../lib/sqlGenerator';
import { openDashboardWindow } from '../../../lib/windowManager';
import { emitCrossWindow } from '../../../lib/crossWindowBus';
import { createEmptyDashboard } from '../../dashboard/DashboardPanel';
import { usePanelStore } from '../../../stores/panelStore';
import {
  buildExplainAction,
  buildFixSqlAction,
  buildRetryAction,
} from '../../../lib/aiQueryActions';
import type { ChartConfig } from '../../../types/chart';
import type { BindParams } from '../../../stores/queryExecActions';
import type { DroppedTablePayload, SqlEditorHandle } from '../../../components/SqlEditor';
import type { ExecuteKind } from './contracts';
import {
  buildQueryPanelDiagnosisContext,
  readCurrentQueryPanelRetryValidationInput,
} from './contracts';

export interface QueryDropHandlerOptions {
  connectionId: string;
  dbSessionId: string;
  databaseType?: string;
  editorRef: MutableRefObject<SqlEditorHandle | null>;
}

/**
 * Detect SQL context around the drop position and return the appropriate
 * insertion text and mode.
 *
 * - Empty document → SELECT * FROM <table>
 * - After FROM/JOIN/LEFT JOIN/RIGHT JOIN/CROSS JOIN/INNER JOIN/comma → raw table name
 * - After AS keyword → raw table alias
 * - Unknown context (e.g. after WHERE) → prefix with FROM
 */
export function detectDropContext(
  doc: string,
  pos: number,
  qualifiedName: string,
): { text: string; raw: boolean } {
  // Empty document → full SELECT statement
  if (!doc || doc.trim().length === 0) {
    return { text: `SELECT * FROM ${qualifiedName} `, raw: true };
  }

  // Look backwards from pos for the last meaningful keyword
  const before = doc.slice(0, pos);
  const after = doc.slice(pos);
  const trimmed = before.trimEnd();

  // Match trailing SQL keywords (case-insensitive)
  const joinPattern = /\b(?:LEFT|RIGHT|CROSS|INNER|FULL)?\s*(?:JOIN|FROM)\s*$/i;
  const commaAfterFrom = /\bFROM\b[^,]*,\s*$/i;
  const asKeyword = /\bAS\s*$/i;

  if (joinPattern.test(trimmed) || commaAfterFrom.test(trimmed) || asKeyword.test(trimmed)) {
    return { text: ` ${qualifiedName} `, raw: true };
  }

  // For any other context, insert the raw table identifier with appropriate boundary spacing,
  // rather than forcibly prepending 'FROM' which corrupts existing SQL statements.
  const needsLeadingSpace = before.length > 0 && !/\s$/.test(before);
  const needsTrailingSpace = after.length > 0 && !/^\s/.test(after);
  const prefix = needsLeadingSpace ? ' ' : '';
  const suffix = needsTrailingSpace ? ' ' : '';

  return { text: `${prefix}${qualifiedName}${suffix}`, raw: true };
}

/**
 * §Track S6-D step 4: queryDropHandler
 *
 * - Empty editor → generate SELECT statement
 * - Non-empty editor → smart context-aware table insertion
 * - Cross-connection reject (payload.connectionId != editor connectionId)
 * - Legacy payload compat (DroppedTablePayload format)
 */
export function createQueryDropHandler({
  connectionId,
  dbSessionId,
  databaseType,
  editorRef,
}: QueryDropHandlerOptions) {
  return async (payload: DroppedTablePayload, pos: number | null) => {
    const dbType = databaseType || 'sqlite';
    const targetSessionId = dbSessionId || connectionId;

    // Check if editor has content via the public getDocument handle
    const doc = editorRef.current?.getDocument?.() ?? '';
    const editorHasContent = doc.trim().length > 0;

    if (editorHasContent) {
      // §S6-D step 4: non-empty editor → smart context-aware insertion
      const dropPos = pos ?? doc.length;

      for (const t of payload.tables) {
        const displayRef = formatTableIdentifier(t.tableName, dbType, t.schema);
        const ctx = detectDropContext(doc, dropPos, displayRef);

        editorRef.current?.rawInsert?.(ctx.text, dropPos);
      }
      editorRef.current?.focus?.();
      return;
    }

    // §S6-D step 4: empty editor → generate SELECT statement (existing behavior)
    const generatedSqls: string[] = [];

    for (const t of payload.tables) {
      const tableSchema = await fetchTableSchemaForSqlGeneration({
        dbSessionId: targetSessionId,
        tableName: t.tableName,
        schema: t.schema,
        databaseType: dbType,
      });
      const tableRef = t.schema ? `${t.schema}.${t.tableName}` : t.tableName;
      generatedSqls.push(
        generateTableSqlWithFallbacks(tableSchema, 'select', dbType, {
          schemaPrefix: t.schema,
          tableName: t.tableName,
          tableRefLabel: tableRef,
        }),
      );
    }

    const combinedGenerated = generatedSqls.join('\n\n');
    if (!combinedGenerated) return;

    editorRef.current?.insertAt(combinedGenerated, pos);
  };
}

export interface UseQueryPanelWorkflowsOptions {
  panelId: string;
  connectionId: string;
  dbSessionId: string;
  databaseType?: string;
  connectionName?: string;
  database?: string;
  schema?: string;
  sql: string;
  error: string | null | undefined;
  chartConfig: ChartConfig | undefined;
  resultViewMode: 'table' | 'chart';
  activeResultRowsLength: number;
  boundPayload: BindParams | undefined;
  paramValuesRef: MutableRefObject<Record<string, string>>;
  schemaState: import('./contracts').QueryDiagnosisSchemaState;
  serverVersion?: string;
  selectedDatabase?: string | null;
  runExecute: (kind: ExecuteKind) => Promise<void>;
  confirmRetry: (options: {
    title: string;
    message: string;
    confirmLabel?: string;
    kind?: 'warning' | 'info';
  }) => Promise<boolean>;
  updateSql: (panelId: string, sql: string) => void;
  showMessageDialog: (text: string, kind?: 'error' | 'success') => void;
  t: (key: import('../../../locales').I18nKey, params?: Record<string, string | number>) => string;
}

export function useQueryPanelWorkflows({
  panelId,
  connectionId,
  dbSessionId,
  databaseType,
  connectionName,
  database,
  schema,
  sql,
  error,
  chartConfig,
  resultViewMode,
  activeResultRowsLength,
  boundPayload,
  paramValuesRef,
  schemaState,
  serverVersion,
  selectedDatabase,
  runExecute,
  confirmRetry,
  updateSql,
  showMessageDialog,
  t,
}: UseQueryPanelWorkflowsOptions) {
  const [diagnosisVisible, setDiagnosisVisible] = useState(false);
  const [explainResult, setExplainResult] = useState<import('../../../types').ExplainResult | null>(
    null,
  );
  const [explainLoading, setExplainLoading] = useState(false);
  const [explainError, setExplainError] = useState<string | null>(null);
  const [showExplain, setShowExplain] = useState(false);
  const [addToDashboardOpen, setAddToDashboardOpen] = useState(false);
  const [favoriteName, setFavoriteName] = useState('');
  const [showFavoriteDialog, setShowFavoriteDialog] = useState(false);
  const [favoriteDialogSql, setFavoriteDialogSql] = useState('');
  const pendingFavSqlRef = useRef('');

  const diagnosisContext = useMemo(
    () =>
      buildQueryPanelDiagnosisContext({
        execution: { sql, error: error ?? null },
        connectionId,
        dbSessionId,
        databaseType,
        connectionName,
        database,
        schema,
        serverVersion,
        schemaState,
      }),
    [
      connectionId,
      connectionName,
      database,
      databaseType,
      dbSessionId,
      error,
      schema,
      schemaState,
      serverVersion,
      sql,
    ],
  );

  const explainAction = useMemo(() => buildExplainAction(diagnosisContext), [diagnosisContext]);
  const retryAction = useMemo(
    () => buildRetryAction(diagnosisContext, boundPayload ?? {}),
    [boundPayload, diagnosisContext],
  );

  const handleExplain = useCallback(async () => {
    if (!sql.trim()) return;
    setExplainLoading(true);
    setExplainError(null);
    setShowExplain(true);
    try {
      setExplainResult(await queryCommands.getExplain(dbSessionId, sql, selectedDatabase));
    } catch (e) {
      setExplainResult(null);
      setExplainError(e instanceof Error ? e.message : String(e));
    } finally {
      setExplainLoading(false);
    }
  }, [dbSessionId, selectedDatabase, sql]);

  const handleApplyFixSql = useCallback(
    (nextSql: string) => {
      buildFixSqlAction(diagnosisContext, nextSql).applyToEditor((draft) =>
        updateSql(panelId, draft.draftSql),
      );
    },
    [diagnosisContext, panelId, updateSql],
  );

  const handleRetry = useCallback(async () => {
    const validation = retryAction.invoke(
      {
        sql,
        contextFingerprint: diagnosisContext.ok
          ? diagnosisContext.context.contextFingerprint
          : null,
        boundParams: boundPayload ?? {},
      },
      () => undefined,
    );
    if (!validation.ok) return;
    const confirmed = await confirmRetry({
      title: t('query.retry'),
      message: t('query.retryConfirm'),
      confirmLabel: t('query.retry'),
      kind: 'info',
    });
    if (!confirmed) return;
    const latestValidationInput = readCurrentQueryPanelRetryValidationInput(
      panelId,
      paramValuesRef.current,
    );
    if (!latestValidationInput) return;
    let retryExecution: Promise<void> | undefined;
    const finalValidation = retryAction.invoke(latestValidationInput, () => {
      retryExecution = runExecute('full');
    });
    if (finalValidation.ok && retryExecution) await retryExecution;
  }, [
    boundPayload,
    confirmRetry,
    diagnosisContext,
    panelId,
    paramValuesRef,
    retryAction,
    runExecute,
    sql,
    t,
  ]);

  const handleAddToDashboardConfirm = useCallback(
    (dashboardId: string, newName?: string) => {
      void (async () => {
        if (!sql.trim() || activeResultRowsLength === 0) return;
        setAddToDashboardOpen(false);
        try {
          let targetId = dashboardId;
          if (dashboardId === 'new') {
            const board = createEmptyDashboard(newName?.trim() || t('query.dashboard.defaultName'));
            await dashboardCommands.saveDashboard(board);
            targetId = board.id;
          }
          const created = await dashboardCommands.createWidgetFromSql({
            dashboardId: targetId,
            connectionId,
            sql,
            title:
              (
                usePanelStore.getState().panels.find((p) => p.id === panelId) as
                  | import('../../../stores/panelStore').QueryPanel
                  | undefined
              )?.title || undefined,
            viewMode: resultViewMode,
            chartConfig,
          });
          void emitCrossWindow('dashboard:changed', { dashboardId: created.dashboard.id });
          openDashboardWindow(created.dashboard.id, created.dashboard.name);
        } catch (e) {
          showMessageDialog(e instanceof Error ? e.message : String(e), 'error');
        }
      })();
    },
    [
      activeResultRowsLength,
      chartConfig,
      connectionId,
      panelId,
      resultViewMode,
      showMessageDialog,
      sql,
      t,
    ],
  );

  const openAddFavoriteDialog = useCallback((nextSql: string) => {
    const trimmed = nextSql.trim();
    if (!trimmed) return;
    pendingFavSqlRef.current = trimmed;
    setFavoriteDialogSql(trimmed);
    setFavoriteName('');
    setShowFavoriteDialog(true);
  }, []);

  const handleExplainError = useMemo(
    () =>
      explainAction.enabled
        ? () => {
            explainAction.invoke(() => setDiagnosisVisible(true));
          }
        : undefined,
    [explainAction],
  );

  return {
    diagnosisVisible,
    setDiagnosisVisible,
    explainResult,
    explainLoading,
    explainError,
    showExplain,
    setShowExplain,
    addToDashboardOpen,
    setAddToDashboardOpen,
    favoriteName,
    setFavoriteName,
    showFavoriteDialog,
    setShowFavoriteDialog,
    favoriteDialogSql,
    pendingFavSqlRef,
    diagnosisContext,
    retryAction,
    handleExplain,
    handleApplyFixSql,
    handleRetry,
    handleAddToDashboardConfirm,
    openAddFavoriteDialog,
    handleExplainError,
  };
}
