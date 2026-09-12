import { useState } from 'react';
import { CheckCircle2, FlaskConical } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import {
  CONNECTION_IMPORT_APP_LABEL,
  type ConnectionImportSource,
} from '../../../components/connection/ConnectionShareDialog';
import {
  useConnectionImport,
  type ConnectionImportResult,
} from '../../../components/connection/useConnectionImport';
import { ConnectionImportFields } from '../../../components/connection/ConnectionImportFields';
import { StepTag } from './WelcomeStep';
import type { ImportSummary } from '../wizardState';

interface ImportStepProps {
  /** Source preselected from the S0 pre-check. */
  initialSource: ConnectionImportSource;
  /** Every client config detected on this machine (checkmarks in the picker). */
  detectedApps: readonly string[];
  importResult?: ImportSummary;
  onImported: (result: ImportSummary) => void;
}

const SOURCE_ORDER: readonly ConnectionImportSource[] = [
  'file',
  'dbeaver',
  'datagrip',
  'navicat',
  'tableplus',
  'dbx',
];

function sourceLabel(source: ConnectionImportSource, fileLabel: string): string {
  return source === 'file' ? fileLabel : CONNECTION_IMPORT_APP_LABEL[source];
}

/* ---------- S1 (import entry): inline import form, never a dialog ---------- */

export function ImportStep({
  initialSource,
  detectedApps,
  importResult,
  onImported,
}: Readonly<ImportStepProps>) {
  const { t } = useI18n();
  const [source, setSource] = useState<ConnectionImportSource>(initialSource);

  const handleSuccess = (result: ConnectionImportResult) => {
    const label = sourceLabel(source, t('onboarding.s1.importFileSource'));
    onImported({
      imported: result.imported,
      overwritten: result.overwritten,
      source: result.sourceFormat ?? label,
    });
  };

  const imp = useConnectionImport({ source, onImportSuccess: handleSuccess });
  const written = (importResult?.imported ?? 0) + (importResult?.overwritten ?? 0);

  return (
    <div className="flex min-h-0 w-full flex-1 flex-col" data-testid="onboarding-step-s1-import">
      <StepTag label={t('onboarding.s1.stepLabel')} />
      <h2 className="mb-2.5 text-[25px] font-bold tracking-tight leading-tight text-fg">
        {t('onboarding.s1.importTitle')}
      </h2>
      <p className="mb-5 max-w-[56ch] text-[13.5px] leading-[1.65] text-fg-secondary">
        {t('onboarding.s1.importSubtitle')}
      </p>

      {/* Source picker — the form below is rendered inline, not in a dialog. */}
      <div className="mb-4 flex flex-wrap gap-2" data-testid="onboarding-import-sources">
        {SOURCE_ORDER.map((s) => (
          <button
            key={s}
            type="button"
            data-testid={`onboarding-import-source-${s}`}
            onClick={() => setSource(s)}
            className={cn(
              'inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-[12.5px] font-medium transition-colors',
              source === s
                ? 'border-accent/55 bg-accent/10 text-fg'
                : 'border-edge bg-surface text-fg-secondary hover:border-edge-hi hover:text-fg',
            )}
          >
            {sourceLabel(s, t('onboarding.s1.importFileSource'))}
            {detectedApps.includes(s) && (
              <CheckCircle2
                className="h-3 w-3 text-green"
                data-testid={`onboarding-import-found-${s}`}
              />
            )}
          </button>
        ))}
      </div>

      {/* Inline import form */}
      <div
        className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-edge bg-surface"
        data-testid="onboarding-import-form"
      >
        <div className="min-h-0 flex-1 overflow-y-auto p-5">
          <ConnectionImportFields import={imp} variant="inline" />
          {importResult && written > 0 && (
            <div
              className="mt-4 flex items-start gap-2.5 rounded-[10px] border border-green/25 bg-green/10 p-3.5 text-[12.5px] leading-[1.6] text-green"
              data-testid="onboarding-import-success"
            >
              <CheckCircle2 className="mt-0.5 h-[15px] w-[15px] shrink-0" />
              <span>
                {importResult.imported > 0
                  ? t('onboarding.s1.importSuccess', {
                      count: importResult.imported,
                      source: importResult.source,
                    })
                  : t('onboarding.s1.importUpdated', {
                      count: importResult.overwritten,
                      source: importResult.source,
                    })}
              </span>
            </div>
          )}
        </div>

        <footer className="flex shrink-0 items-center gap-3 border-t border-edge bg-surface-alt px-5 py-3">
          <Button
            variant="primary"
            onClick={() => void imp.submit()}
            disabled={imp.busy}
            data-testid="onboarding-import-submit"
          >
            <FlaskConical className="h-3.5 w-3.5" />
            {imp.busy ? t('onboarding.s1.importing') : imp.primaryActionLabel}
          </Button>
          <span className="flex-1 text-xs font-mono text-fg-muted">
            {imp.detecting ? t('onboarding.s1.importDetecting') : ''}
          </span>
        </footer>
      </div>
    </div>
  );
}
