import type { QueryStreamEvent, StatementResult } from '../types';

export interface StreamableQueryTab {
  results: StatementResult[];
  running: boolean;
  error: string | null;
  executionTimeMs: number | null;
  executionId: string | null;
}

function emptyStatement(sql: string): StatementResult {
  return {
    sql,
    columns: [],
    rows: [],
    executionTimeMs: 0,
    truncated: false,
  };
}

/** Apply one stream event. Does not interpret batch size as a SQL row cap. */
export function applyQueryStreamEvent<T extends StreamableQueryTab>(
  tab: T,
  event: QueryStreamEvent,
  baseOffset: number = 0,
): T {
  switch (event.type) {
    case 'executionStarted':
      return { ...tab, executionId: event.executionId };
    case 'statementStart': {
      const targetIndex = event.index + baseOffset;
      const results = tab.results.slice();
      while (results.length <= targetIndex) {
        results.push(emptyStatement(''));
      }
      results[targetIndex] = {
        sql: event.sql,
        columns: event.columns,
        rows: [],
        executionTimeMs: 0,
        truncated: false,
      };
      return { ...tab, results, error: null };
    }
    case 'rows': {
      const targetIndex = event.index + baseOffset;
      const results = tab.results.map((result, index) =>
        index === targetIndex ? { ...result, rows: result.rows.concat(event.rows) } : result,
      );
      return { ...tab, results };
    }
    case 'statementEnd': {
      const targetIndex = event.index + baseOffset;
      const results = tab.results.map((result, index) =>
        index === targetIndex
          ? {
              ...result,
              rowsAffected: event.rowsAffected,
              executionTimeMs: event.executionTimeMs,
              truncated: event.truncated,
            }
          : result,
      );
      return { ...tab, results };
    }
    case 'done':
      return {
        ...tab,
        running: false,
        executionTimeMs: event.totalTimeMs,
        error: null,
        executionId: null,
      };
    default:
      return tab;
  }
}
