import { useEffect } from 'react';
import { Info } from 'lucide-react';
import { useI18n } from '../../../hooks/useI18n';
import { useAiStore } from '../../../stores/aiStore';
import { ModelProfileForm } from '../../settings/ai/ModelProfileForm';
import type { ModelProfileDraft } from '../../settings/ai/useModelProfileDraft';
import { StepTag } from './WelcomeStep';

interface AiProviderStepProps {
  draft: ModelProfileDraft;
}

/**
 * S2 — always the second step, whichever entry S0 was started from.
 *
 * The form itself is the settings' {@link ModelProfileForm} (shared with
 * Settings → AI → Add model): same provider/protocol/key/endpoint/model fields,
 * same validate, same safety gate. Only the heading, the security note and the
 * primary action (the journey footer's Finish) are journey specific.
 */
export function AiProviderStep({ draft }: Readonly<AiProviderStepProps>) {
  const { t } = useI18n();
  const loadProviders = useAiStore((s) => s.loadProviders);

  useEffect(() => {
    void loadProviders();
  }, [loadProviders]);

  return (
    <div className="w-full max-w-[680px]" data-testid="onboarding-step-s2-ai">
      <StepTag label={t('onboarding.s2.stepLabel')} />
      <h2 className="mb-2.5 text-[25px] font-bold tracking-tight leading-tight text-fg">
        {t('onboarding.s2.title')}
      </h2>
      <p className="mb-6 max-w-[56ch] text-[13.5px] leading-[1.65] text-fg-secondary">
        {t('onboarding.s2.subtitle')}
      </p>

      {/* Same form as Settings → AI (provider list, key, endpoint, model, safety gate). */}
      <div className="rounded-xl border border-edge bg-surface p-5">
        <ModelProfileForm draft={draft} variant="inline" />
      </div>

      {/* Security note */}
      <div className="mt-5 flex items-start gap-2.5 rounded-[10px] border border-accent/16 bg-accent/5 p-3.5 text-[12.5px] leading-[1.6] text-fg-secondary">
        <Info className="mt-0.5 h-[15px] w-[15px] shrink-0 text-accent" />
        <span>{t('onboarding.s2.securityNote')}</span>
      </div>

      <p className="mt-3 text-[12.5px] text-fg-muted">{t('onboarding.s2.optionalHint')}</p>
    </div>
  );
}
