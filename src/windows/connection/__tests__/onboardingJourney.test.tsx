/**
 * Onboarding Continuous Lifecycle Journey Tests.
 *
 * Validates the full first-run wizard state machine per
 * `docs/development/interaction-and-testing-principles.md` (Principle 3):
 * 1. Complete lifecycle: welcome -> sample init -> query -> insights -> completion
 * 2. Premature skip at step 2 leaves store cleanly skipped
 * 3. DOM-level guide bar interactions (quick run -> complete -> unmount)
 * 4. Persistence recovery: completed/skipped status survives module reload
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { useOnboardingStore } from '../../../stores/onboardingStore';
import { OnboardingGuideBar } from '../OnboardingGuideBar';

const STORAGE_KEY = 'datazen:onboarding-state-v1';

describe('Onboarding Continuous Journey Test', () => {
  beforeEach(() => {
    localStorage.clear();
    useOnboardingStore.getState().resetOnboarding();
  });

  afterEach(() => {
    cleanup();
  });

  it('runs complete lifecycle: welcome -> sample init -> step 2 query -> step 3 insights -> completion', () => {
    // 1. Initial state
    expect(useOnboardingStore.getState().status).toBe('not_started');
    expect(useOnboardingStore.getState().step).toBe(1);

    // 2. User clicks "Open Sample SQLite"
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    expect(useOnboardingStore.getState().status).toBe('active');
    expect(useOnboardingStore.getState().step).toBe(2);

    // 3. User executes the preset query -> success with rows
    useOnboardingStore.getState().markQueryExecuted();
    expect(useOnboardingStore.getState().queryExecuted).toBe(true);
    expect(useOnboardingStore.getState().step).toBe(3);

    // 4. User explores chart or AI action
    useOnboardingStore.getState().markAiOrChartExplored();
    expect(useOnboardingStore.getState().aiOrChartExplored).toBe(true);

    // 5. Completion
    useOnboardingStore.getState().completeOnboarding();
    expect(useOnboardingStore.getState().status).toBe('completed');
  });

  it('allows premature skip at step 2 and leaves store cleanly skipped', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    expect(useOnboardingStore.getState().status).toBe('active');

    useOnboardingStore.getState().skipOnboarding();
    expect(useOnboardingStore.getState().status).toBe('skipped');

    // Subsequent query execution does not reactivate guide bar
    useOnboardingStore.getState().markQueryExecuted();
    expect(useOnboardingStore.getState().status).toBe('skipped');
  });

  describe('DOM-level guide bar journey', () => {
    it('advances from step 2 quick run to step 3 complete and unmounts on completion', () => {
      useOnboardingStore.getState().startOnboarding('sample_sqlite');
      const onRun = vi.fn();

      const { rerender, container } = render(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);

      expect(screen.getByTestId('onboarding-guide-bar')).toBeInTheDocument();
      expect(screen.getByTestId('onboarding-quick-run-btn')).toBeInTheDocument();
      expect(screen.queryByTestId('onboarding-complete-btn')).not.toBeInTheDocument();

      fireEvent.click(screen.getByTestId('onboarding-quick-run-btn'));
      expect(onRun).toHaveBeenCalledOnce();
      expect(useOnboardingStore.getState().step).toBe(3);
      expect(useOnboardingStore.getState().queryExecuted).toBe(true);

      rerender(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);
      expect(screen.queryByTestId('onboarding-quick-run-btn')).not.toBeInTheDocument();
      expect(screen.getByTestId('onboarding-complete-btn')).toBeInTheDocument();

      fireEvent.click(screen.getByTestId('onboarding-complete-btn'));
      expect(useOnboardingStore.getState().status).toBe('completed');

      rerender(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);
      expect(container.firstChild).toBeNull();
    });
  });

  describe('Persistence recovery journey', () => {
    it('preserves completed status after module reload and does not re-open wizard', async () => {
      useOnboardingStore.getState().startOnboarding('sample_sqlite');
      useOnboardingStore.getState().markQueryExecuted();
      useOnboardingStore.getState().completeOnboarding();
      expect(localStorage.getItem(STORAGE_KEY)).toContain('"status":"completed"');

      vi.resetModules();
      const mod = await import('../../../stores/onboardingStore');
      const reloaded = mod.useOnboardingStore.getState();

      expect(reloaded.status).toBe('completed');
      expect(reloaded.step).toBe(3);

      const { container } = render(<OnboardingGuideBar />);
      expect(container.firstChild).toBeNull();
    });

    it('preserves skipped status after module reload and does not re-open wizard', async () => {
      useOnboardingStore.getState().startOnboarding('sample_sqlite');
      useOnboardingStore.getState().skipOnboarding();
      expect(localStorage.getItem(STORAGE_KEY)).toContain('"status":"skipped"');

      vi.resetModules();
      const mod = await import('../../../stores/onboardingStore');
      const reloaded = mod.useOnboardingStore.getState();

      expect(reloaded.status).toBe('skipped');

      const { container } = render(<OnboardingGuideBar />);
      expect(container.firstChild).toBeNull();
    });
  });
});
