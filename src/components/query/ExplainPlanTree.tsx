import { useMemo, useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';
import type { ExplainPlanNode } from '../../types';

interface ExplainPlanTreeProps {
  planTree?: ExplainPlanNode | null;
}

function PlanTreeNode({ node, depth }: { node: ExplainPlanNode; depth: number }) {
  const [expanded, setExpanded] = useState(depth < 2);
  const hasChildren = node.children.length > 0;

  return (
    <div className="selectable">
      <div
        className={cn(
          'flex items-start gap-1 rounded px-1 py-0.5 hover:bg-surface-raised/60',
          hasChildren && 'cursor-pointer',
        )}
        style={{ paddingLeft: `${depth * 12}px` }}
        onClick={() => {
          if (hasChildren) setExpanded((v) => !v);
        }}
      >
        <span className="mt-0.5 shrink-0 text-fg-muted">
          {hasChildren ? (
            expanded ? (
              <ChevronDown className="h-3 w-3" />
            ) : (
              <ChevronRight className="h-3 w-3" />
            )
          ) : (
            <span className="inline-block h-3 w-3" />
          )}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="text-xs font-medium text-fg">{node.label}</span>
            {node.cost !== undefined && (
              <span className="rounded bg-blue-500/10 px-1.5 py-0.5 text-[10px] text-blue-400">
                cost {node.cost.toFixed(2)}
              </span>
            )}
            {node.rows !== undefined && (
              <span className="rounded bg-purple-500/10 px-1.5 py-0.5 text-[10px] text-purple-400">
                rows {node.rows}
              </span>
            )}
          </div>
          {node.details.length > 0 && (
            <div className="mt-0.5 space-y-0.5">
              {node.details.slice(0, 6).map((detail) => (
                <div key={`${node.id}-${detail.key}`} className="text-[10px] text-fg-muted">
                  <span className="text-fg-secondary">{detail.key}: </span>
                  {detail.value}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
      {expanded &&
        node.children.map((child) => (
          <PlanTreeNode key={child.id} node={child} depth={depth + 1} />
        ))}
    </div>
  );
}

export function ExplainPlanTree({ planTree }: ExplainPlanTreeProps) {
  const { t } = useI18n();
  const root = useMemo(() => planTree ?? null, [planTree]);

  if (!root) return null;

  return (
    <div>
      <div className="mb-1.5 text-[11px] font-medium uppercase tracking-wider text-fg-muted">
        {t('explain.planTree')}
      </div>
      <div className="max-h-[280px] overflow-auto rounded border border-edge bg-surface-alt p-2">
        <PlanTreeNode node={root} depth={0} />
      </div>
    </div>
  );
}
