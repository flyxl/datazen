import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { cn } from '../../../lib/cn';
import { useI18n } from '../../../hooks/useI18n';
import {
  ER_COLLAPSED_FOOTER_HEIGHT,
  ER_COLUMN_ROW_HEIGHT,
  ER_HEADER_HEIGHT,
  ER_NODE_WIDTH,
  erColumnRowCenterY,
} from './nodeMetrics';

interface TableNodeData {
  tableName: string;
  columns: { name: string; type: string; isPk: boolean; isFk: boolean }[];
  /**
   * Columns an edge touches. Only these get per-row connection points, so a wide
   * table does not carry four handles on every row.
   */
  handleColumns?: string[];
  highlighted?: boolean;
  dimmed?: boolean;
  collapsed?: boolean;
  [key: string]: unknown;
}

export const TableNode = memo(function TableNode({ data }: NodeProps) {
  const { t } = useI18n();
  const { tableName, columns, handleColumns, highlighted, dimmed, collapsed } =
    data as unknown as TableNodeData;
  const touched = new Set(handleColumns ?? []);

  return (
    <div
      data-testid="er-table-node"
      className={cn(
        // Width is fixed, not fluid: `layoutErGraph` packs nodes by the width it
        // is told, and a node free to render between 180px and 280px would be
        // packed against a number that is often wrong.
        'rounded-lg border shadow-md',
        highlighted ? 'border-accent bg-accent/5' : 'border-edge bg-surface',
        dimmed && 'opacity-30',
      )}
      style={{
        width: ER_NODE_WIDTH,
        WebkitFontSmoothing: 'antialiased',
        MozOsxFontSmoothing: 'grayscale',
      }}
    >
      <div
        className={cn(
          'flex items-center gap-1 rounded-t-lg px-3 text-xs font-semibold',
          highlighted ? 'bg-accent/15 text-accent' : 'bg-surface-alt text-fg',
        )}
        style={{ height: ER_HEADER_HEIGHT }}
      >
        <button
          type="button"
          className="shrink-0 opacity-60 hover:opacity-100"
          title={collapsed ? t('erDiagram.expand') : t('erDiagram.collapse')}
          onClick={(e) => {
            e.stopPropagation();
            window.dispatchEvent(new CustomEvent('er-toggle-collapse', { detail: tableName }));
          }}
        >
          {collapsed ? <ChevronRight className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />}
        </button>
        <span className="truncate">{tableName}</span>
        <span className="ml-auto text-[10px] font-normal text-fg-muted">{columns.length}</span>
      </div>
      {!collapsed && (
        <div>
          {columns.map((col, index) => (
            <div
              key={col.name}
              className={cn(
                'flex items-center gap-2 border-t border-edge/50 px-3 text-[11px]',
                col.isPk && 'bg-yellow-500/5',
                col.isFk && 'bg-accent/5',
              )}
              style={{ height: ER_COLUMN_ROW_HEIGHT }}
            >
              {col.isPk && (
                <span className="shrink-0 rounded bg-yellow-500/20 px-1 text-[9px] font-bold text-yellow-500">
                  PK
                </span>
              )}
              {col.isFk && (
                <span className="shrink-0 rounded bg-accent/20 px-1 text-[9px] font-bold text-accent">
                  FK
                </span>
              )}
              <span className="truncate text-fg">{col.name}</span>
              <span className="ml-auto shrink-0 text-fg-muted">{col.type}</span>
              {touched.has(col.name) && (
                <>
                  {/* Four variants, because an edge may leave or enter on either
                      side: a cycle makes a relationship point leftwards. */}
                  <Handle
                    type="source"
                    position={Position.Right}
                    id={`${col.name}:s-r`}
                    style={{ top: erColumnRowCenterY(index) }}
                    className="!h-1.5 !w-1.5 !bg-accent"
                  />
                  <Handle
                    type="source"
                    position={Position.Left}
                    id={`${col.name}:s-l`}
                    style={{ top: erColumnRowCenterY(index) }}
                    className="!h-1.5 !w-1.5 !bg-accent"
                  />
                  <Handle
                    type="target"
                    position={Position.Left}
                    id={`${col.name}:t-l`}
                    style={{ top: erColumnRowCenterY(index) }}
                    className="!h-1.5 !w-1.5 !bg-accent"
                  />
                  <Handle
                    type="target"
                    position={Position.Right}
                    id={`${col.name}:t-r`}
                    style={{ top: erColumnRowCenterY(index) }}
                    className="!h-1.5 !w-1.5 !bg-accent"
                  />
                </>
              )}
            </div>
          ))}
        </div>
      )}
      {collapsed && (
        <div
          className="flex items-center border-t border-edge/50 px-3 text-[10px] text-fg-muted"
          style={{ height: ER_COLLAPSED_FOOTER_HEIGHT }}
        >
          {columns.length} {t('erDiagram.columns')}
        </div>
      )}
      {/* Node-level fallbacks, used when a column cannot be pointed at: a
          collapsed table has no rows rendered, and a self-reference has no
          left or right. */}
      <Handle
        type="target"
        position={Position.Left}
        id="node:t-l"
        className="!bg-accent !w-2 !h-2"
      />
      <Handle
        type="source"
        position={Position.Right}
        id="node:s-r"
        className="!bg-accent !w-2 !h-2"
      />
      <Handle
        type="source"
        position={Position.Left}
        id="node:s-l"
        className="!bg-accent !w-2 !h-2"
      />
      <Handle
        type="target"
        position={Position.Right}
        id="node:t-r"
        className="!bg-accent !w-2 !h-2"
      />
    </div>
  );
});
