import type {
  ColumnTypeOverride,
  PlanRequirement,
  PlanStatement,
  SchemaDiffPlan,
  StatementRisk,
  TypeSuggestion,
} from '../../commands/schemaDiff';
import { rollbackCompletenessCounts } from '../../commands/schemaDiff';
import { useI18n } from '../../hooks/useI18n';

function riskClass(risk: StatementRisk): string {
  switch (risk) {
    case 'additive':
      return 'text-success';
    case 'destructive':
      return 'text-danger';
    case 'rewrite':
      return 'text-warning';
  }
}

function requirementTarget(req: PlanRequirement): string {
  return req.column ? `${req.table}.${req.column}` : req.table;
}

function PlanRequirements({ requirements }: { requirements: PlanRequirement[] }) {
  const { t } = useI18n();

  if (requirements.length === 0) {
    return null;
  }

  return (
    <div className="space-y-2" data-testid="schema-diff-plan-requirements">
      {requirements.map((req) => {
        const target = requirementTarget(req);
        if (req.kind === 'Backfill') {
          return (
            <div
              key={`backfill-${target}-${req.reason}`}
              className="rounded border border-yellow-500/30 bg-yellow-500/10 p-2 text-sm text-yellow-400"
            >
              <div className="font-medium">⚠ {t('schemaDiff.requirement.backfillTitle')}</div>
              <div className="font-mono text-xs">{target}</div>
              <div className="mt-1 text-xs">{t('schemaDiff.requirement.backfillHint')}</div>
            </div>
          );
        }
        return (
          <div
            key={`unsupported-${target}-${req.reason}`}
            className="rounded border border-red-500/30 bg-red-500/10 p-2 text-sm text-red-400"
          >
            <div className="font-medium">❌ {t('schemaDiff.requirement.unsupportedTitle')}</div>
            <div className="text-xs">
              {target}: {req.reason}
            </div>
          </div>
        );
      })}
    </div>
  );
}

const COMMON_MYSQL_TEXT_TYPES = [
  'VARCHAR(255)',
  'VARCHAR(128)',
  'VARCHAR(64)',
  'VARCHAR(500)',
  'TEXT',
  'MEDIUMTEXT',
  'LONGTEXT',
];

