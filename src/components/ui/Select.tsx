import { Select as UiSelect, type SelectProps as UiSelectProps } from '@datazen/ui';
import { useI18n } from '../../hooks/useI18n';

export type { SelectOption, SelectProps, SelectLabels } from '@datazen/ui';

export function Select(props: UiSelectProps) {
  const { t } = useI18n();
  const labels = {
    placeholder: t('select.placeholder'),
    noMatches: t('select.noMatches'),
    toggleOptions: t('select.toggleOptions'),
    ...props.labels,
  };
  return <UiSelect {...props} labels={labels} />;
}
