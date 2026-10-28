import { legacyInvoke as invoke } from '@datazen/backend-client';
import { OnboardingWizard } from './OnboardingWizard';

/**
 * Standalone onboarding window.
 *
 * Rendered inside a dedicated Tauri window (`?window=onboarding`) created by
 * Rust at startup when onboarding has not yet been completed.  On completion
 * the wizard writes settings to disk via the standard IPC path; we then call
 * the Rust `onboarding_complete` command which creates the main window and
 * closes this wizard window.
 */
export function OnboardingWindow() {
  const handleComplete = async () => {
    await invoke('onboarding_complete');
  };

  return <OnboardingWizard onComplete={handleComplete} />;
}
