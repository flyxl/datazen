import type { SchemaDiffObjectIdentity, SchemaDiffObjectKind } from '../../commands/schemaDiff';
import { useI18n } from '../../hooks/useI18n';
import { Checkbox } from '../../components/ui/Checkbox';
import { schemaDiffObjectIdentityKey } from './schemaDiffObjectIdentity';

export interface SchemaDiffObjectLoadErrors {
  source: Partial<Record<SchemaDiffObjectKind, string>>;
  target: Partial<Record<SchemaDiffObjectKind, string>>;
}

interface SchemaDiffUnifiedObjectsPickerProps {
  loading: boolean;
  sourceObjects: SchemaDiffObjectIdentity[];
  targetObjects: SchemaDiffObjectIdentity[];
  selectedSourceKeys: string[];
  selectedTargetKeys: string[];
  errors: SchemaDiffObjectLoadErrors;
  crossDialect: boolean;
  onToggleSource: (object: SchemaDiffObjectIdentity) => void;
  onToggleTarget: (object: SchemaDiffObjectIdentity) => void;
  onSelectAll: (side: 'source' | 'target', selected: boolean) => void;
  onClearSelections: () => void;
  onRetry: () => void;
}

const OBJECT_KINDS: SchemaDiffObjectKind[] = [
  'view',
  'type',
  'sequence',
  'function',
  'procedure',
  'trigger',
];

function displayObjectIdentity(object: SchemaDiffObjectIdentity): string {
  const name = object.schema ? `${object.schema}.${object.name}` : object.name;
  const signature = object.signature == null ? '' : `(${object.signature})`;
  const target = object.targetName
    ? ` → ${object.targetSchema ? `${object.targetSchema}.` : ''}${object.targetName}`
    : '';
  return `${name}${signature}${target}`;
}

