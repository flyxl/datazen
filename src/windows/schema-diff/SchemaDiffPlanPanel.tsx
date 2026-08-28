import type { PlanStatement, SchemaDiffPlan, StatementRisk } from '../../commands/schemaDiff';
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

export function SchemaDiffPlanPanel({
  plan,
  allowDestructive,
  includeIndexes,
  onAllowDestructiveChange,
  onIncludeIndexesChange,
  onRegenerate,
  regenerating,
}: {
  plan: SchemaDiffPlan;
  allowDestructive: boolean;
  includeIndexes: boolean;
  onAllowDestructiveChange: (v: boolean) => void;
  onIncludeIndexesChange: (v: boolean) => void;
  onRegenerate: () => void;
  regenerating?: boolean;
}) {
  const { t } = useI18n();

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
          <li className="text-sm text-fg-muted">{t('schemaDiff.emptyPlan')}</li>
        )}
      </ul>
    </div>
  );
}
