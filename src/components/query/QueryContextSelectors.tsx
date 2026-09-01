import { Fragment } from 'react';
import { Database } from 'lucide-react';
import { Select } from '../ui/Select';
import { pathHierarchySelectorSegmentsForUi } from '../../lib/queryContextPath';
import { useI18n } from '../../hooks/useI18n';

export interface QueryContextSelectorsProps {
  isMultiDb: boolean;
  isPathHierarchy: boolean;
  databases: readonly string[];
  currentDatabase: string | null;
  namespaceTree: import('../../lib/sqlNamespace').SqlNamespace;
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

const PATH_HIERARCHY_SELECT_CLASS =
  '!h-6 !text-[11px] shrink-0 !w-auto !min-w-[4rem] !max-w-[5.5rem]';
const PATH_HIERARCHY_SELECTORS_MIN_WIDTH = 'min-w-[9rem]';

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
    const segments = pathHierarchySelectorSegmentsForUi(
      namespaceTree,
      pathAliases,
      databases,
      contextPath,
    );

    return (
      <div
        className={`flex shrink-0 items-center gap-0.5 ${PATH_HIERARCHY_SELECTORS_MIN_WIDTH}`}
        data-testid="query-context-selectors"
      >
        <Database className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
        {segments.map((segment, index) => (
          <Fragment
            key={
              segment.kind === 'label'
                ? `label-${index}-${segment.name}`
                : `select-${segment.levelIndex}`
            }
          >
            {index > 0 && (
              <span className="px-0.5 text-[10px] text-fg-muted/70" aria-hidden="true">
                /
              </span>
            )}
            {segment.kind === 'label' ? (
              <span
                className="max-w-[4.5rem] truncate text-[11px] text-fg-muted"
                title={segment.name}
                data-testid="query-context-path-label"
              >
                {segment.name}
              </span>
            ) : (
              <Select
                value={segment.value}
                options={segment.options.map((name) => ({ value: name, label: name }))}
                onChange={(value) => onSelectLevel(segment.levelIndex, value)}
                placeholder={t(levelLabelKey(segment.levelIndex, true))}
                className={PATH_HIERARCHY_SELECT_CLASS}
                title={t(levelLabelKey(segment.levelIndex, true))}
                searchable
                fitContent
                disabled={segment.options.length === 0}
              />
            )}
          </Fragment>
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
          searchable
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
