import { describe, it, expect } from 'vitest';
import { EditorState } from '@codemirror/state';
import {
  executionStateField,
  StartExecutionEffect,
  CancelExecutionEffect,
  FinishExecutionEffect,
  matchesRunningTarget,
  isIdleOrCancelled,
  INITIAL_EXECUTION_STATE,
} from '../executionState';

function stateWithExec(doc: string) {
  return EditorState.create({
    doc,
    extensions: [executionStateField],
  });
}

function getExecState(state: EditorState) {
  return state.field(executionStateField);
}

describe('executionState', () => {
  describe('[tester] initial state', () => {
    it('starts as idle with no target', () => {
      const state = stateWithExec('SELECT 1;');
      const exec = getExecState(state);
      expect(exec.status).toBe('idle');
      expect(exec.targetRange).toBeNull();
      expect(exec.documentVersion).toBeNull();
    });
  });

  describe('[tester] StartExecutionEffect', () => {
    it('transitions to running with target range', () => {
      const state = stateWithExec('SELECT 1;');
      const next = state.update({
        effects: StartExecutionEffect.of({
          targetRange: { from: 0, to: 8 },
          documentVersion: 1,
        }),
      }).state;
      const exec = getExecState(next);
      expect(exec.status).toBe('running');
      expect(exec.targetRange).toEqual({ from: 0, to: 8 });
      expect(exec.documentVersion).toBe(1);
    });

    it('overwrites previous running state', () => {
      let state = stateWithExec('SELECT 1; SELECT 2;');
      state = state.update({
        effects: StartExecutionEffect.of({
          targetRange: { from: 0, to: 8 },
          documentVersion: 1,
        }),
      }).state;
      state = state.update({
        effects: StartExecutionEffect.of({
          targetRange: { from: 11, to: 19 },
          documentVersion: 2,
        }),
      }).state;
      const exec = getExecState(state);
      expect(exec.status).toBe('running');
      expect(exec.targetRange).toEqual({ from: 11, to: 19 });
      expect(exec.documentVersion).toBe(2);
    });
  });

  describe('[tester] CancelExecutionEffect', () => {
    it('transitions from running to cancelling', () => {
      let state = stateWithExec('SELECT 1;');
      state = state.update({
        effects: StartExecutionEffect.of({
          targetRange: { from: 0, to: 8 },
          documentVersion: 1,
        }),
      }).state;
      state = state.update({
        effects: CancelExecutionEffect.of(),
      }).state;
      const exec = getExecState(state);
      expect(exec.status).toBe('cancelling');
      expect(exec.targetRange).toEqual({ from: 0, to: 8 });
    });
  });

  describe('[tester] FinishExecutionEffect', () => {
    it('transitions from running to idle', () => {
      let state = stateWithExec('SELECT 1;');
      state = state.update({
        effects: StartExecutionEffect.of({
          targetRange: { from: 0, to: 8 },
          documentVersion: 1,
        }),
      }).state;
      state = state.update({
        effects: FinishExecutionEffect.of(),
      }).state;
      const exec = getExecState(state);
      expect(exec.status).toBe('idle');
      expect(exec.targetRange).toBeNull();
      expect(exec.documentVersion).toBeNull();
    });

    it('transitions from cancelling to idle', () => {
      let state = stateWithExec('SELECT 1;');
      state = state.update({
        effects: StartExecutionEffect.of({
          targetRange: { from: 0, to: 8 },
          documentVersion: 1,
        }),
      }).state;
      state = state.update({
        effects: CancelExecutionEffect.of(),
      }).state;
      state = state.update({
        effects: FinishExecutionEffect.of(),
      }).state;
      expect(getExecState(state).status).toBe('idle');
    });
  });

  describe('matchesRunningTarget', () => {
    it('returns true when status, version, and range all match', () => {
      const state = {
        status: 'running' as const,
        targetRange: { from: 0, to: 8 },
        documentVersion: 1,
      };
      expect(matchesRunningTarget(state, 1, { from: 0, to: 8 })).toBe(true);
    });

    it('returns false when status is not running', () => {
      const state = {
        status: 'idle' as const,
        targetRange: { from: 0, to: 8 },
        documentVersion: 1,
      };
      expect(matchesRunningTarget(state, 1, { from: 0, to: 8 })).toBe(false);
    });

    it('returns false when documentVersion mismatches', () => {
      const state = {
        status: 'running' as const,
        targetRange: { from: 0, to: 8 },
        documentVersion: 1,
      };
      expect(matchesRunningTarget(state, 2, { from: 0, to: 8 })).toBe(false);
    });

    it('returns false when targetRange from mismatches', () => {
      const state = {
        status: 'running' as const,
        targetRange: { from: 0, to: 8 },
        documentVersion: 1,
      };
      expect(matchesRunningTarget(state, 1, { from: 5, to: 8 })).toBe(false);
    });

    it('returns false when targetRange to mismatches', () => {
      const state = {
        status: 'running' as const,
        targetRange: { from: 0, to: 8 },
        documentVersion: 1,
      };
      expect(matchesRunningTarget(state, 1, { from: 0, to: 10 })).toBe(false);
    });

    it('returns false when targetRange is null', () => {
      const state = {
        status: 'running' as const,
        targetRange: null,
        documentVersion: 1,
      };
      expect(matchesRunningTarget(state, 1, { from: 0, to: 8 })).toBe(false);
    });
  });

  describe('isIdleOrCancelled', () => {
    it('returns true for idle', () => {
      expect(isIdleOrCancelled({ status: 'idle', targetRange: null, documentVersion: null })).toBe(
        true,
      );
    });

    it('returns true for cancelling', () => {
      expect(
        isIdleOrCancelled({
          status: 'cancelling',
          targetRange: { from: 0, to: 8 },
          documentVersion: 1,
        }),
      ).toBe(true);
    });

    it('returns false for running', () => {
      expect(
        isIdleOrCancelled({
          status: 'running',
          targetRange: { from: 0, to: 8 },
          documentVersion: 1,
        }),
      ).toBe(false);
    });
  });

  describe('[tester] state snapshot immutability', () => {
    it('does not mutate previous state on effect', () => {
      const state = stateWithExec('SELECT 1;');
      const before = getExecState(state);
      state.update({
        effects: StartExecutionEffect.of({
          targetRange: { from: 0, to: 8 },
          documentVersion: 1,
        }),
      });
      expect(before.status).toBe('idle');
      expect(before.targetRange).toBeNull();
    });
  });
});
