import { useEffect, useRef, useState } from 'react';
import { useI18n } from '../../../hooks/useI18n';
import { Dialog } from '../../ui/Dialog';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';

export interface TableJoinInfo {
  /** `INNER` / `LEFT` / … as emitted by the generator. */
  type: string;
  /** The `ON` predicates, already rendered as SQL text. */
  on: string;
}

export interface TableOptionsDialogProps {
  /** Table whose chip was clicked; `null` keeps the dialog closed. */
  table: string | null;
  alias: string;
  /** Present when a join introduces this table; null for FROM / unjoined. */
  join: TableJoinInfo | null;
  onApply: (table: string, alias: string) => void;
  onRemove: (table: string) => void;
  onClose: () => void;
}

/**
 * The options behind one FROM chip: its alias, and (read-only) how the table
 * enters the query.
 *
 * The join type and its `ON` predicates are owned by the relation group that
 * drew them — the canvas popover and this dialog would otherwise be two editors
 * of one composite key that could disagree, so this side only reports them. The
 * alias is per-table and is genuinely editable here.
 */
export function TableOptionsDialog({
  table,
  alias,
  join,
  onApply,
  onRemove,
  onClose,
}: TableOptionsDialogProps) {
  const { t } = useI18n();
  const [draftAlias, setDraftAlias] = useState('');
  const seededRef = useRef<string | null>(null);

  useEffect(() => {
    if (!table) {
      seededRef.current = null;
      return;
    }
    if (seededRef.current === table) return;
    seededRef.current = table;
    setDraftAlias(alias);
  }, [table, alias]);

  return (
    <Dialog
      open={!!table}
      title={t('query.visualBuilder.tableOptionsTitle')}
      onClose={onClose}
      testId="qb-table-options"
      className="max-w-md"
      footer={
        <div className="flex w-full items-center justify-between gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              if (table) onRemove(table);
              // The dialog owns its own dismissal: leaving the modal overlay up
              // after the action blocks every control underneath it.
              onClose();
            }}
            data-testid="qb-table-opt-remove"
          >
            {t('query.visualBuilder.removeTable')}
          </Button>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onClose} data-testid="qb-table-opt-cancel">
              {t('query.visualBuilder.cancel')}
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={() => {
                if (table) onApply(table, draftAlias);
                onClose();
              }}
              data-testid="qb-table-opt-apply"
            >
              {t('query.visualBuilder.ok')}
            </Button>
          </div>
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-2">
          <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.tables')}</span>
          <span className="truncate font-mono text-[12px] text-fg" data-testid="qb-table-opt-name">
            {table ?? ''}
          </span>
        </div>

        <label className="flex items-center justify-between gap-2">
          <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.alias')}</span>
          <Input
            value={draftAlias}
            onChange={(e) => setDraftAlias(e.target.value)}
            className="h-8 w-48 text-xs"
            data-testid="qb-table-opt-alias"
          />
        </label>

        {join && (
          <>
            <div className="flex items-center justify-between gap-2">
              <span className="text-[11px] text-fg-muted">
                {t('query.visualBuilder.joinLabel')}
              </span>
              <span className="text-[12px] text-fg" data-testid="qb-table-opt-join">
                {join.type} JOIN
              </span>
            </div>
            <div className="flex flex-col gap-1 border-t border-edge pt-3">
              <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.joinOn')}</span>
              <span
                className="break-all font-mono text-[12px] text-accent"
                data-testid="qb-table-opt-on"
              >
                {join.on}
              </span>
            </div>
          </>
        )}

        {!join && (
          <p className="text-[11px] text-warning" data-testid="qb-table-opt-unjoined">
            {t('query.visualBuilder.unjoinedTable')}
          </p>
        )}
      </div>
    </Dialog>
  );
}
