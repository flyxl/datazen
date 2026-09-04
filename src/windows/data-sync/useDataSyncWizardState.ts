import { useCallback, useState } from 'react';
import { DEFAULT_SYNC_OPTIONS, type SyncOptions } from '../../commands/sync';
import type { SyncState } from './utils';
import {
  type DataSyncTableResult,
  type TableDiffFilter,
} from './mappingView';

export type WizardStep = 'endpoints' | 'setup' | 'objects' | 'compare' | 'preview' | 'result';

export const WIZARD_STEPS: WizardStep[] = [
  'endpoints',
  'setup',
  'objects',
  'compare',
  'preview',
  'result',
];

export const NARROW_WIZARD_STEPS: WizardStep[] = ['endpoints', 'setup', 'result'];

export function useDataSyncWizardState() {
  const [syncOptions, setSyncOptions] = useState<SyncOptions>(DEFAULT_SYNC_OPTIONS);
  const [mappingResults, setMappingResults] = useState<DataSyncTableResult[]>([]);
  const [disabledTables, setDisabledTables] = useState<Set<string>>(new Set());
  const [step, setStep] = useState<WizardStep>('endpoints');
  const [inspectionComplete, setInspectionComplete] = useState(false);
  const [syncState, setSyncState] = useState<SyncState>('idle');
  const [selectedTableKey, setSelectedTableKey] = useState<string | null>(null);
  const [tableFilter, setTableFilter] = useState<TableDiffFilter>('all');
  const [tableSearch, setTableSearch] = useState('');

  const resetCompareState = useCallback(() => {
    setMappingResults([]);
    setDisabledTables(new Set());
    setInspectionComplete(false);
    setSelectedTableKey(null);
    setSyncState('idle');
    setStep('endpoints');
  }, []);

  return {
    syncOptions,
    setSyncOptions,
    mappingResults,
    setMappingResults,
    disabledTables,
    setDisabledTables,
    step,
    setStep,
    inspectionComplete,
    setInspectionComplete,
    syncState,
    setSyncState,
    selectedTableKey,
    setSelectedTableKey,
    tableFilter,
    setTableFilter,
    tableSearch,
    setTableSearch,
    resetCompareState,
  };
}
