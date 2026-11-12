import { useCallback, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { settingsCommands } from '../../commands/settings';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import {
  checkForUpdates,
  currentVariant,
  downloadAndInstallUpdate,
  getUpdateChannel,
  isUpdaterSupported,
  relaunchToApplyUpdate,
  type UpdateProgress,
} from '../../lib/updater';

import { SectionTitle } from './settingsUi';

/** GitHub Releases hosts every SKU's installer, including the ones with no updater channel. */
const RELEASES_URL = 'https://github.com/flyxl/datazen/releases';

function progressLabel(
  progress: UpdateProgress,
  t: (key: import('../../locales').TranslationKey) => string,
): string {
  switch (progress.phase) {
    case 'checking':
      return t('settings.updater.checking');
    case 'downloading':
      if (progress.total) {
        const pct = Math.min(100, Math.round((progress.downloaded / progress.total) * 100));
        return t('settings.updater.downloading').replace('{pct}', String(pct));
      }
      return t('settings.updater.downloadingIndeterminate');
    case 'installing':
      return t('settings.updater.installing');
    case 'done':
      return t('settings.updater.readyToRestart').replace('{version}', progress.version);
    default:
      return '';
  }
}

/**
 * Update card for builds that have no in-app updater channel.
 *
 * Returning `null` here (as this used to) hid the update story entirely from
 * every non-Basic SKU, which is exactly the confusion that let the "variant
 * silently replaced by Basic" bug go unnoticed: the user saw no update UI and
 * then, if they enabled startup checks on a build that *did* have one, got a
 * different SKU installed after a check. Naming the build and pointing at the matching
 * installer keeps the expectation honest.
 */
function ManualUpdateSection() {
  const { t } = useI18n();
  const variant = currentVariant();

  return (
    <>
      <SectionTitle hint={t('settings.updater.manualDescription')}>
        {t('settings.updater.title')}
      </SectionTitle>

      <p className="text-xs text-fg-muted">
        {t('settings.updater.manualVariant').replace('{variant}', variant)}
      </p>

      <div className="flex flex-wrap items-center gap-3">
        <Button variant="secondary" onClick={() => void settingsCommands.openPath(RELEASES_URL)}>
          {t('settings.updater.openReleases')}
        </Button>
      </div>
    </>
  );
}

export function UpdateSection() {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pendingRestartVersion, setPendingRestartVersion] = useState<string | null>(null);
  const [confirmRestart, confirmRestartDialog] = useConfirmDialog();
  const updateChannel = getUpdateChannel();

  const handleCheck = useCallback(async () => {
    if (!isUpdaterSupported()) {
      setError(t('settings.updater.unavailable'));
      return;
    }

    setBusy(true);
    setError(null);
    setStatus(t('settings.updater.checking'));

    const result = await checkForUpdates();
    if (result.status === 'upToDate') {
      setStatus(t('settings.updater.upToDate'));
    } else if (result.status === 'available') {
      setStatus(t('settings.updater.available').replace('{version}', result.version));
    } else if (result.status === 'error') {
      setError(result.message);
      setStatus(null);
    }
    setBusy(false);
  }, [t]);

  const handleRestart = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await relaunchToApplyUpdate();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  }, []);

  const handleDownload = useCallback(async () => {
    if (!isUpdaterSupported()) {
      setError(t('settings.updater.unavailable'));
      return;
    }

    setBusy(true);
    setError(null);

    const result = await downloadAndInstallUpdate((progress) => {
      setStatus(progressLabel(progress, t));
    });

    if (result.status === 'upToDate') {
      setStatus(t('settings.updater.upToDate'));
    } else if (result.status === 'installed') {
      setPendingRestartVersion(result.version);
      setStatus(t('settings.updater.readyToRestart').replace('{version}', result.version));
      const message = t('settings.updater.restartPrompt').replace('{version}', result.version);
      const restartNow = await confirmRestart({
        title: t('settings.updater.title'),
        message,
        confirmLabel: t('settings.updater.restartNow'),
        cancelLabel: t('settings.updater.restartLater'),
        kind: 'info',
      });
      if (restartNow) {
        try {
          await relaunchToApplyUpdate();
          return;
        } catch (e) {
          setError(e instanceof Error ? e.message : String(e));
        }
      }
      setStatus(t('settings.updater.restartLaterHint').replace('{version}', result.version));
    } else if (result.status === 'error') {
      setError(result.message);
      setStatus(null);
    }
    setBusy(false);
  }, [t]);

  if (updateChannel === 'none') {
    return null;
  }

  if (updateChannel === 'manual') {
    return <ManualUpdateSection />;
  }

  return (
    <>
      <SectionTitle hint={t('settings.updater.description')}>
        {t('settings.updater.title')}
      </SectionTitle>

      <div className="flex flex-wrap items-center gap-3">
        <Button variant="secondary" disabled={busy} onClick={() => void handleCheck()}>
          {t('settings.updater.check')}
        </Button>
        {!pendingRestartVersion ? (
          <Button variant="primary" disabled={busy} onClick={() => void handleDownload()}>
            {t('settings.updater.downloadInstall')}
          </Button>
        ) : (
          <Button variant="primary" disabled={busy} onClick={() => void handleRestart()}>
            {t('settings.updater.restartNow')}
          </Button>
        )}
      </div>

      {status && <p className="text-xs text-fg-secondary">{status}</p>}
      {error && <p className="select-text text-xs text-red-500">{error}</p>}
      {confirmRestartDialog}
    </>
  );
}
