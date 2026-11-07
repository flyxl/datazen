import { CheckCircle2 } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import { connectionSummary, type WizardState } from '../wizardState';

interface DoneStepProps {
  state: WizardState;
  onOpen: () => void;
}

/* ---------- S3: Done ---------- */

export function DoneStep({ state, onOpen }: Readonly<DoneStepProps>) {
  const { t } = useI18n();
  const summary = connectionSummary(state);

  const connValue =
    summary.kind === 'imported'
      ? summary.imported > 0
        ? t('onboarding.s3.importedValue', {
            count: summary.imported,
            source: summary.source,
          })
        : t('onboarding.s3.updatedValue', {
            count: summary.overwritten,
            source: summary.source,
          })
      : summary.kind === 'connection'
        ? summary.name
        : t('onboarding.s3.notConfigured');

  const aiValue =
    state.aiConfigured && state.aiLabel ? state.aiLabel : t('onboarding.s3.aiNotConfigured');

  return (
    <div className="flex flex-col items-center gap-6 py-8" data-testid="onboarding-step-s3">
      {/* Check mark */}
      <div className="flex h-16 w-16 items-center justify-center rounded-full border border-green/30 bg-green/10 text-green shadow-[0_0_0_8px_rgba(95,208,138,0.04)]">
        <svg
          className="h-7 w-7"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.4"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="m4 12.5 5 5L20 6.5" />
        </svg>
      </div>

      <div className="text-center">
        <h2 className="mb-2.5 text-[25px] font-bold tracking-tight leading-tight text-fg">
          {t('onboarding.s3.title')}
        </h2>
        <p className="mx-auto max-w-[48ch] text-[13.5px] leading-[1.65] text-fg-secondary">
          {t('onboarding.s3.subtitle')}
        </p>
      </div>

      {/* Summary rows */}
      <div className="w-full max-w-[480px] overflow-hidden rounded-xl border border-edge bg-surface">
        <SummaryRow
          label={t('onboarding.s3.connLabel')}
          value={connValue}
          skipped={summary.kind === 'none'}
          testId="onboarding-summary-connection"
        />
        <SummaryRow
          label={t('onboarding.s3.aiLabel')}
          value={aiValue}
          skipped={!state.aiConfigured}
          testId="onboarding-summary-ai"
        />
        <SummaryRow
          label={t('onboarding.s3.storageLabel')}
          value={t('onboarding.s3.storageValue')}
          last
          testId="onboarding-summary-storage"
        />
      </div>

      {/* Sample hint */}
      {state.entry === 'sample' && (
        <p
          className="text-center text-[13px] text-accent"
          dangerouslySetInnerHTML={{ __html: `💡 ${t('onboarding.s3.nextHint')}` }}
        />
      )}

      <Button variant="primary" onClick={onOpen} data-testid="onboard-open-datazen">
        {t('onboarding.s3.openBtn')}
      </Button>
    </div>
  );
}

function SummaryRow({
  label,
  value,
  skipped,
  last,
  testId,
}: {
  label: string;
  value: string;
  skipped?: boolean;
  last?: boolean;
  testId?: string;
}) {
  return (
    <div
      className={cn(
        'flex items-center gap-3 px-[18px] py-3 text-[13px]',
        !last && 'border-b border-edge/65',
      )}
      data-testid={testId}
    >
      <CheckCircle2
        className={cn('h-[15px] w-[15px] shrink-0', skipped ? 'text-fg-muted' : 'text-green')}
      />
      <span className="w-[112px] shrink-0 text-[12.5px] text-fg-secondary">{label}</span>
      <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[12px] text-fg">
        {value}
      </span>
    </div>
  );
}
