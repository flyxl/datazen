import { useCallback } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { useI18n } from '../../hooks/useI18n';
import { getUrlParam } from '../../lib/windowKind';
import { SettingsContent } from './SettingsContent';
import { parseSettingsSection } from './settingsSections';

/** Legacy standalone settings sub-window shell (kept for unit tests). */
export function SettingsWindow() {
  const { t } = useI18n();

  const handleClose = useCallback(async () => {
    if (!('__TAURI_INTERNALS__' in globalThis)) return;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().close();
  }, []);

  const initialSection = parseSettingsSection(getUrlParam('section'));

  return (
    <div className="flex h-screen flex-col bg-surface text-fg">
      <TitleBar title={t('win.settings')} />
      <SettingsContent
        initialSection={initialSection}
        showCloseButton
        onClose={() => void handleClose()}
      />
    </div>
  );
}
