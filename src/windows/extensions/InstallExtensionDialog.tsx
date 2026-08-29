import { useState } from 'react';
import { Badge } from '../../components/ui/Badge';
import { Button } from '../../components/ui/Button';
import { CopyableError } from '../../components/ui/CopyableError';
import { Dialog } from '../../components/ui/Dialog';
import { useI18n } from '../../hooks/useI18n';
import {
  extensionCommands,
  type ExtensionPackageKind,
} from '../../commands/extensions';
import { useExtensionStore } from '../../stores/extensionStore';
import type { ExtensionManifest, ExtensionSummary } from '../../types/extension';
import { PERMISSION_LABELS } from './permissionLabels';

export interface InstallExtensionDialogProps {
  open: boolean;
  onClose: () => void;
  /** Called after a successful install and store refresh. */
  onInstalled?: (plugin: ExtensionSummary) => void;
}

type InstallStep = 'select' | 'review';

/**
 * Install a UI plugin from a local `.zip` package or unpacked directory.
 *
 * Two-step flow per PRD §4.3/§8-Q1: native pick → inspect (validate only,
 * nothing written) → review name/version/author/permission badges → explicit
 * confirmation performs the actual install. Filesystem paths never cross IPC.
 */
export function InstallExtensionDialog({ open, onClose, onInstalled }: InstallExtensionDialogProps) {
  const { t } = useI18n();
  const [step, setStep] = useState<InstallStep>('select');
  const [pickToken, setPickToken] = useState<string | null>(null);
  const [packageLabel, setPackageLabel] = useState('');
  const [manifest, setManifest] = useState<ExtensionManifest | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const backToSelect = () => {
    setStep('select');
    setPickToken(null);
    setPackageLabel('');
    setManifest(null);
    setError(null);
  };

  /** Step 1 → 2: native picker + validate-only inspection; no side effects on disk. */
  const handlePick = async (packageKind: ExtensionPackageKind) => {
    if (inspecting || installing) return;
    setInspecting(true);
    setError(null);
    try {
      const preview = await extensionCommands.inspectExtensionPackageWithDialog(packageKind);
      if (!preview) {
        return;
      }
      setPickToken(preview.pickToken);
      setPackageLabel(preview.packageLabel);
      setManifest(preview.manifest);
      setStep('review');
    } catch (e) {
      setPickToken(null);
      setPackageLabel('');
      setManifest(null);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setInspecting(false);
    }
  };

  /** Step 2: the user confirmed — now write the package. */
  const handleInstall = async () => {
    if (!pickToken || !manifest || installing) return;
    setInstalling(true);
    setError(null);
    try {
      const installed = await extensionCommands.installExtension(pickToken);
      await useExtensionStore.getState().fetch();
      backToSelect();
      onClose();
      onInstalled?.(installed);
    } catch (e) {
      backToSelect();
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
        step === 'review' ? (
          <>
            <Button
              variant="ghost"
              data-testid="plugin-install-back"
              onClick={backToSelect}
              disabled={installing}
            >
              {t('plugins.install.back')}
            </Button>
            <Button
              data-testid="plugin-install-confirm"
              onClick={() => void handleInstall()}
              disabled={!manifest || !pickToken || installing}
            >
              {installing ? t('plugins.install.installing') : t('plugins.install.confirm')}
            </Button>
          </>
        ) : (
          <>
            <Button variant="ghost" onClick={onClose} disabled={inspecting}>
              {t('common.cancel')}
            </Button>
          </>
        )
      }
    >
      {step === 'review' && manifest ? (
        <div className="flex flex-col gap-3" data-testid="plugin-install-review">
          <div className="text-xs text-fg-muted" data-testid="plugin-install-package-label">
            {packageLabel}
          </div>
          <div>
            <div className="text-sm font-semibold text-fg">{manifest.name}</div>
            <div className="mt-0.5 text-xs text-fg-muted">
              v{manifest.version}
              {manifest.author ? ` · ${manifest.author}` : ''}
            </div>
          </div>
          {manifest.description ? (
            <p className="text-xs leading-relaxed text-fg-secondary">{manifest.description}</p>
          ) : null}
          <div className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-fg-secondary">
              {t('plugins.install.permissions')}
            </span>
            <div className="flex flex-wrap gap-1" data-testid="plugin-install-permissions">
              {manifest.permissions.length === 0 ? (
                <span className="text-xs text-fg-muted">{t('plugins.install.noPermissions')}</span>
              ) : (
                manifest.permissions.map((perm) => (
                  <Badge key={perm} title={PERMISSION_LABELS[perm] ?? perm}>
                    {perm}
                  </Badge>
                ))
              )}
            </div>
          </div>
          {error ? (
            <CopyableError
              message={error}
              copyButton
              data-testid="plugin-install-error"
              className="text-sm text-red-400"
            />
          ) : null}
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <p className="text-xs text-fg-secondary">{t('plugins.install.pickPrompt')}</p>
          <div className="flex flex-wrap gap-2">
            <Button
              data-testid="plugin-install-browse-zip"
              onClick={() => void handlePick('zip')}
              disabled={inspecting}
            >
              {inspecting ? t('plugins.install.inspecting') : t('plugins.install.browseZip')}
            </Button>
            <Button
              variant="secondary"
              data-testid="plugin-install-browse-folder"
              onClick={() => void handlePick('folder')}
              disabled={inspecting}
            >
              {inspecting ? t('plugins.install.inspecting') : t('plugins.install.browseFolder')}
            </Button>
          </div>
          {error ? (
            <CopyableError
              message={error}
              copyButton
              data-testid="plugin-install-error"
              className="text-sm text-red-400"
            />
          ) : null}
        </div>
      )}
    </Dialog>
  );
}
