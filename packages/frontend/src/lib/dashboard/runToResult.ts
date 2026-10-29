import type { StatementResult } from '../../types';
import type { WidgetRun } from '../../types/dashboard';

/** Adapt a persisted widget run snapshot for the chart transform pipeline. */
export function widgetRunToStatementResult(run: WidgetRun): StatementResult {
  return {
    sql: '',
    columns: run.columns.map((name) => ({
      name,
      dataType: 'unknown',
      nullable: true,
    })),
    rows: run.rows as StatementResult['rows'],
    executionTimeMs: 0,
  };
}
