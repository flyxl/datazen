/**
 * Onboarding Continuous Lifecycle Journey Tests.
 *
 * Validates the full first-run wizard state machine per
 * `docs/development/interaction-and-testing-principles.md` (Principle 3):
 * 1. Complete lifecycle: welcome -> sample init -> query -> insights -> completion
 * 2. Premature skip at step 2 leaves store cleanly skipped
 * 3. DOM-level guide bar interactions (quick run -> complete -> unmount)
 * 4. End-to-end: WelcomePage click -> guide bar DOM journey -> completion
 * 5. Persistence recovery: completed/skipped status survives module reload
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { useOnboardingStore } from '../../../stores/onboardingStore';
import { OnboardingGuideBar } from '../OnboardingGuideBar';
import { WelcomePage } from '../../welcome/WelcomePage';
import { sampleDataCommands } from '../../../commands/sampleData';

const STORAGE_KEY = 'datazen:onboarding-state-v1';

const fetchConnectionsMock = vi.fn();

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/sampleData', () => ({
  sampleDataCommands: {
    initSampleDatabase: vi.fn(),
  },
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: {
    getState: () => ({
      fetchConnections: fetchConnectionsMock,
    }),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  openNewConnectionDialog: vi.fn(),
}));

vi.mock('../../../lib/connectionShare', () => ({
  openConnectionShareDialog: vi.fn(),
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title: string }) => <div data-testid="title-bar">{title}</div>,
}));

vi.mock('../../../components/MenuBar', () => ({
  MenuBar: () => <div data-testid="menu-bar">menu</div>,
}));

vi.mock('../../../components/ThemeToggle', () => ({
  ThemeToggle: () => <div data-testid="theme-toggle">theme</div>,
}));

describe('Onboarding Continuous Journey Test', () => {
  beforeEach(() => {
    localStorage.clear();
    useOnboardingStore.getState().resetOnboarding();
    vi.clearAllMocks();
    fetchConnectionsMock.mockResolvedValue(undefined);
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

  describe('End-to-end multi-step continuous journey', () => {
    it('WelcomePage sample click -> guide bar quick run -> explore -> complete unmounts', async () => {
      const mockConn = { id: 'sample_sqlite', name: 'Sample E-Commerce' };
      vi.mocked(sampleDataCommands.initSampleDatabase).mockResolvedValueOnce(mockConn);

      // 1. Welcome page: user opens sample database
      render(<WelcomePage />);
      expect(useOnboardingStore.getState().status).toBe('not_started');
      expect(useOnboardingStore.getState().step).toBe(1);

      fireEvent.click(screen.getByTestId('welcome-open-sample'));

      await waitFor(() => {
        expect(sampleDataCommands.initSampleDatabase).toHaveBeenCalled();
        expect(useOnboardingStore.getState().status).toBe('active');
        expect(useOnboardingStore.getState().step).toBe(2);
        expect(useOnboardingStore.getState().sampleConnectionId).toBe('sample_sqlite');
      });

      cleanup();

      // 2. Connection workspace: guide bar at step 2
      const onRun = vi.fn();
      const { rerender, container } = render(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);

      expect(screen.getByTestId('onboarding-guide-bar')).toBeInTheDocument();
      expect(screen.getByTestId('onboarding-quick-run-btn')).toBeInTheDocument();
      expect(screen.queryByTestId('onboarding-complete-btn')).not.toBeInTheDocument();

      // 3. Quick run advances to step 3
      fireEvent.click(screen.getByTestId('onboarding-quick-run-btn'));
      expect(onRun).toHaveBeenCalledOnce();
      expect(useOnboardingStore.getState().step).toBe(3);
      expect(useOnboardingStore.getState().queryExecuted).toBe(true);

      rerender(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);
      expect(screen.queryByTestId('onboarding-quick-run-btn')).not.toBeInTheDocument();
      expect(screen.getByTestId('onboarding-complete-btn')).toBeInTheDocument();

      // 4. Explore insights (chart/AI action marks exploration)
      useOnboardingStore.getState().markAiOrChartExplored();
      expect(useOnboardingStore.getState().aiOrChartExplored).toBe(true);

      // 5. Complete onboarding and guide bar unmounts
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
      const storeMod = await import('../../../stores/onboardingStore');
      const guideBarMod = await import('../OnboardingGuideBar');
      const reloaded = storeMod.useOnboardingStore.getState();

      expect(reloaded.status).toBe('completed');
      expect(reloaded.step).toBe(3);

      const { container } = render(<guideBarMod.OnboardingGuideBar />);
      expect(container.firstChild).toBeNull();
    });

    it('preserves skipped status after module reload and does not re-open wizard', async () => {
      useOnboardingStore.getState().startOnboarding('sample_sqlite');
      useOnboardingStore.getState().skipOnboarding();
      expect(localStorage.getItem(STORAGE_KEY)).toContain('"status":"skipped"');

      vi.resetModules();
      const storeMod = await import('../../../stores/onboardingStore');
      const guideBarMod = await import('../OnboardingGuideBar');
      const reloaded = storeMod.useOnboardingStore.getState();

      expect(reloaded.status).toBe('skipped');

      const { container } = render(<guideBarMod.OnboardingGuideBar />);
      expect(container.firstChild).toBeNull();
    });
  });
});
