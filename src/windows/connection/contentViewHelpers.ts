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
import {
  type Panel,
  type TablePanel,
  type ViewPanel,
  type QueryPanel,
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
      return createElement(Table2, { className: `${iconClass} text-blue-400` });
    case 'view':
      return createElement(Eye, { className: `${iconClass} text-purple-400` });
    case 'query':
      return createElement(Code2, { className: iconClass });
    case 'create-table':
      return createElement(TableProperties, { className: iconClass });
    case 'er-diagram':
      return createElement(GitFork, { className: iconClass });
    case 'objects':
      return createElement(Code2, { className: iconClass });
    case 'privileges':
      return createElement(KeyRound, { className: iconClass });
    case 'server-status':
      return createElement(Activity, { className: `${iconClass} text-emerald-400` });
    case 'processes':
      return createElement(Database, { className: `${iconClass} text-rose-400` });
    case 'db-object': {
      const kind = (panel as DatabaseObjectPanel).objectKind;
      if (kind === 'trigger')
        return createElement(Zap, { className: `${iconClass} text-amber-400` });
      if (kind === 'procedure')
        return createElement(Braces, { className: `${iconClass} text-emerald-400` });
      if (kind === 'sequence')
        return createElement(Hash, { className: `${iconClass} text-cyan-400` });
      if (kind === 'type')
        return createElement(Shapes, { className: `${iconClass} text-pink-400` });
      return createElement(Braces, { className: `${iconClass} text-orange-400` });
    }
    case 'redis-db':
      return createElement(Database, { className: `${iconClass} text-teal-400` });
    default:
      return null;
  }
}

export function getPanelLabel(panel: Panel): string {
  switch (panel.type) {
    case 'table':
      return (panel as TablePanel).tableName;
    case 'view':
      return (panel as ViewPanel).viewName;
    case 'query':
      return (panel as QueryPanel).title;
    case 'create-table':
      return 'New Table';
    case 'er-diagram':
      return 'ER Diagram';
    case 'objects':
      return 'Objects';
    case 'privileges':
      return 'Privileges';
    case 'server-status':
      return 'Server Status';
    case 'processes':
      return 'Processes';
    case 'db-object':
      return (panel as DatabaseObjectPanel).objectName;
    case 'redis-db':
      return `${panel.connectionName}@${(panel as RedisDbPanel).dbName}`;
    default:
      return '';
  }
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
  connectionId: string,
  activeConnections: ReturnType<typeof useActiveConnectionStore.getState>['connections'],
  savedConnections: ReturnType<typeof useConnectionStore.getState>['connections'],
): ConnectionContext | null {
  const entry = Object.values(activeConnections).find((e) => e.connectionId === connectionId);
  if (!entry?.connectionId) return null;
  const saved = savedConnections.find((c) => c.id === entry.configId);
  if (!saved) return null;
  return {
    configId: entry.configId,
    connectionId: entry.connectionId,
    connectionName: saved.name,
    databaseType: saved.databaseType,
  };
}
