import { Database } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { canOpenStructureEditor } from '../../lib/structureEditor/canOpenStructureEditor';
import { resolveExportScope } from '../../lib/exportCapability';
import { resolveCreateTableSchema } from '../../lib/structureEditor/resolveCreateTableSchema';
import { invalidateSchemaCache } from '../../lib/schemaCache';
import { getConnectionView } from '../../lib/connectionViews';
import {
  type Panel,
  type SubTabId,
  type ViewPanel,
  type ErDiagramPanel,
  type RedisDbPanel,
} from '../../stores/panelStore';
import { getSubTabs, getViewSubTabs } from './contentViewHelpers';
import { StructureView } from './StructureView';
import { TableView } from './TableView';
import { IndexesView } from './IndexesView';
import { ForeignKeysView } from './ForeignKeysView';
import { DDLView } from './DDLView';
import { QueryPanel } from './QueryPanel';
import { TableStructureEditor } from './TableStructureEditor';
import { ErDiagramView } from './ErDiagramView';
import { ObjectBrowser } from './ObjectBrowser';
import { DatabaseObjectView } from './DatabaseObjectView';
import { PrivilegeView } from './PrivilegeView';

export interface PanelContentRendererProps {
  activePanel: Panel | null;
  currentDatabase: string | null;
  lastTableSchema: string | null;
  onSetSubTab: (panelId: string, subTab: SubTabId) => void;
  onExitStructureEditing: (panelId: string) => void;
  onEditTableStructure: (name: string) => void;
  onSelectTable: (table: string, schema?: string) => void;
  onOpenErDiagram: (focus?: string) => void;
  onClosePanel: (panelId: string) => void;
  onRefresh: () => void;
  resolveTableSchema: (table: string) => string | null;
}

export function PanelContentRenderer({
  activePanel,
  currentDatabase,
  lastTableSchema,
  onSetSubTab,
  onExitStructureEditing,
  onEditTableStructure,
  onSelectTable,
  onOpenErDiagram,
  onClosePanel,
  onRefresh,
  resolveTableSchema,
}: PanelContentRendererProps) {
  const { t } = useI18n();

  if (!activePanel) {
    return (
      <div className="flex flex-1 items-center justify-center text-fg-muted">
        <div className="text-center">
          <Database className="mx-auto h-10 w-10 opacity-20" />
          <div className="mt-3 text-sm">
            {`${t('connWin.selectTable')} (⌘N ${t('connWin.newQuery')})`}
          </div>
        </div>
      </div>
    );
  }

  if (activePanel.type === 'redis-db') {
    const viewMode = DB_REGISTRY[activePanel.databaseType]?.connectionView ?? 'sql';
    const RedisView = getConnectionView(viewMode);
    return (
      <RedisView
        key={activePanel.id}
        connectionId={activePanel.connectionId}
        configId={activePanel.configId}
        connectionName={activePanel.connectionName}
        databaseType={activePanel.databaseType}
        initialDatabase={(activePanel as RedisDbPanel).dbName}
        hideSidebar
        isActive
      />
    );
  }

  return (
    <SqlPanelContent
      panel={activePanel}
      currentDatabase={currentDatabase}
      lastTableSchema={lastTableSchema}
      onSetSubTab={onSetSubTab}
      onExitStructureEditing={onExitStructureEditing}
      onEditTableStructure={onEditTableStructure}
      onSelectTable={onSelectTable}
      onOpenErDiagram={onOpenErDiagram}
      onClosePanel={onClosePanel}
      onRefresh={onRefresh}
      resolveTableSchema={resolveTableSchema}
    />
  );
}

interface SqlPanelContentProps {
  panel: Panel;
  currentDatabase: string | null;
  lastTableSchema: string | null;
  onSetSubTab: (panelId: string, subTab: SubTabId) => void;
  onExitStructureEditing: (panelId: string) => void;
  onEditTableStructure: (name: string) => void;
  onSelectTable: (table: string, schema?: string) => void;
  onOpenErDiagram: (focus?: string) => void;
  onClosePanel: (panelId: string) => void;
  onRefresh: () => void;
  resolveTableSchema: (table: string) => string | null;
}

