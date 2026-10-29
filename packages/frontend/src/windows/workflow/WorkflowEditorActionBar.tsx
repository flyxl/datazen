import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';

interface WorkflowEditorActionBarProps {
  onSave: () => void;
  onCancel: () => void;
  saveDisabled?: boolean;
  saveTestId?: string;
}

/**
 * Shared bottom action bar for the workflow edit panel (Visual + YAML tabs).
 *
 * Rendered once as a *sibling* of the tab content areas, so both tabs reuse the
 * exact same Save / Cancel buttons and stay visually consistent.
 */
export function WorkflowEditorActionBar({
  onSave,
  onCancel,
  saveDisabled,
  saveTestId,
}: WorkflowEditorActionBarProps) {
  const { t } = useI18n();
  return (
    <div className="flex shrink-0 items-center justify-end gap-3 border-t border-edge bg-surface px-6 py-3">
      <Button variant="secondary" onClick={onCancel} className="px-6">
        {t('common.cancel')}
      </Button>
      <Button onClick={onSave} className="px-6" disabled={saveDisabled} data-testid={saveTestId}>
        {t('common.save')}
      </Button>
    </div>
  );
}
