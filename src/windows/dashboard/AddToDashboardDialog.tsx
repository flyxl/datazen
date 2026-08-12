import { useEffect, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { Input } from '../../components/ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { dashboardCommands } from '../../commands/dashboard';
import type { Dashboard } from '../../types/dashboard';

export interface AddToDashboardDialogProps {
  open: boolean;
  onClose: () => void;
  onConfirm: (dashboardId: string | 'new', newName?: string) => void;
}

export function AddToDashboardDialog({
  open,
  onClose,
  onConfirm,
}: Readonly<AddToDashboardDialogProps>) {
  const { t } = useI18n();
  const [dashboards, setDashboards] = useState<Dashboard[]>([]);
  const [loading, setLoading] = useState(false);
  const [selection, setSelection] = useState<string | 'new'>('new');
  const [newName, setNewName] = useState('');

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    setSelection('new');
    setNewName(t('dashboard.defaultName'));
    void dashboardCommands
      .listDashboards()
      .then((list) => {
        if (cancelled) return;
        setDashboards(list);
        if (list.length > 0) {
          setSelection(list[0]!.id);
        } else {
          setSelection('new');
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only refetch when dialog opens
  }, [open]);

  const handleConfirm = () => {
    if (selection === 'new') {
      const name = newName.trim() || t('dashboard.defaultName');
      onConfirm('new', name);
      return;
    }
    onConfirm(selection);
  };

  return (
    <Dialog
      open={open}
      title={t('dashboard.addToDashboard')}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" className="h-8 px-3 text-xs" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button
            className="h-8 px-3 text-xs"
            data-testid="add-to-dashboard-confirm"
            disabled={loading || (selection === 'new' && !newName.trim())}
            onClick={handleConfirm}
          >
            {t('common.ok')}
          </Button>
        </>
      }
    >
      <div className="space-y-3" data-testid="add-to-dashboard-dialog">
        <p className="text-xs text-fg-muted">{t('dashboard.selectPanel')}</p>
        {loading && <p className="text-xs text-fg-muted">{t('common.loading')}</p>}
        {!loading && (
          <div className="space-y-2">
            {dashboards.map((board) => (
              <label
                key={board.id}
                className="flex cursor-pointer items-center gap-2 rounded-md border border-edge px-3 py-2 text-sm hover:bg-surface-raised"
              >
                <input
                  type="radio"
                  name="dashboard-target"
                  checked={selection === board.id}
                  onChange={() => setSelection(board.id)}
                  data-testid="dashboard-target-option"
                />
                <span className="truncate">{board.name}</span>
              </label>
            ))}
            <label className="flex cursor-pointer items-start gap-2 rounded-md border border-edge px-3 py-2 text-sm hover:bg-surface-raised">
              <input
                type="radio"
                name="dashboard-target"
                checked={selection === 'new'}
                onChange={() => setSelection('new')}
                data-testid="dashboard-target-new"
              />
              <span className="min-w-0 flex-1 space-y-2">
                <span className="block">{t('dashboard.createNewPanel')}</span>
                {selection === 'new' && (
                  <Input
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    placeholder={t('dashboard.name')}
                    data-testid="dashboard-new-panel-name"
                  />
                )}
              </span>
            </label>
          </div>
        )}
      </div>
    </Dialog>
  );
}
