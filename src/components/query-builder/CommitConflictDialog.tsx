import { useI18n } from '../../hooks/useI18n';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';

export type CommitConflictChoice = 'replace' | 'append' | 'keep';

export interface CommitConflictDialogProps {
  open: boolean;
  /** Called with the user's choice. `keep` means "stay in the builder". */
  onChoose: (choice: CommitConflictChoice) => void;
}

/**
 * Three-way resolution shown when OK is pressed while the SQL editor already
 * holds different content.
 *
 * Deliberately not `useConfirmDialog`: that helper is two-button, and silently
 * overwriting hand-written SQL is exactly the regression this guards against
 * (PRD §6.3).
 */
export function CommitConflictDialog({ open, onChoose }: CommitConflictDialogProps) {
  const { t } = useI18n();

  return (
    <Dialog
      open={open}
      title={t('query.visualBuilder.conflictTitle')}
      onClose={() => onChoose('keep')}
      testId="qb-commit-conflict-dialog"
      className="max-w-md"
      footer={
        <Button
          variant="ghost"
          size="sm"
          onClick={() => onChoose('keep')}
          data-testid="qb-conflict-keep"
        >
          {t('query.visualBuilder.keepEditing')}
        </Button>
      }
    >
      <div className="flex flex-col gap-3">
        <p className="text-sm text-fg-secondary">{t('query.visualBuilder.conflictMessage')}</p>
        <div className="flex flex-col gap-2">
          <Button
            variant="primary"
            size="sm"
            className="justify-start"
            onClick={() => onChoose('replace')}
            data-testid="qb-conflict-replace"
          >
            {t('query.visualBuilder.conflictReplace')}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            className="justify-start"
            onClick={() => onChoose('append')}
            data-testid="qb-conflict-append"
          >
            {t('query.visualBuilder.conflictAppend')}
          </Button>
        </div>
        <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.conflictKeep')}</span>
      </div>
    </Dialog>
  );
}
