import { Button } from '../../components/ui/Button';
import { CopyableError } from '../../components/ui/CopyableError';
import { Dialog } from '../../components/ui/Dialog';
import { useI18n } from '../../hooks/useI18n';
import type { useSchemaDiffSavedSetups } from './useSchemaDiffSavedSetups';

interface SchemaDiffSavedSetupDialogsProps {
  savedSetups: ReturnType<typeof useSchemaDiffSavedSetups>;
}

export function SchemaDiffSavedSetupDialogs({ savedSetups }: SchemaDiffSavedSetupDialogsProps) {
  const { t } = useI18n();
  const closeImport = () => {
    savedSetups.setImportConfigOpen(false);
    savedSetups.setImportConfigText('');
    savedSetups.setImportConfigError('');
  };
  const closeProfile = () => {
    savedSetups.setProfileDialogOpen(false);
    savedSetups.setProfileError('');
  };

  return (
    <>
      <Dialog
        open={savedSetups.importConfigOpen}
        title={t('schemaDiff.importConfigTitle')}
        description={t('schemaDiff.importConfigHint')}
        testId="schema-diff-import-config-dialog"
        onClose={closeImport}
        footer={
          <>
            <Button variant="ghost" onClick={closeImport}>
              {t('common.cancel')}
            </Button>
            <Button
              data-testid="schema-diff-import-config-confirm"
              disabled={!savedSetups.importConfigText.trim()}
              onClick={() => savedSetups.applyImportedConfig(savedSetups.importConfigText)}
            >
              {t('schemaDiff.importConfigConfirm')}
            </Button>
          </>
        }
      >
        <textarea
          data-testid="schema-diff-import-config-text"
          className="h-48 w-full resize-y rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg"
          value={savedSetups.importConfigText}
          placeholder={t('schemaDiff.importConfigPlaceholder')}
          onChange={(event) => {
            savedSetups.setImportConfigText(event.target.value);
            if (savedSetups.importConfigError) savedSetups.setImportConfigError('');
          }}
        />
        {savedSetups.importConfigError && (
          <CopyableError
            message={savedSetups.importConfigError}
            className="error-message mt-2"
            data-testid="schema-diff-import-config-error"
          />
        )}
      </Dialog>
      <Dialog
        open={savedSetups.profileDialogOpen}
        title={t('schemaDiff.profileSaveTitle')}
        description={t('schemaDiff.profileSaveHint')}
        testId="schema-diff-profile-dialog"
        onClose={closeProfile}
        footer={
          <>
            <Button variant="ghost" onClick={closeProfile}>
              {t('common.cancel')}
            </Button>
            <Button
              data-testid="schema-diff-profile-save-confirm"
              disabled={!savedSetups.profileName.trim()}
              onClick={() => void savedSetups.handleSaveProfile()}
            >
              {t('schemaDiff.profileSave')}
            </Button>
          </>
        }
      >
        <input
          data-testid="schema-diff-profile-name"
          className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg"
          value={savedSetups.profileName}
          placeholder={t('schemaDiff.profileNamePlaceholder')}
          onChange={(event) => {
            savedSetups.setProfileName(event.target.value);
            if (savedSetups.profileError) savedSetups.setProfileError('');
          }}
        />
        {savedSetups.profileError && (
          <CopyableError message={savedSetups.profileError} className="error-message mt-2" />
        )}
      </Dialog>
    </>
  );
}
