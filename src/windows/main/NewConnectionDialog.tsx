import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { ConnectionFormBody } from '../../components/connection/ConnectionFormBody';
import { useConnectionForm } from '../../components/connection/useConnectionForm';

export interface NewConnectionDialogProps {
  open: boolean;
  onClose: () => void;
}

export function NewConnectionDialog({ open, onClose }: NewConnectionDialogProps) {
  const { t } = useI18n();
  const form = useConnectionForm({ onAfterSave: onClose });

  return (
    <Dialog
      open={open}
      title={t('newConn.title')}
      description={t('newConn.description')}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button variant="secondary" onClick={() => void form.onTest()} disabled={form.testing}>
            {form.testing ? t('newConn.testing') : t('newConn.testConnection')}
          </Button>
          <Button variant="primary" onClick={() => void form.onSave()}>
            {t('common.save')}
          </Button>
        </>
      }
    >
      <div className="max-h-[60vh] overflow-y-auto pr-1">
        <ConnectionFormBody form={form} showDbTypeSelect variant="dialog" />
      </div>
    </Dialog>
  );
}
