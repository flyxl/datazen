import type { PlanStatement, StructureChangePlan } from '../../../lib/structureEditor/types';
import { useI18n } from '../../../hooks/useI18n';

function riskClass(risk: PlanStatement['risk']): string {
  switch (risk) {
    case 'additive':
      return 'text-success';
    case 'destructive':
      return 'text-danger';
    case 'rewrite':
      return 'text-warning';
  }
}

interface StructurePlanPreviewProps {
  plan: StructureChangePlan;
  onClose: () => void;
}

export function StructurePlanPreview({ plan, onClose }: StructurePlanPreviewProps) {
  const { t } = useI18n();

  return (
    <div className="border-t border-edge bg-surface-alt">
      <div className="flex items-center justify-between px-4 py-2">
        <span className="text-xs font-medium text-fg-secondary">{t('common.sqlPreview')}</span>
        <button type="button" className="text-xs text-fg-muted hover:text-fg" onClick={onClose}>
          {t('common.close')}
        </button>
      </div>

      {plan.warnings && plan.warnings.length > 0 && (
        <div className="mx-4 mb-2 rounded border border-warning/40 bg-surface p-2 text-xs text-fg-secondary">
          <div className="mb-1 font-medium text-warning">{t('schemaDiff.warnings')}</div>
          <ul className="list-inside list-disc space-y-0.5">
            {plan.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </div>
      )}

      <div className="px-4 pb-1 text-xs text-fg-muted">
        {plan.statements.length} {t('schemaDiff.statements')}
      </div>

      <ul className="max-h-48 space-y-2 overflow-auto px-4 pb-3">
        {plan.statements.map((stmt, i) => (
          <li
            key={`${i}-${stmt.summary}`}
            className="rounded border border-edge bg-surface p-2 font-mono text-[11px]"
          >
            <div className="mb-1 flex items-center gap-2">
              <span className={`font-sans text-[10px] uppercase ${riskClass(stmt.risk)}`}>
                {stmt.risk}
              </span>
              <span className="font-sans text-fg">{stmt.summary}</span>
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
