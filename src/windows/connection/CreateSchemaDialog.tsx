import { AdminCreateDialog } from '../../components/admin/AdminCreateDialog';
import { useI18n } from '../../hooks/useI18n';

interface CreateSchemaDialogProps {
  open: boolean;
  onClose: () => void;
  dbSessionId: string;
  /** F1: target catalog for create_schema (PG / SQL Server). */
  database?: string | null;
  onCreated?: () => void | Promise<void>;
}

export function CreateSchemaDialog({
  open,
  onClose,
  dbSessionId,
  database = null,
  onCreated,
}: CreateSchemaDialogProps) {
  const { t } = useI18n();

  return (
    <AdminCreateDialog
      open={open}
      onClose={onClose}
      dbSessionId={dbSessionId}
      command="create_schema"
      titleKey="common.createSchema"
      nameLabelKey="createSchema.name"
      namePlaceholder="my_schema"
      fieldLabels={{
        owner: t('createSchema.owner'),
      }}
      database={database}
      onCreated={onCreated ? () => onCreated() : undefined}
    />
  );
}
