import { ChevronRight } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { WIZARD_STEPS } from './useDataSyncWizardState';

export interface DataSyncWizardStepperProps {
  stepIndex: number;
}

/** Progress bar of the Data Sync wizard. */
export function DataSyncWizardStepper({ stepIndex }: DataSyncWizardStepperProps) {
  const { t } = useI18n();

  return (
    <div className="border-b border-edge px-6 py-3">
      <div className="mx-auto flex max-w-4xl flex-wrap items-center justify-center gap-1">
        {WIZARD_STEPS.map((s, i) => (
          <div key={s} className="flex items-center gap-1">
            {i > 0 && <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" aria-hidden />}
            <span
              data-testid={`data-sync-step-${s}`}
              className={cn(
                'flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs',
                i === stepIndex
                  ? 'font-semibold text-accent'
                  : i < stepIndex
                    ? 'text-accent/80'
                    : 'text-fg-muted',
              )}
            >
              <span
                className={cn(
                  'flex h-5 w-5 items-center justify-center rounded-full text-[10px] font-semibold',
                  i === stepIndex
                    ? 'bg-accent text-on-accent'
                    : i < stepIndex
                      ? 'bg-accent/20 text-accent'
                      : 'bg-surface-raised text-fg-muted',
                )}
              >
                {i + 1}
              </span>
              {t(`sync.step.${s}`)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
