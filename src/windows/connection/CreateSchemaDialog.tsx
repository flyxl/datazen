import { useCallback, useState } from 'react';
import { Loader2 } from 'lucide-react';
import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { driverCommands } from '../../commands/driver';
import { useI18n } from '../../hooks/useI18n';
import type { DatabaseType } from '../../types';

interface CreateSchemaDialogProps {
  open: boolean;
  onClose: () => void;
  connectionId: string;
  databaseType: DatabaseType;
  onCreated?: () => void | Promise<void>;
}

export function CreateSchemaDialog({
  open,
  onClose,
  connectionId,
  databaseType,
  onCreated,
}: CreateSchemaDialogProps) {
  const { t } = useI18n();
  const [name, setName] = useState('');
  const [owner, setOwner] = useState('');
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const resetForm = useCallback(() => {
    setName('');
    setOwner('');
    setError(null);
  }, []);

  const handleClose = useCallback(() => {
    resetForm();
    onClose();
  }, [resetForm, onClose]);

  const handleCreate = useCallback(async () => {
    if (!name.trim()) return;
    setRunning(true);
    setError(null);
    try {
      const input: Record<string, unknown> = { name: name.trim() };
      if (owner.trim()) input.owner = owner.trim();

      await driverCommands.execute({ connectionId, command: 'create_schema', input });
      resetForm();
      await onCreated?.();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }, [name, owner, connectionId, resetForm, onCreated, onClose]);

  return (
    <Dialog
      open={open}
      title={t('createSchema.title')}
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
            {t('createSchema.create')}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <label className="block text-xs text-fg-muted mb-1">{t('createSchema.name')}</label>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void handleCreate();
            }}
            placeholder="my_schema"
            autoFocus
          />
        </div>
        {(databaseType === 'postgresql' || (databaseType as string) === 'sqlserver') && (
          <div>
            <label className="block text-xs text-fg-muted mb-1">{t('createSchema.owner')}</label>
            <Input
              value={owner}
              onChange={(e) => setOwner(e.target.value)}
              placeholder={databaseType === 'postgresql' ? 'postgres' : 'dbo'}
            />
          </div>
        )}
        {error && <div className="rounded bg-red-500/10 p-3 text-sm text-red-400">{error}</div>}
      </div>
    </Dialog>
  );
}
