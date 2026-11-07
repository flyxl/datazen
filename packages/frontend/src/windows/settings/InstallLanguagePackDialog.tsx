import { useState } from 'react';
import { Badge } from '../../components/ui/Badge';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { useI18n } from '../../hooks/useI18n';
import { languagePackCommands, type LanguagePackPreview } from '../../commands/languagePacks';
import { CopyableError } from '../../components/ui/CopyableError';
import { useLanguagePackStore } from '../../stores/languagePackStore';

export interface InstallLanguagePackDialogProps {
  open: boolean;
  onClose: () => void;
}

type InstallStep = 'select' | 'review';

/**
 * Install an external language pack from a local `.zip` package.
 *
 * Two-step flow (mirror of `InstallWappDialog`): native pick → inspect
 * (validate only, nothing written) → review label/code/version/message
 * counts per domain → explicit confirmation performs the actual install.
 * Filesystem paths never cross IPC.
 */
export function InstallLanguagePackDialog({ open, onClose }: InstallLanguagePackDialogProps) {
  const { t } = useI18n();
  const [step, setStep] = useState<InstallStep>('select');
  const [pickToken, setPickToken] = useState<string | null>(null);
  const [packageLabel, setPackageLabel] = useState('');
  const [preview, setPreview] = useState<LanguagePackPreview | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const backToSelect = () => {
    setStep('select');
    setPickToken(null);
    setPackageLabel('');
    setPreview(null);
    setError(null);
  };

  /** Step 1 → 2: native picker + validate-only inspection; no side effects on disk. */
  const handlePick = async () => {
    if (inspecting || installing) return;
    setInspecting(true);
    setError(null);
    try {
      const picked = await languagePackCommands.inspectLanguagePackFileWithDialog('zip');
      if (!picked) {
        return;
      }
      setPickToken(picked.pickToken);
      setPackageLabel(picked.packageLabel);
      setPreview(picked);
      setStep('review');
    } catch (e) {
      setPickToken(null);
      setPackageLabel('');
      setPreview(null);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setInspecting(false);
    }
  };

  /** Step 2: the user confirmed — now write the package. */
  const handleInstall = async () => {
    if (!pickToken || !preview || installing) return;
    setInstalling(true);
    setError(null);
    try {
      await languagePackCommands.installLanguagePack(pickToken);
      // The `language-packs:changed` broadcast refreshes every window's store;
      // this explicit refetch covers the installing window as well.
      await useLanguagePackStore.getState().fetch();
      backToSelect();
      onClose();
    } catch (e) {
      backToSelect();
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setInstalling(false);
    }
  };

  const body =
    step === 'review' && preview ? (
      <div className="space-y-3" data-testid="language-pack-review">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm font-medium text-fg">{preview.manifest.label}</span>
          <Badge>{preview.manifest.code}</Badge>
          <Badge>{`v${preview.manifest.version}`}</Badge>
          <span className="text-xs text-fg-muted">
            {t('settings.languagePacks.install.messages', {
              count: preview.messageCount,
            })}
          </span>
        </div>
        {preview.manifest.author && (
          <p className="text-xs text-fg-muted">
            {t('settings.languagePacks.install.author', { author: preview.manifest.author })}
          </p>
        )}
        <div>
          <p className="mb-1 text-xs font-medium text-fg-secondary">
            {t('settings.languagePacks.install.domains')}
          </p>
          <div className="flex flex-wrap gap-1.5" data-testid="language-pack-review-domains">
            {preview.domains.map((d) => (
              <Badge key={d.domain} data-testid={`language-pack-domain-${d.domain}`}>
                {`${d.domain} · ${d.messageCount}`}
              </Badge>
            ))}
          </div>
        </div>
        <p className="text-xs text-fg-muted" data-testid="language-pack-review-file">
          {packageLabel}
        </p>
      </div>
    ) : (
      <div className="space-y-3">
        <Button
          data-testid="language-pack-install-pick"
          onClick={() => void handlePick()}
          disabled={inspecting || installing}
        >
          {inspecting
            ? t('settings.languagePacks.install.inspecting')
            : t('settings.languagePacks.install.pick')}
        </Button>
        {error && (
          <CopyableError message={error} copyButton data-testid="language-pack-install-error" />
        )}
      </div>
    );

  return (
    <Dialog
      open={open}
      title={t('settings.languagePacks.install.title')}
      description={t('settings.languagePacks.install.description')}
      onClose={onClose}
      footer={
        step === 'review' ? (
          <>
            <Button
              variant="ghost"
              data-testid="language-pack-install-back"
              onClick={backToSelect}
              disabled={installing}
            >
              {t('settings.languagePacks.install.back')}
            </Button>
            <Button
              data-testid="language-pack-install-confirm"
              onClick={() => void handleInstall()}
              disabled={!preview || !pickToken || installing}
            >
              {installing
                ? t('settings.languagePacks.install.installing')
                : t('settings.languagePacks.install.confirm')}
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
      {body}
    </Dialog>
  );
}
