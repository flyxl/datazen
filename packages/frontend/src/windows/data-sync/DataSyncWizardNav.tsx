import { ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Spinner } from '../../components/ui/Spinner';
import { useI18n } from '../../hooks/useI18n';

export interface DataSyncWizardNavProps {
  stepIndex: number;
  busy: boolean;
  canNext: boolean;
  /** The preview and result steps navigate with the execute bar instead. */
  showNext: boolean;
  onBack: () => void;
  onNext: () => void;
}

/** Back / next navigation of the Data Sync wizard. */
export function DataSyncWizardNav({
  stepIndex,
  busy,
  canNext,
  showNext,
  onBack,
  onNext,
}: DataSyncWizardNavProps) {
  const { t } = useI18n();

  return (
    <div className="flex shrink-0 items-center justify-between border-t border-edge px-6 py-3">
      <Button
        variant="ghost"
        data-testid="data-sync-back"
        disabled={stepIndex === 0 || busy}
        onClick={onBack}
      >
        <ChevronLeft className="h-4 w-4" /> {t('sync.back')}
      </Button>
      {showNext ? (
        <Button data-testid="data-sync-next" disabled={!canNext || busy} onClick={onNext}>
          {busy ? <Spinner size="lg" /> : null}
          {t('sync.next')}
          <ChevronRight className="h-4 w-4" />
        </Button>
      ) : null}
    </div>
  );
}
