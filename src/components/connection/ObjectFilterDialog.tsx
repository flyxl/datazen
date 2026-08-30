import { useEffect, useState } from 'react';
import { Button } from '../ui/Button';
import { Dialog } from '../ui/Dialog';
import { Input } from '../ui/Input';
import { useI18n } from '../../hooks/useI18n';
import {
  getObjectFilter,
  type ObjectFilterPrefs,
  withObjectFilterOptions,
} from '../../lib/objectFilter';
import type { ConnectionConfig } from '../../types';

export interface ObjectFilterDialogProps {
  open: boolean;
  connection: ConnectionConfig | null;
  onClose: () => void;
  onSave: (config: ConnectionConfig) => void | Promise<void>;
}

export function ObjectFilterDialog({ open, connection, onClose, onSave }: ObjectFilterDialogProps) {
  const { t } = useI18n();
  const [hideSystemSchemas, setHideSystemSchemas] = useState(false);
  const [tableNameInclude, setTableNameInclude] = useState('');
  const [tableNameExclude, setTableNameExclude] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open || !connection) return;
    const filter = getObjectFilter(connection);
    setHideSystemSchemas(filter.hideSystemSchemas === true);
    setTableNameInclude(filter.tableNameInclude ?? '');
    setTableNameExclude(filter.tableNameExclude ?? '');
  }, [open, connection]);

  const handleSave = async () => {
    if (!connection) return;
    setSaving(true);
    try {
      const filter: ObjectFilterPrefs = {
        hideSystemSchemas: hideSystemSchemas || undefined,
        tableNameInclude: tableNameInclude.trim() || undefined,
        tableNameExclude: tableNameExclude.trim() || undefined,
      };
      await onSave(withObjectFilterOptions(connection, filter));
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t('common.objectFilter')}
      testId="object-filter-dialog"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button variant="primary" disabled={saving} onClick={() => void handleSave()}>
            {t('common.save')}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <label className="flex items-center gap-2 text-sm text-fg">
          <input
            type="checkbox"
            checked={hideSystemSchemas}
            onChange={(e) => setHideSystemSchemas(e.target.checked)}
          />
          {t('objectFilter.hideSystemSchemas')}
        </label>
        <div>
          <label className="mb-1 block text-xs text-fg-muted">{t('objectFilter.include')}</label>
          <Input
            value={tableNameInclude}
            onChange={(e) => setTableNameInclude(e.target.value)}
            placeholder={t('objectFilter.includePlaceholder')}
          />
        </div>
        <div>
          <label className="mb-1 block text-xs text-fg-muted">{t('objectFilter.exclude')}</label>
          <Input
            value={tableNameExclude}
            onChange={(e) => setTableNameExclude(e.target.value)}
            placeholder={t('objectFilter.excludePlaceholder')}
          />
        </div>
        <p className="text-xs text-fg-muted">{t('objectFilter.hint')}</p>
      </div>
    </Dialog>
  );
}
