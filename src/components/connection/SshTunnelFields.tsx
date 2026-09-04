import { FileKey2, KeyRound, Shield } from 'lucide-react';
import type { SshAuthMethod } from '../../types';
import { Input } from '../ui/Input';
import { PathInput } from '../ui/PathInput';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { Label } from './shared';
import type { ConnectionFormState } from './useConnectionForm';

export interface SshTunnelFieldsProps {
  form: ConnectionFormState;
  /** Window variant uses bg-surface-alt for inner panel; dialog uses bg-surface */
  innerPanelClassName?: string;
}

export function SshTunnelFields({
  form,
  innerPanelClassName = 'bg-surface',
}: SshTunnelFieldsProps) {
  const { t } = useI18n();
  if (!form.supportsSSH) return null;

  return (
    <div>
      <label
        data-testid="new-conn-ssh-tunnel"
        className="flex items-center gap-2 text-sm text-fg-secondary"
      >
        <input
          type="checkbox"
          data-testid="new-conn-ssh-tunnel-checkbox"
          checked={form.sshEnabled}
          onChange={(e) => form.setSshEnabled(e.target.checked)}
          className="h-4 w-4 rounded border-edge bg-surface accent-accent focus:ring-accent/25"
        />
        {t('newConn.sshTunnel')}
      </label>

      {form.sshEnabled && (
        <div
          className={cn(
            'mt-3 grid grid-cols-1 gap-3 rounded-md border border-edge p-3 md:grid-cols-2',
            innerPanelClassName,
          )}
        >
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
              {[
                {
                  id: 'password' as SshAuthMethod,
                  icon: KeyRound,
                  label: t('newConn.authPassword'),
                },
                { id: 'private_key' as SshAuthMethod, icon: FileKey2, label: t('newConn.authKey') },
                { id: 'agent' as SshAuthMethod, icon: Shield, label: t('newConn.authAgent') },
              ].map((opt) => (
                <button
                  key={opt.id}
                  type="button"
                  onClick={() => form.setSshAuthMethod(opt.id)}
                  className={cn(
                    'flex flex-1 items-center justify-center gap-1.5 rounded-md border px-3 py-2 text-xs transition-colors',
                    form.sshAuthMethod === opt.id
                      ? 'border-accent bg-accent/10 text-accent'
                      : 'border-edge bg-surface text-fg-secondary',
                  )}
                >
                  <opt.icon className="h-3.5 w-3.5" />
                  {opt.label}
                </button>
              ))}
            </div>
          </div>

          {form.sshAuthMethod === 'agent' ? (
            <div className="md:col-span-2 text-[11px] text-fg-muted">
              {t('newConn.authAgentHint')}
            </div>
          ) : form.sshAuthMethod === 'password' ? (
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
                <PathInput
                  value={form.sshKeyPath}
                  onChange={form.setSshKeyPath}
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

          <div className="md:col-span-2">
            <label className="flex items-center gap-2 text-sm text-fg-secondary">
              <input
                type="checkbox"
                checked={form.sshJumpEnabled}
                onChange={(e) => form.setSshJumpEnabled(e.target.checked)}
                className="h-4 w-4 rounded border-edge bg-surface accent-accent focus:ring-accent/25"
              />
              {t('newConn.sshJump')}
            </label>
          </div>

          {form.sshJumpEnabled && (
            <>
              <div>
                <Label required>{t('newConn.sshJumpHost')}</Label>
                <Input
                  value={form.sshJumpHost}
                  onChange={(e) => form.setSshJumpHost(e.target.value)}
                  placeholder="bastion.example.com"
                />
              </div>
              <div>
                <Label required>{t('newConn.sshJumpPort')}</Label>
                <Input
                  value={form.sshJumpPort}
                  onChange={(e) => form.setSshJumpPort(e.target.value)}
                  placeholder="22"
                />
              </div>
              <div className="md:col-span-2">
                <Label required>{t('newConn.sshJumpUsername')}</Label>
                <Input
                  value={form.sshJumpUsername}
                  onChange={(e) => form.setSshJumpUsername(e.target.value)}
                  placeholder="ubuntu"
                />
              </div>
              <div className="md:col-span-2">
                <div className="flex gap-2">
                  {[
                    { id: 'password' as SshAuthMethod, label: t('newConn.authPassword') },
                    { id: 'private_key' as SshAuthMethod, label: t('newConn.authKey') },
                    { id: 'agent' as SshAuthMethod, label: t('newConn.authAgent') },
                  ].map((opt) => (
                    <button
                      key={opt.id}
                      type="button"
                      onClick={() => form.setSshJumpAuthMethod(opt.id)}
                      className={cn(
                        'flex flex-1 items-center justify-center rounded-md border px-3 py-2 text-xs transition-colors',
                        form.sshJumpAuthMethod === opt.id
                          ? 'border-accent bg-accent/10 text-accent'
                          : 'border-edge bg-surface text-fg-secondary',
                      )}
                    >
                      {opt.label}
                    </button>
                  ))}
                </div>
              </div>
              {form.sshJumpAuthMethod === 'password' && (
                <div className="md:col-span-2">
                  <Label required>{t('newConn.sshPassword')}</Label>
                  <Input
                    type="password"
                    value={form.sshJumpPassword}
                    onChange={(e) => form.setSshJumpPassword(e.target.value)}
                  />
                </div>
              )}
              {form.sshJumpAuthMethod === 'private_key' && (
                <>
                  <div className="md:col-span-2">
                    <Label required>{t('newConn.privateKey')}</Label>
                    <PathInput
                      value={form.sshJumpKeyPath}
                      onChange={form.setSshJumpKeyPath}
                      placeholder="~/.ssh/id_rsa"
                    />
                  </div>
                  <div className="md:col-span-2">
                    <Label>{t('newConn.passphrase')}</Label>
                    <Input
                      type="password"
                      value={form.sshJumpPassphrase}
                      onChange={(e) => form.setSshJumpPassphrase(e.target.value)}
                    />
                  </div>
                </>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
