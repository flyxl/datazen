import { useCallback, useState } from 'react';
import { FolderPlus, Loader2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { driverCommands } from '../../commands/driver';
import { useSchemaStore } from '../../stores/schemaStore';
import { useI18n } from '../../hooks/useI18n';
import type { DatabaseType } from '../../types';

interface CreateSchemaPanelProps {
  connectionId: string;
  databaseType: DatabaseType;
}

export function CreateSchemaPanel({ connectionId, databaseType }: CreateSchemaPanelProps) {
  const { t } = useI18n();
  const [name, setName] = useState('');
  const [owner, setOwner] = useState('');
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const loadForConnection = useSchemaStore((s) => s.loadForConnection);

  const handleCreate = useCallback(async () => {
    if (!name.trim()) return;
    setRunning(true);
    setError(null);
    setSuccess(null);
    try {
      const input: Record<string, unknown> = { name: name.trim() };
      if (owner.trim()) input.owner = owner.trim();

      await driverCommands.execute({
        connectionId,
        command: 'create_schema',
        input,
      });
      setSuccess(t('createSchema.success', { name: name.trim() }));
      setName('');
      setOwner('');
      void loadForConnection(connectionId, { databaseType });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }, [name, owner, connectionId, databaseType, loadForConnection, t]);

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
        {databaseType === 'postgresql' && (
          <div>
            <label className="block text-xs text-fg-muted mb-1">{t('createSchema.owner')}</label>
            <Input
              value={owner}
              onChange={(e) => setOwner(e.target.value)}
              placeholder="postgres"
            />
          </div>
        )}

        {error && <div className="rounded bg-red-500/10 p-3 text-sm text-red-400">{error}</div>}
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
