import { useEffect, useMemo, useState } from 'react';
import { Plus } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
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
  const [selection, setSelection] = useState('');
  const [creatingNew, setCreatingNew] = useState(false);
  const [newName, setNewName] = useState('');

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    setCreatingNew(false);
    setNewName(t('dashboard.defaultName'));
    void dashboardCommands
      .listDashboards()
      .then((list) => {
        if (cancelled) return;
        setDashboards(list);
        if (list.length > 0) {
          setSelection(list[0]!.id);
        } else {
          setCreatingNew(true);
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

  const selectOptions = useMemo(
    () => dashboards.map((b) => ({ value: b.id, label: b.name })),
    [dashboards],
  );

  const handleConfirm = () => {
    if (creatingNew) {
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
            disabled={loading || (creatingNew && !newName.trim()) || (!creatingNew && !selection)}
            onClick={handleConfirm}
          >
            {t('common.ok')}
          </Button>
        </>
      }
    >
      <div className="space-y-3" data-testid="add-to-dashboard-dialog">
        {loading && <p className="text-xs text-fg-muted">{t('common.loading')}</p>}
        {!loading && !creatingNew && (
          <div className="space-y-2">
            <p className="text-xs text-fg-muted">{t('dashboard.selectPanel')}</p>
            <Select
              value={selection}
              options={selectOptions}
              onChange={setSelection}
              placeholder={t('dashboard.selectPanel')}
              data-testid="dashboard-target-select"
            />
            <button
              type="button"
              className="flex items-center gap-1 text-xs text-accent hover:underline"
              data-testid="dashboard-target-new"
              onClick={() => setCreatingNew(true)}
            >
              <Plus className="h-3 w-3" />
              {t('dashboard.createNewPanel')}
            </button>
          </div>
        )}
        {!loading && creatingNew && (
          <div className="space-y-2">
            <p className="text-xs font-medium text-fg">{t('dashboard.createNewPanel')}</p>
            <Input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder={t('dashboard.name')}
              data-testid="dashboard-new-panel-name"
              autoFocus
            />
            {dashboards.length > 0 && (
              <button
                type="button"
                className="text-xs text-accent hover:underline"
                onClick={() => {
                  setCreatingNew(false);
                  setSelection(dashboards[0]!.id);
                }}
              >
                {t('dashboard.selectExisting')}
              </button>
            )}
          </div>
        )}
      </div>
    </Dialog>
  );
}
