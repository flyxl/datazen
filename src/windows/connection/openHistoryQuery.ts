import { useConnectionStore } from '../../stores/connectionStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { usePanelStore, type QueryPanel } from '../../stores/panelStore';
import { nextPanelId } from '../../stores/panelTypes';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { splitPathHierarchyDatabasePin } from '../../lib/queryContextPath';
import type { QueryHistoryEntry } from '../../types';

export interface OpenHistoryQueryOptions {
  onSelectConnection?: (connectionId: string) => void;
  currentConnectionId?: string | null;
}

/**
 * 统一复用：将选中的历史 SQL 在对应的数据库连接中打开新查询面板。
 * 覆盖场景：
 * 1. 首页/默认页最近查询卡片直接点击
 * 2. 全局历史查询弹窗（GlobalQueryHistoryDialog）点击「在连接中打开」
 * 若连接已连通，直接新建 QueryPanel 并填入 SQL；若连接未连通，先发起连接，并设置 pendingHistoryQuery 等待连通后自动建立面板。
 */
export function openHistoryQuery(
  entry: Pick<QueryHistoryEntry, 'sql'> &
    Partial<Pick<QueryHistoryEntry, 'connectionId' | 'database' | 'schema'>>,
  options?: OpenHistoryQueryOptions,
) {
  const savedConnections = useConnectionStore.getState().connections;
  const activeConnections = useActiveConnectionStore.getState().connections;

  // 1. 解析目标连接 ID
  const targetConnId =
    entry.connectionId ||
    options?.currentConnectionId ||
    Object.values(activeConnections).find((c) => c?.status === 'connected')?.connectionId ||
    savedConnections[0]?.id;

  if (!targetConnId) return;

  const savedConn = savedConnections.find((c) => c.id === targetConnId);
  if (!savedConn) return;

  // 2. 如果提供了 onSelectConnection，则激活/选中该连接 Tab
  if (options?.onSelectConnection) {
    options.onSelectConnection(targetConnId);
  }

  // 3. 检查当前会话是否已经连通
  const activeEntry = activeConnections[targetConnId];
  const isConnected = activeEntry?.status === 'connected' && !!activeEntry.dbSessionId;

  if (isConnected && activeEntry.dbSessionId) {
    const panelId = nextPanelId('qry');
    let panelDatabase = entry.database?.trim() || savedConn.database || undefined;
    let namespacePath: string[] | undefined;
    const meta = DB_REGISTRY[savedConn.databaseType];
    if (meta?.namespaceEnsure === 'path-hierarchy' && panelDatabase?.includes('/')) {
      const split = splitPathHierarchyDatabasePin(panelDatabase);
      panelDatabase = split.root || undefined;
      namespacePath = split.namespacePath.length > 0 ? split.namespacePath : undefined;
    }
    const db = panelDatabase || '';
    const panel: QueryPanel = {
      connectionId: targetConnId,
      dbSessionId: activeEntry.dbSessionId,
      connectionName: savedConn.name,
      databaseType: savedConn.databaseType,
      type: 'query',
      id: panelId,
      title: db ? `${savedConn.name}@${db}` : savedConn.name,
      database: panelDatabase,
      schema: entry.schema?.trim() || undefined,
      namespacePath,
    };
    usePanelStore.getState().addPanel(panel, true);
    if (entry.sql) {
      usePanelStore.getState().updateSql(panelId, entry.sql);
    }
    usePanelStore.getState().setActivePanel(panelId);
  } else {
    // 4. 连接尚未建立，设置 pending intent，由 usePanelHandlers 在连接建立后消费并创建面板
    usePanelStore.getState().setPendingHistoryQuery({
      connectionId: targetConnId,
      sql: entry.sql,
      database: entry.database,
      schema: entry.schema,
    });
  }
}
