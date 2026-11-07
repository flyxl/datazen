import { AdminCreateDialog } from '../../components/admin/AdminCreateDialog';
import { useI18n } from '../../hooks/useI18n';

interface CreateDatabaseDialogProps {
  open: boolean;
  onClose: () => void;
  dbSessionId: string;
  onCreated?: (dbName: string) => void | Promise<void>;
}

export function CreateDatabaseDialog({
  open,
  onClose,
  dbSessionId,
  onCreated,
}: CreateDatabaseDialogProps) {
  const { t } = useI18n();

  return (
    <AdminCreateDialog
      open={open}
      onClose={onClose}
      dbSessionId={dbSessionId}
      command="create_database"
      titleKey="common.createDatabase"
      nameLabelKey="createDb.name"
      namePlaceholder="my_database"
      fieldLabels={{
        encoding: t('createDb.encoding'),
        owner: t('createDb.owner'),
      }}
      onCreated={onCreated}
    />
  );
}
