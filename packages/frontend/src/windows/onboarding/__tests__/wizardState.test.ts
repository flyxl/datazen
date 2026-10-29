import { describe, it, expect } from 'vitest';
import {
  wizardReducer,
  INITIAL_STATE,
  canContinueFromStepOne,
  canLeaveStepOne,
  connectionSummary,
  stepIndexFor,
  stepLabelKey,
  type WizardState,
} from '../wizardState';

function atStepOne(entry: WizardState['entry']): WizardState {
  return { ...INITIAL_STATE, step: 's1', entry };
}

describe('wizardReducer — entry selection', () => {
  it('S0 → S1 (manual) via ENTER_MANUAL', () => {
    const next = wizardReducer(INITIAL_STATE, { type: 'ENTER_MANUAL' });
    expect(next.step).toBe('s1');
    expect(next.entry).toBe('manual');
  });

  it('S0 → S1 (import) via ENTER_IMPORT — the import form is a step, not a dialog', () => {
    const next = wizardReducer(INITIAL_STATE, { type: 'ENTER_IMPORT' });
    expect(next.step).toBe('s1');
    expect(next.entry).toBe('import');
  });

  it('S0 → S1 (sample) via ENTER_SAMPLE', () => {
    const next = wizardReducer(INITIAL_STATE, { type: 'ENTER_SAMPLE' });
    expect(next.step).toBe('s1');
    expect(next.entry).toBe('sample');
  });
});

describe('wizardReducer — step 2 is always the AI step', () => {
  it.each(['manual', 'import', 'sample'] as const)(
    '%s entry reaches S2 (AI provider) before S3',
    (entry) => {
      let state = atStepOne(entry);
      if (entry === 'manual') {
        state = wizardReducer(state, { type: 'SAVE_SUCCESS', connectionName: 'Prod' });
      } else if (entry === 'import') {
        state = wizardReducer(state, {
          type: 'IMPORT_SUCCESS',
          result: { imported: 3, overwritten: 0, source: 'DBeaver' },
        });
      } else {
        state = wizardReducer(state, {
          type: 'SAMPLE_READY',
          path: '/tmp/playground.db',
          connectionName: 'Sample Playground',
        });
      }
      if (entry !== 'manual') {
        expect(state.step).toBe('s1');
        state = wizardReducer(state, { type: 'CONTINUE' });
      }
      expect(state.step).toBe('s2');

      state = wizardReducer(state, { type: 'FINISH', aiLabel: 'OpenAI' });
      expect(state.step).toBe('s3');
      expect(state.aiConfigured).toBe(true);
    },
  );

  it('manual entry advances to S2 only through SAVE_SUCCESS', () => {
    const state = atStepOne('manual');
    expect(canContinueFromStepOne(state)).toBe(false);
    expect(wizardReducer(state, { type: 'CONTINUE' }).step).toBe('s1');
    expect(wizardReducer(state, { type: 'SAVE_SUCCESS', connectionName: 'Prod' }).step).toBe('s2');
  });
});

describe('canLeaveStepOne', () => {
  it('manual is complete once the connection is saved', () => {
    expect(canLeaveStepOne({ ...INITIAL_STATE, entry: 'manual' })).toBe(true);
  });

  it('import needs at least one written connection (imported or overwritten)', () => {
    expect(canLeaveStepOne(atStepOne('import'))).toBe(false);
    expect(
      canLeaveStepOne({
        ...atStepOne('import'),
        importResult: { imported: 0, overwritten: 0, source: 'DBeaver' },
      }),
    ).toBe(false);
    expect(
      canLeaveStepOne({
        ...atStepOne('import'),
        importResult: { imported: 2, overwritten: 0, source: 'DBeaver' },
      }),
    ).toBe(true);
    // Re-importing an export of this very machine only overwrites — still done.
    expect(
      canLeaveStepOne({
        ...atStepOne('import'),
        importResult: { imported: 0, overwritten: 3, source: 'DataZen' },
      }),
    ).toBe(true);
  });

  it('sample needs a seeded database', () => {
    expect(canLeaveStepOne(atStepOne('sample'))).toBe(false);
    expect(canLeaveStepOne({ ...atStepOne('sample'), samplePath: '/tmp/playground.db' })).toBe(
      true,
    );
  });

  it('CONTINUE is a no-op while step 1 is incomplete', () => {
    const state = atStepOne('import');
    expect(wizardReducer(state, { type: 'CONTINUE' }).step).toBe('s1');
  });
});

