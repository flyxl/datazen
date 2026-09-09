import { describe, it, expect, beforeEach } from 'vitest';
import { useOnboardingStore } from '../onboardingStore';

describe('onboardingStore', () => {
  beforeEach(() => {
    localStorage.clear();
    useOnboardingStore.getState().resetOnboarding();
  });

  it('initializes with not_started status and step 1', () => {
    const state = useOnboardingStore.getState();
    expect(state.status).toBe('not_started');
    expect(state.step).toBe(1);
    expect(state.queryExecuted).toBe(false);
    expect(state.aiOrChartExplored).toBe(false);
  });

  it('starts onboarding and advances to step 2 when sample connection provided', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    let state = useOnboardingStore.getState();
    expect(state.status).toBe('active');
    expect(state.sampleConnectionId).toBe('sample_sqlite');
    expect(state.step).toBe(2);

    useOnboardingStore.getState().markQueryExecuted();
    state = useOnboardingStore.getState();
    expect(state.queryExecuted).toBe(true);
    expect(state.step).toBe(3);

    useOnboardingStore.getState().markAiOrChartExplored();
    state = useOnboardingStore.getState();
    expect(state.aiOrChartExplored).toBe(true);

    useOnboardingStore.getState().completeOnboarding();
    state = useOnboardingStore.getState();
    expect(state.status).toBe('completed');
  });

  it('allows skipping onboarding at any point and persists skipped status', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    expect(useOnboardingStore.getState().status).toBe('active');

    useOnboardingStore.getState().skipOnboarding();
    expect(useOnboardingStore.getState().status).toBe('skipped');
    expect(localStorage.getItem('datazen:onboarding-state-v1')).toContain('"status":"skipped"');
  });
});
