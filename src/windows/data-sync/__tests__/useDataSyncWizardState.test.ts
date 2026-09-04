import { describe, expect, it } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import {
  NARROW_WIZARD_STEPS,
  useDataSyncWizardState,
  WIZARD_STEPS,
} from '../useDataSyncWizardState';

describe('[tester] useDataSyncWizardState', () => {
  it('exposes default wizard state and step constants', () => {
    const { result } = renderHook(() => useDataSyncWizardState());
    expect(result.current.step).toBe('endpoints');
    expect(result.current.syncState).toBe('idle');
    expect(result.current.mappingResults).toEqual([]);
    expect(WIZARD_STEPS).toContain('compare');
    expect(NARROW_WIZARD_STEPS).toEqual(['endpoints', 'setup', 'result']);
  });

  it('resetCompareState clears compare-related fields without resetting step', () => {
    const { result } = renderHook(() => useDataSyncWizardState());

    act(() => {
      result.current.setStep('compare');
      result.current.setMappingResults([{ table: 'users', status: 'ok' } as never]);
      result.current.setDisabledTables(new Set(['users']));
      result.current.setInspectionComplete(true);
      result.current.setSelectedTableKey('users');
      result.current.setSyncState('running');
    });

    act(() => {
      result.current.resetCompareState();
    });

    expect(result.current.step).toBe('compare');
    expect(result.current.mappingResults).toEqual([]);
    expect(result.current.disabledTables.size).toBe(0);
    expect(result.current.inspectionComplete).toBe(false);
    expect(result.current.selectedTableKey).toBeNull();
    expect(result.current.syncState).toBe('idle');
  });
});
