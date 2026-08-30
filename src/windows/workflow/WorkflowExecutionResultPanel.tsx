import { useMemo, useState } from 'react';
import {
  AlertTriangle,
  BarChart3,
  ChevronDown,
  ChevronRight,
  TableProperties,
} from 'lucide-react';
import { ChartView } from '../../components/chart/ChartView';
import { isChartableResult } from '../../lib/chart/fieldInference';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';
import type { TranslationKey } from '../../locales';
import type { StepExecutionResult, WorkflowExecutionResult } from '../../types';
import type { ChartConfig } from '../../types/chart';
import { extractStepColumnNames, stepToStatementResult } from './workflowStepResultUtils';

type TFn = (key: TranslationKey, params?: Record<string, string | number>) => string;

export function WorkflowExecutionResultPanel({
  result,
  t,
}: {
  result: WorkflowExecutionResult;
  t: TFn;
}) {
  return (
    <div className="border border-edge rounded-md bg-surface overflow-hidden">
      <div
        className={`flex items-center justify-between px-3 py-2 text-xs font-medium border-b border-edge ${result.success ? 'bg-green-500/5 text-green-600 dark:text-green-400' : 'bg-red-500/5 text-red-600 dark:text-red-400'}`}
      >
        <span>
          {result.success ? '✓' : '✗'}{' '}
          {result.success ? t('workflows.result') : t('common.executionFailed')}
        </span>
        <span className="text-fg-muted font-normal">{result.totalTimeMs}ms</span>
      </div>

      {result.steps.map((step) => (
        <WorkflowStepResultRow key={step.stepId} step={step} />
      ))}

      {result.error && (
        <div className="px-3 py-2 text-xs text-red-400 bg-red-500/5 border-t border-edge">
          {result.error}
        </div>
      )}

      {result.finalOutput && (
        <div className="px-3 py-2 border-t border-edge">
          <div className="text-[10px] text-fg-muted mb-1">{t('workflows.finalOutput')}</div>
          <pre className="text-xs text-fg-secondary whitespace-pre-wrap break-words max-h-40 overflow-auto">
            {result.finalOutput}
          </pre>
        </div>
      )}
    </div>
  );
}

