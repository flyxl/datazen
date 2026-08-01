import { useCallback, useEffect } from 'react';
import { AlertTriangle, ArrowDownToLine, Loader2, Settings, Sparkles, Zap } from 'lucide-react';
import { Button } from '../ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { cn } from '../../lib/cn';
import { openSettingsWindow } from '../../lib/windowManager';

interface ExplainPanelProps {
  connectionId: string;
  sql: string;
  explainOutput: string;
  onApplySql?: (sql: string) => void;
}

const severityColors: Record<string, string> = {
  high: 'border-red-500/30 bg-red-500/10 text-red-400',
  medium: 'border-yellow-500/30 bg-yellow-500/10 text-yellow-400',
  low: 'border-blue-500/30 bg-blue-500/10 text-blue-400',
};

const severityIcons: Record<string, string> = {
  high: 'bg-red-500',
  medium: 'bg-yellow-500',
  low: 'bg-blue-500',
};

export function ExplainPanel({ connectionId, sql, explainOutput, onApplySql }: ExplainPanelProps) {
  const { t } = useI18n();
  const analysis = useAiStore((s) => s.explainAnalysis);
  const isAnalyzing = useAiStore((s) => s.isAnalyzingExplain);
  const explainError = useAiStore((s) => s.explainError);
  const isConfigured = useAiStore((s) => s.isConfigured);
  const analyzeExplain = useAiStore((s) => s.analyzeExplain);
  const clearExplainAnalysis = useAiStore((s) => s.clearExplainAnalysis);

  useEffect(() => {
    clearExplainAnalysis();
    return () => {
      clearExplainAnalysis();
    };
  }, [explainOutput, sql, clearExplainAnalysis]);

  const handleAnalyze = useCallback(() => {
    void analyzeExplain({ connectionId, explainOutput, originalSql: sql });
  }, [analyzeExplain, connectionId, explainOutput, sql]);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto">
      {/* Raw EXPLAIN output */}
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
        <pre className="max-h-[200px] overflow-auto rounded border border-edge bg-surface-alt p-2 font-mono text-xs text-fg-secondary">
          {explainOutput}
        </pre>
      </div>

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
                    className={cn('rounded border px-3 py-2', severityColors[b.severity] ?? severityColors.low)}
                  >
                    <div className="flex items-center gap-2">
                      <span className={cn('inline-block h-2 w-2 rounded-full', severityIcons[b.severity] ?? severityIcons.low)} />
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
                  <div key={i} className="rounded border border-green-500/20 bg-green-500/5 px-3 py-2">
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
          <span className="flex-1">{t('explain.notConfigured')}</span>
          <Button variant="primary" className="h-6 gap-1 px-2 text-[11px]" onClick={() => openSettingsWindow('ai')}>
            <Settings className="h-3 w-3" />
            {t('settings.ai.goToConfigure')}
          </Button>
        </div>
      )}
    </div>
  );
}
