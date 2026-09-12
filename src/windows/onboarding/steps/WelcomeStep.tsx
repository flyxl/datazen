import { ArrowRight, Download, FlaskConical, Plus } from 'lucide-react';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import { CONNECTION_IMPORT_APP_LABEL } from '../../../components/connection/ConnectionShareDialog';
import type { ConnectionImportApp } from '../../../components/connection/ConnectionShareDialog';
import type { EntryType } from '../wizardState';

interface WelcomeStepProps {
  /** Client config found on this machine, if any (import card sub-line). */
  detectedApp: ConnectionImportApp | null;
  selectedEntry: EntryType;
  onImport: () => void;
  onManual: () => void;
  onSample: () => void;
}

/* ---------- S0: Welcome / Three stacked choice cards ---------- */

export function WelcomeStep({
  detectedApp,
  selectedEntry,
  onImport,
  onManual,
  onSample,
}: Readonly<WelcomeStepProps>) {
  const { t } = useI18n();

  return (
    <div className="w-full max-w-[640px]" data-testid="onboarding-step-s0">
      <h2 className="mb-2.5 text-[25px] font-bold tracking-tight leading-tight text-fg">
        {t('onboarding.s0.title')}
      </h2>
      <p className="mb-7 max-w-[56ch] text-[13.5px] leading-[1.65] text-fg-secondary">
        {t('onboarding.s0.subtitle')}
      </p>

      <div className="flex flex-col gap-3">
        {/* Import — primary card */}
        <button
          type="button"
          onClick={onImport}
          data-testid="onboarding-entry-import"
          className={cn(
            'group flex items-center gap-4 rounded-xl border p-[18px_20px] text-left transition-all hover:-translate-y-px',
            'bg-white dark:bg-[linear-gradient(180deg,#121824,#0f131c)]',
            selectedEntry === 'import'
              ? 'border-accent/55 shadow-[0_0_0_3px_rgba(79,195,247,0.10)]'
              : 'border-accent/45 shadow-[0_0_0_3px_rgba(79,195,247,0.06)] hover:border-accent/35 hover:bg-accent-dim dark:hover:bg-[#141a26]',
          )}
        >
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[11px] bg-accent/12 text-accent border border-accent/18">
            <Download className="h-[19px] w-[19px]" />
          </div>
          <div className="min-w-0 flex-1">
            <b className="block text-[14.5px] font-semibold tracking-tight text-fg">
              {t('onboarding.s0.importCard')}
            </b>
            <span className="mt-0.5 block text-[12.5px] leading-relaxed text-fg-secondary">
              {t('onboarding.s0.importCardDesc')}
            </span>
            {detectedApp && (
              <span
                className="mt-0.5 block font-mono text-[11.5px] text-green"
                data-testid="onboarding-detected-app"
              >
                {t('onboarding.s0.importDetectedApp', {
                  app: CONNECTION_IMPORT_APP_LABEL[detectedApp],
                })}
              </span>
            )}
          </div>
          <span className="shrink-0 rounded-full bg-accent px-2.5 py-[3px] text-[10px] font-bold uppercase tracking-wider text-on-accent">
            {t('onboarding.s0.importFastest')}
          </span>
          <ArrowRight className="h-4 w-4 shrink-0 text-fg-muted transition-all group-hover:text-accent group-hover:translate-x-0.5" />
        </button>

        {/* Manual */}
        <button
          type="button"
          onClick={onManual}
          data-testid="onboarding-entry-manual"
          className="group flex items-center gap-4 rounded-xl border border-edge bg-white dark:bg-[linear-gradient(180deg,#121824,#0f131c)] p-[18px_20px] text-left transition-all hover:border-edge-hi hover:bg-surface-raised dark:hover:bg-[#141a26] hover:-translate-y-px"
        >
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[11px] bg-accent/12 text-accent border border-accent/18">
            <Plus className="h-[19px] w-[19px]" />
          </div>
          <div className="min-w-0 flex-1">
            <b className="block text-[14.5px] font-semibold tracking-tight text-fg">
              {t('onboarding.s0.manualCard')}
            </b>
            <span className="mt-0.5 block text-[12.5px] leading-relaxed text-fg-secondary">
              {t('onboarding.s0.manualCardDesc')}
            </span>
          </div>
          <ArrowRight className="h-4 w-4 shrink-0 text-fg-muted transition-all group-hover:text-accent group-hover:translate-x-0.5" />
        </button>

        {/* Sample */}
        <button
          type="button"
          onClick={onSample}
          data-testid="onboarding-entry-sample"
          className="group flex items-center gap-4 rounded-xl border border-edge bg-white dark:bg-[linear-gradient(180deg,#121824,#0f131c)] p-[18px_20px] text-left transition-all hover:border-edge-hi hover:bg-surface-raised dark:hover:bg-[#141a26] hover:-translate-y-px"
        >
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[11px] bg-accent/12 text-accent border border-accent/18">
            <FlaskConical className="h-[19px] w-[19px]" />
          </div>
          <div className="min-w-0 flex-1">
            <b className="block text-[14.5px] font-semibold tracking-tight text-fg">
              {t('onboarding.s0.sampleCard')}
            </b>
            <span className="mt-0.5 block text-[12.5px] leading-relaxed text-fg-secondary">
              {t('onboarding.s0.sampleCardDesc')}
            </span>
          </div>
          <ArrowRight className="h-4 w-4 shrink-0 text-fg-muted transition-all group-hover:text-accent group-hover:translate-x-0.5" />
        </button>
      </div>
    </div>
  );
}

/* ---------- shared step chrome ---------- */

export function StepTag({ label }: { label: string }) {
  return (
    <div className="mb-4 flex items-center gap-2">
      <span className="inline-block rounded-full bg-accent/12 border border-accent/20 px-2.5 py-1 text-[11px] font-semibold uppercase tracking-wider text-accent">
        {label}
      </span>
    </div>
  );
}
