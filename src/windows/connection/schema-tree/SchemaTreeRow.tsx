import {
  Braces,
  ChevronDown,
  ChevronRight,
  Database,
  Eye,
  FolderClosed,
  FolderOpen,
  Loader2,
  Table2,
} from 'lucide-react';
import { ThemedIcon } from '../../../components/ThemedIcon';
import { cn } from '../../../lib/cn';
import type { DatabaseObject, TableInfo } from '../../../types';
import type { SchemaTreeCategoryDef } from './schemaTreeCategories';
import { OBJECT_KIND_CATEGORIES } from './schemaTreeCategories';
import { formatRowCount } from './formatRowCount';
import { setDragPayload, type DragPayloadOptions } from './schemaTreeDrag';
import { createDragGhost, removeDragGhost } from '../navigator/utils';
import type { SchemaTreeNodeContextMenuPayload } from './SchemaTree';

export interface FlatRow {
  type: 'db' | 'schema' | 'category' | 'table' | 'object' | 'cat-empty' | 'db-loading' | 'empty';
  dbName?: string;
  schemaName?: string;
  key?: string;
  cat?: SchemaTreeCategoryDef;
  count?: number;
  expanded?: boolean;
  depth: number;
  item?: TableInfo;
  obj?: DatabaseObject;
  colHits?: string[];
  loading?: boolean;
}

interface RowRendererProps {
  row: FlatRow;
  depthPadding: (depth: number) => string;
  selectedTable: string | null;
  isSingleDbMode: boolean;
  onSelectTable: (table: string, schema?: string) => void;
  onNodeContextMenu?: (payload: SchemaTreeNodeContextMenuPayload) => void;
  onToggleDb?: (dbName: string) => void;
  onToggleSchema?: (schemaKey: string) => void;
  onToggleCategory?: (catKey: string, catId: string) => void;
  dragOptions?: DragPayloadOptions;
}

export function SchemaTreeRow({
  row,
  depthPadding,
  selectedTable,
  isSingleDbMode,
  onSelectTable,
  onNodeContextMenu,
  onToggleDb,
  onToggleSchema,
  onToggleCategory,
  dragOptions,
}: RowRendererProps) {
  switch (row.type) {
    case 'db':
      return <DbRow row={row} onToggleDb={onToggleDb} onNodeContextMenu={onNodeContextMenu} />;
    case 'schema':
      return <SchemaRow row={row} depthPadding={depthPadding} onToggleSchema={onToggleSchema} />;
    case 'category':
      return (
        <CategoryRow row={row} depthPadding={depthPadding} onToggleCategory={onToggleCategory} />
      );
    case 'db-loading':
      return <DbLoadingRow />;
    case 'table':
      return (
        <TableRow
          row={row}
          depthPadding={depthPadding}
          selectedTable={selectedTable}
          isSingleDbMode={isSingleDbMode}
          onSelectTable={onSelectTable}
          onNodeContextMenu={onNodeContextMenu}
          dragOptions={dragOptions}
        />
      );
    case 'object':
      return (
        <ObjectRow row={row} depthPadding={depthPadding} onNodeContextMenu={onNodeContextMenu} />
      );
    case 'cat-empty':
      return <CatEmptyRow row={row} depthPadding={depthPadding} />;
    case 'empty':
      return <EmptyRow />;
    default:
      return null;
  }
}

function DbRow({
  row,
  onToggleDb,
  onNodeContextMenu,
}: Pick<RowRendererProps, 'row' | 'onToggleDb' | 'onNodeContextMenu'>) {
  return (
    <button
      type="button"
      data-testid="schema-tree-node"
      data-tree-node="db"
      data-db-name={row.dbName}
      className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm hover:bg-surface-raised text-fg-secondary"
      onClick={() => onToggleDb?.(row.dbName!)}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onNodeContextMenu?.({
          kind: 'database',
          name: row.dbName!,
          x: e.clientX,
          y: e.clientY,
        });
      }}
    >
      {row.expanded ? (
        <ChevronDown className="h-3.5 w-3.5 shrink-0" />
      ) : (
        <ChevronRight className="h-3.5 w-3.5 shrink-0" />
      )}
      <ThemedIcon
        id="schema.database"
        className="h-3.5 w-3.5 shrink-0 text-teal-400"
        fallback={Database}
      />
      <span className="selectable min-w-0 truncate">{row.dbName}</span>
      {row.loading && <Loader2 className="h-3 w-3 shrink-0 animate-spin text-fg-muted" />}
    </button>
  );
}

function SchemaRow({
  row,
  depthPadding,
  onToggleSchema,
}: Pick<RowRendererProps, 'row' | 'depthPadding' | 'onToggleSchema'>) {
  return (
    <button
      type="button"
      data-testid="schema-tree-node"
      data-tree-node="schema"
      data-schema-name={row.schemaName}
      className="flex w-full items-center gap-2 py-1.5 pr-2 text-left text-[13px] hover:bg-surface-raised text-fg-secondary"
      style={{ paddingLeft: depthPadding(row.depth) }}
      onClick={() => onToggleSchema?.(`${row.dbName}::${row.schemaName}`)}
    >
      {row.expanded ? (
        <ChevronDown className="h-3 w-3 shrink-0" />
      ) : (
        <ChevronRight className="h-3 w-3 shrink-0" />
      )}
      <ThemedIcon
        id="schema.schema"
        className="h-3.5 w-3.5 shrink-0 text-teal-400"
        fallback={row.expanded ? FolderOpen : FolderClosed}
      />
      <span className="min-w-0 truncate">{row.schemaName || 'Default'}</span>
    </button>
  );
}

