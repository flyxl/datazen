import type { ReactNode } from 'react';
import {
  Braces,
  Code2,
  Database,
  Eye,
  GitFork,
  Hash,
  KeyRound,
  Activity,
  Shapes,
  Table2,
  TableProperties,
  Zap,
} from 'lucide-react';
import { createElement } from 'react';
import { ThemedIcon } from '../../components/ThemedIcon';
import {
  type Panel,
  type TablePanel,
  type ViewPanel,
  type QueryPanel,
  type CreateTablePanel,
  type DatabaseObjectPanel,
  type RedisDbPanel,
  type ConnectionContext,
  type SubTabId,
} from '../../stores/panelStore';
import type { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import type { useConnectionStore } from '../../stores/connectionStore';
import type { TranslationKey } from '../../locales';

const iconClass = 'h-3.5 w-3.5 shrink-0';

export function getPanelIcon(panel: Panel): ReactNode {
  switch (panel.type) {
    case 'table':
      return createElement(ThemedIcon, {
        id: 'schema.table',
        className: `${iconClass} text-blue-400`,
        fallback: Table2,
      });
    case 'view':
      return createElement(ThemedIcon, {
        id: 'schema.view',
        className: `${iconClass} text-purple-400`,
        fallback: Eye,
      });
    case 'query':
      return createElement(ThemedIcon, {
        id: 'common.newQuery',
        className: iconClass,
        fallback: Code2,
      });
    case 'create-table':
      return createElement(ThemedIcon, {
        id: 'common.newTable',
        className: iconClass,
        fallback: TableProperties,
      });
    case 'er-diagram':
      return createElement(ThemedIcon, {
        id: 'common.erDiagram',
        className: iconClass,
        fallback: GitFork,
      });
    case 'objects':
      return createElement(ThemedIcon, {
        id: 'common.objects',
        className: iconClass,
        fallback: Code2,
      });
    case 'privileges':
      return createElement(ThemedIcon, {
        id: 'action.privileges',
        className: iconClass,
        fallback: KeyRound,
      });
    case 'server-status':
      return createElement(ThemedIcon, {
        id: 'action.serverStatus',
        className: `${iconClass} text-emerald-400`,
        fallback: Activity,
      });
    case 'processes':
      return createElement(ThemedIcon, {
        id: 'action.processes',
        className: `${iconClass} text-rose-400`,
        fallback: Database,
      });
    case 'db-object': {
      const kind = (panel as DatabaseObjectPanel).objectKind;
      if (kind === 'trigger')
        return createElement(ThemedIcon, {
          id: 'schema.trigger',
          className: `${iconClass} text-amber-400`,
          fallback: Zap,
        });
      if (kind === 'procedure')
        return createElement(ThemedIcon, {
          id: 'schema.procedure',
          className: `${iconClass} text-emerald-400`,
          fallback: Braces,
        });
      if (kind === 'sequence')
        return createElement(ThemedIcon, {
          id: 'schema.sequence',
          className: `${iconClass} text-cyan-400`,
          fallback: Hash,
        });
      if (kind === 'type')
        return createElement(ThemedIcon, {
          id: 'schema.type',
          className: `${iconClass} text-pink-400`,
          fallback: Shapes,
        });
      return createElement(ThemedIcon, {
        id: 'schema.function',
        className: `${iconClass} text-orange-400`,
        fallback: Braces,
      });
    }
    // 'redis-db' kept as panel type discriminant for TypeScript union narrowing.
    // Conditional behaviour is driven by DB_REGISTRY[dbType]?.isKeyValue.
    case 'redis-db':
      return createElement(ThemedIcon, {
        id: 'schema.redisDatabase',
        className: `${iconClass} text-teal-400`,
        fallback: Database,
      });
    default:
      return null;
  }
}

export function getPanelShortLabel(panel: Panel, t?: (key: TranslationKey) => string): string {
  const tr = (key: TranslationKey, fallback: string) => (t ? t(key) : fallback);
  switch (panel.type) {
    case 'table':
      return (panel as TablePanel).tableName;
    case 'view':
      return (panel as ViewPanel).viewName;
    case 'query':
      return (panel as QueryPanel).title;
    case 'create-table':
      return tr('common.newTable', 'New Table');
    case 'er-diagram':
      return tr('common.erDiagram', 'ER Diagram');
    case 'objects':
      return tr('objects.title', 'Objects');
    case 'privileges':
      return tr('privileges.title', 'Privileges');
    case 'server-status':
      return tr('serverStatus.dashboardTitle', 'Server Dashboard');
    case 'processes':
      return tr('common.processList', 'Process List');
    case 'db-object':
      return (panel as DatabaseObjectPanel).objectName;
    // 'redis-db' kept for union narrowing; DB_REGISTRY.isKeyValue governs branching elsewhere.
    case 'redis-db':
      return (panel as RedisDbPanel).dbName;
    default:
      return '';
  }
}

export function resolvePanelTabDatabase(
  panel: Panel,
  sessionDatabase?: string | null,
): string | null {
  switch (panel.type) {
    case 'table':
      return (panel as TablePanel).database ?? sessionDatabase ?? null;
    case 'view':
      return (panel as ViewPanel).database ?? sessionDatabase ?? null;
    case 'create-table':
      return (panel as CreateTablePanel).database ?? sessionDatabase ?? null;
    // 'redis-db' kept for union narrowing; DB_REGISTRY.isKeyValue governs branching elsewhere.
    case 'redis-db':
      return (panel as RedisDbPanel).dbName;
    default:
      return sessionDatabase ?? null;
  }
}

/** Tab title: `connection · database · object` (database omitted when unknown). */
export function getPanelTabLabel(
  panel: Panel,
  sessionDatabase?: string | null,
  t?: (key: TranslationKey) => string,
): string {
  const short = getPanelShortLabel(panel, t);
  const database = resolvePanelTabDatabase(panel, sessionDatabase)?.trim();
  const parts = [panel.connectionName];
  if (database) parts.push(database);
  if (short) parts.push(short);
  return parts.join(' · ');
}

export function getPanelLabel(panel: Panel, t?: (key: TranslationKey) => string): string {
  return getPanelShortLabel(panel, t);
}

export function getSubTabs(
  t: (key: TranslationKey) => string,
  readOnly?: boolean,
): { id: SubTabId; label: string }[] {
  if (readOnly) {
    return [
      { id: 'data', label: t('connWin.data') },
      { id: 'structure', label: t('connWin.structure') },
    ];
  }
  return [
    { id: 'data', label: t('connWin.data') },
    { id: 'structure', label: t('connWin.structure') },
    { id: 'indexes', label: t('connWin.indexes') },
    { id: 'foreignKeys', label: t('connWin.foreignKeys') },
    { id: 'ddl', label: 'DDL' },
  ];
}

export function getViewSubTabs(
  t: (key: TranslationKey) => string,
): { id: SubTabId; label: string }[] {
  return [
    { id: 'data', label: t('connWin.data') },
    { id: 'structure', label: t('connWin.structure') },
    { id: 'ddl', label: 'DDL' },
  ];
}

export function resolveConnectionContext(
  dbSessionId: string,
  activeConnections: ReturnType<typeof useActiveConnectionStore.getState>['connections'],
  savedConnections: ReturnType<typeof useConnectionStore.getState>['connections'],
): ConnectionContext | null {
  const entry = Object.values(activeConnections).find((e) => e.dbSessionId === dbSessionId);
  if (!entry?.dbSessionId) return null;
  const saved = savedConnections.find((c) => c.id === entry.connectionId);
  if (!saved) return null;
  return {
    connectionId: entry.connectionId,
    dbSessionId: entry.dbSessionId,
    connectionName: saved.name,
    databaseType: saved.databaseType,
  };
}

/**
 * 按 connectionId（持久化连接 ID）直接解析当前活动连接上下文。
 * 右键菜单已知用户点击的连接的 connectionId，据此同步取到该连接的实时 dbSessionId，
 * 明确绑定面板，避免读取「全局活动连接」串数据。
 */
export function resolveConnectionContextByConnection(
  connectionId: string,
  activeConnections: ReturnType<typeof useActiveConnectionStore.getState>['connections'],
  savedConnections: ReturnType<typeof useConnectionStore.getState>['connections'],
): ConnectionContext | null {
  const entry = activeConnections[connectionId];
  if (!entry?.dbSessionId) return null;
  const saved = savedConnections.find((c) => c.id === connectionId);
  if (!saved) return null;
  return {
    connectionId,
    dbSessionId: entry.dbSessionId,
    connectionName: saved.name,
    databaseType: saved.databaseType,
  };
}
