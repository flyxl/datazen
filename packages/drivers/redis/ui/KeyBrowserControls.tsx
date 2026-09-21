import { FolderTree, List } from 'lucide-react';
import { Select } from '@datazen/ui';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';
import { KEY_TYPE_FILTERS } from './keyTree';

export type KeyBrowserViewMode = 'flat' | 'tree';

export interface KeyBrowserControlsProps {
  keyType: string;
  onKeyTypeChange: (value: string) => void;
  viewMode: KeyBrowserViewMode;
  onViewModeChange: (mode: KeyBrowserViewMode) => void;
  withMemory: boolean;
  onWithMemoryChange: (value: boolean) => void;
  noTtlOnly: boolean;
  onNoTtlOnlyChange: (value: boolean) => void;
}

export function KeyBrowserControls({
  keyType,
  onKeyTypeChange,
  viewMode,
  onViewModeChange,
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
      <div className="flex overflow-hidden rounded-md border border-edge">
        <button
          type="button"
          className={cn(
            'flex h-7 items-center gap-1 px-2 text-xs transition-colors',
            viewMode === 'flat'
              ? 'bg-accent/10 text-accent'
              : 'text-fg-secondary hover:bg-surface-raised',
          )}
          title={t('redis.flatView')}
          onClick={() => onViewModeChange('flat')}
        >
          <List className="h-3.5 w-3.5" />
          {t('redis.flatView')}
        </button>
        <button
          type="button"
          className={cn(
            'flex h-7 items-center gap-1 border-l border-edge px-2 text-xs transition-colors',
            viewMode === 'tree'
              ? 'bg-accent/10 text-accent'
              : 'text-fg-secondary hover:bg-surface-raised',
          )}
          title={t('redis.treeView')}
          onClick={() => onViewModeChange('tree')}
        >
          <FolderTree className="h-3.5 w-3.5" />
          {t('redis.treeView')}
        </button>
      </div>
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
