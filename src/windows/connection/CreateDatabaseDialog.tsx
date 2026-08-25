import { useCallback, useState } from 'react';
import { Loader2 } from 'lucide-react';
import { Dialog } from '../../components/ui/Dialog';
import { CopyableError } from '../../components/ui/CopyableError';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { AdminSchemaFields } from '../../components/admin/AdminSchemaFields';
import { driverCommands } from '../../commands/driver';
import { useConnectionCommand } from '../../hooks/useConnectionCommand';
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
  const { definition } = useConnectionCommand(open ? dbSessionId : undefined, 'create_database');
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

      await driverCommands.execute({ dbSessionId, command: 'create_database', input });
      const created = name.trim();
      resetForm();
      await onCreated?.(created);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }, [name, optionalValues, dbSessionId, resetForm, onCreated, onClose]);

  return (
    <Dialog
      open={open}
      title={t('createDb.title')}
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
            {t('createDb.create')}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <label className="block text-xs text-fg-muted mb-1">{t('createDb.name')}</label>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void handleCreate();
            }}
            placeholder="my_database"
            autoFocus
          />
        </div>
        <AdminSchemaFields
          definition={definition}
          values={optionalValues}
          onChange={handleOptionalChange}
          exclude={['name']}
          labels={{
            encoding: t('createDb.encoding'),
            owner: t('createDb.owner'),
          }}
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
