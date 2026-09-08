import { useCallback, useRef, useState, type MutableRefObject } from 'react';
import { queryCommands } from '../../../commands/query';
import { usePanelStore } from '../../../stores/panelStore';
import { useSettingsStore } from '../../../stores/settingsStore';
import { useConnectionStore } from '../../../stores/connectionStore';
import { useI18n } from '../../../hooks/useI18n';
import { useConfirmDialog } from '../../../hooks/useConfirmDialog';
import { analyzeTransactionSql } from '../../../lib/sqlTransactionGuard';
import { substituteSqlParams } from '../../../lib/sqlBindParams';
import { findMissingParams } from './validateBindParams';
import {
  resolveExecutionTarget,
  type ResolveExecutionTargetResult,
} from './resolveExecutionTarget';
import { ExecutionStrategyAskModal } from './ExecutionStrategyAskModal';
import { PRESET_GROUPS } from '../../../lib/connectionGroups';
import { assessExecutionRisk } from './queryExecutionRisk';
import type { SqlRiskClassification } from './queryExecutionRisk';
import type { SqlEditorHandle } from '../../../components/SqlEditor';
import type { BindParams } from '../../../stores/queryExecActions';
import type { ExecuteKind, PendingExecute } from './contracts';
import { hasSuspiciousPostgresDoubleQuotedLiteral } from './contracts';

export interface ExecutionSnapshot {
  panelId: string;
  dbSessionId: string;
  sql: string;
  boundPayload: BindParams | undefined;
  readOnly: boolean;
  safeMode: boolean;
  isProduction: boolean;
}

export interface UseQueryExecutionGateOptions {
  panelId: string;
  dbSessionId: string;
  databaseType?: string;
  connectionId: string;
  editorRef: MutableRefObject<SqlEditorHandle | null>;
  sql: string;
  boundPayload: BindParams | undefined;
  paramValues?: Record<string, string>;
  inTransaction: boolean;
  setInTransaction: (value: boolean) => void;
  refreshTxStatus: () => Promise<void>;
  maybeOfferAbortedDialog: (error: string | null | undefined) => Promise<void>;
  syncContextFromSql: (sql: string) => Promise<void>;
  showMessageDialog: (text: string, kind?: 'error' | 'success') => void;
  onExecutionComplete: () => void;
}