function ObjectSideList({
  side,
  objects,
  selectedKeys,
  errorByKind,
  crossDialect,
  onToggle,
}: {
  side: 'source' | 'target';
  objects: SchemaDiffObjectIdentity[];
  selectedKeys: Set<string>;
  errorByKind: SchemaDiffObjectLoadErrors['source'];
  crossDialect: boolean;
  onToggle: (object: SchemaDiffObjectIdentity) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="min-w-0">
      <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-fg-muted">
        {t(side === 'source' ? 'schemaDiff.objectSource' : 'schemaDiff.objectTarget')}
      </div>
      <ul className="divide-y divide-edge overflow-hidden rounded border border-edge bg-surface">
        {OBJECT_KINDS.map((kind) => {
          const rows = objects.filter((object) => object.kind === kind);
          const error = errorByKind[kind];
          if (rows.length === 0 && !error) return null;
          return (
            <li key={kind} data-testid={`schema-diff-object-kind-${side}-${kind}`}>
              <details open={kind === 'view' || undefined}>
                <summary className="cursor-pointer px-2 py-1.5 text-xs text-fg-muted hover:bg-surface-alt">
                  <span className="font-medium text-fg">{t(`schemaDiff.objectKind.${kind}`)}</span>
                  <span className="ml-2">{rows.length}</span>
                </summary>
                {error ? (
                  <div
                    className="px-3 pb-2 text-xs text-danger"
                    data-testid={`schema-diff-object-error-${side}-${kind}`}
                  >
                    {t('schemaDiff.objectLoadFailed', { error })}
                  </div>
                ) : rows.length === 0 ? (
                  <div className="px-3 pb-2 text-xs text-fg-muted">
                    {t('schemaDiff.objectKindEmpty')}
                  </div>
                ) : (
                  <ul className="border-t border-edge">
                    {rows.map((object, index) => {
                      const key = schemaDiffObjectIdentityKey(object);
                      const label = displayObjectIdentity(object);
                      return (
                        <li
                          key={key}
                          data-testid={`schema-diff-object-row-${side}-${kind}-${index}`}
                          data-object-kind={kind}
                          data-object-identity={key}
                          className="flex items-center gap-2 border-b border-edge/60 px-2 py-1.5 text-xs last:border-b-0"
                        >
                          <Checkbox
                            aria-label={`${t(side === 'source' ? 'schemaDiff.objectSource' : 'schemaDiff.objectTarget')}: ${label}`}
                            checked={selectedKeys.has(key)}
                            disabled={crossDialect && !selectedKeys.has(key)}
                            onChange={() => onToggle(object)}
                          />
                          <span className="min-w-0 flex-1 break-all font-mono">{label}</span>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </details>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export function SchemaDiffUnifiedObjectsPicker({
  loading,
  sourceObjects,
  targetObjects,
  selectedSourceKeys,
  selectedTargetKeys,
  errors,
  crossDialect,
  onToggleSource,
  onToggleTarget,
  onSelectAll,
  onClearSelections,
  onRetry,
}: SchemaDiffUnifiedObjectsPickerProps) {
  const { t } = useI18n();
  const sourceErrors = Object.keys(errors.source).length > 0;
  const targetErrors = Object.keys(errors.target).length > 0;
  const hasSelectedObjects = selectedSourceKeys.length > 0 || selectedTargetKeys.length > 0;

  return (
    <section
      className="rounded-lg border border-edge bg-surface-alt p-3"
      data-testid="schema-diff-unified-object-picker"
    >
      <div className="mb-3 flex flex-wrap items-start justify-between gap-2">
        <div>
          <h3 className="text-sm font-medium text-fg">{t('schemaDiff.schemaObjects')}</h3>
          <p className="mt-1 text-xs text-fg-muted">{t('schemaDiff.schemaObjectsHint')}</p>
        </div>
        {(sourceErrors || targetErrors) && (
          <button
            type="button"
            className="text-xs text-accent hover:underline"
            disabled={loading}
            onClick={onRetry}
            data-testid="schema-diff-object-retry"
          >
            {t('schemaDiff.objectRetry')}
          </button>
        )}
      </div>
      {crossDialect && (
        <div
          className="mb-3 rounded border border-warning/40 bg-warning/5 p-2 text-xs text-fg-muted"
          role="status"
          data-testid="schema-diff-cross-dialect-objects-note"
        >
          <p>{t('schemaDiff.crossDialectNote')}</p>
          <p className="mt-1">{t('schemaDiff.crossDialectObjectNote')}</p>
          {hasSelectedObjects && (
            <button
              type="button"
              className="mt-2 text-accent hover:underline"
              data-testid="schema-diff-clear-object-selections"
              onClick={onClearSelections}
            >
              {t('schemaDiff.clearObjectSelections')}
            </button>
          )}
        </div>
      )}
      {(sourceErrors || targetErrors) && (
        <p className="mb-3 text-xs text-warning" role="status">
          {t('schemaDiff.objectPartialLoad')}
        </p>
      )}
      {loading ? (
        <div className="py-5 text-center text-xs text-fg-muted" aria-live="polite">
          {t('schemaDiff.schemaObjectsLoading')}
        </div>
      ) : (
        <div className="grid min-w-0 grid-cols-1 gap-3 lg:grid-cols-2">
          <div className="min-w-0 space-y-1">
            <div className="flex items-center justify-between text-xs">
              <span className="text-fg-muted">
                {t('schemaDiff.objectSideSelectionCount', {
                  count: selectedSourceKeys.length,
                  total: sourceObjects.length,
                })}
              </span>
              <div className="flex gap-2">
                <button
                  type="button"
                  className="text-accent hover:underline"
                  disabled={crossDialect}
                  onClick={() => onSelectAll('source', true)}
                >
                  {t('schemaDiff.objectSelectAllSource')}
                </button>
                <button
                  type="button"
                  className="text-accent hover:underline"
                  onClick={() => onSelectAll('source', false)}
                >
                  {t('schemaDiff.objectSelectNoneSource')}
                </button>
              </div>
            </div>
            <ObjectSideList
              side="source"
              objects={sourceObjects}
              selectedKeys={new Set(selectedSourceKeys)}
              errorByKind={errors.source}
              crossDialect={crossDialect}
              onToggle={onToggleSource}
            />
          </div>
          <div className="min-w-0 space-y-1">
            <div className="flex items-center justify-between text-xs">
              <span className="text-fg-muted">
                {t('schemaDiff.objectSideSelectionCount', {
                  count: selectedTargetKeys.length,
                  total: targetObjects.length,
                })}
              </span>
              <div className="flex gap-2">
                <button
                  type="button"
                  className="text-accent hover:underline"
                  disabled={crossDialect}
                  onClick={() => onSelectAll('target', true)}
                >
                  {t('schemaDiff.objectSelectAllTarget')}
                </button>
                <button
                  type="button"
                  className="text-accent hover:underline"
                  onClick={() => onSelectAll('target', false)}
                >
                  {t('schemaDiff.objectSelectNoneTarget')}
                </button>
              </div>
            </div>
            <ObjectSideList
              side="target"
              objects={targetObjects}
              selectedKeys={new Set(selectedTargetKeys)}
              errorByKind={errors.target}
              crossDialect={crossDialect}
              onToggle={onToggleTarget}
            />
          </div>
        </div>
      )}
    </section>
  );
}