describe('wizardReducer — navigation and skip', () => {
  it('S1 → S0 via BACK, S2 → S1 via BACK (any entry)', () => {
    expect(wizardReducer(atStepOne('manual'), { type: 'BACK' }).step).toBe('s0');
    for (const entry of ['manual', 'import', 'sample'] as const) {
      const s2: WizardState = { ...INITIAL_STATE, step: 's2', entry };
      expect(wizardReducer(s2, { type: 'BACK' }).step).toBe('s1');
    }
  });

  it('SKIP from any step lands on S3 with AI unconfigured', () => {
    for (const step of ['s0', 's1', 's2'] as const) {
      const next = wizardReducer({ ...INITIAL_STATE, step }, { type: 'SKIP' });
      expect(next.step).toBe('s3');
      expect(next.aiConfigured).toBe(false);
      expect(next.aiLabel).toBeNull();
    }
  });

  it('BACK is a no-op on S0 and S3', () => {
    expect(wizardReducer(INITIAL_STATE, { type: 'BACK' }).step).toBe('s0');
    expect(
      wizardReducer({ ...INITIAL_STATE, step: 's3', entry: 'sample' }, { type: 'BACK' }).step,
    ).toBe('s3');
  });
});

describe('step indicator', () => {
  it('numbers step 1 and step 2 and hides the indicator elsewhere', () => {
    expect(stepIndexFor('s0')).toBe(0);
    expect(stepIndexFor('s1')).toBe(1);
    expect(stepIndexFor('s2')).toBe(2);
    expect(stepIndexFor('s3')).toBe(0);
    expect(stepLabelKey('s1')).toBe('onboarding.s1.stepLabel');
    expect(stepLabelKey('s2')).toBe('onboarding.s2.stepLabel');
  });
});

describe('connectionSummary', () => {
  it('reports imported counts, created connections and nothing', () => {
    expect(
      connectionSummary({
        ...INITIAL_STATE,
        entry: 'import',
        importResult: { imported: 4, overwritten: 1, source: 'DBeaver JSON' },
      }),
    ).toEqual({ kind: 'imported', imported: 4, overwritten: 1, source: 'DBeaver JSON' });
    expect(
      connectionSummary({ ...INITIAL_STATE, entry: 'manual', connectionName: 'Prod' }),
    ).toEqual({ kind: 'connection', name: 'Prod' });
    expect(connectionSummary({ ...INITIAL_STATE, entry: 'sample' })).toEqual({ kind: 'none' });
  });
});

describe('journey: import entry runs the whole wizard', () => {
  it('S0 → import step → inline success → AI step → finish → done', () => {
    let state = INITIAL_STATE;

    state = wizardReducer(state, { type: 'ENTER_IMPORT' });
    expect(state.step).toBe('s1');
    expect(canLeaveStepOne(state)).toBe(false);

    // The inline form reports the import result without any dialog round-trip.
    state = wizardReducer(state, {
      type: 'IMPORT_SUCCESS',
      result: { imported: 6, overwritten: 0, source: 'TablePlus' },
    });
    expect(state.step).toBe('s1');
    expect(canLeaveStepOne(state)).toBe(true);

    state = wizardReducer(state, { type: 'CONTINUE' });
    expect(state.step).toBe('s2');

    state = wizardReducer(state, { type: 'FINISH', aiLabel: 'DeepSeek' });
    expect(state.step).toBe('s3');
    expect(connectionSummary(state)).toEqual({
      kind: 'imported',
      imported: 6,
      overwritten: 0,
      source: 'TablePlus',
    });
  });
});

describe('journey: sample entry seeds then still configures AI', () => {
  it('S0 → sample step → ready → AI step → skip → done', () => {
    let state = INITIAL_STATE;

    state = wizardReducer(state, { type: 'ENTER_SAMPLE' });
    expect(state.step).toBe('s1');
    // A failed seeding keeps step 1 locked instead of advancing.
    state = wizardReducer(state, { type: 'SAMPLE_FAILED' });
    expect(canLeaveStepOne(state)).toBe(false);

    state = wizardReducer(state, {
      type: 'SAMPLE_READY',
      path: '/data/sample/playground.db',
      connectionName: 'Sample Playground',
    });
    expect(canLeaveStepOne(state)).toBe(true);

    state = wizardReducer(state, { type: 'CONTINUE' });
    expect(state.step).toBe('s2');

    state = wizardReducer(state, { type: 'SKIP' });
    expect(state.step).toBe('s3');
    expect(connectionSummary(state)).toEqual({
      kind: 'connection',
      name: 'Sample Playground',
    });
  });
});
