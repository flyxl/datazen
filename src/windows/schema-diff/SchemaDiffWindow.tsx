import { useCallback, useEffect, useMemo, useState } from 'react';
import { BookOpen, Copy, Loader2 } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { Button } from '../../components/ui/Button';
import { SchemaDiffPanel, formatSchemaDiffText } from '../../components/schema/SchemaDiffPanel';
import {
  dialectSupportsTransactionalDdl,
  exportPlanSql,
  planHasDestructive,
  schemaDiffCommands,
  type SchemaDiffConfigJson,
  type SchemaDiffDeployResult,
  type SchemaDiffPlan,
} from '../../commands/schemaDiff';
import { useSettings } from '../../hooks/useSettings';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import { openDocsWindow } from '../../lib/windowManager';
import type { TableSchemaDiff } from '../../types';
import { isSchemaDiffLimitationsDismissed } from '../../lib/schemaDiffLimitationsPrefs';
import { SchemaDiffLimitationsDialog } from './SchemaDiffLimitationsDialog';
import { SchemaDiffEndpointsBar } from './SchemaDiffEndpointsBar';
import { SchemaDiffTableListPanel } from './SchemaDiffTableListPanel';
import { SchemaDiffRightPanel, type SchemaDiffRightPanelTab } from './SchemaDiffRightPanel';
import { useSchemaDiffEndpoints } from './useSchemaDiffEndpoints';

