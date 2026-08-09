import { useCallback, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import {
  checkForUpdates,
  downloadAndInstallUpdate,
  isUpdaterSupported,
  type UpdateProgress,
} from '../../lib/updater';

function ToggleRow({ label, checked, onChange }: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between">
      <div className="text-sm text-fg-secondary">{label}</div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors ${
          checked ? 'bg-accent' : 'bg-edge'
        }`}
      >
        <span
          className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${
            checked ? 'translate-x-[18px]' : 'translate-x-[3px]'
          }`}
        />
      </button>
    </div>
  );
}

function progressLabel(progress: UpdateProgress, t: (key: import('../../locales').TranslationKey) => string): string {
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
      return t('settings.updater.installed').replace('{version}', progress.version);
    default:
      return '';
  }
}

export function UpdateSection({
  checkOnStartup,
  onCheckOnStartupChange,
}: {
  checkOnStartup: boolean;
  onCheckOnStartupChange: (v: boolean) => void;
}) {
  const { t } = useI18n();
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const settings = useSettingsStore((s) => s.settings);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

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
      setStatus(t('settings.updater.installed').replace('{version}', result.version));
    } else if (result.status === 'error') {
      setError(result.message);
      setStatus(null);
    }
    setBusy(false);
  }, [t]);

  const handleStartupToggle = async (enabled: boolean) => {
    onCheckOnStartupChange(enabled);
    await updateSettings({ ...settings, checkForUpdatesOnStartup: enabled });
  };

  if (!isUpdaterSupported()) {
    return null;
  }

  return (
    <>
      <h2 className="text-[13px] font-semibold uppercase tracking-wider text-fg-muted">
        {t('settings.updater.title')}
      </h2>
      <p className="text-xs text-fg-muted">{t('settings.updater.description')}</p>

      <ToggleRow
        label={t('settings.updater.checkOnStartup')}
        checked={checkOnStartup}
        onChange={(v) => void handleStartupToggle(v)}
      />

      <div className="flex flex-wrap items-center gap-3">
        <Button variant="secondary" disabled={busy} onClick={() => void handleCheck()}>
          {t('settings.updater.check')}
        </Button>
        <Button variant="primary" disabled={busy} onClick={() => void handleDownload()}>
          {t('settings.updater.downloadInstall')}
        </Button>
      </div>

      {status && <p className="text-xs text-fg-secondary">{status}</p>}
      {error && <p className="text-xs text-red-500">{error}</p>}
    </>
  );
}