function WorkflowStepResultRow({ step }: { step: StepExecutionResult }) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);
  const [viewMode, setViewMode] = useState<'table' | 'chart'>('table');
  const [chartConfig, setChartConfig] = useState<ChartConfig | undefined>();
  const statusIcon =
    step.status === 'success'
      ? '✓'
      : step.status === 'skipped'
        ? '⏭'
        : step.status === 'timed_out'
          ? '⏱'
          : '✗';
  const statusColor =
    step.status === 'success'
      ? 'text-green-500'
      : step.status === 'skipped'
        ? 'text-yellow-500'
        : 'text-red-400';

  const colInfos = useMemo(() => extractStepColumnNames(step.result), [step.result]);
  const statementResult = useMemo(() => stepToStatementResult(step), [step]);
  const rows = statementResult?.rows;
  const rowsCount = (step.result?.rows_count as number | undefined) ?? rows?.length ?? 0;
  const chartable = useMemo(
    () => statementResult != null && isChartableResult(statementResult),
    [statementResult],
  );
  const hasData = colInfos.length > 0 && rows && rows.length > 0;

  return (
    <div className="border-b border-edge last:border-b-0">
      <button
        type="button"
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-1.5 text-xs hover:bg-surface-raised/50 transition-colors"
      >
        {expanded ? (
          <ChevronDown className="h-3 w-3 text-fg-muted shrink-0" />
        ) : (
          <ChevronRight className="h-3 w-3 text-fg-muted shrink-0" />
        )}
        <span className={`${statusColor} shrink-0`}>{statusIcon}</span>
        <span className="font-medium text-fg truncate">{step.stepId}</span>
        <span className="text-fg-muted">[{step.stepType}]</span>
        {step.connectionName && (
          <span className="text-accent text-[10px]">{step.connectionName}</span>
        )}
        <span className="ml-auto text-fg-muted shrink-0">{step.executionTimeMs}ms</span>
      </button>

      {expanded && (
        <div className="px-3 pb-2 space-y-1">
          {step.sqlExecuted && (
            <div>
              <div className="text-[10px] text-fg-muted">SQL</div>
              <pre className="text-[11px] font-mono text-fg-secondary bg-surface-alt/50 p-1.5 rounded whitespace-pre-wrap break-words max-h-24 overflow-auto">
                {step.sqlExecuted}
              </pre>
            </div>
          )}

          {step.error && <div className="text-[11px] text-red-400">{step.error}</div>}

          {hasData && (
            <div>
              <div className="flex items-center gap-2">
                <div className="text-[10px] text-fg-muted">{rowsCount} row(s)</div>
                {chartable && (
                  <div className="flex items-center gap-0.5 rounded-md bg-surface p-0.5 ml-auto">
                    <button
                      type="button"
                      onMouseDown={(e) => e.preventDefault()}
                      className={cn(
                        'flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[10px] transition-colors',
                        viewMode === 'table'
                          ? 'bg-accent/20 text-accent font-medium'
                          : 'text-fg-muted hover:text-fg-secondary',
                      )}
                      onClick={() => setViewMode('table')}
                    >
                      <TableProperties className="h-2.5 w-2.5" />
                      {t('chart.viewTable')}
                    </button>
                    <button
                      type="button"
                      onMouseDown={(e) => e.preventDefault()}
                      className={cn(
                        'flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[10px] transition-colors',
                        viewMode === 'chart'
                          ? 'bg-accent/20 text-accent font-medium'
                          : 'text-fg-muted hover:text-fg-secondary',
                      )}
                      onClick={() => setViewMode('chart')}
                    >
                      <BarChart3 className="h-2.5 w-2.5" />
                      {t('chart.viewChart')}
                    </button>
                  </div>
                )}
              </div>

              {viewMode === 'chart' && statementResult ? (
                <div
                  className="mt-1 border border-edge rounded overflow-hidden"
                  style={{ height: 260 }}
                >
                  {statementResult.rows.length > 1000 && (
                    <div className="flex items-center gap-1 bg-surface-alt px-2 py-0.5 text-[10px] text-yellow-400">
                      <AlertTriangle className="h-2.5 w-2.5" />
                      {t('chart.sampledWarning', { limit: '1000' })}
                    </div>
                  )}
                  <ChartView
                    result={statementResult}
                    savedConfig={chartConfig}
                    onConfigChange={setChartConfig}
                    onDataPointClick={() => setViewMode('table')}
                  />
                </div>
              ) : (
                <div className="overflow-auto max-h-40 border border-edge rounded mt-0.5">
                  <table className="w-full text-[11px]">
                    <thead className="sticky top-0">
                      <tr className="bg-surface-alt">
                        {colInfos.map((col) => (
                          <th
                            key={col.name}
                            className="px-2 py-1 text-left font-semibold text-fg border-b border-edge whitespace-nowrap"
                          >
                            {col.name}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {rows.slice(0, 20).map((row, i) => (
                        <tr
                          key={i}
                          className="border-b border-edge last:border-b-0 hover:bg-surface-raised/30"
                        >
                          {row.map((val, j) => (
                            <td
                              key={j}
                              className="px-2 py-0.5 text-fg-secondary whitespace-nowrap max-w-[200px] truncate"
                            >
                              {val == null ? (
                                <span className="text-fg-muted italic">null</span>
                              ) : (
                                String(val)
                              )}
                            </td>
                          ))}
                        </tr>
                      ))}
                      {rows.length > 20 && (
                        <tr>
                          <td
                            colSpan={colInfos.length}
                            className="px-2 py-0.5 text-fg-muted text-center"
                          >
                            ... {rows.length - 20} more
                          </td>
                        </tr>
                      )}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}

          {step.stepType === 'ai' && step.result?.result != null && (
            <pre className="text-[11px] text-fg-secondary whitespace-pre-wrap break-words max-h-40 overflow-auto">
              {String(step.result.result as string)}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}
