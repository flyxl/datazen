import { useMemo, useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';

interface PlanNode {
  id: string;
  label: string;
  cost?: number;
  rows?: number;
  details: Array<{ key: string; value: string }>;
  children: PlanNode[];
}

interface ExplainPlanTreeProps {
  planJson: unknown;
}

const MYSQL_OPERATION_KEYS = [
  'nested_loop',
  'grouping_operation',
  'ordering_operation',
  'duplicating_weedout',
  'materialized_from_subquery',
  'buffer_result',
  'hash_join',
] as const;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function formatValue(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return JSON.stringify(value);
}

function parseNumeric(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string') {
    const parsed = Number.parseFloat(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return undefined;
}

function parsePostgresNode(node: Record<string, unknown>, id: string): PlanNode {
  const nodeType = String(node['Node Type'] ?? 'Plan');
  const relation = node['Relation Name'];
  const index = node['Index Name'];
  const alias = node['Alias'];
  const suffix = [relation, index, alias].find((v) => typeof v === 'string') as string | undefined;
  const label = suffix ? `${nodeType} · ${suffix}` : nodeType;

  const details = Object.entries(node)
    .filter(([key]) => key !== 'Plans' && key !== 'Node Type')
    .map(([key, value]) => ({ key, value: formatValue(value) }));

  const children = Array.isArray(node.Plans)
    ? node.Plans.filter(isRecord).map((child, index) => parsePostgresNode(child, `${id}.${index}`))
    : [];

  return {
    id,
    label,
    cost: parseNumeric(node['Total Cost']),
    rows: parseNumeric(node['Plan Rows']),
    details,
    children,
  };
}

function parsePostgresPlan(planJson: unknown): PlanNode | null {
  if (!Array.isArray(planJson) || planJson.length === 0) return null;
  const root = planJson[0];
  if (!isRecord(root) || !isRecord(root.Plan)) return null;
  return parsePostgresNode(root.Plan, 'pg');
}

function mysqlTableLabel(table: Record<string, unknown>): string {
  const tableName = typeof table.table_name === 'string' ? table.table_name : 'table';
  const accessType = typeof table.access_type === 'string' ? table.access_type : 'access';
  return `${accessType} → ${tableName}`;
}

function mysqlTableDetails(table: Record<string, unknown>): Array<{ key: string; value: string }> {
  return Object.entries(table)
    .filter(([key]) => key !== 'table_name' && key !== 'access_type')
    .map(([key, value]) => ({ key, value: formatValue(value) }));
}

function parseMysqlBlock(block: Record<string, unknown>, id: string): PlanNode[] {
  const nodes: PlanNode[] = [];

  if (isRecord(block.table)) {
    nodes.push({
      id: `${id}.table`,
      label: mysqlTableLabel(block.table),
      cost: parseNumeric(
        block.cost_info && isRecord(block.cost_info) ? block.cost_info.query_cost : undefined,
      ),
      rows: parseNumeric(block.table.rows_examined_per_scan),
      details: mysqlTableDetails(block.table),
      children: [],
    });
  }

  for (const key of MYSQL_OPERATION_KEYS) {
    const value = block[key];
    if (key === 'nested_loop' && Array.isArray(value)) {
      value.forEach((item, index) => {
        if (!isRecord(item)) return;
        const children = parseMysqlBlock(item, `${id}.nl.${index}`);
        if (children.length === 1) {
          nodes.push(children[0]);
        } else if (children.length > 1) {
          nodes.push({
            id: `${id}.nl.${index}`,
            label: 'nested loop',
            details: [],
            children,
          });
        }
      });
      continue;
    }

    if (isRecord(value)) {
      const children = parseMysqlBlock(value, `${id}.${key}`);
      nodes.push({
        id: `${id}.${key}`,
        label: key.replace(/_/g, ' '),
        cost: parseNumeric(isRecord(value.cost_info) ? value.cost_info.query_cost : undefined),
        details: [],
        children,
      });
    }
  }

  return nodes;
}

function parseMysqlPlan(planJson: unknown): PlanNode | null {
  if (!isRecord(planJson) || !isRecord(planJson.query_block)) return null;
  const children = parseMysqlBlock(planJson.query_block, 'mysql');
  if (children.length === 0) return null;
  if (children.length === 1) return children[0];

  return {
    id: 'mysql.root',
    label: 'query block',
    cost: parseNumeric(
      isRecord(planJson.query_block.cost_info)
        ? planJson.query_block.cost_info.query_cost
        : undefined,
    ),
    details: [],
    children,
  };
}

function parsePlanJson(planJson: unknown): PlanNode | null {
  return parsePostgresPlan(planJson) ?? parseMysqlPlan(planJson);
}

function PlanTreeNode({ node, depth }: { node: PlanNode; depth: number }) {
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

export function ExplainPlanTree({ planJson }: ExplainPlanTreeProps) {
  const { t } = useI18n();
  const root = useMemo(() => parsePlanJson(planJson), [planJson]);

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
