import { ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Spinner } from '../../components/ui/Spinner';
import { useI18n } from '../../hooks/useI18n';
import type { SchemaDiffWizardStep } from './SchemaDiffWizardProgress';

export function SchemaDiffWindowNavigation({
  stepIndex,
  loading,
  step,
  deployAllowed,
  canNext,
  onBack,
  onDeploy,
  onNext,
}: {
  stepIndex: number;
  loading: boolean;
  step: SchemaDiffWizardStep;
  deployAllowed: boolean;
  canNext: boolean;
  onBack: () => void;
  onDeploy: () => void;
  onNext: () => void;
}) {
  const { t } = useI18n();

  return (
    <div className="flex shrink-0 items-center justify-between border-t border-edge px-6 py-3">
      <Button variant="ghost" disabled={stepIndex === 0 || loading} onClick={onBack}>
        <ChevronLeft className="h-4 w-4" /> {t('schemaDiff.back')}
      </Button>
      <div className="flex items-center gap-2">
        {step === 'deploy' ? (
          <Button
            variant="run"
            data-testid="schema-diff-deploy"
            disabled={!deployAllowed || loading}
            onClick={onDeploy}
          >
            {loading ? <Spinner size="lg" /> : t('schemaDiff.deploy')}
          </Button>
        ) : (
          <Button data-testid="schema-diff-next" disabled={!canNext || loading} onClick={onNext}>
            {loading ? <Spinner size="lg" /> : t('schemaDiff.next')}
            <ChevronRight className="h-4 w-4" />
          </Button>
        )}
      </div>
    </div>
  );
}
