import { useCallback, useState } from 'react';
import { FolderPlus, Loader2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { CopyableError } from '../../components/ui/CopyableError';
import { Input } from '../../components/ui/Input';
import { AdminSchemaFields } from '../../components/admin/AdminSchemaFields';
import { driverCommands } from '../../commands/driver';
import { useConnectionCommand } from '../../hooks/useConnectionCommand';
import { useSchemaStore } from '../../stores/schemaStore';
import { useI18n } from '../../hooks/useI18n';
import type { DatabaseType } from '../../types';

interface CreateSchemaPanelProps {
  connectionId: string;
  databaseType: DatabaseType;
}

export function CreateSchemaPanel({ connectionId, databaseType }: CreateSchemaPanelProps) {
  const { t } = useI18n();
  const { definition } = useConnectionCommand(connectionId, 'create_schema');
  const [name, setName] = useState('');
  const [optionalValues, setOptionalValues] = useState<Record<string, string>>({});
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const loadForConnection = useSchemaStore((s) => s.loadForConnection);

  const handleOptionalChange = useCallback((field: string, value: string) => {
    setOptionalValues((prev) => ({ ...prev, [field]: value }));
  }, []);

  const handleCreate = useCallback(async () => {
    if (!name.trim()) return;
    setRunning(true);
    setError(null);
    setSuccess(null);
    try {
      const input: Record<string, unknown> = { name: name.trim() };
      for (const [field, value] of Object.entries(optionalValues)) {
        if (value.trim()) input[field] = value.trim();
      }

      await driverCommands.execute({
        dbSessionId: connectionId,
        command: 'create_schema',
        input,
      });
      setSuccess(t('createSchema.success', { name: name.trim() }));
      setName('');
      setOptionalValues({});
      void loadForConnection(connectionId, { databaseType });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }, [name, optionalValues, connectionId, databaseType, loadForConnection, t]);

  return (
    <div className="flex flex-1 flex-col overflow-auto">
      <div className="flex shrink-0 items-center gap-2 border-b border-edge px-4 py-2">
        <FolderPlus className="h-4 w-4 text-emerald-400" />
        <span className="text-sm font-medium">{t('createSchema.title')}</span>
      </div>
      <div className="mx-auto w-full max-w-lg p-6 space-y-4">
        <div>
          <label className="block text-xs text-fg-muted mb-1">{t('createSchema.name')}</label>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="my_schema"
            autoFocus
          />
        </div>
        <AdminSchemaFields
          definition={definition}
          values={optionalValues}
          onChange={handleOptionalChange}
          exclude={['name']}
          labels={{
            owner: t('createSchema.owner'),
          }}
        />

        {error && (
          <CopyableError
            message={error}
            className="rounded bg-red-500/10 p-3 text-sm text-red-400"
          />
        )}
        {success && (
          <div className="rounded bg-green-500/10 p-3 text-sm text-green-400">{success}</div>
        )}

        <Button
          variant="primary"
          onClick={() => void handleCreate()}
          disabled={!name.trim() || running}
        >
          {running && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
          {t('createSchema.create')}
        </Button>
      </div>
    </div>
  );
}
