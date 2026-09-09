import { X } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useOnboardingStore } from '../../stores/onboardingStore';

export interface OnboardingGuideBarProps {
  onExecuteSampleQuery?: () => void;
}

export function OnboardingGuideBar({ onExecuteSampleQuery }: OnboardingGuideBarProps) {
  const { t } = useI18n();
  const status = useOnboardingStore((s) => s.status);
  const step = useOnboardingStore((s) => s.step);

  if (status !== 'active') {
    return null;
  }

  const handleSkip = () => {
    useOnboardingStore.getState().skipOnboarding();
  };

  const handleQuickRun = () => {
    onExecuteSampleQuery?.();
    useOnboardingStore.getState().markQueryExecuted();
  };

  const handleComplete = () => {
    useOnboardingStore.getState().completeOnboarding();
  };

  const message =
    step === 2 ? t('onboarding.step2.message') : step === 3 ? t('onboarding.step3.message') : null;

  return (
    <div
      data-testid="onboarding-guide-bar"
      className="flex h-[38px] shrink-0 items-center gap-3 border-b border-accent/20 bg-accent/5 px-3"
    >
      <span className="shrink-0 rounded bg-accent/15 px-2 py-0.5 text-xs font-medium text-accent">
        {t('onboarding.badge', { step })}
      </span>

      {message && (
        <span className="min-w-0 flex-1 truncate text-xs text-fg-secondary">{message}</span>
      )}

      <div className="ml-auto flex shrink-0 items-center gap-1.5">
        {step === 2 && (
          <Button
            data-testid="onboarding-quick-run-btn"
            size="sm"
            variant="primary"
            onClick={handleQuickRun}
          >
            {t('onboarding.step2.action')}
          </Button>
        )}

        {step === 3 && (
          <Button
            data-testid="onboarding-complete-btn"
            size="sm"
            variant="primary"
            onClick={handleComplete}
          >
            {t('onboarding.step3.action')}
          </Button>
        )}

        <Button
          data-testid="onboarding-skip-btn"
          size="sm"
          variant="ghost"
          className="text-fg-secondary"
          onClick={handleSkip}
        >
          {t('onboarding.skip')}
        </Button>

        <Button
          data-testid="onboarding-close-btn"
          size="sm"
          variant="ghost"
          className="h-7 w-7 px-0 text-fg-secondary"
          aria-label={t('onboarding.skip')}
          onClick={handleSkip}
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  );
}
