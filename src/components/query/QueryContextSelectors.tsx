import { Fragment, useEffect, useRef } from 'react';
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

const QUERY_CONTEXT_COMPACT_SELECT_CLASS =
  '!h-6 !text-[11px] shrink-0 !w-auto !min-w-[4rem] !max-w-[7rem]';
const QUERY_CONTEXT_SELECTORS_MIN_WIDTH = 'min-w-0';
const QUERY_CONTEXT_OPTIONS_MIN_WIDTH = 176;

function QueryContextSelectorsShell({ children }: { children: React.ReactNode }) {
  return (
    <div
      className={`flex shrink-0 items-center gap-0.5 ${QUERY_CONTEXT_SELECTORS_MIN_WIDTH}`}
      data-testid="query-context-selectors"
    >
      <Database className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
      {children}
    </div>
  );
}

function QueryContextCompactSelect({
  value,
  options,
  placeholderKey,
  disabled,
  levelIndex,
  onChange,
}: {
  value: string;
  options: readonly string[];
  placeholderKey: 'query.database' | 'query.catalog' | 'query.schema';
  disabled?: boolean;
  levelIndex?: number;
  onChange: (value: string) => void;
}) {
  const { t } = useI18n();
  const placeholder = t(placeholderKey);

  return (
    <Select
      value={value}
      options={options.map((name) => ({ value: name, label: name }))}
      onChange={onChange}
      placeholder={placeholder}
      className={QUERY_CONTEXT_COMPACT_SELECT_CLASS}
      title={placeholder}
      searchable
      fitContent
      blurOnSelect
      listMinWidth={QUERY_CONTEXT_OPTIONS_MIN_WIDTH}
      disabled={disabled}
      triggerDataAttrs={
        levelIndex === undefined ? undefined : { 'data-query-context-level': String(levelIndex) }
      }
    />
  );
}

function PathHierarchyQueryContextSelectors({
  namespaceTree,
  pathAliases,
  databases,
  contextPath,
  onSelectLevel,
}: {
  namespaceTree: import('../../lib/sqlNamespace').SqlNamespace;
  pathAliases: Record<string, string>;
  databases: readonly string[];
  contextPath: readonly string[];
  onSelectLevel: (index: number, value: string) => void;
}) {
  const pendingFocusLevelRef = useRef<number | null>(null);
  const segments = pathHierarchySelectorSegmentsForUi(
    namespaceTree,
    pathAliases,
    databases,
    contextPath,
  );

  useEffect(() => {
    const level = pendingFocusLevelRef.current;
    if (level === null) return;
    pendingFocusLevelRef.current = null;
    requestAnimationFrame(() => {
      const input = document.querySelector(
        `[data-query-context-level="${level}"] input`,
      ) as HTMLInputElement | null;
      input?.focus();
    });
  }, [contextPath, segments]);

  const handlePathLevelSelect = (index: number, value: string) => {
    pendingFocusLevelRef.current = index + 1;
    onSelectLevel(index, value);
  };

  return (
    <QueryContextSelectorsShell>
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
            <QueryContextCompactSelect
              value={segment.value}
              options={segment.options}
              placeholderKey={levelLabelKey(segment.levelIndex, true)}
              disabled={segment.options.length === 0}
              levelIndex={segment.levelIndex}
              onChange={(value) => handlePathLevelSelect(segment.levelIndex, value)}
            />
          )}
        </Fragment>
      ))}
    </QueryContextSelectorsShell>
  );
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
  if (isPathHierarchy) {
    return (
      <PathHierarchyQueryContextSelectors
        namespaceTree={namespaceTree}
        pathAliases={pathAliases}
        databases={databases}
        contextPath={contextPath}
        onSelectLevel={onSelectLevel}
      />
    );
  }

  if ((!isMultiDb || databases.length === 0) && !contextSchema) return null;

  return (
    <QueryContextSelectorsShell>
      {isMultiDb && databases.length > 0 && (
        <QueryContextCompactSelect
          value={currentDatabase ?? ''}
          options={databases}
          placeholderKey="query.database"
          onChange={(db) => onSelectLevel(0, db)}
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
    </QueryContextSelectorsShell>
  );
}
