import { useCallback } from 'react';
import { ArrowLeft } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { ThemeToggle } from '../../components/ThemeToggle';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { SettingsContent } from './SettingsContent';

export interface SettingsPageProps {
  initialSection?: string;
  onBack: () => void;
}

export function SettingsPage({ initialSection, onBack }: Readonly<SettingsPageProps>) {
  const { t } = useI18n();

  const handleBack = useCallback(() => {
    onBack();
  }, [onBack]);

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface text-fg" data-testid="settings-page">
      <TitleBar
        title={t('win.settings')}
        leftContent={
          <Button
            variant="ghost"
            className="h-8 gap-1 px-2 text-xs"
            onClick={handleBack}
            data-testid="settings-back"
            title={t('common.back')}
          >
            <ArrowLeft className="h-3.5 w-3.5" />
            {t('common.back')}
          </Button>
        }
        rightContent={<ThemeToggle />}
      />
      <SettingsContent initialSection={initialSection} />
    </div>
  );
}