function SqlPanelContent({
  panel,
  currentDatabase,
  lastTableSchema,
  onSetSubTab,
  onExitStructureEditing,
  onEditTableStructure,
  onSelectTable,
  onOpenErDiagram,
  onClosePanel,
  onRefresh,
  resolveTableSchema,
}: SqlPanelContentProps) {
  const { t } = useI18n();
  const panelDbMeta = DB_REGISTRY[panel.databaseType];
  const panelIsReadOnly = panelDbMeta?.readOnly === true;
  const panelShowStructureEditor = canOpenStructureEditor(panelDbMeta) && !panelIsReadOnly;
  const panelExportScope = resolveExportScope(panelDbMeta);

  if (panel.type === 'table') {
    return (
      <>
        <SubTabBar
          tabs={getSubTabs(t, panelIsReadOnly)}
          activeTab={panel.subTab}
          onSelect={(id) => onSetSubTab(panel.id, id)}
        />
        <div className="flex min-h-0 flex-1 flex-col">
          {panel.subTab === 'data' && (
            <TableView
              connectionId={panel.connectionId}
              database={currentDatabase ?? ''}
              tableName={panel.tableName}
              databaseType={panel.databaseType}
              dataExportCapability={panelExportScope}
            />
          )}
          {panel.subTab === 'structure' &&
            (panel.structureEditing ? (
              <TableStructureEditor
                connectionId={panel.connectionId}
                databaseType={panel.databaseType}
                schema={resolveTableSchema(panel.tableName)}
                mode="alter"
                tableName={panel.tableName}
                showBackButton
                onSuccess={() => {
                  invalidateSchemaCache(panel.connectionId, panel.tableName);
                  onExitStructureEditing(panel.id);
                  onRefresh();
                }}
                onCancel={() => onExitStructureEditing(panel.id)}
              />
            ) : (
              <StructureView
                connectionId={panel.connectionId}
                tableName={panel.tableName}
                onEditStructure={
                  panelShowStructureEditor ? () => onEditTableStructure(panel.tableName) : undefined
                }
              />
            ))}
          {panel.subTab === 'indexes' && (
            <IndexesView
              connectionId={panel.connectionId}
              tableName={panel.tableName}
              databaseType={panel.databaseType}
              onEditStructure={
                panelShowStructureEditor ? () => onSetSubTab(panel.id, 'structure') : undefined
              }
            />
          )}
          {panel.subTab === 'foreignKeys' && (
            <ForeignKeysView connectionId={panel.connectionId} tableName={panel.tableName} />
          )}
          {panel.subTab === 'ddl' && (
            <DDLView
              connectionId={panel.connectionId}
              tableName={panel.tableName}
              databaseType={panel.databaseType}
            />
          )}
        </div>
      </>
    );
  }

  if (panel.type === 'view') {
    return (
      <>
        <SubTabBar
          tabs={getViewSubTabs(t)}
          activeTab={(panel as ViewPanel).subTab}
          onSelect={(id) => onSetSubTab(panel.id, id)}
        />
        <div className="flex min-h-0 flex-1 flex-col">
          {(panel as ViewPanel).subTab === 'data' && (
            <TableView
              connectionId={panel.connectionId}
              database={currentDatabase ?? ''}
              tableName={(panel as ViewPanel).viewName}
              databaseType={panel.databaseType}
              dataExportCapability={panelExportScope}
            />
          )}
          {(panel as ViewPanel).subTab === 'structure' && (
            <StructureView
              connectionId={panel.connectionId}
              tableName={(panel as ViewPanel).viewName}
            />
          )}
          {(panel as ViewPanel).subTab === 'ddl' && (
            <DDLView
              connectionId={panel.connectionId}
              tableName={(panel as ViewPanel).viewName}
              databaseType={panel.databaseType}
              isView
            />
          )}
        </div>
      </>
    );
  }

  if (panel.type === 'query') {
    return (
      <QueryPanel
        connectionId={panel.connectionId}
        configId={panel.configId}
        queryTabId={(panel as import('../../stores/panelStore').QueryPanel).queryTabId}
        databaseType={panel.databaseType}
      />
    );
  }

  if (panel.type === 'create-table') {
    const schema = resolveCreateTableSchema(panel.databaseType, {
      currentDatabase,
      contextSchema: lastTableSchema,
    });
    return (
      <TableStructureEditor
        connectionId={panel.connectionId}
        databaseType={panel.databaseType}
        schema={schema}
        mode="create"
        onSuccess={() => {
          onClosePanel(panel.id);
          onRefresh();
        }}
        onCancel={() => onClosePanel(panel.id)}
      />
    );
  }

  if (panel.type === 'er-diagram' && currentDatabase) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <ErDiagramView
          connectionId={panel.connectionId}
          database={currentDatabase}
          focusTable={(panel as ErDiagramPanel).focusTable}
          onSelectTable={onSelectTable}
          onFocusTable={(table) => onOpenErDiagram(table)}
        />
      </div>
    );
  }

  if (panel.type === 'objects') {
    return <ObjectBrowser connectionId={panel.connectionId} databaseType={panel.databaseType} />;
  }

  if (panel.type === 'privileges') {
    return <PrivilegeView connectionId={panel.connectionId} />;
  }

  if (panel.type === 'db-object') {
    return (
      <DatabaseObjectView
        connectionId={panel.connectionId}
        databaseType={panel.databaseType}
        objectKind={(panel as import('../../stores/panelStore').DatabaseObjectPanel).objectKind}
        objectName={(panel as import('../../stores/panelStore').DatabaseObjectPanel).objectName}
        objectSchema={(panel as import('../../stores/panelStore').DatabaseObjectPanel).objectSchema}
      />
    );
  }

  return null;
}

function SubTabBar({
  tabs,
  activeTab,
  onSelect,
}: {
  tabs: { id: SubTabId; label: string }[];
  activeTab: SubTabId;
  onSelect: (id: SubTabId) => void;
}) {
  return (
    <div className="flex shrink-0 border-b border-edge bg-surface-alt">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          className={cn(
            'relative px-5 py-2 text-[13px] transition-colors',
            activeTab === tab.id
              ? 'bg-surface text-fg font-medium'
              : 'text-fg-secondary hover:text-fg',
          )}
          onClick={() => onSelect(tab.id)}
        >
          {tab.label}
          <span
            className={cn(
              'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
              activeTab === tab.id ? 'opacity-100' : 'opacity-0',
            )}
          />
        </button>
      ))}
    </div>
  );
}