function TypeSuggestionsNotice({
  suggestions,
  overrides = [],
  onOverrideChange,
  onApply,
  regenerating,
}: {
  suggestions: TypeSuggestion[];
  overrides?: ColumnTypeOverride[];
  onOverrideChange?: (table: string, column: string, targetType: string) => void;
  onApply?: () => void;
  regenerating?: boolean;
}) {
  const { t } = useI18n();

  if (suggestions.length === 0) {
    return null;
  }

  return (
    <div
      className="rounded-lg border border-yellow-500/30 bg-yellow-500/10 p-3 text-xs"
      data-testid="schema-diff-type-suggestions"
    >
      <div className="flex items-center gap-1.5 font-medium text-yellow-400">
        <span>⚠</span>
        <span>{t('schemaDiff.typeSuggestions.title')}</span>
      </div>
      <p className="mt-1 text-fg-secondary">{t('schemaDiff.typeSuggestions.desc')}</p>

      <div className="mt-2.5 overflow-x-auto">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-edge/60 text-[11px] text-fg-muted font-normal">
              <th className="pb-1.5 pr-3">{t('schemaDiff.typeSuggestions.column')}</th>
              <th className="pb-1.5 pr-3">{t('schemaDiff.typeSuggestions.sourceType')}</th>
              <th className="pb-1.5 pr-3">{t('schemaDiff.typeSuggestions.targetType')}</th>
              <th className="pb-1.5">{t('schemaDiff.typeSuggestions.reason')}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-edge/30">
            {suggestions.map((sug) => {
              const currentOverride =
                overrides.find((o) => o.table === sug.table && o.column === sug.column)
                  ?.targetType ??
                sug.currentType ??
                sug.suggestedType;

              return (
                <tr key={`${sug.table}.${sug.column}`} className="py-1.5">
                  <td className="py-1.5 pr-3 font-mono font-medium text-fg">
                    {sug.table}.{sug.column}
                  </td>
                  <td className="py-1.5 pr-3 font-mono text-fg-secondary">{sug.sourceType}</td>
                  <td className="py-1.5 pr-3">
                    <div className="flex items-center gap-1.5">
                      <input
                        type="text"
                        data-testid={`type-suggestion-input-${sug.column}`}
                        list={`type-options-${sug.table}-${sug.column}`}
                        value={currentOverride}
                        onChange={(e) => onOverrideChange?.(sug.table, sug.column, e.target.value)}
                        className="w-32 rounded border border-edge bg-surface px-2 py-0.5 font-mono text-xs text-fg focus:border-accent focus:outline-none"
                      />
                      <datalist id={`type-options-${sug.table}-${sug.column}`}>
                        {COMMON_MYSQL_TEXT_TYPES.map((ty) => (
                          <option key={ty} value={ty} />
                        ))}
                      </datalist>
                    </div>
                  </td>
                  <td className="py-1.5 text-fg-muted">
                    <span
                      className={`inline-block rounded px-1.5 py-0.5 text-[10px] ${
                        sug.isKeyOrIndexed
                          ? 'bg-yellow-500/20 text-yellow-300 font-medium'
                          : 'bg-surface-alt text-fg-muted'
                      }`}
                    >
                      {sug.reason}
                    </span>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <div className="mt-2.5 flex justify-end">
        <button
          type="button"
          data-testid="schema-diff-apply-suggestions"
          disabled={regenerating}
          onClick={onApply}
          className="rounded bg-yellow-500/20 px-2.5 py-1 text-xs font-medium text-yellow-300 hover:bg-yellow-500/30 disabled:opacity-50 transition-colors"
        >
          {t('schemaDiff.typeSuggestions.apply')}
        </button>
      </div>
    </div>
  );
}

function PlanRollbackStatus({ plan }: { plan: SchemaDiffPlan }) {
  const { t } = useI18n();
  const total = plan.statements.length;

  if (total === 0) {
    return null;
  }

  const { complete, missing } = rollbackCompletenessCounts(plan);

  if (missing === 0) {
    return (
      <p className="text-sm text-emerald-400" data-testid="schema-diff-rollback-status">
        ✅ {t('schemaDiff.rollback.available')}
      </p>
    );
  }

  if (complete === 0) {
    return (
      <p className="text-sm text-red-400" data-testid="schema-diff-rollback-status">
        ❌ {t('schemaDiff.rollback.none')}
      </p>
    );
  }

  return (
    <p className="text-sm text-yellow-400" data-testid="schema-diff-rollback-status">
      ⚠ {t('schemaDiff.rollback.partial', { count: missing })}
    </p>
  );
}

export function SchemaDiffPlanPanel({
  plan,
  allowDestructive,
  includeIndexes,
  onAllowDestructiveChange,
  onIncludeIndexesChange,
  onRegenerate,
  regenerating,
  typeOverrides,
  onTypeOverrideChange,
  onApplyTypeOverrides,
}: {
  plan: SchemaDiffPlan;
  allowDestructive: boolean;
  includeIndexes: boolean;
  onAllowDestructiveChange: (v: boolean) => void;
  onIncludeIndexesChange: (v: boolean) => void;
  onRegenerate: () => void;
  regenerating?: boolean;
  typeOverrides?: ColumnTypeOverride[];
  onTypeOverrideChange?: (table: string, column: string, targetType: string) => void;
  onApplyTypeOverrides?: () => void;
}) {
  const { t } = useI18n();
  const requirements = plan.requirements ?? [];

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-4 text-sm">
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={allowDestructive}
            onChange={(e) => onAllowDestructiveChange(e.target.checked)}
            data-testid="schema-diff-allow-destructive"
          />
          {t('schemaDiff.allowDestructive')}
        </label>
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={includeIndexes}
            onChange={(e) => onIncludeIndexesChange(e.target.checked)}
            data-testid="schema-diff-include-indexes"
          />
          {t('schemaDiff.includeIndexes')}
        </label>
        <button
          type="button"
          className="text-accent underline disabled:opacity-50"
          disabled={regenerating}
          onClick={onRegenerate}
        >
          {t('schemaDiff.regeneratePlan')}
        </button>
      </div>

      {!plan.sameDialect && (
        <p className="text-sm text-warning">{t('schemaDiff.crossDialectNote')}</p>
      )}

      {plan.typeSuggestions && plan.typeSuggestions.length > 0 && (
        <TypeSuggestionsNotice
          suggestions={plan.typeSuggestions}
          overrides={typeOverrides}
          onOverrideChange={onTypeOverrideChange}
          onApply={onApplyTypeOverrides}
          regenerating={regenerating}
        />
      )}

      {plan.warnings.length > 0 && (
        <div className="rounded border border-warning/40 bg-surface-alt p-2 text-xs text-fg-secondary">
          <div className="mb-1 font-medium text-warning">{t('schemaDiff.warnings')}</div>
          <ul className="list-inside list-disc space-y-0.5">
            {plan.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </div>
      )}

      <div className="text-xs text-fg-muted">
        {plan.sourceDialect} → {plan.targetDialect} · {plan.statements.length}{' '}
        {t('schemaDiff.statements')}
      </div>

      <PlanRequirements requirements={requirements} />

      <ul className="max-h-64 space-y-2 overflow-auto">
        {plan.statements.map((stmt: PlanStatement, i) => (
          <li
            key={`${i}-${stmt.summary}`}
            className="rounded border border-edge bg-surface-alt p-2 font-mono text-[11px]"
          >
            <div className="mb-1 flex items-center gap-2">
              <span className={`font-sans text-[10px] uppercase ${riskClass(stmt.risk)}`}>
                {stmt.risk}
              </span>
              <span className="text-fg">{stmt.summary}</span>
            </div>
            <pre className="whitespace-pre-wrap text-fg-secondary">{stmt.sql}</pre>
          </li>
        ))}
        {plan.statements.length === 0 && (
          <li
            data-testid="schema-diff-empty-plan"
            className="rounded border border-edge bg-surface-alt px-3 py-2 text-sm text-fg-muted"
          >
            {plan.warnings.length > 0
              ? t('schemaDiff.emptyPlanSkipped')
              : t('schemaDiff.emptyPlanNoDiff')}
          </li>
        )}
      </ul>

      <PlanRollbackStatus plan={plan} />
    </div>
  );
}
