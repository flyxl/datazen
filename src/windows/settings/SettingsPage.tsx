import { TitleBar } from '../../components/TitleBar';
import { ThemeToggle } from '../../components/ThemeToggle';
import { useI18n } from '../../hooks/useI18n';
import { SettingsContent } from './SettingsContent';

export interface SettingsPageProps {
  initialSection?: string;
  onBack: () => void;
}

export function SettingsPage({ initialSection, onBack }: Readonly<SettingsPageProps>) {
  const { t } = useI18n();

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface text-fg" data-testid="settings-page">
      <TitleBar title={t('win.settings')} rightContent={<ThemeToggle />} />
      <SettingsContent initialSection={initialSection} onBack={onBack} />
    </div>
  );
}
