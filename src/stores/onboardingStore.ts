import { create } from 'zustand';

export type OnboardingStatus = 'not_started' | 'active' | 'completed' | 'skipped';

export interface OnboardingState {
  status: OnboardingStatus;
  step: 1 | 2 | 3;
  sampleConnectionId: string | null;
  queryExecuted: boolean;
  aiOrChartExplored: boolean;

  startOnboarding: (sampleConnectionId?: string) => void;
  advanceToStep: (step: 2 | 3) => void;
  markQueryExecuted: () => void;
  markAiOrChartExplored: () => void;
  completeOnboarding: () => void;
  skipOnboarding: () => void;
  resetOnboarding: () => void;
}

const STORAGE_KEY = 'datazen:onboarding-state-v1';

interface PersistedState {
  status: OnboardingStatus;
  step: 1 | 2 | 3;
  sampleConnectionId: string | null;
  queryExecuted: boolean;
  aiOrChartExplored: boolean;
}

function loadPersistedState(): PersistedState {
  if (typeof window === 'undefined' || !window.localStorage) {
    return {
      status: 'not_started',
      step: 1,
      sampleConnectionId: null,
      queryExecuted: false,
      aiOrChartExplored: false,
    };
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return {
        status: 'not_started',
        step: 1,
        sampleConnectionId: null,
        queryExecuted: false,
        aiOrChartExplored: false,
      };
    }
    const parsed = JSON.parse(raw) as Partial<PersistedState>;
    return {
      status: parsed.status ?? 'not_started',
      step: parsed.step ?? 1,
      sampleConnectionId: parsed.sampleConnectionId ?? null,
      queryExecuted: !!parsed.queryExecuted,
      aiOrChartExplored: !!parsed.aiOrChartExplored,
    };
  } catch {
    return {
      status: 'not_started',
      step: 1,
      sampleConnectionId: null,
      queryExecuted: false,
      aiOrChartExplored: false,
    };
  }
}

function persistState(state: PersistedState): void {
  if (typeof window === 'undefined' || !window.localStorage) return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // best-effort
  }
}

export const useOnboardingStore = create<OnboardingState>((set, get) => {
  const initial = loadPersistedState();

  const updateAndSave = (patch: Partial<PersistedState>) => {
    set((prev) => {
      const next: PersistedState = {
        status: patch.status ?? prev.status,
        step: patch.step ?? prev.step,
        sampleConnectionId:
          patch.sampleConnectionId !== undefined
            ? patch.sampleConnectionId
            : prev.sampleConnectionId,
        queryExecuted: patch.queryExecuted ?? prev.queryExecuted,
        aiOrChartExplored: patch.aiOrChartExplored ?? prev.aiOrChartExplored,
      };
      persistState(next);
      return { ...prev, ...next };
    });
  };

  return {
    ...initial,

    startOnboarding: (sampleConnectionId?: string) => {
      updateAndSave({
        status: 'active',
        step: sampleConnectionId ? 2 : 1,
        sampleConnectionId: sampleConnectionId ?? null,
      });
    },

    advanceToStep: (step: 2 | 3) => {
      updateAndSave({ step });
    },

    markQueryExecuted: () => {
      const { step } = get();
      updateAndSave({
        queryExecuted: true,
        step: step === 2 ? 3 : step,
      });
    },

    markAiOrChartExplored: () => {
      updateAndSave({ aiOrChartExplored: true });
    },

    completeOnboarding: () => {
      updateAndSave({ status: 'completed' });
    },

    skipOnboarding: () => {
      updateAndSave({ status: 'skipped' });
    },

    resetOnboarding: () => {
      const reset: PersistedState = {
        status: 'not_started',
        step: 1,
        sampleConnectionId: null,
        queryExecuted: false,
        aiOrChartExplored: false,
      };
      persistState(reset);
      set(reset);
    },
  };
});
