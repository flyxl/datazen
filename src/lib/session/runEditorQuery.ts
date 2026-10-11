import { useSettingsStore } from '../../stores/settingsStore';
import type { ColumnInfo, QueryStreamEvent, Value } from '../../types';
import { ExecutionProjection } from './ExecutionProjection';
import {
  editorOwnerForPane,
  getQueryPanelSessionRegistry,
  queryPanelExecutionTarget,
} from './queryPanelSessionRoot';

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

/** Execute and consume results on the pane's owned session, including bound queries. */
export async function runEditorQuery(
  key: string,
  connectionId: string,
  sql: string,
  database: string | null | undefined,
  schema: string | null | undefined,
  params: Record<string, string | number | boolean | null> | undefined,
  onEvent: (event: QueryStreamEvent) => void,
): Promise<void> {
  const controller = getQueryPanelSessionRegistry().get(
    key,
    queryPanelExecutionTarget(connectionId, database, schema),
    await editorOwnerForPane(key),
  );
  const started = performance.now();
  const settings = useSettingsStore.getState().settings;
  const receipt = await controller.execute({
    command: 'query',
    input: {
      sql,
      recordHistory: true,
      ...(params ? { params } : {}),
      ...(settings.limitSelectResults && settings.queryResultLimit > 0
        ? { limit: settings.queryResultLimit }
        : {}),
    },
  });
  onEvent({ type: 'executionStarted', executionId: receipt.executionId });
  const projection = new ExecutionProjection(
    controller.client,
    receipt,
    controller.snapshot?.handle ?? null,
    (event) => controller.acceptSession(event.session),
  );
  try {
    // Recovery reads only published chunks and deduplicates them by artifact/index.
    // It also handles executions that settled before the first subscription.
    for (;;) {
      await projection.recover();
      const view = projection.view;
      if (view && ['succeeded', 'failed', 'cancelled'].includes(view.state)) break;
      await new Promise<void>((resolve) => setTimeout(resolve, 50));
    }
    const statements = new Map<
      number,
      {
        sql: string;
        columns: ColumnInfo[];
        rows: (Value | null)[][];
        rowsAffected?: number;
        truncated: boolean;
      }
    >();
    for (const chunk of projection.chunks) {
      const envelope = record(chunk.payload);
      const output = record(envelope?.output);
      if (!output || typeof output.statementIndex !== 'number') continue;
      const index = output.statementIndex;
      const statement = statements.get(index) ?? {
        sql: '',
        columns: [],
        rows: [],
        truncated: false,
      };
      if (output.kind === 'rows') {
        if (typeof output.sql === 'string') statement.sql = output.sql;
        if (Array.isArray(output.rows) && output.rows.every(Array.isArray)) {
          statement.rows.push(...(output.rows as (Value | null)[][]));
        }
      } else if (output.kind === 'statementMetadata') {
        if (Array.isArray(output.columns)) {
          statement.columns = output.columns.filter((column): column is ColumnInfo => {
            const value = record(column);
            return (
              !!value &&
              typeof value.name === 'string' &&
              typeof value.dataType === 'string' &&
              typeof value.nullable === 'boolean'
            );
          });
        }
        statement.truncated = output.truncated === true;
      }
      if (typeof output.rowsAffected === 'number') statement.rowsAffected = output.rowsAffected;
      statements.set(index, statement);
    }
    for (const [index, statement] of [...statements].sort(([a], [b]) => a - b)) {
      onEvent({ type: 'statementStart', index, sql: statement.sql, columns: statement.columns });
      onEvent({ type: 'rows', index, rows: statement.rows });
      onEvent({
        type: 'statementEnd',
        index,
        rowsAffected: statement.rowsAffected,
        executionTimeMs: 0,
        truncated: statement.truncated,
      });
    }
    await controller.refresh();
    if (projection.view?.state !== 'succeeded') {
      throw new Error(projection.view?.errorCode ?? 'Execution outcome unavailable');
    }
    onEvent({ type: 'done', totalTimeMs: performance.now() - started });
  } finally {
    await projection.dispose();
  }
}