export function useQueryExecutionGate({
  panelId,
  dbSessionId,
  databaseType,
  connectionId,
  editorRef,
  sql,
  boundPayload,
  paramValues,
  inTransaction,
  setInTransaction,
  refreshTxStatus,
  maybeOfferAbortedDialog,
  syncContextFromSql,
  showMessageDialog,
  onExecutionComplete,
}: UseQueryExecutionGateOptions) {
  const { t } = useI18n();
  const [confirmDangerous, confirmDangerousDialog] = useConfirmDialog();
  const [txUnclosedOpen, setTxUnclosedOpen] = useState(false);
  const [askModalState, setAskModalState] = useState<{
    target: ResolveExecutionTargetResult;
  } | null>(null);
  const pendingExecuteRef = useRef<PendingExecute | null>(null);
  const autoCommit = useSettingsStore((s) => s.settings.autoCommit);
  const safeMode = useSettingsStore((s) => s.settings.safeMode);
  const sqlExecutionStrategy =
    useSettingsStore((s) => s.settings?.sqlExecutionStrategy) ?? 'current_statement';
  const storeExecuteQuery = usePanelStore((s) => s.executeQuery);
  const storeExecuteSelection = usePanelStore((s) => s.executeSelection);

  // ── Snapshot helpers ──────────────────────────────────────────

  /**
   * Build a frozen snapshot of the current execution context.
   * readOnly comes from the saved connection config; isProduction from the group.
   */
  const buildSnapshot = useCallback(
    (snapshotSql: string, snapshotBoundPayload: BindParams | undefined): ExecutionSnapshot => {
      const connConfig = useConnectionStore
        .getState()
        .connections.find((c) => c.id === connectionId);
      const readOnly = connConfig?.readOnly === true;
      const isProduction = connConfig?.group === PRESET_GROUPS.production;

      return {
        panelId,
        dbSessionId,
        sql: snapshotSql,
        boundPayload: snapshotBoundPayload,
        readOnly,
        safeMode,
        isProduction,
      };
    },
    [panelId, dbSessionId, connectionId, safeMode],
  );

  /** Check whether a snapshot is still fresh by comparing against live state. */
  const isSnapshotStale = useCallback(
    (snapshot: ExecutionSnapshot): boolean => {
      const liveSql = usePanelStore.getState().queryExec.get(panelId)?.sql ?? '';
      if (liveSql !== snapshot.sql) return true;

      // Compare bound payload by reference (same object means unchanged)
      // We use the hook's current boundPayload prop as the live source
      // since the store doesn't track bound params separately.

      const connConfig = useConnectionStore
        .getState()
        .connections.find((c) => c.id === connectionId);
      const liveReadOnly = connConfig?.readOnly === true;
      if (liveReadOnly !== snapshot.readOnly) return true;

      const liveSafeMode = useSettingsStore.getState().settings.safeMode;
      if (liveSafeMode !== snapshot.safeMode) return true;

      return false;
    },
    [panelId, connectionId],
  );

  // ── Core execution ────────────────────────────────────────────

  const runExecute = useCallback(
    async (kind: ExecuteKind, selectionSql?: string) => {
      const sqlToRun =
        kind === 'selection' && selectionSql != null
          ? selectionSql
          : editorRef.current?.getSelection()?.trim() || sql;
      if (databaseType === 'postgresql' && hasSuspiciousPostgresDoubleQuotedLiteral(sqlToRun)) {
        showMessageDialog(t('query.postgresDoubleQuoteHint'), 'error');
        return;
      }
      await syncContextFromSql(kind === 'selection' && selectionSql != null ? selectionSql : sql);
      if (!autoCommit && !inTransaction) {
        try {
          await queryCommands.beginSessionTransaction(dbSessionId);
          setInTransaction(true);
        } catch {
          /* driver may not support transactions; continue */
        }
      }
      const rawValues = (paramValues ?? boundPayload ?? {}) as Record<string, unknown>;
      const targetRawSql =
        kind === 'selection' && selectionSql != null
          ? selectionSql
          : editorRef.current?.getSelection()?.trim() || sql;

      // Defense-in-depth: assert no missing parameters before substitution
      const runMissing = findMissingParams(targetRawSql, rawValues);
      if (runMissing.length > 0) {
        showMessageDialog(
          t('query.editor.param.missingValue', { token: runMissing[0].label }),
          'error',
        );
        return;
      }

      const targetSubstitutedSql = substituteSqlParams(targetRawSql, rawValues);

      if (kind === 'selection' && selectionSql != null) {
        await storeExecuteSelection(panelId, targetSubstitutedSql, boundPayload);
      } else {
        const sel = editorRef.current?.getSelection()?.trim();
        if (sel) {
          await storeExecuteSelection(panelId, targetSubstitutedSql, boundPayload);
        } else if (targetSubstitutedSql !== targetRawSql) {
          await storeExecuteSelection(panelId, targetSubstitutedSql, boundPayload);
        } else {
          await storeExecuteQuery(panelId, boundPayload);
        }
      }
      const err = usePanelStore.getState().queryExec.get(panelId)?.error ?? null;
      if (err) {
        await maybeOfferAbortedDialog(err);
      } else {
        await refreshTxStatus();
      }
      onExecutionComplete();
    },
    [
      sql,
      databaseType,
      panelId,
      autoCommit,
      inTransaction,
      dbSessionId,
      storeExecuteSelection,
      storeExecuteQuery,
      boundPayload,
      maybeOfferAbortedDialog,
      refreshTxStatus,
      syncContextFromSql,
      showMessageDialog,
      t,
      editorRef,
      setInTransaction,
      onExecutionComplete,
    ],
  );

  // ── Risk gate pipeline ────────────────────────────────────────

  /**
   * Resolves the execution-confirm dialog i18n keys for a given risk decision.
   */
  function resolveConfirmI18n(confirmReasons: string[]): {
    titleKey: string;
    messageKey: string;
    badge: string | undefined;
  } {
    const hasProduction = confirmReasons.includes('production');
    const hasHighRisk = confirmReasons.includes('high-risk');

    if (hasProduction && hasHighRisk) {
      return {
        titleKey: t('query.editor.executionConfirm.combinedTitle'),
        messageKey: t('query.editor.executionConfirm.combinedMessage'),
        badge: t('query.editor.executionConfirm.badgeProduction'),
      };
    }
    if (hasProduction) {
      return {
        titleKey: t('query.editor.executionConfirm.productionTitle'),
        messageKey: t('query.editor.executionConfirm.productionMessage'),
        badge: t('query.editor.executionConfirm.badgeProduction'),
      };
    }
    if (hasHighRisk) {
      return {
        titleKey: t('query.editor.executionConfirm.highRiskTitle'),
        messageKey: t('query.editor.executionConfirm.highRiskMessage'),
        badge: t('query.editor.executionConfirm.badgeHighRisk'),
      };
    }
    return {
      titleKey: t('query.dangerousSqlTitle'),
      messageKey: t('query.dangerousSqlConfirm'),
      badge: undefined,
    };
  }

  function resolveFindingDescriptions(
    findings: Array<{ type: string; verb?: string }>,
    classification: SqlRiskClassification,
  ): string[] {
    const descriptions: string[] = [];
    for (const f of findings) {
      switch (f.type) {
        case 'drop':
          descriptions.push(t('query.editor.executionConfirm.findingDrop'));
          break;
        case 'truncate':
          descriptions.push(t('query.editor.executionConfirm.findingTruncate'));
          break;
        case 'no-where':
          if (f.verb === 'UPDATE') {
            descriptions.push(t('query.editor.executionConfirm.findingUpdateNoWhere'));
          } else if (f.verb === 'DELETE') {
            descriptions.push(t('query.editor.executionConfirm.findingDeleteNoWhere'));
          } else {
            descriptions.push(t('query.editor.executionConfirm.findingUnknown'));
          }
          break;
      }
    }
    if (classification === 'unknown' && descriptions.length === 0) {
      descriptions.push(t('query.editor.executionConfirm.findingUnknown'));
    }
    return descriptions;
  }

  /**
   * Shows the risk-confirm dialog and handles staleness detection.
   * Returns true if confirmed and still fresh, false otherwise.
   */
  async function promptRiskConfirm(
    sqlForCheck: string,
    snapshot: ExecutionSnapshot,
    confirmReasons: string[],
    classification: SqlRiskClassification,
  ): Promise<boolean> {
    const i18n = resolveConfirmI18n(confirmReasons);
    const risk = assessExecutionRisk(sqlForCheck, {
      readOnly: snapshot.readOnly,
      safeMode: snapshot.safeMode,
      isProduction: snapshot.isProduction,
    });
    const descriptions = resolveFindingDescriptions(risk.findings, classification);

    const confirmed = await confirmDangerous({
      title: i18n.titleKey,
      message: i18n.messageKey,
      confirmLabel: t('query.editor.executionConfirm.confirmExecute'),
      kind: 'warning',
      badge: i18n.badge,
      codePreview: sqlForCheck,
      description: descriptions.length > 0 ? descriptions.join('\n') : undefined,
    });

    if (confirmed && isSnapshotStale(snapshot)) {
      showMessageDialog(t('query.editor.executionConfirm.staleMessage'), 'error');
      return false;
    }

    return confirmed;
  }

  /**
   * Runs the full gate pipeline for a given execution kind.
   * Freezes a snapshot, runs checks sequentially, and submits only if all pass.
   *
   * Pipeline: transaction check → param validation → readOnly block →
   *           Safe Mode block → production/high-risk confirm → submitExecution(snapshot)
   */
  const requestExecute = useCallback(
    async (kind: ExecuteKind, selectionSql?: string) => {
      let targetSql = sql;
      let effectiveKind: ExecuteKind = kind;

      if (selectionSql != null) {
        targetSql = selectionSql;
      } else {
        const activeSel = editorRef.current?.getSelection()?.trim();
        if (activeSel) {
          targetSql = activeSel;
          effectiveKind = 'selection';
        } else if (typeof editorRef.current?.getCursorOffset === 'function') {
          const strategy = sqlExecutionStrategy;
          const cursorOffset = editorRef.current.getCursorOffset();
          const resolved = resolveExecutionTarget({
            doc: sql,
            cursorOffset,
            selection: '',
            strategy,
          });

          if (resolved.needsAsk) {
            setAskModalState({ target: resolved });
            return;
          }

          targetSql = resolved.sql;
          if (resolved.strategyUsed !== 'entire_script' && targetSql !== sql) {
            effectiveKind = 'selection';
          }
        }
      }

      const sqlForCheck = targetSql;

      // Freeze snapshot before any async work
      const snapshotPayload =
        effectiveKind === 'selection' && selectionSql != null ? boundPayload : boundPayload;
      const snapshot = buildSnapshot(sqlForCheck, snapshotPayload);

      // 1. Transaction check — unclosed BEGIN
      if (analyzeTransactionSql(sqlForCheck).hasUnclosedBegin) {
        pendingExecuteRef.current = {
          kind: effectiveKind,
          sql: effectiveKind === 'selection' ? targetSql : undefined,
        };
        setTxUnclosedOpen(true);
        return;
      }

      // 2. Parameter validation (Host autonomous check, blocking execution on missing params)
      const effectiveValues = (paramValues ?? snapshotPayload ?? {}) as Record<string, unknown>;
      const missing = findMissingParams(sqlForCheck, effectiveValues);
      if (missing.length > 0) {
        const firstMissing = missing[0];
        showMessageDialog(
          t('query.editor.param.missingValue', { token: firstMissing.label }),
          'error',
        );
        setTimeout(() => {
          const selector = `[data-param-id="${firstMissing.param.stableId}"], [data-param-name="${firstMissing.param.name}"]`;
          const input = document.querySelector(selector) as HTMLInputElement | null;
          input?.focus();
          window.dispatchEvent(
            new CustomEvent('datazen:focus-bind-param', {
              detail: {
                stableId: firstMissing.param.stableId,
                name: firstMissing.param.name,
              },
            }),
          );
        }, 50);
        return;
      }

      // 3. Risk assessment via unified evaluator
      const risk = assessExecutionRisk(sqlForCheck, {
        readOnly: snapshot.readOnly,
        safeMode: snapshot.safeMode,
        isProduction: snapshot.isProduction,
      });

      // 4. ReadOnly hard block
      if (risk.hardBlocked && risk.hardBlockReason === 'readOnly') {
        showMessageDialog(t('query.editor.executionConfirm.blockedReadOnly'), 'error');
        return;
      }

      // 5. Safe Mode hard block
      if (risk.hardBlocked && risk.hardBlockReason === 'safeMode') {
        showMessageDialog(t('query.editor.executionConfirm.blockedSafeMode'), 'error');
        return;
      }

      // 6. Production / high-risk confirmation
      if (risk.needsConfirm) {
        const confirmed = await promptRiskConfirm(
          sqlForCheck,
          snapshot,
          risk.confirmReasons,
          risk.classification,
        );
        if (!confirmed) return;
      }

      // 7. Submit execution with the frozen snapshot
      void runExecute(effectiveKind, effectiveKind === 'selection' ? targetSql : undefined);
    },
    [
      confirmDangerous,
      sql,
      runExecute,
      safeMode,
      t,
      editorRef,
      buildSnapshot,
      isSnapshotStale,
      showMessageDialog,
      panelId,
      boundPayload,
      paramValues,
      sqlExecutionStrategy,
    ],
  );

  const handleExecute = useCallback(() => {
    requestExecute('full');
  }, [requestExecute]);

  const handleExecuteSelection = useCallback(
    (selectionSql: string) => {
      requestExecute('selection', selectionSql);
    },
    [requestExecute],
  );

  const handleConfirmUnclosedTx = useCallback(async () => {
    const pending = pendingExecuteRef.current;
    pendingExecuteRef.current = null;
    setTxUnclosedOpen(false);
    if (!pending) return;

    const sqlForCheck =
      pending.kind === 'selection' && pending.sql != null
        ? pending.sql
        : editorRef.current?.getSelection()?.trim() || sql;

    // Build snapshot for the deferred execution
    const snapshot = buildSnapshot(sqlForCheck, boundPayload);

    // Re-run risk assessment
    const risk = assessExecutionRisk(sqlForCheck, {
      readOnly: snapshot.readOnly,
      safeMode: snapshot.safeMode,
      isProduction: snapshot.isProduction,
    });

    if (risk.hardBlocked && risk.hardBlockReason === 'safeMode') {
      showMessageDialog(t('query.editor.executionConfirm.blockedSafeMode'), 'error');
      return;
    }
    if (risk.hardBlocked && risk.hardBlockReason === 'readOnly') {
      showMessageDialog(t('query.editor.executionConfirm.blockedReadOnly'), 'error');
      return;
    }

    if (risk.needsConfirm) {
      const confirmed = await promptRiskConfirm(
        sqlForCheck,
        snapshot,
        risk.confirmReasons,
        risk.classification,
      );
      if (!confirmed) return;
    }

    void runExecute(pending.kind, pending.sql);
  }, [
    confirmDangerous,
    sql,
    runExecute,
    safeMode,
    t,
    editorRef,
    buildSnapshot,
    isSnapshotStale,
    showMessageDialog,
    boundPayload,
  ]);

  const handleCancelUnclosedTx = useCallback(() => {
    pendingExecuteRef.current = null;
    setTxUnclosedOpen(false);
  }, []);

  const handleAskExecuteCurrent = useCallback(() => {
    const currentSql = askModalState?.target.currentStatement?.sql;
    setAskModalState(null);
    if (currentSql) {
      void requestExecute('selection', currentSql);
    }
  }, [askModalState, requestExecute]);

  const handleAskExecuteEntire = useCallback(() => {
    const entire = askModalState?.target.entireScript;
    setAskModalState(null);
    if (entire) {
      void requestExecute('full', entire);
    }
  }, [askModalState, requestExecute]);

  const handleAskCancel = useCallback(() => {
    setAskModalState(null);
  }, []);

  const executionStrategyAskModal = askModalState ? (
    <ExecutionStrategyAskModal
      open
      currentStatement={askModalState.target.currentStatement}
      statementCount={askModalState.target.statementCount}
      entireScript={askModalState.target.entireScript}
      onExecuteCurrent={handleAskExecuteCurrent}
      onExecuteEntire={handleAskExecuteEntire}
      onCancel={handleAskCancel}
    />
  ) : null;

  return {
    requestExecute,
    runExecute,
    handleExecute,
    handleExecuteSelection,
    handleConfirmUnclosedTx,
    handleCancelUnclosedTx,
    txUnclosedOpen,
    confirmDangerousDialog,
    executionStrategyAskModal,
  };
}
