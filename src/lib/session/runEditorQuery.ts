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
  const signatures = new Map<number, string>();
  let disconnectedReads = 0;
  try {
    for (;;) {
      try {
        await projection.recover();
        disconnectedReads = 0;
      } catch (error) {
        // Recovery never re-submits SQL. Retain the published prefix and retry
        // missing reads on the same execution; a prolonged outage surfaces.
        if (++disconnectedReads > 20) throw error;
        await new Promise<void>((resolve) => setTimeout(resolve, 250));
        continue;
      }
      const statements = new Map<number, import('../../types').StatementResult>();
      for (const chunk of projection.chunks) {
        const envelope = record(chunk.payload);
        const output = record(envelope?.output);
        if (!output || typeof output.statementIndex !== 'number') continue;
        const index = output.statementIndex;
        const statement = statements.get(index) ?? {
          sql: '',
          columns: [],
          rows: [],
          executionTimeMs: 0,
          truncated: false,
          source: null,
          runtimeBinding: null,
          provenance: projection.view?.provenance ?? null,
        };
        if (chunk.source && (output.kind === 'statementMetadata' || !statement.source))
          statement.source = chunk.source;
        if (output.kind === 'rows') {
          if (typeof output.sql === 'string') statement.sql = output.sql;
          const earlyColumns = Array.isArray(output.columns) ? output.columns : envelope?.columns;
          if (Array.isArray(earlyColumns) && statement.columns.length === 0) {
            statement.columns = earlyColumns.filter((column): column is ColumnInfo => {
              const value = record(column);
              return (
                !!value &&
                typeof value.name === 'string' &&
                typeof value.dataType === 'string' &&
                typeof value.nullable === 'boolean'
              );
            });
          }
          if (Array.isArray(output.rows) && output.rows.every(Array.isArray)) {
            statement.rows.push(...(output.rows as (Value | null)[][]));
          }
        } else if (output.kind === 'statementSummary') {
          if (typeof output.sql === 'string') statement.sql = output.sql;
        } else if (output.kind === 'statementMetadata' || output.kind === 'statementColumns') {
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
        statement.runtimeBinding =
          statement.source?.writableMapping === 'verified'
            ? projection.writableBinding(controller.snapshot?.handle ?? null)
            : null;
        statements.set(index, statement);
      }
      for (const [index, result] of [...statements].sort(([a], [b]) => a - b)) {
        const signature = JSON.stringify(result);
        if (signatures.get(index) === signature) continue;
        signatures.set(index, signature);
        onEvent({ type: 'statementSnapshot', index, result });
      }
      const view = projection.view;
      if (view && ['succeeded', 'failed', 'cancelled'].includes(view.state)) {
        // Preserve the finalized published result even when the session is
        // lost or this execution fails/cancels after publishing a prefix.
        onEvent({
          type: 'publishedArtifacts',
          artifacts: {
            executionId: view.executionId,
            artifactIds: [...view.artifactIds],
            resultCompleteness: view.resultCompleteness,
          },
        });
        // The result and observed context have distinct lifetimes. Refresh only
        // this controller; matching handles prevent an old run moving a replacement.
        await controller.refresh().catch(() => undefined);
        if (view.state !== 'succeeded') {
          const diagnostic = projection.chunks
            .map((chunk) => record(record(chunk.payload)?.output))
            .find(
              (output) => output?.kind === 'executionError' && typeof output.message === 'string',
            );
          throw new Error(
            `${view.state}: ${diagnostic?.message ?? view.errorCode ?? 'Execution outcome unavailable'}; effect: ${view.effectOutcome}`,
          );
        }
        onEvent({ type: 'done', totalTimeMs: performance.now() - started });
        break;
      }
      await new Promise<void>((resolve) => setTimeout(resolve, 50));
    }
  } finally {
    await projection.dispose();
  }
}
