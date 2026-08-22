import { useState } from 'react';
import { Button } from '../../components/ui/Button';
import { CopyableError } from '../../components/ui/CopyableError';
import { Dialog } from '../../components/ui/Dialog';
import { PathInput } from '../../components/ui/PathInput';
import { useI18n } from '../../hooks/useI18n';
import { pluginCommands } from '../../commands/plugins';
import { usePluginStore } from '../../stores/pluginStore';
import type { PluginSummary } from '../../types/plugin';

export interface InstallPluginDialogProps {
  open: boolean;
  onClose: () => void;
  /** Called after a successful install and store refresh. */
  onInstalled?: (plugin: PluginSummary) => void;
}

/** Install a UI plugin from a local `.zip` package path. */
export function InstallPluginDialog({ open, onClose, onInstalled }: InstallPluginDialogProps) {
  const { t } = useI18n();
  const [path, setPath] = useState('');
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleInstall = async () => {
    if (!path.trim() || installing) return;
    setInstalling(true);
    setError(null);
    try {
      const installed = await pluginCommands.installPluginFromPath(path.trim());
      await usePluginStore.getState().fetch();
      setPath('');
      onClose();
      onInstalled?.(installed);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setInstalling(false);
    }
  };

  return (
    <Dialog
      open={open}
      title={t('plugins.install.title')}
      description={t('plugins.install.description')}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={installing}>
            {t('common.cancel')}
          </Button>
          <Button
            data-testid="plugin-install-confirm"
            onClick={() => void handleInstall()}
            disabled={!path.trim() || installing}
          >
            {installing ? t('plugins.install.installing') : t('plugins.install.confirm')}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <label className="flex flex-col gap-1.5">
          <span className="text-xs font-medium text-fg-secondary">
            {t('plugins.install.pathLabel')}
          </span>
          <PathInput
            value={path}
            onChange={(v) => {
              setPath(v);
              setError(null);
            }}
            placeholder={t('plugins.install.pathPlaceholder')}
            dialogOptions={{
              filters: [{ name: 'Plugin package', extensions: ['zip'] }],
            }}
          />
        </label>
        {error ? (
          <CopyableError
            message={error}
            copyButton
            data-testid="plugin-install-error"
            className="text-sm text-red-400"
          />
        ) : null}
      </div>
    </Dialog>
  );
}
