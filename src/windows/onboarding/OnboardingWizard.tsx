import { useCallback, useEffect, useReducer, useState } from 'react';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { useModelProfileDraft } from '../settings/ai/useModelProfileDraft';
import { POST_ONBOARDING_SAMPLE_KEY, type PostOnboardingSample } from '../../lib/windowManager';
import { BrandSidebar } from './BrandSidebar';
import { WizardFooter } from './WizardFooter';
import { WelcomeStep } from './steps/WelcomeStep';
import { ImportStep } from './steps/ImportStep';
import { ManualConnectionStep } from './steps/ManualConnectionStep';
import { SampleStep } from './steps/SampleStep';
import { AiProviderStep } from './steps/AiProviderStep';
import { DoneStep } from './steps/DoneStep';
import { useDetectedImportSource } from './useDetectedImportSource';
import { completedOnboardingState } from './onboardingGate';
import {
  INITIAL_STATE,
  canContinueFromStepOne,
  stepIndexFor,
  wizardReducer,
  type WizardState,
} from './wizardState';
import type { ConnectionImportSource } from '../../components/connection/ConnectionShareDialog';

/**
 * Preset SQL filled into the query editor when the user enters the main workspace
 * via the sample-database onboarding path.
 */
const SAMPLE_PRESET_SQL = `SELECT region, SUM(amount) AS total
FROM demo_sales
GROUP BY region
ORDER BY total DESC;`;

/**
 * First-run journey shell (see docs/reviews/startup_journey_proposal.md).
 *
 * S0 entry choice → S1 step 1 of 2 (inline import form / connection form /
 * sample dataset) → S2 step 2 of 2 (AI provider, always) → S3 done.
 */
export function OnboardingWizard() {
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [state, dispatch] = useReducer(wizardReducer, INITIAL_STATE);
  const saveProfile = useAiStore((s) => s.saveProfile);
  // Step 2 is the settings' AI profile form (shared hook + view). The journey
  // marks the fresh profile as the default one.
  const aiDraft = useModelProfileDraft({
    profile: null,
    onSave: (profile) => saveProfile({ ...profile, isDefault: true }),
    onSaved: (profile) => dispatch({ type: 'FINISH', aiLabel: profile.name }),
  });

  // Pre-check which client configs live on this machine (entry card + step 1).
  const { detected, detectedApps } = useDetectedImportSource();
  const [importSource, setImportSource] = useState<ConnectionImportSource>('file');
  useEffect(() => {
    if (detected) setImportSource(detected);
  }, [detected]);

  const handleOpen = useCallback(() => {
    // When the user finishes via the sample-database path, stash a directive
    // so ConnectionPage can auto-open the connection and pre-fill a query.
    if (state.entry === 'sample' && state.connectionName) {
      const payload: PostOnboardingSample = {
        connectionName: state.connectionName,
        sql: SAMPLE_PRESET_SQL,
      };
      try {
        localStorage.setItem(POST_ONBOARDING_SAMPLE_KEY, JSON.stringify(payload));
      } catch {
        // localStorage unavailable — graceful no-op.
      }
    }
    void updateSettings({ onboarding: completedOnboardingState() });
  }, [updateSettings, state.entry, state.connectionName]);

  const handleFinish = useCallback(async () => {
    // Nothing typed → AI stays unconfigured (the journey never forces a key).
    if (!aiDraft.hasCredentials) {
      dispatch({ type: 'FINISH', aiLabel: null });
      return;
    }
    // `requestSave` may open the high-risk egress confirmation; that path
    // reports back through `onSaved` above.
    await aiDraft.requestSave();
  }, [aiDraft]);

  const isStepOne = state.step === 's1';
  // Step 1 of the manual entry advances through the connection form's own Save
  // button; the other entries expose Continue in the footer.
  const continueVisible = isStepOne && state.entry !== 'manual';

  return (
    <div className="flex h-screen bg-surface text-fg" data-testid="onboarding-wizard">
      <BrandSidebar />

      <main className="flex flex-1 flex-col overflow-hidden">
        {/* Content area */}
        <div className="flex flex-1 overflow-y-auto p-[40px_48px_32px]">
          {state.step === 's0' && (
            <WelcomeStep
              detectedApp={detected}
              selectedEntry={state.entry}
              onImport={() => dispatch({ type: 'ENTER_IMPORT' })}
              onManual={() => dispatch({ type: 'ENTER_MANUAL' })}
              onSample={() => dispatch({ type: 'ENTER_SAMPLE' })}
            />
          )}

          {isStepOne && state.entry === 'manual' && (
            <ManualConnectionStep
              onSaved={(connectionName) => dispatch({ type: 'SAVE_SUCCESS', connectionName })}
            />
          )}

          {isStepOne && state.entry === 'import' && (
            <ImportStep
              initialSource={importSource}
              detectedApps={detectedApps}
              importResult={state.importResult}
              onImported={(result) => dispatch({ type: 'IMPORT_SUCCESS', result })}
            />
          )}

          {isStepOne && state.entry === 'sample' && (
            <SampleStep
              readyConnectionName={state.samplePath ? state.connectionName : undefined}
              onReady={({ path, connectionName }) =>
                dispatch({ type: 'SAMPLE_READY', path, connectionName })
              }
              onFailed={() => dispatch({ type: 'SAMPLE_FAILED' })}
            />
          )}

          {state.step === 's2' && <AiProviderStep draft={aiDraft} />}

          {state.step === 's3' && <DoneStep state={state} onOpen={handleOpen} />}
        </div>

        {/* Global footer — step dots + skip/back/continue */}
        {state.step !== 's3' && (
          <WizardFooter
            step={state.step}
            canContinue={canContinueFromStepOne(state)}
            showContinue={continueVisible}
            showFinish={state.step === 's2'}
            busy={aiDraft.savingState}
            onSkip={() => dispatch({ type: 'SKIP' })}
            onBack={() => dispatch({ type: 'BACK' })}
            onContinue={() => dispatch({ type: 'CONTINUE' })}
            onFinish={() => void handleFinish()}
          />
        )}
      </main>
    </div>
  );
}

/**
 * Step indicator helper kept next to the shell so tests can assert the visible
 * step number without reaching into the footer markup.
 */
export function visibleStepNumber(step: WizardState['step']): 1 | 2 | null {
  const index = stepIndexFor(step);
  return index === 0 ? null : (index as 1 | 2);
}
