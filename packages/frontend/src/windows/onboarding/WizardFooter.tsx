import { X } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import {
  WIZARD_CHROME_TEXT_CLASS,
  WIZARD_FOOTER_HEIGHT_CLASS,
  WIZARD_FOOTER_PX_CLASS,
} from './onboardingLayout';
import { stepIndexFor, stepLabelKey, type Step } from './wizardState';

interface WizardFooterProps {
  step: Step;
  /** Continue / Finish availability (step 1 completeness). */
  canContinue: boolean;
  /** Step 1 has its own primary action (manual / sample / import forms). */
  showContinue: boolean;
  showFinish: boolean;
  busy?: boolean;
  onSkip: () => void;
  onBack: () => void;
  onContinue: () => void;
  onFinish: () => void;
}

/**
 * Global wizard footer: step indicator on the left, skip/back/continue on the
 * right. Shares its fixed height with the brand sidebar's version row so both
 * lines of text sit on the same bottom line.
 */
export function WizardFooter({
  step,
  canContinue,
  showContinue,
  showFinish,
  busy = false,
  onSkip,
  onBack,
  onContinue,
  onFinish,
}: WizardFooterProps) {
  const { t } = useI18n();
  const stepIndex = stepIndexFor(step);
  const showSteps = stepIndex > 0;

  return (
    <footer
      className={cn(
        'flex shrink-0 items-center gap-4 border-t border-edge bg-surface',
        WIZARD_FOOTER_HEIGHT_CLASS,
        WIZARD_FOOTER_PX_CLASS,
      )}
      data-testid="onboarding-footer"
    >
      {/* Step dots — hidden (still laying out) on the entry and done screens. */}
      <div
        className={cn('flex items-center gap-2', !showSteps && 'invisible')}
        aria-hidden={!showSteps}
        data-testid="onboarding-step-indicator"
      >
        {[1, 2].map((i) => (
          <div
            key={i}
            className={cn(
              'h-[7px] rounded-full transition-all',
              stepIndex === i
                ? 'w-5 bg-accent'
                : stepIndex > i
                  ? 'w-[7px] bg-accent/40'
                  : 'w-[7px] bg-edge',
            )}
          />
        ))}
        <span className={cn('ml-1', WIZARD_CHROME_TEXT_CLASS)} data-testid="onboarding-step-label">
          {t(stepLabelKey(step))}
        </span>
      </div>

      {/* Buttons */}
      <div className="ml-auto flex items-center gap-2.5">
        {step !== 's3' && (
          <button
            type="button"
            onClick={onSkip}
            data-testid="onboarding-skip"
            className="inline-flex items-center gap-1.5 border-none bg-transparent px-1.5 py-2.5 text-[12px] text-fg-muted transition-colors hover:text-fg-secondary cursor-pointer"
          >
            <X className="h-3 w-3" />
            {t('onboarding.common.skip')}
          </button>
        )}
        {step !== 's0' && step !== 's3' && (
          <Button variant="secondary" onClick={onBack} data-testid="onboarding-back">
            {t('onboarding.common.back')}
          </Button>
        )}
        {showContinue && (
          <Button
            variant="primary"
            onClick={onContinue}
            disabled={!canContinue}
            data-testid="onboarding-continue"
          >
            {t('onboarding.common.continue')}
          </Button>
        )}
        {showFinish && (
          <Button
            variant="primary"
            onClick={onFinish}
            disabled={busy}
            data-testid="onboarding-finish"
          >
            {t('onboarding.common.finish')}
          </Button>
        )}
      </div>
    </footer>
  );
}
