import { useSettingsStore } from '../../stores/settingsStore';
import type { StatementResult } from '../../types';
import { isChartableResult } from './fieldInference';

/** Decide table vs chart after a successful query (honors Settings.autoChartOnQuery). */
export function resolvePostQueryViewMode(result: StatementResult | undefined): 'table' | 'chart' {
  const auto = useSettingsStore.getState().settings.autoChartOnQuery === true;
  if (auto && result && isChartableResult(result)) {
    return 'chart';
  }
  return 'table';
}
