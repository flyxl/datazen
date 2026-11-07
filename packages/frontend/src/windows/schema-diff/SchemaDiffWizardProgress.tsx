import { ChevronRight } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';

export type SchemaDiffWizardStep = 'endpoints' | 'objects' | 'compare' | 'plan' | 'deploy';

export const SCHEMA_DIFF_STEPS: SchemaDiffWizardStep[] = [
  'endpoints',
  'objects',
  'compare',
  'plan',
  'deploy',
];

export const SCHEMA_DIFF_NARROW_STEPS: SchemaDiffWizardStep[] = ['endpoints', 'objects', 'deploy'];

export function SchemaDiffWizardProgress({ step }: { step: SchemaDiffWizardStep }) {
  const { t } = useI18n();
  const stepIndex = SCHEMA_DIFF_STEPS.indexOf(step);
  return (
    <div className="border-b border-edge px-6 py-3">
      <div className="mx-auto flex max-w-4xl flex-wrap items-center justify-center gap-1">
        {SCHEMA_DIFF_STEPS.map((item, index) => (
          <div key={item} className="flex items-center gap-1">
            {index > 0 && <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" aria-hidden />}
            <span
              data-testid={`schema-diff-step-${item}`}
              className={cn(
                'flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs',
                index === stepIndex
                  ? 'font-semibold text-accent'
                  : index < stepIndex
                    ? 'text-accent/80'
                    : 'text-fg-muted',
              )}
            >
              <span
                className={cn(
                  'flex h-5 w-5 items-center justify-center rounded-full text-[10px] font-semibold',
                  index === stepIndex
                    ? 'bg-accent text-on-accent'
                    : index < stepIndex
                      ? 'bg-accent/20 text-accent'
                      : 'bg-surface-raised text-fg-muted',
                )}
              >
                {index + 1}
              </span>
              {t(`schemaDiff.step.${item}`)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
