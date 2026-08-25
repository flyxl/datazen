import { useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2 } from 'lucide-react';
import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { CopyableError } from '../../components/ui/CopyableError';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { PrivilegeSelector, usePrivilegeOptions } from '../../components/admin/PrivilegeSelector';
import { driverCommands } from '../../commands/driver';
import { databaseCommands } from '../../commands/database';
import { useConnectionCommand } from '../../hooks/useConnectionCommand';
import { hasSchemaField } from '../../lib/commandSchema';
import { useI18n } from '../../hooks/useI18n';

interface CreateUserDialogProps {
  open: boolean;
  onClose: () => void;
  dbSessionId: string;
  onCreated?: (username: string) => void;
}

export function CreateUserDialog({
  open,
  onClose,
  dbSessionId,
  onCreated,
}: CreateUserDialogProps) {
  const { t } = useI18n();
  const { definition: grantDefinition } = useConnectionCommand(
    open ? dbSessionId : undefined,
    'grant_privileges',
  );
  const { all: allPrivileges } = usePrivilegeOptions(grantDefinition);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [selectedPrivileges, setSelectedPrivileges] = useState<Set<string>>(new Set());
  const [grantOption, setGrantOption] = useState(false);
  const [targetDatabase, setTargetDatabase] = useState('');
  const [databases, setDatabases] = useState<string[]>([]);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [step, setStep] = useState<'create' | 'grant'>('create');
  const [createdUsername, setCreatedUsername] = useState('');

  const showGrantOption = hasSchemaField(grantDefinition, 'grantOption');

  useEffect(() => {
    if (open && dbSessionId) {
      databaseCommands
        .getDatabases(dbSessionId)
        .then(setDatabases)
        .catch(() => {});
    }
  }, [open, dbSessionId]);

  const resetForm = useCallback(() => {
    setUsername('');
    setPassword('');
    setSelectedPrivileges(new Set());
    setGrantOption(false);
    setTargetDatabase('');
    setError(null);
    setStep('create');
    setCreatedUsername('');
  }, []);

  const handleClose = useCallback(() => {
    resetForm();
    onClose();
  }, [resetForm, onClose]);

  const togglePrivilege = useCallback((priv: string) => {
    setSelectedPrivileges((prev) => {
      const next = new Set(prev);
      if (next.has(priv)) next.delete(priv);
      else next.add(priv);
      return next;
    });
  }, []);

  const selectAllPrivileges = useCallback(() => {
    setSelectedPrivileges((prev) => {
      if (prev.size === allPrivileges.length) return new Set();
      return new Set(allPrivileges);
    });
  }, [allPrivileges]);

  const handleCreate = useCallback(async () => {
    if (!username.trim()) return;
    setRunning(true);
    setError(null);
    try {
      const input: Record<string, unknown> = { username: username.trim() };
      if (password) input.password = password;
      await driverCommands.execute({ dbSessionId, command: 'create_user', input });
      setCreatedUsername(username.trim());
      setStep('grant');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }, [username, password, dbSessionId]);

  const handleGrant = useCallback(async () => {
    if (selectedPrivileges.size === 0) return;
    setRunning(true);
    setError(null);
    try {
      await driverCommands.execute({
        dbSessionId,
        command: 'grant_privileges',
        input: {
          username: createdUsername,
          database: targetDatabase,
          privileges: [...selectedPrivileges],
          grantOption,
        },
      });
      resetForm();
      onCreated?.(createdUsername);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }, [
    selectedPrivileges,
    dbSessionId,
    createdUsername,
    targetDatabase,
    grantOption,
    resetForm,
    onCreated,
    onClose,
  ]);

  const handleSkipGrant = useCallback(() => {
    const name = createdUsername;
    resetForm();
    onCreated?.(name);
    onClose();
  }, [createdUsername, resetForm, onCreated, onClose]);

  const showGrantStep = useMemo(
    () => allPrivileges.length > 0 || hasSchemaField(grantDefinition, 'database'),
    [allPrivileges.length, grantDefinition],
  );

  return (
    <Dialog
      open={open}
      title={step === 'create' ? t('createUser.title') : t('createUser.grantPrivileges')}
      onClose={handleClose}
      className="max-w-md"
      footer={
        step === 'create' ? (
          <>
            <Button variant="ghost" onClick={handleClose}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={() => void handleCreate()}
              disabled={!username.trim() || running}
            >
              {running && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {t('createUser.create')}
            </Button>
          </>
        ) : (
          <>
            <Button variant="ghost" onClick={handleSkipGrant}>
              {t('common.skip') ?? 'Skip'}
            </Button>
            <Button
              variant="primary"
              onClick={() => void handleGrant()}
              disabled={selectedPrivileges.size === 0 || running}
            >
              {running && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {t('createUser.createAndGrant')}
            </Button>
          </>
        )
      }
    >
      {step === 'create' ? (
        <div className="space-y-4">
          <div>
            <label className="block text-xs text-fg-muted mb-1">{t('createUser.username')}</label>
            <Input
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void handleCreate();
              }}
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
              onKeyDown={(e) => {
                if (e.key === 'Enter') void handleCreate();
              }}
              placeholder="••••••••"
            />
          </div>
          {error && (
            <CopyableError
              message={error}
              className="rounded bg-red-500/10 p-3 text-sm text-red-400"
            />
          )}
        </div>
      ) : showGrantStep ? (
        <div className="space-y-4">
          <div className="rounded bg-green-500/10 p-2 text-sm text-green-400">
            {t('createUser.success').replace('{name}', createdUsername)}
          </div>
          <div>
            <label className="block text-xs text-fg-muted mb-1">{t('createUser.database')}</label>
            <Select
              value={targetDatabase}
              onChange={setTargetDatabase}
              placeholder={t('createUser.allDatabases')}
              options={[
                { value: '', label: t('createUser.allDatabases') },
                ...databases.map((db) => ({ value: db, label: db })),
              ]}
            />
          </div>
          {allPrivileges.length > 0 && (
            <div>
              <div className="flex items-center justify-between mb-2">
                <label className="text-xs text-fg-muted">{t('createUser.privileges')}</label>
                <button
                  type="button"
                  className="text-xs text-accent hover:underline"
                  onClick={selectAllPrivileges}
                >
                  {t('createUser.selectAll')}
                </button>
              </div>
              <PrivilegeSelector
                definition={grantDefinition}
                selected={selectedPrivileges}
                onToggle={togglePrivilege}
              />
            </div>
          )}
          {showGrantOption && (
            <label className="flex items-center gap-2 text-sm text-fg cursor-pointer">
              <input
                type="checkbox"
                checked={grantOption}
                onChange={(e) => setGrantOption(e.target.checked)}
                className="rounded border-edge"
              />
              {t('createUser.grantOption')}
            </label>
          )}
          {error && (
            <CopyableError
              message={error}
              className="rounded bg-red-500/10 p-3 text-sm text-red-400"
            />
          )}
        </div>
      ) : (
        <div className="space-y-4">
          <div className="rounded bg-green-500/10 p-2 text-sm text-green-400">
            {t('createUser.success').replace('{name}', createdUsername)}
          </div>
          {error && (
            <CopyableError
              message={error}
              className="rounded bg-red-500/10 p-3 text-sm text-red-400"
            />
          )}
        </div>
      )}
    </Dialog>
  );
}
