import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { BookOpen, Copy, Loader2 } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { SchemaDiffPanel, formatSchemaDiffText } from '../../components/schema/SchemaDiffPanel';
import { syncCommands } from '../../commands/sync';
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
import { listenCrossWindow } from '../../lib/crossWindowBus';
import { openDocsWindow } from '../../lib/windowManager';
import type { ConnectionConfig, TableSchemaDiff } from '../../types';
import { SchemaDiffPlanPanel } from './SchemaDiffPlanPanel';
import { SchemaDiffDeployPanel } from './SchemaDiffDeployPanel';

type Step = 'compare' | 'plan' | 'review';

function parseTableList(raw: string): string[] {
  return raw
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

export function SchemaDiffWindow() {
  useSettings();
  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const [connections, setConnections] = useState<ConnectionConfig[]>([]);
  const [activeConns, setActiveConns] = useState<Record<string, string>>({});
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [tableNamesRaw, setTableNamesRaw] = useState('');
  const [diffs, setDiffs] = useState<TableSchemaDiff[]>([]);
  const [plan, setPlan] = useState<SchemaDiffPlan | null>(null);
  const [step, setStep] = useState<Step>('compare');
  const [allowDestructive, setAllowDestructive] = useState(false);
  const [includeIndexes, setIncludeIndexes] = useState(true);
  const [requireRollback, setRequireRollback] = useState(false);
  const [useTransaction, setUseTransaction] = useState(true);
  const [confirmText, setConfirmText] = useState('');
  const [deployResult, setDeployResult] = useState<SchemaDiffDeployResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:connection-closed', (payload) => {
      const { connectionId } = (payload ?? {}) as { connectionId?: string };
      if (!connectionId) return;
      setActiveConns((prev) => {
        const next = { ...prev };
        for (const [cfgId, connId] of Object.entries(next)) {
          if (connId === connectionId) delete next[cfgId];
        }
        return next;
      });
    }).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, []);

  const loadConnections = useCallback(() => {
    void invoke<ConnectionConfig[]>('get_connections')
      .then(setConnections)
      .catch(() => setConnections([]));
  }, []);

  useEffect(() => {
    loadConnections();
  }, [loadConnections]);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:connections-changed', loadConnections).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, [loadConnections]);

  const connOptions = useMemo(
    () =>
      connections.map((c) => ({
        value: c.id,
        label: `${c.name} (${c.databaseType})`,
      })),
    [connections],
  );

  const targetLabel = useMemo(() => {
    const c = connections.find((x) => x.id === targetId);
    return c ? `${c.name} (${c.databaseType})` : targetId;
  }, [connections, targetId]);

  const ensureConnected = useCallback(
    async (configId: string): Promise<string | null> => {
      if (activeConns[configId]) return activeConns[configId];
      try {
        const connectionId = await invoke<string>('connect', { configId });
        setActiveConns((prev) => ({ ...prev, [configId]: connectionId }));
        return connectionId;
      } catch (e) {
        setError(`${t('sync.connectFailed')} ${e instanceof Error ? e.message : String(e)}`);
        return null;
      }
    },
    [activeConns, t],
  );

  const handleCompare = async () => {
    setError('');
    setDiffs([]);
    setPlan(null);
    setDeployResult(null);
    setStep('compare');
    if (!sourceId || !targetId) {
      setError(t('sync.selectBoth'));
      return;
    }
    if (sourceId === targetId) {
      setError(t('sync.cannotSame'));
      return;
    }
    const tables = parseTableList(tableNamesRaw);
    if (tables.length === 0) {
      setError(t('schemaDiff.tableRequired'));
      return;
    }

    setLoading(true);
    try {
      const srcConnId = await ensureConnected(sourceId);
      const tgtConnId = await ensureConnected(targetId);
      if (!srcConnId || !tgtConnId) return;
      const results: TableSchemaDiff[] = [];
      for (const table of tables) {
        results.push(await syncCommands.compareTableSchemas(srcConnId, tgtConnId, table));
      }
      setDiffs(results);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const buildPlan = async () => {
    setError('');
    setDeployResult(null);
    const tables = parseTableList(tableNamesRaw);
    if (tables.length === 0) {
      setError(t('schemaDiff.tableRequired'));
      return;
    }
    setLoading(true);
    try {
      const srcConnId = await ensureConnected(sourceId);
      const tgtConnId = await ensureConnected(targetId);
      if (!srcConnId || !tgtConnId) return;
      const next = await schemaDiffCommands.preparePlan({
        sourceConnectionId: srcConnId,
        targetConnectionId: tgtConnId,
        tableNames: tables,
        allowDestructive,
        includeIndexes,
      });
      setPlan(next);
      setStep('plan');
      setUseTransaction(dialectSupportsTransactionalDdl(next.targetDialect));
      setConfirmText('');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleDeploy = async () => {
    if (!plan) return;
    setError('');
    setLoading(true);
    try {
      const tgtConnId = await ensureConnected(targetId);
      if (!tgtConnId) return;
      const result = await schemaDiffCommands.executeDeploy({
        targetConnectionId: tgtConnId,
        plan,
        useTransaction,
        confirmDestructive: planHasDestructive(plan) ? confirmText.trim() : undefined,
      });
      setDeployResult(result);
      setStep('review');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

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
      version: 1,
      sourceConfigId: sourceId,
      targetConfigId: targetId,
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
      if (cfg.version !== 1 || !cfg.sourceConfigId || !cfg.targetConfigId) {
        throw new Error(t('schemaDiff.invalidConfig'));
      }
      setSourceId(cfg.sourceConfigId);
      setTargetId(cfg.targetConfigId);
      setTableNamesRaw((cfg.tables ?? []).join('\n'));
      setAllowDestructive(Boolean(cfg.allowDestructive));
      setIncludeIndexes(cfg.includeIndexes ?? true);
      setRequireRollback(Boolean(cfg.requireRollback));
      setPlan(null);
      setDiffs([]);
      setStep('compare');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="flex h-screen flex-col bg-canvas text-fg">
      <TitleBar
        title={t('schemaDiff.title')}
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
      <div className="flex flex-1 flex-col gap-4 overflow-auto p-6">
        <p className="text-sm text-fg-muted">{t('schemaDiff.description')}</p>
        <div className="flex flex-wrap gap-2 text-xs text-fg-muted">
          <span className={step === 'compare' ? 'text-accent' : ''}>
            {t('schemaDiff.stepCompare')}
          </span>
          <span>→</span>
          <span className={step === 'plan' ? 'text-accent' : ''}>{t('schemaDiff.stepPlan')}</span>
          <span>→</span>
          <span className={step === 'review' ? 'text-accent' : ''}>
            {t('schemaDiff.stepReview')}
          </span>
        </div>

        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <label className="space-y-1 text-sm">
            <span className="text-fg-secondary">{t('sync.source')}</span>
            <Select
              value={sourceId}
              onChange={setSourceId}
              options={connOptions}
              placeholder={t('sync.selectSource')}
            />
          </label>
          <label className="space-y-1 text-sm">
            <span className="text-fg-secondary">{t('sync.target')}</span>
            <Select
              value={targetId}
              onChange={setTargetId}
              options={connOptions}
              placeholder={t('sync.selectTarget')}
            />
          </label>
        </div>

        <label className="space-y-1 text-sm">
          <span className="text-fg-secondary">{t('schemaDiff.tables')}</span>
          <textarea
            className="min-h-[72px] w-full rounded-md border border-edge bg-surface px-3 py-2 font-mono text-sm text-fg"
            value={tableNamesRaw}
            onChange={(e) => setTableNamesRaw(e.target.value)}
            placeholder={t('schemaDiff.tablesPlaceholder')}
          />
        </label>

        <div className="flex flex-wrap items-center gap-3">
          <Button variant="primary" disabled={loading} onClick={() => void handleCompare()}>
            {loading && step === 'compare' ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {t('schemaDiff.compare')}
          </Button>
          <Button
            variant="secondary"
            disabled={
              loading || !sourceId || !targetId || parseTableList(tableNamesRaw).length === 0
            }
            onClick={() => void buildPlan()}
          >
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
              {t('schemaDiff.copySql')}
            </Button>
          )}
          <Button variant="ghost" onClick={() => void handleExportConfig()}>
            {t('schemaDiff.exportConfig')}
          </Button>
          <Button variant="ghost" onClick={() => void handleImportConfig()}>
            {t('schemaDiff.importConfig')}
          </Button>
        </div>

        {error && <p className="text-sm text-danger">{error}</p>}

        {diffs.map((diff) => (
          <div key={diff.table} className="rounded-md border border-edge bg-surface p-4">
            <h3 className="mb-3 font-mono text-sm font-medium text-fg">{diff.table}</h3>
            <SchemaDiffPanel diff={diff} />
          </div>
        ))}

        {plan && (
          <div className="rounded-md border border-edge bg-surface p-4">
            <h3 className="mb-3 text-sm font-medium text-fg">{t('schemaDiff.stepPlan')}</h3>
            <SchemaDiffPlanPanel
              plan={plan}
              allowDestructive={allowDestructive}
              includeIndexes={includeIndexes}
              regenerating={loading}
              onAllowDestructiveChange={setAllowDestructive}
              onIncludeIndexesChange={setIncludeIndexes}
              onRegenerate={() => void buildPlan()}
            />
            <div className="mt-4">
              <Button variant="primary" onClick={() => setStep('review')}>
                {t('schemaDiff.stepReview')}
              </Button>
            </div>
          </div>
        )}

        {plan && step === 'review' && (
          <div className="rounded-md border border-edge bg-surface p-4">
            <h3 className="mb-3 text-sm font-medium text-fg">{t('schemaDiff.stepReview')}</h3>
            <SchemaDiffDeployPanel
              plan={plan}
              targetLabel={targetLabel}
              useTransaction={useTransaction}
              onUseTransactionChange={setUseTransaction}
              requireRollback={requireRollback}
              onRequireRollbackChange={setRequireRollback}
              confirmText={confirmText}
              onConfirmTextChange={setConfirmText}
              deploying={loading}
              onDeploy={() => void handleDeploy()}
              result={deployResult}
            />
          </div>
        )}
      </div>
      <StatusBar left={<span className="truncate">{t('schemaDiff.title')}</span>} />
    </div>
  );
}
