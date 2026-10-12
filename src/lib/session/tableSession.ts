import type { SessionHandle } from '@datazen/backend-client';
import { editorOwnerForPane, getQueryPanelSessionRegistry, queryPanelExecutionTarget } from './queryPanelSessionRoot';
import type { TableChangeContext } from '../tableChanges';

export const tableSessionKey = (panelId: string): string => `${panelId}:table`;
export async function tableSessionController(panelId: string, context: TableChangeContext) {
  if (!context.connectionId) throw new Error('Table connection required');
  const key = tableSessionKey(panelId);
  return getQueryPanelSessionRegistry().get(key,
    queryPanelExecutionTarget(context.connectionId, context.database, context.schema), await editorOwnerForPane(key));
}
export function tableManualHandle(panelId: string): SessionHandle | null {
  const view = getQueryPanelSessionRegistry().peek(tableSessionKey(panelId))?.snapshot;
  if (!view) return null;
  if (['lost', 'closed', 'closing'].includes(view.state)) throw new Error('Table session lost; explicitly open a new session.');
  const transaction = view.observedContext.transactionState;
  return transaction === 'active' || transaction === 'aborted' || transaction === 'unknown' ? view.handle : null;
}
