import { useEffect, useState } from 'react';
import { dashboardCommands } from '../../commands/dashboard';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import type { MonitorScheduleGrantSummary } from '../../types/dashboard';

export function MonitorScheduleAuthorization({
  dashboardId,
  widgetId,
  dirty,
}: Readonly<{
  dashboardId: string;
  widgetId: string;
  dirty: boolean;
}>) {
  const { t } = useI18n();
  const [grant, setGrant] = useState<MonitorScheduleGrantSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    setGrant(null);
    setError(null);
    dashboardCommands
      .getMonitorScheduleGrant(dashboardId, widgetId)
      .then((value) => {
        if (active) setGrant(value);
      })
      .catch((reason: unknown) => {
        if (active) setError(String(reason));
      });
    return () => {
      active = false;
    };
  }, [dashboardId, widgetId]);
  const change = async (revoke: boolean) => {
    setBusy(true);
    setError(null);
    try {
      if (revoke) {
        await dashboardCommands.revokeMonitorSchedule(dashboardId, widgetId);
        setGrant(null);
      } else {
        setGrant(await dashboardCommands.authorizeMonitorSchedule(dashboardId, widgetId));
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };
  const authorized = grant && !grant.revoked && Date.parse(grant.expiresAt) > Date.now();
  return (
    <section className="space-y-2 rounded-md border border-edge p-3 text-xs">
      <p className="font-medium text-fg">{t('dashboard.scheduleAuthorization')}</p>
      <p className="text-fg-muted">{t('dashboard.scheduleAuthorizationHint')}</p>
      <p className="text-fg-muted">
        {authorized
          ? t('dashboard.scheduleAuthorizedUntil', {
              time: new Date(grant.expiresAt).toLocaleString(),
            })
          : t('dashboard.scheduleUnauthorized')}
      </p>
      {dirty && <p className="text-fg-muted">{t('dashboard.scheduleSaveFirst')}</p>}
      {error && (
        <p role="alert" className="text-red-400">
          {error}
        </p>
      )}
      <div className="flex gap-2">
        <Button
          variant="secondary"
          size="sm"
          disabled={busy || dirty}
          onClick={() => void change(false)}
        >
          {t('dashboard.scheduleAuthorizeHour')}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          disabled={busy || !grant || grant.revoked}
          onClick={() => void change(true)}
        >
          {t('dashboard.scheduleRevoke')}
        </Button>
      </div>
    </section>
  );
}
