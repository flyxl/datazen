import { useEffect, useRef, useState } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { formatPanelContextPath } from '../../lib/panelContextPath';
import { canOpenStructureEditor } from '../../lib/structureEditor/canOpenStructureEditor';
import { resolveExportScope } from '../../lib/exportCapability';
import { resolveCreateTableSchema } from '../../lib/structureEditor/resolveCreateTableSchema';
import { invalidateSchemaCache } from '../../lib/schemaCache';
import { getConnectionView } from '../../lib/connectionViews';
import { databaseCommands } from '../../commands/database';
import { useSchemaStore } from '../../stores/schemaStore';
import {
  type Panel,
  type SubTabId,
  type ViewPanel,
  type ErDiagramPanel,
  type RedisDbPanel,
  type CreateTablePanel,
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
  if (!activePanel) {
    return null;
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
          contextPath={formatPanelContextPath({
            connectionName: panel.connectionName,
            database: panel.database ?? currentDatabase,
            schema: panel.tableSchema,
          })}
        />
        <div className="flex min-h-0 flex-1 flex-col">
          {panel.subTab === 'data' && (
            <TableView
              connectionId={panel.connectionId}
              database={panel.database ?? currentDatabase ?? ''}
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
                database={panel.database ?? currentDatabase}
                schema={panel.tableSchema ?? resolveTableSchema(panel.tableName)}
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
          contextPath={formatPanelContextPath({
            connectionName: panel.connectionName,
            database: (panel as ViewPanel).database ?? currentDatabase,
            schema: (panel as ViewPanel).viewSchema,
          })}
        />
        <div className="flex min-h-0 flex-1 flex-col">
          {(panel as ViewPanel).subTab === 'data' && (
            <TableView
              connectionId={panel.connectionId}
              database={(panel as ViewPanel).database ?? currentDatabase ?? ''}
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
        panelId={panel.id}
        connectionId={panel.connectionId}
        configId={panel.configId}
        databaseType={panel.databaseType}
      />
    );
  }

  if (panel.type === 'create-table') {
    const createPanel = panel as CreateTablePanel;
    const panelDatabase = createPanel.database ?? currentDatabase;
    const schema = resolveCreateTableSchema(panel.databaseType, {
      currentDatabase: panelDatabase,
      contextSchema: createPanel.tableSchema ?? lastTableSchema,
    });
    return (
      <CreateTablePanelContent
        connectionId={panel.connectionId}
        databaseType={panel.databaseType}
        database={panelDatabase}
        schema={schema}
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
    return <PrivilegeView connectionId={panel.connectionId} databaseType={panel.databaseType} />;
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

interface CreateTablePanelContentProps {
  connectionId: string;
  databaseType: import('../../types').DatabaseType;
  database: string | null;
  schema: string | null;
  onSuccess: () => void;
  onCancel: () => void;
}

/** Ensures multi-db session is on the target database before rendering the editor. */
function CreateTablePanelContent({
  connectionId,
  databaseType,
  database,
  schema,
  onSuccess,
  onCancel,
}: CreateTablePanelContentProps) {
  const { t } = useI18n();
  const isMultiDb = useSchemaStore((s) => s.isMultiDatabase);
  const [dbReady, setDbReady] = useState(!isMultiDb || !database);
  const dbSwitchedRef = useRef<string | null>(null);

  useEffect(() => {
    if (!isMultiDb || !database) {
      setDbReady(true);
      return;
    }
    const key = `${connectionId}\0${database}`;
    if (dbSwitchedRef.current === key) {
      setDbReady(true);
      return;
    }
    let cancelled = false;
    setDbReady(false);
    void databaseCommands
      .useDatabase(connectionId, database)
      .then(() => {
        if (cancelled) return;
        dbSwitchedRef.current = key;
        setDbReady(true);
      })
      .catch(() => {
        if (cancelled) return;
        setDbReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, [connectionId, database, isMultiDb]);

  if (!dbReady) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
        {t('common.loading')}
      </div>
    );
  }

  return (
    <TableStructureEditor
      connectionId={connectionId}
      databaseType={databaseType}
      database={database}
      schema={schema}
      mode="create"
      onSuccess={onSuccess}
      onCancel={onCancel}
    />
  );
}

function SubTabBar({
  tabs,
  activeTab,
  onSelect,
  contextPath,
}: {
  tabs: { id: SubTabId; label: string }[];
  activeTab: SubTabId;
  onSelect: (id: SubTabId) => void;
  contextPath?: string;
}) {
  return (
    <div className="flex shrink-0 items-center border-b border-edge bg-surface-alt">
      <div className="flex min-w-0 flex-1">
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
      {contextPath ? (
        <span
          data-testid="panel-context-path"
          className="max-w-[45%] shrink-0 truncate px-4 text-xs text-fg-muted"
          title={contextPath}
        >
          {contextPath}
        </span>
      ) : null}
    </div>
  );
}
