import type { ExplainResult } from '../../types';
import { ExecutionProjection } from './ExecutionProjection';
import {
  editorOwnerForPane,
  getQueryPanelSessionRegistry,
  queryPanelExecutionTarget,
} from './queryPanelSessionRoot';

/** Explain shares the editor's actor, physical session and observed context. */
export async function runEditorExplain(
  key: string,
  connectionId: string,
  sql: string,
  database: string | null | undefined,
  schema: string | null | undefined,
  params?: Record<string, string | number | boolean | null>,
): Promise<ExplainResult> {
  const controller = getQueryPanelSessionRegistry().get(
    key,
    queryPanelExecutionTarget(connectionId, database, schema),
    await editorOwnerForPane(key),
  );
  const receipt = await controller.execute({
    command: 'explain',
    input: { sql, ...(params && Object.keys(params).length > 0 ? { params } : {}) },
  });
  const projection = new ExecutionProjection(
    controller.client,
    receipt,
    controller.snapshot?.handle ?? null,
    (event) => controller.acceptSession(event.session),
  );
  let failedReads = 0;
  try {
    for (;;) {
      try {
        await projection.recover();
        failedReads = 0;
      } catch (error) {
        // Recover the accepted execution. Never re-submit SQL on a lost reply.
        if (++failedReads > 20) throw error;
        await new Promise<void>((resolve) => setTimeout(resolve, 250));
        continue;
      }
      const view = projection.view;
      if (view && ['succeeded', 'failed', 'cancelled'].includes(view.state)) {
        await controller.refresh().catch(() => undefined);
        if (view.state !== 'succeeded')
          throw new Error(`${view.state}: ${view.errorCode}; effect: ${view.effectOutcome}`);
        for (const chunk of projection.chunks) {
          const output = (chunk.payload as { output?: { kind?: string; data?: unknown } } | null)
            ?.output;
          if (output?.kind !== 'commandResult') continue;
          const data = output.data;
          if (
            data &&
            typeof data === 'object' &&
            'planText' in data &&
            typeof data.planText === 'string'
          )
            return data as ExplainResult;
        }
        throw new Error('Explain result unavailable');
      }
      await new Promise<void>((resolve) => setTimeout(resolve, 50));
    }
  } finally {
    await projection.dispose();
  }
}
