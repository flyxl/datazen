import { describe, it, expect, beforeEach, vi } from 'vitest';

const STORAGE_KEY = 'datazen:onboarding-state-v1';

describe('onboardingStore', () => {
  let useOnboardingStore: typeof import('../onboardingStore').useOnboardingStore;

  beforeEach(async () => {
    vi.resetModules();
    localStorage.clear();
    const mod = await import('../onboardingStore');
    useOnboardingStore = mod.useOnboardingStore;
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

  it('starts onboarding at step 1 when no sample connection is provided', () => {
    useOnboardingStore.getState().startOnboarding();
    const state = useOnboardingStore.getState();
    expect(state.status).toBe('active');
    expect(state.step).toBe(1);
    expect(state.sampleConnectionId).toBeNull();
  });

  it('advances to a specific step via advanceToStep', () => {
    useOnboardingStore.getState().startOnboarding();
    expect(useOnboardingStore.getState().step).toBe(1);

    useOnboardingStore.getState().advanceToStep(2);
    expect(useOnboardingStore.getState().step).toBe(2);

    useOnboardingStore.getState().advanceToStep(3);
    expect(useOnboardingStore.getState().step).toBe(3);
    expect(localStorage.getItem(STORAGE_KEY)).toContain('"step":3');
  });

  it('allows skipping onboarding at any point and persists skipped status', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    expect(useOnboardingStore.getState().status).toBe('active');

    useOnboardingStore.getState().skipOnboarding();
    expect(useOnboardingStore.getState().status).toBe('skipped');
    expect(localStorage.getItem(STORAGE_KEY)).toContain('"status":"skipped"');
  });

  it('falls back to defaults when persisted status or step are invalid', async () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        status: 'corrupted',
        step: 99,
        sampleConnectionId: 'sample_sqlite',
        queryExecuted: true,
        aiOrChartExplored: true,
      }),
    );

    vi.resetModules();
    const mod = await import('../onboardingStore');
    const state = mod.useOnboardingStore.getState();

    expect(state.status).toBe('not_started');
    expect(state.step).toBe(1);
    expect(state.sampleConnectionId).toBe('sample_sqlite');
    expect(state.queryExecuted).toBe(true);
    expect(state.aiOrChartExplored).toBe(true);
  });

  it('falls back to defaults when persisted payload is malformed JSON', async () => {
    localStorage.setItem(STORAGE_KEY, '{not-valid-json');

    vi.resetModules();
    const mod = await import('../onboardingStore');
    const state = mod.useOnboardingStore.getState();

    expect(state.status).toBe('not_started');
    expect(state.step).toBe(1);
    expect(state.sampleConnectionId).toBeNull();
    expect(state.queryExecuted).toBe(false);
    expect(state.aiOrChartExplored).toBe(false);
  });

  it('hydrates state from localStorage when the store module is reloaded', async () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    useOnboardingStore.getState().markQueryExecuted();
    useOnboardingStore.getState().markAiOrChartExplored();

    vi.resetModules();
    const mod = await import('../onboardingStore');
    const state = mod.useOnboardingStore.getState();

    expect(state.status).toBe('active');
    expect(state.step).toBe(3);
    expect(state.sampleConnectionId).toBe('sample_sqlite');
    expect(state.queryExecuted).toBe(true);
    expect(state.aiOrChartExplored).toBe(true);
  });
});
