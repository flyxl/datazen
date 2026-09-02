import { useState } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { ThemeToggle } from '../../components/ThemeToggle';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { useI18n } from '../../hooks/useI18n';
import { SettingsContent } from './SettingsContent';

export interface SettingsPageProps {
  initialSection?: string;
  onBack: () => void;
}

export function SettingsPage({ initialSection, onBack }: Readonly<SettingsPageProps>) {
  const { t } = useI18n();
  const [confirmLeave, confirmLeaveDialog] = useConfirmDialog();
  const [isDirty, setIsDirty] = useState(false);

  const handleBack = async () => {
    if (
      isDirty &&
      !(await confirmLeave({
        title: t('settings.unsavedChangesTitle'),
        message: t('settings.unsavedChangesMessage'),
        confirmLabel: t('settings.discardChanges'),
        cancelLabel: t('common.cancel'),
      }))
    ) {
      return;
    }
    onBack();
  };

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface text-fg" data-testid="settings-page">
      <TitleBar title={t('win.settings')} rightContent={<ThemeToggle />} />
      <SettingsContent
        initialSection={initialSection}
        onBack={() => void handleBack()}
        onDirtyChange={setIsDirty}
      />
      {confirmLeaveDialog}
    </div>
  );
}
