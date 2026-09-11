import { describe, it, expect } from 'vitest';
import {
  wizardReducer,
  INITIAL_STATE,
  type WizardState,
  type WizardAction,
} from '../wizardState';

describe('wizardReducer', () => {
  it('S0 → S1 via ENTER_MANUAL', () => {
    const next = wizardReducer(INITIAL_STATE, { type: 'ENTER_MANUAL' });
    expect(next.step).toBe('s1');
    expect(next.entry).toBe('manual');
  });

  it('S0 → S2 via IMPORT_SUCCESS', () => {
    const next = wizardReducer(INITIAL_STATE, {
      type: 'IMPORT_SUCCESS',
      result: { imported: 5, source: 'DBeaver' },
    });
    expect(next.step).toBe('s2');
    expect(next.entry).toBe('import');
    expect(next.importResult).toEqual({ imported: 5, source: 'DBeaver' });
  });

  it('S0 → S3 via SAMPLE', () => {
    const next = wizardReducer(INITIAL_STATE, { type: 'SAMPLE' });
    expect(next.step).toBe('s3');
    expect(next.entry).toBe('sample');
  });

  it('S1 → S2 via SAVE_SUCCESS', () => {
    const s1: WizardState = { ...INITIAL_STATE, step: 's1', entry: 'manual' };
    const next = wizardReducer(s1, { type: 'SAVE_SUCCESS' });
    expect(next.step).toBe('s2');
    expect(next.tested).toBe(true);
  });

  it('S2 → S3 via FINISH', () => {
    const s2: WizardState = {
      ...INITIAL_STATE,
      step: 's2',
      entry: 'manual',
      tested: true,
    };
    const next = wizardReducer(s2, { type: 'FINISH' });
    expect(next.step).toBe('s3');
  });

  it('S1 → S3 via SKIP (sets validated null)', () => {
    const s1: WizardState = { ...INITIAL_STATE, step: 's1', entry: 'manual' };
    const next = wizardReducer(s1, { type: 'SKIP' });
    expect(next.step).toBe('s3');
    expect(next.validated).toBeNull();
  });

  it('S2 → S3 via SKIP', () => {
    const s2: WizardState = {
      ...INITIAL_STATE,
      step: 's2',
      entry: 'manual',
      tested: true,
    };
    const next = wizardReducer(s2, { type: 'SKIP' });
    expect(next.step).toBe('s3');
    expect(next.validated).toBeNull();
  });

  it('S0 → S3 via SKIP', () => {
    const next = wizardReducer(INITIAL_STATE, { type: 'SKIP' });
    expect(next.step).toBe('s3');
  });

  it('S1 → S0 via BACK', () => {
    const s1: WizardState = { ...INITIAL_STATE, step: 's1', entry: 'manual' };
    const next = wizardReducer(s1, { type: 'BACK' });
    expect(next.step).toBe('s0');
  });

  it('S2 (import) → S0 via BACK', () => {
    const s2: WizardState = {
      ...INITIAL_STATE,
      step: 's2',
      entry: 'import',
      tested: true,
    };
    const next = wizardReducer(s2, { type: 'BACK' });
    expect(next.step).toBe('s0');
  });

  it('S2 (manual) → S1 via BACK', () => {
    const s2: WizardState = {
      ...INITIAL_STATE,
      step: 's2',
      entry: 'manual',
      tested: true,
    };
    const next = wizardReducer(s2, { type: 'BACK' });
    expect(next.step).toBe('s1');
  });

  it('S0 → S0 via BACK (no-op)', () => {
    const next = wizardReducer(INITIAL_STATE, { type: 'BACK' });
    expect(next.step).toBe('s0');
  });

  it('S3 → S3 via BACK (no-op)', () => {
    const s3: WizardState = {
      ...INITIAL_STATE,
      step: 's3',
      entry: 'sample',
    };
    const next = wizardReducer(s3, { type: 'BACK' });
    expect(next.step).toBe('s3');
  });

  describe('journey: manual happy path', () => {
    it('S0 → S1 → test fail → retry → S2 → finish → S3', () => {
      let state = INITIAL_STATE;

      state = wizardReducer(state, { type: 'ENTER_MANUAL' });
      expect(state.step).toBe('s1');

      state = wizardReducer(state, { type: 'SAVE_SUCCESS' });
      expect(state.step).toBe('s2');
      expect(state.tested).toBe(true);

      state = wizardReducer(state, { type: 'FINISH' });
      expect(state.step).toBe('s3');
    });
  });

  describe('journey: import path', () => {
    it('S0 → import success → S2 → skip → S3', () => {
      let state = INITIAL_STATE;

      state = wizardReducer(state, {
        type: 'IMPORT_SUCCESS',
        result: { imported: 12, source: 'DataGrip' },
      });
      expect(state.step).toBe('s2');
      expect(state.entry).toBe('import');

      state = wizardReducer(state, { type: 'SKIP' });
      expect(state.step).toBe('s3');
      expect(state.validated).toBeNull();
    });
  });

  describe('journey: sample path', () => {
    it('S0 → sample → S3 (direct)', () => {
      const state = wizardReducer(INITIAL_STATE, { type: 'SAMPLE' });
      expect(state.step).toBe('s3');
      expect(state.entry).toBe('sample');
    });
  });
});
