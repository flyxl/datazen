import { Dialog } from '../../../components/ui/Dialog';
import { useI18n } from '../../../hooks/useI18n';
import type { AiModelProfile } from '../../../types';
import { ModelProfileForm } from './ModelProfileForm';
import { useModelProfileDraft } from './useModelProfileDraft';

interface ModelProfileDialogProps {
  open: boolean;
  onClose: () => void;
  profile: AiModelProfile | null;
  onSave: (profile: AiModelProfile) => Promise<boolean>;
}

/**
 * Settings dialog shell around the shared {@link ModelProfileForm}.
 * All draft/validate/save behaviour lives in `useModelProfileDraft`, which the
 * first-run journey reuses inline.
 */
export function ModelProfileDialog({
  open,
  onClose,
  profile,
  onSave,
}: Readonly<ModelProfileDialogProps>) {
  const { t } = useI18n();
  const draft = useModelProfileDraft({
    profile,
    open,
    onSave,
    onSaved: () => onClose(),
  });

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={profile ? t('settings.ai.editModel') : t('settings.ai.addModel')}
      className="max-w-xl"
    >
      <ModelProfileForm draft={draft} onCancel={onClose} />
    </Dialog>
  );
}
