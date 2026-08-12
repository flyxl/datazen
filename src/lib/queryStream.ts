import type { QueryStreamEvent, StatementResult } from '../types';

export interface StreamableQueryTab {
  results: StatementResult[];
  running: boolean;
  error: string | null;
  executionTimeMs: number | null;
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
): T {
  switch (event.type) {
    case 'statementStart': {
      const results = tab.results.slice();
      while (results.length <= event.index) {
        results.push(emptyStatement(''));
      }
      results[event.index] = {
        sql: event.sql,
        columns: event.columns,
        rows: [],
        executionTimeMs: 0,
        truncated: false,
      };
      return { ...tab, results, error: null };
    }
    case 'rows': {
      const results = tab.results.map((result, index) =>
        index === event.index
          ? { ...result, rows: result.rows.concat(event.rows) }
          : result,
      );
      return { ...tab, results };
    }
    case 'statementEnd': {
      const results = tab.results.map((result, index) =>
        index === event.index
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
      };
    default:
      return tab;
  }
}
