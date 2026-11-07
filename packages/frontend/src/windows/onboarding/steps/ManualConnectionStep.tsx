import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { FlaskConical } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { Input } from '../../../components/ui/Input';
import { DbTypeBadge } from '../../../components/DbTypeBadge';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import { DB_REGISTRY, sortDbTypesByPopularity } from '../../../lib/databaseTypes';
import { filterDbTypesByQuery } from '../../../lib/filterDbTypes';
import { ConnectionFormBody } from '../../../components/connection/ConnectionFormBody';
import { useConnectionForm } from '../../../components/connection/useConnectionForm';
import { useConnectionClipboardFill } from '../../../components/connection/useConnectionClipboardFill';
import { useConnectionStore } from '../../../stores/connectionStore';
import { connectionCommands } from '../../../commands/connection';
import type { DatabaseType } from '../../../types';
import { StepTag } from './WelcomeStep';

const ALL_DB_TYPES: { value: DatabaseType; label: string }[] = sortDbTypesByPopularity(
  (Object.entries(DB_REGISTRY) as [DatabaseType, (typeof DB_REGISTRY)[DatabaseType]][]).map(
    ([value, meta]) => ({ value, label: meta.label }),
  ),
);

interface ManualConnectionStepProps {
  onSaved: (connectionName: string) => void;
}

/* ---------- S1 (manual entry): mirrors the NewConnectionDialog layout ---------- */

export function ManualConnectionStep({ onSaved }: Readonly<ManualConnectionStepProps>) {
  const { t } = useI18n();
  const connections = useConnectionStore((s) => s.connections);
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const fetchGroups = useConnectionStore((s) => s.fetchGroups);

  const [availableDrivers, setAvailableDrivers] = useState<string[] | null>(null);
  const [driverQuery, setDriverQuery] = useState('');
  const [tested, setTested] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<'ok' | 'fail' | null>(null);

  const nameRef = useRef('');

  useEffect(() => {
    void fetchConnections();
    void fetchGroups();
    connectionCommands
      .getAvailableDrivers()
      .then(setAvailableDrivers)
      .catch(() => setAvailableDrivers(null));
  }, [fetchConnections, fetchGroups]);

  const form = useConnectionForm({
    defaultGroup: null,
    existingConnections: connections,
    onAfterSave: () => {
      onSaved(nameRef.current);
    },
  });
  nameRef.current = form.name;

  // Paste-to-fill stays available, but the journey never reads the clipboard on
  // its own: the read needs a user gesture on macOS and otherwise freezes the
  // form (and anything driving the window) until the paste prompt is answered.
  useConnectionClipboardFill(form, {
    enabled: true,
    autoRead: false,
    availableTypes: availableDrivers,
  });

  const dbTypes = useMemo(() => {
    const available = !availableDrivers
      ? ALL_DB_TYPES
      : ALL_DB_TYPES.filter((db) => availableDrivers.includes(db.value));
    return filterDbTypesByQuery(available, driverQuery);
  }, [availableDrivers, driverQuery]);

  const handleTest = useCallback(async () => {
    setTesting(true);
    setTestResult(null);
    try {
      await form.onTest();
      setTested(true);
      setTestResult('ok');
    } catch {
      setTested(false);
      setTestResult('fail');
    } finally {
      setTesting(false);
    }
  }, [form]);

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="onboarding-step-s1-manual">
      <StepTag label={t('onboarding.s1.stepLabel')} />
      <h2 className="mb-2.5 text-[25px] font-bold tracking-tight leading-tight text-fg">
        {t('onboarding.s1.title')}
      </h2>
      <p
        className="mb-5 max-w-[56ch] text-[13.5px] leading-[1.65] text-fg-secondary"
        dangerouslySetInnerHTML={{ __html: t('onboarding.s1.subtitle') }}
      />

      {/* Connection form: left driver sidebar + right form + footer */}
      <div className="flex min-h-0 flex-1 overflow-hidden rounded-xl border border-edge">
        {/* Left driver sidebar (220px) */}
        <aside className="flex w-[220px] shrink-0 flex-col border-r border-edge bg-surface">
          <div className="shrink-0 p-3">
            <Input
              value={driverQuery}
              onChange={(e) => setDriverQuery(e.target.value)}
              placeholder={t('newConn.searchDrivers')}
              className="h-8 text-xs font-mono"
            />
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-1 pb-2">
            <div className="flex flex-col">
              {dbTypes.length === 0 ? (
                <div className="px-3 py-2 text-xs text-fg-muted">{t('newConn.noDriversMatch')}</div>
              ) : (
                dbTypes.map((db) => (
                  <button
                    key={db.value}
                    type="button"
                    data-testid={`onboard-driver-${db.value}`}
                    onClick={() => form.handleDatabaseTypeChange(db.value)}
                    className={cn(
                      'flex items-center gap-2.5 px-3.5 py-2 text-left text-[13px] font-medium transition-colors select-none',
                      form.databaseType === db.value
                        ? 'bg-accent/10 text-fg border-r-2 border-accent'
                        : 'text-fg-secondary hover:bg-accent/6 hover:text-fg',
                    )}
                  >
                    <DbTypeBadge databaseType={db.value} size={26} />
                    <span>{db.label}</span>
                  </button>
                ))
              )}
            </div>
          </div>
        </aside>

        {/* Right form */}
        <main className="flex min-h-0 min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-1 overflow-y-auto p-6">
            <ConnectionFormBody form={form} variant="window" />
          </div>

          {/* Test / Save footer */}
          <footer className="flex shrink-0 items-center gap-3 border-t border-edge bg-surface px-8 py-3.5">
            <Button
              variant="secondary"
              onClick={() => void handleTest()}
              disabled={testing}
              data-testid="onboard-test-connection"
              className="gap-1.5"
            >
              <FlaskConical className="h-3.5 w-3.5" />
              {testing ? t('newConn.testing') : t('onboarding.s1.testBtn')}
            </Button>
            <span
              className={cn(
                'flex-1 text-xs font-mono',
                testResult === 'ok' && 'text-green',
                testResult === 'fail' && 'text-red',
                !testResult && 'text-fg-muted',
              )}
            >
              {testResult === 'ok' && `✓ ${t('onboarding.s1.testSuccess')}`}
              {testResult === 'fail' && `✕ ${t('onboarding.s1.testFail')}`}
            </span>
            <Button
              variant="primary"
              onClick={() => void form.onSave()}
              disabled={!tested}
              data-testid="onboard-s1-save"
            >
              {t('common.save')}
            </Button>
          </footer>
        </main>
      </div>
    </div>
  );
}
