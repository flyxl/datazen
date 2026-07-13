import { FileKey2, KeyRound } from 'lucide-react';
import { Input } from '../ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { Label } from './shared';
import type { ConnectionFormState } from './useConnectionForm';

export interface SshTunnelFieldsProps {
  form: ConnectionFormState;
  /** Window variant uses bg-surface-alt for inner panel; dialog uses bg-surface */
  innerPanelClassName?: string;
}

export function SshTunnelFields({ form, innerPanelClassName = 'bg-surface' }: SshTunnelFieldsProps) {
  const { t } = useI18n();
  if (!form.meta.supportsSSH) return null;

  return (
    <div>
      <label className="flex items-center gap-2 text-sm text-fg-secondary">
        <input
          type="checkbox"
          checked={form.sshEnabled}
          onChange={(e) => form.setSshEnabled(e.target.checked)}
          className="h-4 w-4 rounded border-edge bg-surface text-blue-500 focus:ring-blue-500/25"
        />
        {t('newConn.sshTunnel')}
      </label>

      {form.sshEnabled && (
        <div className={cn('mt-3 grid grid-cols-1 gap-3 rounded-md border border-edge p-3 md:grid-cols-2', innerPanelClassName)}>
          <div>
            <Label required>{t('newConn.sshHost')}</Label>
            <Input
              value={form.sshHost}
              onChange={(e) => form.setSshHost(e.target.value)}
              placeholder="ssh.example.com"
            />
          </div>
          <div>
            <Label required>{t('newConn.sshPort')}</Label>
            <Input
              value={form.sshPort}
              onChange={(e) => form.setSshPort(e.target.value)}
              placeholder="22"
            />
          </div>
          <div className="md:col-span-2">
            <Label required>{t('newConn.sshUsername')}</Label>
            <Input
              value={form.sshUsername}
              onChange={(e) => form.setSshUsername(e.target.value)}
              placeholder="root"
              onKeyDown={form.tabFill(form.setSshUsername)}
            />
          </div>

          <div className="md:col-span-2">
            <Label required>{t('newConn.authMethod')}</Label>
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => form.setSshAuthMethod('password')}
                className={cn(
                  'flex flex-1 items-center justify-center gap-1.5 rounded-md border px-3 py-2 text-xs transition-colors',
                  form.sshAuthMethod === 'password'
                    ? 'border-blue-500 bg-blue-500/10 text-blue-400'
                    : 'border-edge bg-surface text-fg-secondary',
                )}
              >
                <KeyRound className="h-3.5 w-3.5" />
                {t('newConn.authPassword')}
              </button>
              <button
                type="button"
                onClick={() => form.setSshAuthMethod('private_key')}
                className={cn(
                  'flex flex-1 items-center justify-center gap-1.5 rounded-md border px-3 py-2 text-xs transition-colors',
                  form.sshAuthMethod === 'private_key'
                    ? 'border-blue-500 bg-blue-500/10 text-blue-400'
                    : 'border-edge bg-surface text-fg-secondary',
                )}
              >
                <FileKey2 className="h-3.5 w-3.5" />
                {t('newConn.authKey')}
              </button>
            </div>
          </div>

          {form.sshAuthMethod === 'password' ? (
            <div className="md:col-span-2">
              <Label required>{t('newConn.sshPassword')}</Label>
              <Input
                type="password"
                value={form.sshPassword}
                onChange={(e) => form.setSshPassword(e.target.value)}
              />
            </div>
          ) : (
            <>
              <div className="md:col-span-2">
                <Label required>{t('newConn.privateKey')}</Label>
                <Input
                  value={form.sshKeyPath}
                  onChange={(e) => form.setSshKeyPath(e.target.value)}
                  placeholder="~/.ssh/id_rsa"
                  onKeyDown={form.tabFill(form.setSshKeyPath)}
                />
              </div>
              <div className="md:col-span-2">
                <Label>{t('newConn.passphrase')}</Label>
                <Input
                  type="password"
                  value={form.sshPassphrase}
                  onChange={(e) => form.setSshPassphrase(e.target.value)}
                  placeholder={t('newConn.passphraseHint')}
                />
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
