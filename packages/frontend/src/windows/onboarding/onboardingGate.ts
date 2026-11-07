/**
 * First-run journey gate.
 *
 * The backend owns the fresh-install-vs-upgrade decision (`Store::load_all`):
 * - a data dir that has never persisted `settings.json` is a fresh install and
 *   materializes `onboarding = { completed: false }` → journey is shown;
 * - an installation whose `settings.json` predates the journey is an upgrade
 *   and is marked `{ completed: true }` → journey stays hidden.
 *
 * So the UI only has to render the journey when the backend explicitly reports
 * an unfinished onboarding state. A missing state (`undefined`/`null`) means
 * "upgrading user, never onboarded before" and must NOT show the journey —
 * gating on `completed` alone would re-onboard every existing user once.
 */

/** Revision of the first-run journey. Bump only to re-onboard every user. */
export const ONBOARDING_VERSION = 1;

export interface OnboardingState {
  completed: boolean;
  version: number;
}

/** True when the first-run journey should replace the main workspace. */
export function shouldShowOnboarding(state?: OnboardingState | null): boolean {
  if (!state) return false;
  return state.completed !== true;
}

/** Settings payload written when the user leaves the journey. */
export function completedOnboardingState(): OnboardingState {
  return { completed: true, version: ONBOARDING_VERSION };
}
