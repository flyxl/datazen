import { useCallback, type MouseEvent } from 'react';
import { ChevronDown, ListFilter } from 'lucide-react';
import { ToolbarButton } from '../../../../components/ui/ToolbarButton';
import { showNativeContextMenu } from '../../../../lib/nativeContextMenu';
import { useI18n } from '../../../../hooks/useI18n';
import { useSettingsStore } from '../../../../stores/settingsStore';
import { tid } from '../../../../lib/tid';
import type { SqlExecutionStrategy } from '../../../../types';
import type { I18nKey } from '../../../../locales';

export interface ExecutionStrategySelectProps {
  compact?: boolean;
  disabled?: boolean;
}

const STRATEGIES: Array<{
  id: SqlExecutionStrategy;
  labelKey: I18nKey;
}> = [
  { id: 'current_statement', labelKey: 'query.executionStrategy.currentStatement' as I18nKey },
  { id: 'entire_script', labelKey: 'query.executionStrategy.entireScript' as I18nKey },
  { id: 'largest_statement', labelKey: 'query.executionStrategy.largestStatement' as I18nKey },
  { id: 'ask', labelKey: 'query.executionStrategy.ask' as I18nKey },
];

export function ExecutionStrategySelect({ compact, disabled }: ExecutionStrategySelectProps) {
  const { t } = useI18n();
  const currentStrategy =
    useSettingsStore((s) => s.settings.sqlExecutionStrategy) ?? 'current_statement';
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const openMenu = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      const rect = event.currentTarget.getBoundingClientRect();

      showNativeContextMenu(
        STRATEGIES.map((item) => ({
          kind: 'item' as const,
          id: `strategy-${item.id}`,
          label: `${item.id === currentStrategy ? '✓ ' : '   '}${t(item.labelKey)}`,
          action: () => {
            void updateSettings({ sqlExecutionStrategy: item.id });
          },
        })),
        { x: rect.left, y: rect.bottom },
      );
    },
    [currentStrategy, t, updateSettings],
  );

  const activeLabelKey =
    STRATEGIES.find((s) => s.id === currentStrategy)?.labelKey ??
    ('query.executionStrategy.currentStatement' as I18nKey);

  return (
    <ToolbarButton
      compact={compact}
      variant="ghost"
      label={compact ? undefined : t(activeLabelKey)}
      title={`${t('query.executionStrategy.title')}: ${t(activeLabelKey)}`}
      icon={
        <div className="flex items-center gap-0.5">
          <ListFilter className="h-3.5 w-3.5" />
          <ChevronDown className="h-2.5 w-2.5 opacity-60" />
        </div>
      }
      onClick={openMenu}
      disabled={disabled}
      {...tid('editor-execution-strategy-button')}
    />
  );
}
