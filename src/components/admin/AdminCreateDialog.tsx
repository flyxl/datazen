import { useCallback, useState } from 'react';
import { Loader2 } from 'lucide-react';
import { Dialog } from '../ui/Dialog';
import { CopyableError } from '../ui/CopyableError';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { AdminSchemaFields } from './AdminSchemaFields';
import { driverCommands } from '../../commands/driver';
import { useConnectionCommand } from '../../hooks/useConnectionCommand';
import { useI18n } from '../../hooks/useI18n';
import type { I18nKey } from '../../locales';
import { toErrorMessage } from '../../lib/errors';

export interface AdminCreateDialogProps {
  open: boolean;
  onClose: () => void;
  dbSessionId: string;
  command: string;
  /** Dialog title and primary button label (same i18n key) */
  titleKey: I18nKey;
  /** i18n key for the name field label */
  nameLabelKey: I18nKey;
  namePlaceholder: string;
  /** Optional field labels passed to AdminSchemaFields */
  fieldLabels?: Record<string, string>;
  /** Target catalog for create_schema (PG / SQL Server). */
  database?: string | null;
  onCreated?: (name: string) => void | Promise<void>;
}

export function AdminCreateDialog({
  open,
  onClose,
  dbSessionId,
  command,
  titleKey,
  nameLabelKey,
  namePlaceholder,
  fieldLabels,
  database = null,
  onCreated,
}: AdminCreateDialogProps) {
  const { t } = useI18n();
  const { definition } = useConnectionCommand(open ? dbSessionId : undefined, command);
  const [name, setName] = useState('');
  const [optionalValues, setOptionalValues] = useState<Record<string, string>>({});
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const resetForm = useCallback(() => {
    setName('');
    setOptionalValues({});
    setError(null);
  }, []);

  const handleClose = useCallback(() => {
    resetForm();
    onClose();
  }, [resetForm, onClose]);

  const handleOptionalChange = useCallback((field: string, value: string) => {
    setOptionalValues((prev) => ({ ...prev, [field]: value }));
  }, []);

  const handleCreate = useCallback(async () => {
    if (!name.trim()) return;
    setRunning(true);
    setError(null);
    try {
      const input: Record<string, unknown> = { name: name.trim() };
      for (const [field, value] of Object.entries(optionalValues)) {
        if (value.trim()) input[field] = value.trim();
      }

      await driverCommands.execute({
        dbSessionId,
        command,
        input,
        ...(database != null ? { database } : {}),
      });
      const created = name.trim();
      resetForm();
      await onCreated?.(created);
      onClose();
    } catch (err) {
      setError(toErrorMessage(err));
    } finally {
      setRunning(false);
    }
  }, [name, optionalValues, dbSessionId, command, database, resetForm, onCreated, onClose]);

  const title = t(titleKey);

  return (
    <Dialog
      open={open}
      title={title}
      onClose={handleClose}
      className="max-w-md"
      footer={
        <>
          <Button variant="ghost" onClick={handleClose}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            onClick={() => void handleCreate()}
            disabled={!name.trim() || running}
          >
            {running && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            {title}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <label className="block text-xs text-fg-muted mb-1">{t(nameLabelKey)}</label>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void handleCreate();
            }}
            placeholder={namePlaceholder}
            autoFocus
          />
        </div>
        <AdminSchemaFields
          definition={definition}
          values={optionalValues}
          onChange={handleOptionalChange}
          exclude={['name']}
          labels={fieldLabels}
        />
        {error && (
          <CopyableError
            message={error}
            className="rounded bg-red-500/10 p-3 text-sm text-red-400"
          />
        )}
      </div>
    </Dialog>
  );
}
