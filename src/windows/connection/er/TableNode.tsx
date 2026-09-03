import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { cn } from '../../../lib/cn';
import { useI18n } from '../../../hooks/useI18n';

interface TableNodeData {
  tableName: string;
  columns: { name: string; type: string; isPk: boolean; isFk: boolean }[];
  highlighted?: boolean;
  dimmed?: boolean;
  collapsed?: boolean;
  [key: string]: unknown;
}

export const TableNode = memo(function TableNode({ data }: NodeProps) {
  const { t } = useI18n();
  const { tableName, columns, highlighted, dimmed, collapsed } = data as unknown as TableNodeData;

  return (
    <div
      className={cn(
        'min-w-[180px] max-w-[280px] rounded-lg border shadow-md',
        highlighted ? 'border-accent bg-accent/5' : 'border-edge bg-surface',
        dimmed && 'opacity-30',
      )}
      style={{ WebkitFontSmoothing: 'antialiased', MozOsxFontSmoothing: 'grayscale' }}
    >
      <div
        className={cn(
          'flex items-center gap-1 rounded-t-lg px-3 py-2 text-xs font-semibold',
          highlighted ? 'bg-accent/15 text-accent' : 'bg-surface-alt text-fg',
        )}
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
        <div className="max-h-[300px] overflow-y-auto">
          {columns.map((col) => (
            <div
              key={col.name}
              className={cn(
                'flex items-center gap-2 border-t border-edge/50 px-3 py-1 text-[11px]',
                col.isPk && 'bg-yellow-500/5',
                col.isFk && 'bg-blue-500/5',
              )}
            >
              {col.isPk && (
                <span className="shrink-0 rounded bg-yellow-500/20 px-1 text-[9px] font-bold text-yellow-500">
                  PK
                </span>
              )}
              {col.isFk && (
                <span className="shrink-0 rounded bg-blue-500/20 px-1 text-[9px] font-bold text-blue-400">
                  FK
                </span>
              )}
              <span className="truncate text-fg">{col.name}</span>
              <span className="ml-auto shrink-0 text-fg-muted">{col.type}</span>
            </div>
          ))}
        </div>
      )}
      {collapsed && (
        <div className="border-t border-edge/50 px-3 py-1 text-[10px] text-fg-muted">
          {columns.length} {t('erDiagram.columns')}
        </div>
      )}
      <Handle type="target" position={Position.Left} className="!bg-accent !w-2 !h-2" />
      <Handle type="source" position={Position.Right} className="!bg-accent !w-2 !h-2" />
    </div>
  );
});
