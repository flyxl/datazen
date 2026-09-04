import { useCallback, useEffect, useMemo } from 'react';
import { AlertTriangle, ArrowDownToLine, Loader2, Settings, Sparkles, Zap } from 'lucide-react';
import { Button } from '../ui/Button';
import { ExplainPlanTree } from '../query/ExplainPlanTree';
import { DataTable } from '../DataTable/DataTable';
import type { ColumnDef } from '../DataTable/TableHeader';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import type { ExplainPlanNode } from '../../types';
import { cn } from '../../lib/cn';
import { openSettingsWindow } from '../../lib/windowManager';

interface ExplainPanelProps {
  dbSessionId: string;
  sql: string;
  explainOutput: string;
  planJson?: unknown;
  planTree?: ExplainPlanNode | null;
  onApplySql?: (sql: string) => void;
}

const severityColors: Record<string, string> = {
  high: 'border-danger/30 bg-danger/10 text-danger',
  medium: 'border-warning/30 bg-warning/10 text-warning',
  low: 'border-accent/30 bg-accent/10 text-accent',
};

const severityIcons: Record<string, string> = {
  high: 'bg-danger',
  medium: 'bg-warning',
  low: 'bg-accent',
};

export function ExplainPanel({
  dbSessionId,
  sql,
  explainOutput,
  planJson,
  planTree,
  onApplySql,
}: ExplainPanelProps) {
  const { t } = useI18n();
  const analysis = useAiStore((s) => s.explainAnalysis);
  const isAnalyzing = useAiStore((s) => s.isAnalyzingExplain);
  const explainError = useAiStore((s) => s.explainError);
  const isConfigured = useAiStore((s) => s.isConfigured);
  const analyzeExplain = useAiStore((s) => s.analyzeExplain);
  const clearExplainAnalysis = useAiStore((s) => s.clearExplainAnalysis);

  useEffect(() => {
    return () => {
      clearExplainAnalysis();
    };
  }, [clearExplainAnalysis]);

  /**
   * When the driver returns the raw EXPLAIN result set inside planJson
   * (columns + rows, like MySQL / Kiwi), render it as a DataTable — the same
   * view as running EXPLAIN directly in the SQL editor. Falls back to the
   * plain-text output for drivers that only provide plan_text.
   */
  const rawTable = useMemo(() => {
    if (!planJson || typeof planJson !== 'object' || Array.isArray(planJson)) return null;
    const record = planJson as Record<string, unknown>;
    const cols = record['columns'];
    const rows = record['rows'];
    if (!Array.isArray(cols) || !Array.isArray(rows)) return null;
    const columnDefs: ColumnDef[] = cols
      .filter((c): c is string => typeof c === 'string')
      .map((name) => ({ id: name, name, type: 'string' }));
    if (columnDefs.length === 0) return null;
    const dataRows = rows.filter((r): r is unknown[] => Array.isArray(r));
    return { columns: columnDefs, rows: dataRows };
  }, [planJson]);

  const handleAnalyze = useCallback(() => {
    void analyzeExplain({ dbSessionId, explainOutput, originalSql: sql });
  }, [analyzeExplain, dbSessionId, explainOutput, sql]);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto">
      {/* Raw EXPLAIN output — DataTable when the driver provides columns/rows */}
      <div className="border-b border-edge p-3">
        <div className="mb-1.5 flex items-center justify-between">
          <span className="text-[11px] font-medium uppercase tracking-wider text-fg-muted">
            {t('explain.rawOutput')}
          </span>
          {isConfigured && !analysis && !isAnalyzing && (
            <Button
              variant="primary"
              className="h-6 gap-1 px-2 text-[11px]"
              onClick={handleAnalyze}
            >
              <Sparkles className="h-3 w-3" />
              {t('explain.analyze')}
            </Button>
          )}
        </div>
        {rawTable ? (
          <div className="overflow-hidden rounded border border-edge">
            <DataTable
              columns={rawTable.columns}
              rows={rawTable.rows}
              statusBar={
                <div className="flex items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
                  <span>
                    {rawTable.rows.length} {t('common.rows')}
                  </span>
                  <span className="text-edge">|</span>
                  <span>
                    {rawTable.columns.length} {t('common.columns')}
                  </span>
                </div>
              }
            />
          </div>
        ) : (
          <pre className="max-h-[200px] overflow-auto rounded border border-edge bg-surface-alt p-2 font-mono text-xs text-fg-secondary">
            {explainOutput}
          </pre>
        )}
      </div>

      {/* Plan tree */}
      {(planTree != null || planJson != null) && (
        <div className="border-b border-edge p-3">
          <ExplainPlanTree planTree={planTree} />
        </div>
      )}

      {/* AI Analysis */}
      {isAnalyzing && (
        <div className="flex items-center gap-2 p-4 text-xs text-fg-muted">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t('explain.analyzing')}
        </div>
      )}

      {explainError && (
        <div className="p-3">
          <div className="rounded border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400">
            {explainError}
          </div>
        </div>
      )}

      {analysis && (
        <div className="space-y-3 p-3">
          {/* Summary */}
          <div className="rounded border border-edge bg-surface-alt px-3 py-2">
            <div className="mb-1 text-[11px] font-medium uppercase tracking-wider text-fg-muted">
              {t('explain.summary')}
            </div>
            <p className="text-sm text-fg">{analysis.summary}</p>
          </div>

          {/* Bottlenecks */}
          {analysis.bottlenecks.length > 0 && (
            <div>
              <div className="mb-1.5 flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wider text-fg-muted">
                <AlertTriangle className="h-3 w-3" />
                {t('explain.bottlenecks')}
              </div>
              <div className="space-y-1.5">
                {analysis.bottlenecks.map((b, i) => (
                  <div
                    key={i}
                    className={cn(
                      'rounded border px-3 py-2',
                      severityColors[b.severity] ?? severityColors.low,
                    )}
                  >
                    <div className="flex items-center gap-2">
                      <span
                        className={cn(
                          'inline-block h-2 w-2 rounded-full',
                          severityIcons[b.severity] ?? severityIcons.low,
                        )}
                      />
                      <span className="text-xs font-medium">{b.node}</span>
                    </div>
                    <p className="mt-1 text-xs opacity-80">{b.description}</p>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Suggestions */}
          {analysis.suggestions.length > 0 && (
            <div>
              <div className="mb-1.5 flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wider text-fg-muted">
                <Zap className="h-3 w-3" />
                {t('explain.suggestions')}
              </div>
              <div className="space-y-1.5">
                {analysis.suggestions.map((s, i) => (
                  <div
                    key={i}
                    className="rounded border border-green-500/20 bg-green-500/5 px-3 py-2"
                  >
                    <p className="text-xs text-fg-secondary">{s.description}</p>
                    {s.sql && (
                      <div className="mt-1.5">
                        <pre className="rounded border border-edge bg-surface p-2 font-mono text-xs text-green-400">
                          {s.sql}
                        </pre>
                        {onApplySql && (
                          <Button
                            variant="ghost"
                            className="mt-1 h-5 gap-1 px-1.5 text-[10px]"
                            onClick={() => onApplySql(s.sql!)}
                          >
                            <ArrowDownToLine className="h-2.5 w-2.5" />
                            {t('nl2sql.apply')}
                          </Button>
                        )}
                      </div>
                    )}
                    <p className="mt-1 text-[11px] text-fg-muted">
                      {t('explain.expectedImpact')}: {s.impact}
                    </p>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {!isConfigured && (
        <div className="flex items-center gap-2 p-4 text-xs text-fg-muted">
          <Sparkles className="h-3.5 w-3.5" />
          <span className="flex-1">{t('common.aiNotConfigured')}</span>
          <Button
            variant="primary"
            className="h-6 gap-1 px-2 text-[11px]"
            onClick={() => openSettingsWindow('ai')}
          >
            <Settings className="h-3 w-3" />
            {t('settings.ai.goToConfigure')}
          </Button>
        </div>
      )}
    </div>
  );
}
