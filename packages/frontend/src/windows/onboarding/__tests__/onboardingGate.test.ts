import { describe, expect, it } from 'vitest';
import {
  ONBOARDING_VERSION,
  completedOnboardingState,
  shouldShowOnboarding,
} from '../onboardingGate';

describe('shouldShowOnboarding', () => {
  it('shows the journey only for an explicitly unfinished state (fresh install)', () => {
    expect(shouldShowOnboarding({ completed: false, version: ONBOARDING_VERSION })).toBe(true);
  });

  it('never shows the journey for upgrading users (no onboarding state persisted)', () => {
    // Legacy settings.json without the key — the backend marks these as
    // completed, but a missing state must also never onboard anyone.
    expect(shouldShowOnboarding(undefined)).toBe(false);
    expect(shouldShowOnboarding(null)).toBe(false);
  });

  it('hides the journey once it was completed', () => {
    expect(shouldShowOnboarding(completedOnboardingState())).toBe(false);
  });
});

describe('completedOnboardingState', () => {
  it('stamps the current journey revision', () => {
    expect(completedOnboardingState()).toEqual({
      completed: true,
      version: ONBOARDING_VERSION,
    });
  });
});
