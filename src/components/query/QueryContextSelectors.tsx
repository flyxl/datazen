import { Database } from 'lucide-react';
import { Select } from '../ui/Select';
import { namespaceBranchChildNames, type SqlNamespace } from '../../lib/sqlNamespace';
import { namespaceRootsFrom } from '../../lib/queryContextPath';
import { useI18n } from '../../hooks/useI18n';

export interface QueryContextSelectorsProps {
  isMultiDb: boolean;
  isPathHierarchy: boolean;
  databases: readonly string[];
  currentDatabase: string | null;
  namespaceTree: SqlNamespace;
  pathAliases: Record<string, string>;
  contextPath: readonly string[];
  contextSchema?: string | null;
  onSelectLevel: (index: number, value: string) => void;
}

function levelLabelKey(
  index: number,
  pathHierarchy: boolean,
): 'query.database' | 'query.catalog' | 'query.schema' {
  if (pathHierarchy) {
    if (index === 0) return 'query.catalog';
    return 'query.schema';
  }
  if (index === 0) return 'query.database';
  if (index === 1) return 'query.catalog';
  return 'query.schema';
}

export function QueryContextSelectors({
  isMultiDb,
  isPathHierarchy,
  databases,
  currentDatabase,
  namespaceTree,
  pathAliases,
  contextPath,
  contextSchema,
  onSelectLevel,
}: QueryContextSelectorsProps) {
  const { t } = useI18n();

  if (isPathHierarchy) {
    const roots = namespaceRootsFrom(namespaceTree, pathAliases, databases);
    if (roots.length === 0) return null;

    const levels: Array<{ options: string[]; value: string }> = [];
    levels.push({ options: roots, value: contextPath[0] ?? '' });
    for (let i = 0; i < contextPath.length; i++) {
      const children = namespaceBranchChildNames(namespaceTree, [...contextPath.slice(0, i + 1)]);
      if (children.length === 0) break;
      levels.push({ options: children, value: contextPath[i + 1] ?? '' });
    }

    return (
      <div className="flex shrink-0 items-center gap-1.5" data-testid="query-context-selectors">
        <Database className="h-3.5 w-3.5 text-fg-muted" />
        {levels.map((level, index) => (
          <Select
            key={`${index}-${level.value}`}
            value={level.value}
            options={level.options.map((name) => ({ value: name, label: name }))}
            onChange={(value) => onSelectLevel(index, value)}
            placeholder={t(levelLabelKey(index, true))}
            className="!h-6 !text-[11px] max-w-[180px]"
            title={t(levelLabelKey(index, true))}
          />
        ))}
      </div>
    );
  }

  if ((!isMultiDb || databases.length === 0) && !contextSchema) return null;

  return (
    <div className="flex shrink-0 items-center gap-1.5" data-testid="query-context-selectors">
      <Database className="h-3.5 w-3.5 text-fg-muted" />
      {isMultiDb && databases.length > 0 && (
        <Select
          value={currentDatabase ?? ''}
          options={databases.map((db) => ({ value: db, label: db }))}
          onChange={(db) => onSelectLevel(0, db)}
          className="!h-6 !text-[11px] max-w-[180px]"
          title={t('query.database')}
        />
      )}
      {contextSchema && (
        <span
          className="max-w-[140px] truncate text-[11px] text-fg-muted"
          title={contextSchema}
          data-testid="query-context-schema"
        >
          / {contextSchema}
        </span>
      )}
    </div>
  );
}
