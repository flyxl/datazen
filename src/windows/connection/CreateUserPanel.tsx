import { useCallback, useState } from 'react';
import { Loader2, UserPlus } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { CopyableError } from '../../components/ui/CopyableError';
import { Input } from '../../components/ui/Input';
import { driverCommands } from '../../commands/driver';
import { useI18n } from '../../hooks/useI18n';

interface CreateUserPanelProps {
  dbSessionId: string;
}

export function CreateUserPanel({ dbSessionId }: CreateUserPanelProps) {
  const { t } = useI18n();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const handleCreate = useCallback(async () => {
    if (!username.trim()) return;
    setRunning(true);
    setError(null);
    setSuccess(null);
    try {
      const input: Record<string, unknown> = { username: username.trim() };
      if (password) input.password = password;

      await driverCommands.execute({
        dbSessionId,
        command: 'create_user',
        input,
      });
      setSuccess(t('createUser.success', { name: username.trim() }));
      setUsername('');
      setPassword('');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }, [username, password, dbSessionId, t]);

  return (
    <div className="flex flex-1 flex-col overflow-auto">
      <div className="flex shrink-0 items-center gap-2 border-b border-edge px-4 py-2">
        <UserPlus className="h-4 w-4 text-violet-400" />
        <span className="text-sm font-medium">{t('createUser.title')}</span>
      </div>
      <div className="mx-auto w-full max-w-lg p-6 space-y-4">
        <div>
          <label className="block text-xs text-fg-muted mb-1">{t('createUser.username')}</label>
          <Input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder="new_user"
            autoFocus
          />
        </div>
        <div>
          <label className="block text-xs text-fg-muted mb-1">{t('createUser.password')}</label>
          <Input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="••••••••"
          />
        </div>

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
          disabled={!username.trim() || running}
        >
          {running && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
          {t('createUser.create')}
        </Button>
      </div>
    </div>
  );
}
