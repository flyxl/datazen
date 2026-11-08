import { useEffect, useState } from 'react';
import { Trash2 } from 'lucide-react';
import { Badge } from '../../components/ui/Badge';
import { Button } from '../../components/ui/Button';
import { CopyableError } from '../../components/ui/CopyableError';
import { Dialog } from '../../components/ui/Dialog';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import { useLanguagePackStore } from '../../stores/languagePackStore';
import { unloadLanguagePack } from '../../lib/languagePackRuntime';
import { SectionTitle, SettingRow } from './settingsUi';
import { SettingHint } from './SettingHint';
import { InstallLanguagePackDialog } from './InstallLanguagePackDialog';

/** Confirmation state for uninstall; the pack code awaiting removal. */
interface PendingRemoval {
  code: string;
  label: string;
  isActive: boolean;
}

/**
 * Settings → General → Language packs: list of installed external language
 * packs with install / remove. Removing the *active* pack first switches the
 * UI language back to English (builtin — always available), then uninstalls.
 */
export function LanguagePacksCard() {
  const { t } = useI18n();
  const packs = useLanguagePackStore((s) => s.packs);
  const loaded = useLanguagePackStore((s) => s.loaded);
  const storeError = useLanguagePackStore((s) => s.error);
  const fetch = useLanguagePackStore((s) => s.fetch);
  const removePack = useLanguagePackStore((s) => s.remove);
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [installOpen, setInstallOpen] = useState(false);
  const [pendingRemoval, setPendingRemoval] = useState<PendingRemoval | null>(null);
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Settings can be opened after a pack was installed in another window;
    // fetch on mount so this list is never a stale snapshot.
    void fetch();
  }, [fetch]);

  const activeLanguage = settings?.language ?? 'en';

  const requestRemoval = (code: string, label: string) => {
    setError(null);
    setPendingRemoval({ code, label, isActive: code === activeLanguage });
  };

  /**
   * Confirmed removal. When the pack is the active language, persist the
   * English fallback FIRST: the settingsStore change drives `startLocaleSync`
   * → `setLocale('en')`, so the UI switches before the dictionary is dropped.
   */
  const handleRemove = async () => {
    if (!pendingRemoval || removing) return;
    const { code, isActive } = pendingRemoval;
    setRemoving(true);
    setError(null);
    try {
      if (isActive) {
        await updateSettings({ language: 'en' });
      }
      await removePack(code);
      unloadLanguagePack(code);
      setPendingRemoval(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRemoving(false);
    }
  };

  return (
    <div data-testid="settings-language-packs-card">
      <SectionTitle hint={t('settings.languagePacks.subtitle')}>
        {t('settings.languagePacks.title')}
      </SectionTitle>

      <div className="mt-4 space-y-4">
        <SettingRow
          label={t('settings.languagePacks.installed')}
          hint={packs.length === 0 && loaded ? t('settings.languagePacks.emptyHint') : undefined}
        >
          <Button data-testid="language-pack-install-open" onClick={() => setInstallOpen(true)}>
            {t('settings.languagePacks.install.button')}
          </Button>
        </SettingRow>

        {packs.length > 0 && (
          <ul
            className="divide-y divide-edge rounded-lg border border-edge"
            data-testid="language-pack-list"
          >
            {packs.map((pack) => (
              <li
                key={pack.code}
                className="flex items-center justify-between gap-3 px-3 py-2"
                data-testid={`language-pack-row-${pack.code}`}
              >
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm text-fg">{pack.label}</span>
                    <Badge>{pack.code}</Badge>
                    <Badge>{`v${pack.version}`}</Badge>
                    {pack.code === activeLanguage && (
                      <Badge tone="accent" data-testid={`language-pack-active-${pack.code}`}>
                        {t('settings.languagePacks.inUse')}
                      </Badge>
                    )}
                  </div>
                  <p className="mt-0.5 text-xs text-fg-muted">
                    {t('settings.languagePacks.messageCount', {
                      count: pack.messageCount,
                    })}
                    {pack.author ? ` · ${pack.author}` : ''}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  data-testid={`language-pack-remove-${pack.code}`}
                  onClick={() => requestRemoval(pack.code, pack.label)}
                >
                  <Trash2 className="size-4" aria-hidden />
                  {t('settings.languagePacks.remove')}
                </Button>
              </li>
            ))}
          </ul>
        )}

        {(error || storeError) && (
          <CopyableError
            message={error ?? storeError ?? ''}
            copyButton
            data-testid="language-pack-card-error"
          />
        )}
      </div>

      <InstallLanguagePackDialog open={installOpen} onClose={() => setInstallOpen(false)} />

      <Dialog
        open={pendingRemoval !== null}
        title={t('settings.languagePacks.removeTitle')}
        description={
          pendingRemoval?.isActive
            ? t('settings.languagePacks.removeActiveDescription', {
                label: pendingRemoval.label,
              })
            : t('settings.languagePacks.removeDescription', {
                label: pendingRemoval?.label ?? '',
              })
        }
        onClose={() => (removing ? undefined : setPendingRemoval(null))}
        footer={
          <>
            <Button variant="ghost" onClick={() => setPendingRemoval(null)} disabled={removing}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              data-testid="language-pack-remove-confirm"
              onClick={() => void handleRemove()}
              disabled={removing}
            >
              {removing ? t('settings.languagePacks.removing') : t('settings.languagePacks.remove')}
            </Button>
          </>
        }
      >
        <SettingHint
          label={t('settings.languagePacks.removeTitle')}
          text={t('settings.languagePacks.removeHint')}
        />
      </Dialog>
    </div>
  );
}
