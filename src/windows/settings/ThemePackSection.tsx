import { useCallback, useEffect, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { themeCommands } from '../../commands/theme';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import type { ThemePackSummary } from '../../types/themePack';

const BUILTIN_PACK_VALUE = '';
const MISSING_PACK_VALUE = '__missing__';

function SettingRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-start gap-4">
      <div className="w-32 shrink-0 pt-2">
        <div className="text-sm text-fg-secondary">{label}</div>
      </div>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

function isDialogCancelled(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return message.toLowerCase().includes('cancel');
}

export function ThemePackSection() {
  const { t } = useI18n();
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [packs, setPacks] = useState<ThemePackSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadPacks = useCallback(async () => {
    try {
      const list = await themeCommands.listThemePacks();
      setPacks(list);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadPacks();
  }, [loadPacks]);

  const packId = settings.theme.packId;
  const isOrphanPack = packId != null && !packs.some((p) => p.id === packId);
  const packOptions = [
    { value: BUILTIN_PACK_VALUE, label: t('settings.theme.packDefault') },
    ...(isOrphanPack && packId
      ? [{ value: MISSING_PACK_VALUE, label: t('settings.theme.packMissing', { id: packId }) }]
      : []),
    ...packs.map((p) => ({ value: p.id, label: p.name })),
  ];
  const selectValue = isOrphanPack ? MISSING_PACK_VALUE : (packId ?? BUILTIN_PACK_VALUE);

  const handlePackChange = async (value: string) => {
    const nextPackId =
      value === BUILTIN_PACK_VALUE || value === MISSING_PACK_VALUE ? null : value;
    const theme = useSettingsStore.getState().settings.theme;
    if (nextPackId === theme.packId) return;

    setError(null);
    try {
      await updateSettings({ theme: { ...theme, packId: nextPackId } });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleResetOrphan = async () => {
    const theme = useSettingsStore.getState().settings.theme;
    if (!theme.packId) return;

    setError(null);
    try {
      await updateSettings({ theme: { ...theme, packId: null } });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleImport = async () => {
    setBusy(true);
    setError(null);
    try {
      const installed = await themeCommands.installThemePackWithDialog();
      await loadPacks();
      const theme = useSettingsStore.getState().settings.theme;
      await updateSettings({ theme: { ...theme, packId: installed.id } });
    } catch (e) {
      if (!isDialogCancelled(e)) {
        setError(e instanceof Error ? e.message : String(e));
      }
    } finally {
      setBusy(false);
    }
  };

  const handleRemove = async () => {
    const currentPackId = useSettingsStore.getState().settings.theme.packId;
    if (!currentPackId) return;

    setBusy(true);
    setError(null);
    try {
      await themeCommands.removeThemePack(currentPackId);
      await loadPacks();
      const theme = useSettingsStore.getState().settings.theme;
      await updateSettings({ theme: { ...theme, packId: null } });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <SettingRow label={t('settings.theme.pack')}>
        <Select
          value={selectValue}
          options={packOptions}
          onChange={(v) => void handlePackChange(v)}
          disabled={loading || busy}
        />
      </SettingRow>

      {isOrphanPack && (
        <div className="flex items-center gap-3">
          <p className="text-xs text-amber-500">{t('settings.theme.packMissingHint')}</p>
          <Button variant="secondary" disabled={busy} onClick={() => void handleResetOrphan()}>
            {t('settings.theme.packReset')}
          </Button>
        </div>
      )}

      <div className="flex items-center gap-3">
        <Button variant="secondary" disabled={busy} onClick={() => void handleImport()}>
          {t('settings.theme.import')}
        </Button>
        {packId && (
          <Button variant="secondary" disabled={busy} onClick={() => void handleRemove()}>
            {t('settings.theme.remove')}
          </Button>
        )}
      </div>

      {error && <p className="text-xs text-red-500">{error}</p>}
    </>
  );
}