function parseTableList(raw: string): string[] {
  return raw
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

function tableDiffHasChanges(diff: TableSchemaDiff): boolean {
  const missing = diff.missingOnTarget ?? diff.added;
  const extra = diff.extraOnTarget ?? diff.removed;
  return missing.length > 0 || extra.length > 0 || diff.changed.length > 0;
}

export function SchemaDiffWindow() {
  useSettings();
  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const [tableNamesRaw, setTableNamesRaw] = useState('');
  const [diffs, setDiffs] = useState<TableSchemaDiff[]>([]);
  const [plan, setPlan] = useState<SchemaDiffPlan | null>(null);
  const [rightTab, setRightTab] = useState<SchemaDiffRightPanelTab>('plan');
  const [selectedTable, setSelectedTable] = useState<string | null>(null);
  const [allowDestructive, setAllowDestructive] = useState(false);
  const [includeIndexes, setIncludeIndexes] = useState(true);
  const [requireRollback, setRequireRollback] = useState(false);
  const [useTransaction, setUseTransaction] = useState(true);
  const [confirmText, setConfirmText] = useState('');
  const [deployResult, setDeployResult] = useState<SchemaDiffDeployResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [copied, setCopied] = useState(false);
  const [limitationsOpen, setLimitationsOpen] = useState(false);

  const endpoints = useSchemaDiffEndpoints({ onError: setError });

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    if (!isSchemaDiffLimitationsDismissed()) {
      setLimitationsOpen(true);
    }
  }, []);

  const parsedTables = useMemo(() => parseTableList(tableNamesRaw), [tableNamesRaw]);

  useEffect(() => {
    if (parsedTables.length === 0) {
      setSelectedTable(null);
      return;
    }
    setSelectedTable((prev) =>
      prev && parsedTables.includes(prev) ? prev : parsedTables[0] ?? null,
    );
  }, [parsedTables]);

  const tableHasDiff = useMemo(() => {
    const map: Record<string, boolean> = {};
    for (const diff of diffs) {
      map[diff.table] = tableDiffHasChanges(diff);
    }
    return map;
  }, [diffs]);

  const selectedDiff = useMemo(
    () => diffs.find((d) => d.table === selectedTable) ?? null,
    [diffs, selectedTable],
  );

  const targetLabel = useMemo(() => {
    const c = endpoints.targetConn;
    return c ? `${c.name} (${c.databaseType})` : endpoints.targetId;
  }, [endpoints.targetConn, endpoints.targetId]);

  const handleCompare = useCallback(async () => {
    setError('');
    setDiffs([]);
    setPlan(null);
    setDeployResult(null);
    setRightTab('plan');
    if (!endpoints.validateEndpoints()) return;

    const tables = parseTableList(tableNamesRaw);
    if (tables.length === 0) {
      setError(t('schemaDiff.tableRequired'));
      return;
    }

    setLoading(true);
    try {
      const srcConnId = await endpoints.ensureConnected('source');
      const tgtConnId = await endpoints.ensureConnected('target');
      if (!srcConnId || !tgtConnId) return;
      const results: TableSchemaDiff[] = [];
      for (const table of tables) {
        results.push(await schemaDiffCommands.compareTableSchemas(srcConnId, tgtConnId, table));
      }
      setDiffs(results);
      setSelectedTable(tables[0] ?? null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [endpoints, tableNamesRaw, t]);

  const buildPlan = useCallback(async () => {
    setError('');
    setDeployResult(null);
    const tables = parseTableList(tableNamesRaw);
    if (tables.length === 0) {
      setError(t('schemaDiff.tableRequired'));
      return;
    }
    if (!endpoints.validateEndpoints()) return;

    setLoading(true);
    try {
      const srcConnId = await endpoints.ensureConnected('source');
      const tgtConnId = await endpoints.ensureConnected('target');
      if (!srcConnId || !tgtConnId) return;
      const next = await schemaDiffCommands.preparePlan({
        sourceDbSessionId: srcConnId,
        targetDbSessionId: tgtConnId,
        tableNames: tables,
        allowDestructive,
        includeIndexes,
      });
      setPlan(next);
      setRightTab('plan');
      setUseTransaction(dialectSupportsTransactionalDdl(next.targetDialect));
      setConfirmText('');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [allowDestructive, endpoints, includeIndexes, tableNamesRaw, t]);

  const handleDeploy = useCallback(async () => {
    if (!plan) return;
    setError('');
    setLoading(true);
    try {
      const tgtConnId = await endpoints.ensureConnected('target');
      if (!tgtConnId) return;
      const result = await schemaDiffCommands.executeDeploy({
        targetDbSessionId: tgtConnId,
        plan,
        useTransaction,
        confirmDestructive: planHasDestructive(plan) ? confirmText.trim() : undefined,
      });
      setDeployResult(result);
      setRightTab('deploy');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [confirmText, endpoints, plan, useTransaction]);

  const handleCopySummary = async () => {
    if (diffs.length === 0) return;
    try {
      await navigator.clipboard.writeText(diffs.map(formatSchemaDiffText).join('\n\n'));
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  };

  const handleCopySql = async () => {
    if (!plan) return;
    try {
      await navigator.clipboard.writeText(exportPlanSql(plan));
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  };

  const handleExportConfig = async () => {
    const cfg: SchemaDiffConfigJson = {
      version: 2,
      sourceConnectionId: endpoints.sourceId,
      targetConnectionId: endpoints.targetId,
      tables: parseTableList(tableNamesRaw),
      allowDestructive,
      includeIndexes,
      requireRollback,
    };
    try {
      await navigator.clipboard.writeText(JSON.stringify(cfg, null, 2));
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  };

  const handleImportConfig = async () => {
    setError('');
    try {
      const text = await navigator.clipboard.readText();
      const cfg = JSON.parse(text) as SchemaDiffConfigJson;
      if (cfg.version !== 2 || !cfg.sourceConnectionId || !cfg.targetConnectionId) {
        throw new Error(t('schemaDiff.invalidConfig'));
      }
      endpoints.setSourceId(cfg.sourceConnectionId);
      endpoints.setTargetId(cfg.targetConnectionId);
      setTableNamesRaw((cfg.tables ?? []).join('\n'));
      setAllowDestructive(Boolean(cfg.allowDestructive));
      setIncludeIndexes(cfg.includeIndexes ?? true);
      setRequireRollback(Boolean(cfg.requireRollback));
      setPlan(null);
      setDiffs([]);
      setDeployResult(null);
      setRightTab('plan');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const showCompareLoading = loading && diffs.length === 0;

  return (
    <div
      className="flex h-screen min-h-0 flex-col bg-surface text-fg"
      data-testid="schema-diff-window"
    >
      <TitleBar
        title={t('common.schemaDiff')}
        rightContent={
          <Button
            variant="secondary"
            className="h-6 w-6 !px-0"
            title={t('docs.openSchemaDiffHelp')}
            onClick={() => openDocsWindow('schemaDiff')}
          >
            <BookOpen className="h-3 w-3" />
          </Button>
        }
      />

      <SchemaDiffEndpointsBar
        sourceId={endpoints.sourceId}
        targetId={endpoints.targetId}
        sourceDatabase={endpoints.sourceDatabase}
        targetDatabase={endpoints.targetDatabase}
        sourceSchema={endpoints.sourceSchema}
        targetSchema={endpoints.targetSchema}
        sourceDatabases={endpoints.sourceDatabases}
        targetDatabases={endpoints.targetDatabases}
        sourceSchemas={endpoints.sourceSchemas}
        targetSchemas={endpoints.targetSchemas}
        connOptions={endpoints.connOptions}
        targetOptions={endpoints.targetOptions}
        isCrossDialect={endpoints.isCrossDialect}
        busy={loading}
        onSourceChange={endpoints.setSourceId}
        onTargetChange={endpoints.setTargetId}
        onSourceDatabaseChange={endpoints.setSourceDatabase}
        onTargetDatabaseChange={endpoints.setTargetDatabase}
        onSourceSchemaChange={endpoints.setSourceSchema}
        onTargetSchemaChange={endpoints.setTargetSchema}
        onSwap={endpoints.handleSwap}
        onCompare={() => void handleCompare()}
      />

      <div className="shrink-0 border-b border-edge px-6 py-3">
        <div className="flex flex-wrap items-end gap-3">
          <label className="min-w-[12rem] flex-1 space-y-1 text-sm">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              {t('schemaDiff.tables')}
            </span>
            <textarea
              className="min-h-[56px] w-full rounded-md border border-edge bg-surface-alt px-3 py-2 font-mono text-sm text-fg"
              value={tableNamesRaw}
              onChange={(e) => setTableNamesRaw(e.target.value)}
              placeholder={
                endpoints.sourceSchema
                  ? `${endpoints.sourceSchema}.table_name`
                  : t('schemaDiff.tablesPlaceholder')
              }
              data-testid="schema-diff-tables-input"
            />
          </label>

          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="secondary"
              disabled={
                loading ||
                !endpoints.sourceId ||
                !endpoints.targetId ||
                parsedTables.length === 0
              }
              onClick={() => void buildPlan()}
              data-testid="schema-diff-generate-plan"
            >
              {loading && rightTab === 'plan' && !plan ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : null}
              {t('schemaDiff.generatePlan')}
            </Button>
            {diffs.length > 0 && (
              <Button variant="secondary" onClick={() => void handleCopySummary()}>
                <Copy className="h-4 w-4" />
                {copied ? t('common.copied') : t('schemaDiff.copySummary')}
              </Button>
            )}
            {plan && (
              <Button variant="secondary" onClick={() => void handleCopySql()}>
                <Copy className="h-4 w-4" />
                {t('common.copySql')}
              </Button>
            )}
            <Button variant="ghost" onClick={() => void handleExportConfig()}>
              {t('schemaDiff.exportConfig')}
            </Button>
            <Button variant="ghost" onClick={() => void handleImportConfig()}>
              {t('schemaDiff.importConfig')}
            </Button>
          </div>
        </div>

        {error && <p className="mt-2 text-sm text-danger">{error}</p>}
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {showCompareLoading && (
          <div className="flex flex-1 items-center justify-center gap-2 text-sm text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('schemaDiff.compare')}
          </div>
        )}

        {!showCompareLoading && (
          <div className="flex min-h-0 flex-1">
            <div className="flex min-h-0 min-w-0 flex-1">
              <SchemaDiffTableListPanel
                className="w-48 max-w-[40%] flex-none"
                tables={parsedTables}
                selectedTable={selectedTable}
                onSelect={setSelectedTable}
                tableHasDiff={diffs.length > 0 ? tableHasDiff : undefined}
              />

              <div
                className="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto bg-surface-alt/30"
                data-testid="schema-diff-detail-panel"
              >
                {selectedDiff ? (
                  <div className="p-4">
                    <h3 className="mb-3 font-mono text-sm font-medium text-fg">
                      {selectedDiff.table}
                    </h3>
                    <SchemaDiffPanel diff={selectedDiff} />
                  </div>
                ) : (
                  <div className="flex flex-1 items-center justify-center p-4 text-sm text-fg-muted">
                    {diffs.length === 0 ? t('schemaDiff.compare') : t('schemaDiff.tableRequired')}
                  </div>
                )}
              </div>
            </div>

            <SchemaDiffRightPanel
              activeTab={rightTab}
              onTabChange={setRightTab}
              plan={plan}
              allowDestructive={allowDestructive}
              includeIndexes={includeIndexes}
              onAllowDestructiveChange={setAllowDestructive}
              onIncludeIndexesChange={setIncludeIndexes}
              onRegenerate={() => void buildPlan()}
              regenerating={loading && Boolean(plan)}
              targetLabel={targetLabel}
              useTransaction={useTransaction}
              onUseTransactionChange={setUseTransaction}
              requireRollback={requireRollback}
              onRequireRollbackChange={setRequireRollback}
              confirmText={confirmText}
              onConfirmTextChange={setConfirmText}
              deploying={loading && rightTab === 'deploy'}
              onDeploy={() => void handleDeploy()}
              deployResult={deployResult}
            />
          </div>
        )}
      </div>

      <SchemaDiffLimitationsDialog
        open={limitationsOpen}
        onClose={() => setLimitationsOpen(false)}
      />
      <StatusBar left={<span className="truncate">{t('common.schemaDiff')}</span>} />
    </div>
  );
}
