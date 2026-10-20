import type { SyncOptions } from '../../commands/sync';
import { useI18n } from '../../hooks/useI18n';
import { OptionsBar } from './OptionsBar';

export interface DataSyncOptionsStepProps {
  options: SyncOptions;
  onOptionsChange: (next: SyncOptions) => void;
  onEnableDelete: () => void;
}

/** Second step of the Data Sync wizard: the sync options. */
export function DataSyncOptionsStep({
  options,
  onOptionsChange,
  onEnableDelete,
}: DataSyncOptionsStepProps) {
  const { t } = useI18n();

  return (
    <div className="space-y-4 rounded-lg border border-edge bg-surface-alt p-4">
      <div>
        <p className="text-sm font-medium text-fg">{t('sync.optionsTitle')}</p>
        <p className="mt-1 text-xs text-fg-muted">{t('sync.optionsHint')}</p>
      </div>
      <OptionsBar options={options} onChange={onOptionsChange} onEnableDelete={onEnableDelete} />
    </div>
  );
}
