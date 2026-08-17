import { useCallback, useState } from 'react';
import { Gauge } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { useSettings } from '../../hooks/useSettings';
import { useI18n } from '../../hooks/useI18n';
import { getUrlParam } from '../../lib/windowKind';
import { DashboardPanel } from './DashboardPanel';

export function DashboardWindow() {
  useSettings();
  const { t } = useI18n();
  const urlDashboardId = getUrlParam('dashboardId') ?? '';
  const [title, setTitle] = useState(t('dashboard.title'));

  const handleDashboardChange = useCallback(
    (_id: string, name: string) => {
      setTitle(name || t('dashboard.title'));
    },
    [t],
  );

  return (
    <div
      className="flex h-screen min-h-0 flex-col bg-surface text-fg"
      data-testid="dashboard-window"
    >
      <TitleBar title={title} leftContent={<Gauge className="h-4 w-4 text-fg-muted" />} />
      <DashboardPanel
        initialDashboardId={urlDashboardId || undefined}
        onDashboardChange={handleDashboardChange}
      />
      <StatusBar left="" />
    </div>
  );
}