function CategoryRow({
  row,
  depthPadding,
  onToggleCategory,
}: Pick<RowRendererProps, 'row' | 'depthPadding' | 'onToggleCategory'>) {
  return (
    <button
      type="button"
      data-testid="schema-tree-node"
      data-tree-node="category"
      data-cat-id={row.cat?.id}
      className="flex w-full items-center gap-2 py-1.5 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
      style={{ paddingLeft: depthPadding(row.depth) }}
      onClick={() => onToggleCategory?.(row.key!, row.cat!.id)}
    >
      {row.expanded ? (
        <ChevronDown className="h-3 w-3 shrink-0" />
      ) : (
        <ChevronRight className="h-3 w-3 shrink-0" />
      )}
      <ThemedIcon
        id={row.cat!.iconId}
        className={`h-3.5 w-3.5 shrink-0 ${row.cat!.color}`}
        fallback={row.cat!.icon}
      />
      <span className="min-w-0 truncate">{row.cat!.labelKey}</span>
      <span className="ml-auto shrink-0 text-[10px] text-fg-muted">{row.count}</span>
    </button>
  );
}

function DbLoadingRow() {
  return (
    <div className="flex items-center gap-2 px-3 py-1 pl-8 text-xs text-fg-muted">
      <Loader2 className="h-3 w-3 animate-spin" />
      Loading...
    </div>
  );
}

function TableRow({
  row,
  depthPadding,
  selectedTable,
  isSingleDbMode,
  onSelectTable,
  onNodeContextMenu,
  dragOptions,
}: Pick<
  RowRendererProps,
  | 'row'
  | 'depthPadding'
  | 'selectedTable'
  | 'isSingleDbMode'
  | 'onSelectTable'
  | 'onNodeContextMenu'
  | 'dragOptions'
>) {
  const item = row.item!;
  const isView = item.tableType === 'view' || item.tableType === 'materializedView';
  const colHits = row.colHits ?? [];

  return (
    <button
      type="button"
      data-testid="schema-tree-node"
      data-tree-node={isView ? 'view' : 'table'}
      data-item-name={item.name}
      draggable
      onDragStart={(e) => {
        const sel = window.getSelection();
        if (sel && !sel.isCollapsed) sel.removeAllRanges();
        if (dragOptions) {
          setDragPayload(e.dataTransfer, dragOptions);
        }
        if (e.dataTransfer.setDragImage) {
          const ghost = createDragGhost(item.name);
          e.dataTransfer.setDragImage(ghost, 16, 14);
        }
      }}
      onDragEnd={removeDragGhost}
      className={cn(
        'flex w-full cursor-pointer select-none items-center gap-2 py-1.5 pr-3 text-left text-[13px] hover:bg-surface-raised',
        selectedTable === item.name ? 'bg-surface-raised text-fg' : 'text-fg-secondary',
      )}
      style={{ paddingLeft: depthPadding(row.depth) }}
      onClick={() => onSelectTable(item.name, item.schema)}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onNodeContextMenu?.({
          kind: isView ? 'view' : 'table',
          name: item.name,
          x: e.clientX,
          y: e.clientY,
          schema: item.schema ?? undefined,
        });
      }}
      title={colHits.length > 0 ? colHits.slice(0, 8).join(', ') : undefined}
    >
      <ThemedIcon
        id={isView ? 'schema.view' : 'schema.table'}
        className={`h-3.5 w-3.5 shrink-0 ${isView ? 'text-purple-400' : 'text-blue-400'}`}
        fallback={isView ? Eye : Table2}
      />
      <span className="min-w-0 truncate">{item.name}</span>
      {colHits.length > 0 && (
        <span className="shrink-0 text-[10px] text-accent">
          {colHits.length === 1 ? colHits[0] : `${colHits.length} cols`}
        </span>
      )}
      {isSingleDbMode && item.rowCount != null && (
        <span className="ml-auto shrink-0 text-[11px] text-fg-muted">
          {formatRowCount(item.rowCount)}
        </span>
      )}
    </button>
  );
}

function ObjectRow({
  row,
  depthPadding,
  onNodeContextMenu,
}: Pick<RowRendererProps, 'row' | 'depthPadding' | 'onNodeContextMenu'>) {
  const obj = row.obj!;
  const catDef = OBJECT_KIND_CATEGORIES[obj.kind];
  const ObjIcon = catDef?.icon ?? Braces;
  const iconId = catDef?.iconId ?? 'schema.function';
  const objColor = catDef?.color ?? 'text-orange-400';

  return (
    <button
      type="button"
      data-testid="schema-tree-node"
      data-tree-node={obj.kind}
      data-item-name={obj.name}
      className="flex w-full items-center gap-2 py-1.5 pr-3 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
      style={{ paddingLeft: depthPadding(row.depth) }}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onNodeContextMenu?.({
          kind: obj.kind,
          name: obj.name,
          x: e.clientX,
          y: e.clientY,
          schema: obj.schema ?? undefined,
        });
      }}
    >
      <ThemedIcon id={iconId} className={`h-3.5 w-3.5 shrink-0 ${objColor}`} fallback={ObjIcon} />
      <span className="selectable min-w-0 truncate">{obj.name}</span>
    </button>
  );
}

function CatEmptyRow({ row, depthPadding }: Pick<RowRendererProps, 'row' | 'depthPadding'>) {
  return (
    <div
      className="py-1 text-[11px] text-fg-muted"
      style={{ paddingLeft: depthPadding(row.depth) }}
    >
      No tables
    </div>
  );
}

function EmptyRow() {
  return <div className="px-3 py-3 text-center text-xs text-fg-muted">No tables</div>;
}
