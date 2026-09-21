import { Select } from '@datazen/ui';
import { useI18n } from '@datazen/ui';
import { KEY_TYPE_FILTERS } from './keyTree';

export interface KeyBrowserControlsProps {
  keyType: string;
  onKeyTypeChange: (value: string) => void;
  withMemory: boolean;
  onWithMemoryChange: (value: boolean) => void;
  noTtlOnly: boolean;
  onNoTtlOnlyChange: (value: boolean) => void;
}

export function KeyBrowserControls({
  keyType,
  onKeyTypeChange,
  withMemory,
  onWithMemoryChange,
  noTtlOnly,
  onNoTtlOnlyChange,
}: KeyBrowserControlsProps) {
  const { t } = useI18n();

  const typeOptions = KEY_TYPE_FILTERS.map((item) => ({
    value: item.value,
    label: t(item.labelKey as 'redis.type'),
  }));

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Select
        value={keyType}
        onChange={(value) => onKeyTypeChange(value)}
        options={typeOptions}
        className="h-7 w-28 text-xs"
        title={t('redis.filterByType')}
      />
      <label className="flex cursor-pointer items-center gap-1.5 text-xs text-fg-secondary">
        <input
          type="checkbox"
          checked={withMemory}
          onChange={(e) => onWithMemoryChange(e.target.checked)}
          className="rounded border-edge"
        />
        {t('redis.withMemory')}
      </label>
      <label className="flex cursor-pointer items-center gap-1.5 text-xs text-fg-secondary">
        <input
          type="checkbox"
          checked={noTtlOnly}
          onChange={(e) => onNoTtlOnlyChange(e.target.checked)}
          className="rounded border-edge"
          data-testid="redis-no-ttl-only"
        />
        {t('redis.noTtlOnly')}
      </label>
    </div>
  );
}
