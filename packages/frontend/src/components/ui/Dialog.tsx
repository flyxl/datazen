import { Dialog as UiDialog, type DialogProps as UiDialogProps } from '@datazen/ui';
import { useI18n } from '../../hooks/useI18n';

export type { DialogProps } from '@datazen/ui';

export function Dialog(props: UiDialogProps) {
  const { t } = useI18n();
  return <UiDialog {...props} closeLabel={props.closeLabel ?? t('common.close')} />;
}
