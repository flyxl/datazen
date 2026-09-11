import { useCallback, useEffect, useMemo, useReducer, useState } from 'react';
import { CheckCircle2, Database, FileDown, Sparkles } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { DbTypeBadge } from '../../components/DbTypeBadge';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import { cn } from '../../lib/cn';
import {
  DB_REGISTRY,
  sortDbTypesByPopularity,
} from '../../lib/databaseTypes';
import { filterDbTypesByQuery } from '../../lib/filterDbTypes';
import { openConnectionShareDialog } from '../../lib/connectionShare';
import { ConnectionFormBody } from '../../components/connection/ConnectionFormBody';
import { useConnectionForm } from '../../components/connection/useConnectionForm';
import { useConnectionClipboardFill } from '../../components/connection/useConnectionClipboardFill';
import { useConnectionStore } from '../../stores/connectionStore';
import { connectionCommands } from '../../commands/connection';
import type { DatabaseType } from '../../types';
import {
  wizardReducer,
  INITIAL_STATE,
  type WizardState,
  type WizardAction,
} from './wizardState';

/* ---------- driver list data ---------- */

const ALL_DB_TYPES: { value: DatabaseType; label: string }[] =
  sortDbTypesByPopularity(
    (
      Object.entries(DB_REGISTRY) as [
        DatabaseType,
        (typeof DB_REGISTRY)[DatabaseType],
      ][]
    ).map(([value, meta]) => ({ value, label: meta.label })),
  );

/* ---------- AI provider display data ---------- */

const AI_PROVIDERS = [
  { id: 'open_ai', label: 'OpenAI', color: '#10a37f' },
  { id: 'deep_seek', label: 'DeepSeek', color: '#4fc3f7' },
  { id: 'ollama', label: 'Ollama', color: '#fff' },
  { id: 'custom', label: 'Custom', color: '#a78bfa' },
] as const;

/* ---------- brand sidebar (left 352px) ---------- */

function BrandSidebar({ step }: { step: WizardState['step'] }) {
  const labels: Record<WizardState['step'], { heading: string; desc: string }> = {
    s0: { heading: 'Welcome', desc: 'Choose how to get started.' },
    s1: {
      heading: 'Connect',
      desc: 'Set up a database connection.',
    },
    s2: { heading: 'AI Setup', desc: 'Optional AI assistance.' },
    s3: { heading: 'All Done', desc: 'Ready to explore your data.' },
  };
  const { heading, desc } = labels[step];

  return (
    <aside className="flex w-[352px] shrink-0 flex-col border-r border-edge bg-surface p-10">
      <div className="mb-8 flex items-center gap-2.5">
        <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-accent/15">
          <Database className="h-5 w-5 text-accent" />
        </div>
        <span className="text-base font-semibold text-fg">DataZen</span>
      </div>
      <div className="mb-4 flex items-center gap-3">
        {(['s0', 's1', 's2', 's3'] as const).map((s, i) => (
          <div key={s} className="flex items-center gap-3">
            <div
              className={cn(
                'flex h-7 w-7 items-center justify-center rounded-full text-xs font-semibold',
                s === step
                  ? 'bg-accent text-on-accent'
                  : 'bg-surface-alt text-fg-muted',
              )}
            >
              {i + 1}
            </div>
            {i < 3 && (
              <div
                className={cn(
                  'h-px w-6',
                  ['s0', 's1', 's2', 's3'].indexOf(step) > i
                    ? 'bg-accent/40'
                    : 'bg-edge',
                )}
              />
            )}
          </div>
        ))}
      </div>
      <h1 className="mb-2 text-2xl font-bold text-fg">{heading}</h1>
      <p className="text-sm leading-relaxed text-fg-secondary">{desc}</p>
    </aside>
  );
}

/* ---------- S0: Welcome / Three entry cards ---------- */

function S0({
  dispatch,
  onImport,
}: {
  dispatch: (a: WizardAction) => void;
  onImport: () => void;
}) {
  const { t } = useI18n();
  return (
    <div className="flex flex-col items-center gap-8">
      <div className="text-center">
        <h2 className="mb-2 text-xl font-bold text-fg">
          {t('onboarding.s0.title')}
        </h2>
        <p className="text-sm text-fg-secondary">
          {t('onboarding.s0.subtitle')}
        </p>
      </div>
      <div className="grid w-full max-w-2xl grid-cols-3 gap-4">
        {/* Import — primary */}
        <button
          type="button"
          onClick={onImport}
          className="group relative flex flex-col items-start gap-3 rounded-xl border-2 border-accent/50 bg-surface p-5 text-left transition-all hover:border-accent hover:shadow-lg hover:shadow-accent/5"
        >
          <span className="absolute right-3 top-3 rounded-full bg-accent/15 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-accent">
            {t('onboarding.s0.importFastest')}
          </span>
          <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent/10">
            <FileDown className="h-5 w-5 text-accent" />
          </div>
          <div>
            <div className="mb-1 text-sm font-semibold text-fg">
              {t('onboarding.s0.importCard')}
            </div>
            <div className="text-xs leading-relaxed text-fg-secondary">
              {t('onboarding.s0.importCardDesc')}
            </div>
          </div>
        </button>

        {/* Manual */}
        <button
          type="button"
          onClick={() => dispatch({ type: 'ENTER_MANUAL' })}
          className="flex flex-col items-start gap-3 rounded-xl border border-edge bg-surface p-5 text-left transition-all hover:border-edge-hi hover:shadow-md"
        >
          <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-surface-alt">
            <Database className="h-5 w-5 text-fg-secondary" />
          </div>
          <div>
            <div className="mb-1 text-sm font-semibold text-fg">
              {t('onboarding.s0.manualCard')}
            </div>
            <div className="text-xs leading-relaxed text-fg-secondary">
              {t('onboarding.s0.manualCardDesc')}
            </div>
          </div>
        </button>

        {/* Sample */}
        <button
          type="button"
          onClick={() => dispatch({ type: 'SAMPLE' })}
          className="flex flex-col items-start gap-3 rounded-xl border border-edge bg-surface p-5 text-left transition-all hover:border-edge-hi hover:shadow-md"
        >
          <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-surface-alt">
            <Sparkles className="h-5 w-5 text-fg-secondary" />
          </div>
          <div>
            <div className="mb-1 text-sm font-semibold text-fg">
              {t('onboarding.s0.sampleCard')}
            </div>
            <div className="text-xs leading-relaxed text-fg-secondary">
              {t('onboarding.s0.sampleCardDesc')}
            </div>
          </div>
        </button>
      </div>
    </div>
  );
}

/* ---------- S1: Connect (mirrors NewConnectionDialog layout) ---------- */

function S1({
  dispatch,
  onBack,
}: {
  dispatch: (a: WizardAction) => void;
  onBack: () => void;
}) {
  const { t } = useI18n();
  const connections = useConnectionStore((s) => s.connections);
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const fetchGroups = useConnectionStore((s) => s.fetchGroups);

  const [availableDrivers, setAvailableDrivers] = useState<string[] | null>(
    null,
  );
  const [driverQuery, setDriverQuery] = useState('');
  const [tested, setTested] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<'ok' | 'fail' | null>(null);

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
      dispatch({ type: 'SAVE_SUCCESS' });
    },
  });

  useConnectionClipboardFill(form, {
    enabled: true,
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
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="mb-4 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
        {t('onboarding.s1.driver')}
      </div>
      <div className="flex min-h-0 flex-1 overflow-hidden rounded-lg border border-edge">
        {/* Left driver sidebar */}
        <aside className="flex w-[220px] shrink-0 flex-col border-r border-edge bg-surface">
          <div className="shrink-0 p-3 pb-1">
            <Input
              value={driverQuery}
              onChange={(e) => setDriverQuery(e.target.value)}
              placeholder={t('newConn.searchDrivers')}
              className="h-8 text-sm"
            />
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-3">
            <div className="flex flex-col gap-0.5">
              {dbTypes.length === 0 ? (
                <div className="px-2 py-2 text-sm text-fg-muted">
                  {t('newConn.noDriversMatch')}
                </div>
              ) : (
                dbTypes.map((db) => (
                  <button
                    key={db.value}
                    type="button"
                    data-testid={`onboard-driver-${db.value}`}
                    onClick={() => form.handleDatabaseTypeChange(db.value)}
                    className={cn(
                      'flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors select-none',
                      form.databaseType === db.value
                        ? 'bg-surface-raised text-fg'
                        : 'text-fg-secondary hover:bg-surface-alt hover:text-fg',
                    )}
                  >
                    <DbTypeBadge databaseType={db.value} size={20} />
                    <span className="font-medium">{db.label}</span>
                  </button>
                ))
              )}
            </div>
          </div>
        </aside>

        {/* Right form */}
        <main className="flex min-h-0 min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-1 overflow-y-auto p-5">
            <ConnectionFormBody form={form} variant="window" />
          </div>

          {/* Test / Cancel / Save footer */}
          <footer className="flex shrink-0 items-center justify-end gap-3 border-t border-edge bg-surface-alt px-5 py-3">
            <Button
              variant="secondary"
              onClick={() => void handleTest()}
              disabled={testing}
              data-testid="onboard-test-connection"
            >
              {testing
                ? t('newConn.testing')
                : t('onboarding.s1.testBtn')}
            </Button>
            <Button variant="secondary" onClick={onBack} data-testid="onboard-s1-cancel">
              {t('onboarding.common.back')}
            </Button>
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
      {testResult === 'ok' && (
        <p className="mt-2 text-xs text-green-400">
          ✓ {t('onboarding.s1.testSuccess')}
        </p>
      )}
      {testResult === 'fail' && (
        <p className="mt-2 text-xs text-red-400">
          ✕ {t('onboarding.s1.testFail')}
        </p>
      )}
    </div>
  );
}

/* ---------- S2: AI setup ---------- */

function S2({
  validated,
  onValidate,
}: {
  validated: boolean | null;
  onValidate: () => void;
}) {
  const { t } = useI18n();
  const [apiKey, setApiKey] = useState('');
  const [selected, setSelected] = useState<string>('open_ai');

  return (
    <div className="flex flex-col gap-6">
      <div className="text-center">
        <h2 className="mb-2 text-xl font-bold text-fg">
          {t('onboarding.s2.title')}
        </h2>
        <p className="text-sm text-fg-secondary">
          {t('onboarding.s2.subtitle')}
        </p>
      </div>

      {/* Provider grid */}
      <div className="grid grid-cols-4 gap-3">
        {AI_PROVIDERS.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => setSelected(p.id)}
            className={cn(
              'flex flex-col items-center gap-2 rounded-xl border-2 p-4 transition-all',
              selected === p.id
                ? 'border-accent bg-accent/5'
                : 'border-edge bg-surface hover:border-edge-hi',
            )}
          >
            <div
              className="flex h-10 w-10 items-center justify-center rounded-full bg-surface-alt text-sm font-bold"
              style={{ color: p.color }}
            >
              {p.label.charAt(0)}
            </div>
            <span className="text-xs font-medium text-fg">{p.label}</span>
          </button>
        ))}
      </div>

      {/* API key */}
      <div className="mx-auto w-full max-w-md space-y-3">
        <label className="block text-sm font-medium text-fg">
          {t('onboarding.s2.apiKey')}
        </label>
        <Input
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="sk-..."
          className="font-mono"
        />
        <p className="text-xs text-fg-muted">
          🔒 Saved locally with AES-256-GCM encryption.
        </p>
        <Button variant="secondary" onClick={onValidate}>
          Validate key
        </Button>
        {validated === true && (
          <p className="text-xs text-green-400">✓ Key valid</p>
        )}
        {validated === false && (
          <p className="text-xs text-amber-400">
            ⚠ {t('onboarding.s2.skipHint')}
          </p>
        )}
      </div>
    </div>
  );
}

/* ---------- S3: Done ---------- */

function S3({
  entry,
  importResult,
  onOpen,
}: {
  entry: WizardState['entry'];
  importResult?: WizardState['importResult'];
  onOpen: () => void;
}) {
  const { t } = useI18n();
  const connSummary =
    entry === 'sample'
      ? 'Sample dataset · SQLite (bundled)'
      : entry === 'import' && importResult
        ? `${importResult.imported} connections imported · ${importResult.source}`
        : 'Manual connection';

  return (
    <div className="flex flex-col items-center gap-6">
      <CheckCircle2 className="h-16 w-16 text-accent" />
      <div className="text-center">
        <h2 className="mb-2 text-xl font-bold text-fg">
          {t('onboarding.s3.title')}
        </h2>
        <p className="text-sm text-fg-secondary">
          {t('onboarding.s3.subtitle')}
        </p>
      </div>

      <div className="w-full max-w-md space-y-3 rounded-xl border border-edge bg-surface p-5">
        <div className="flex items-center justify-between text-sm">
          <span className="text-fg-secondary">{t('onboarding.s3.connLabel')}</span>
          <span className="font-medium text-fg">{connSummary}</span>
        </div>
        <div className="flex items-center justify-between text-sm">
          <span className="text-fg-secondary">{t('onboarding.s3.aiLabel')}</span>
          <span className="font-medium text-fg">Not configured (optional)</span>
        </div>
        <div className="flex items-center justify-between text-sm">
          <span className="text-fg-secondary">{t('onboarding.s3.storageLabel')}</span>
          <span className="font-medium text-fg">Encrypted locally · AES-256-GCM</span>
        </div>
      </div>

      {entry === 'sample' && (
        <p className="text-center text-xs text-accent">
          💡 {t('onboarding.s3.nextHint')}
        </p>
      )}

      <Button variant="primary" onClick={onOpen} data-testid="onboard-open-datazen">
        {t('onboarding.s3.openBtn')}
      </Button>
    </div>
  );
}

/* ---------- Main OnboardingWizard ---------- */

export function OnboardingWizard() {
  const { t } = useI18n();
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [state, dispatch] = useReducer(wizardReducer, INITIAL_STATE);

  const handleOpen = useCallback(() => {
    void updateSettings({
      onboarding: { completed: true, version: 1 },
    });
  }, [updateSettings]);

  const handleImport = useCallback(() => {
    openConnectionShareDialog('import');
    // Import success is handled externally; for prototype, assume success
    dispatch({ type: 'IMPORT_SUCCESS', result: { imported: 3, source: 'DBeaver' } });
  }, []);

  const handleBack = useCallback(() => {
    dispatch({ type: 'BACK' });
  }, []);

  const handleSkip = useCallback(() => {
    dispatch({ type: 'SKIP' });
  }, []);

  const handleFinish = useCallback(() => {
    dispatch({ type: 'FINISH' });
  }, []);

  const showSkip = state.step !== 's3';
  const showBack = state.step === 's1' || (state.step === 's2' && state.entry === 'import');

  return (
    <div className="flex h-screen bg-surface text-fg">
      <BrandSidebar step={state.step} />

      <main className="flex flex-1 flex-col overflow-hidden">
        <div className="flex flex-1 items-center justify-center overflow-y-auto p-10">
          {state.step === 's0' && (
            <S0 dispatch={dispatch} onImport={handleImport} />
          )}
          {state.step === 's1' && (
            <S1 dispatch={dispatch} onBack={handleBack} />
          )}
          {state.step === 's2' && (
            <S2
              validated={state.validated}
              onValidate={() =>
                dispatch({ type: 'FINISH' })
              }
            />
          )}
          {state.step === 's3' && (
            <S3
              entry={state.entry}
              importResult={state.importResult}
              onOpen={handleOpen}
            />
          )}
        </div>

        {/* Global footer — hidden on S3 */}
        {state.step !== 's3' && (
          <footer className="flex shrink-0 items-center justify-between border-t border-edge bg-surface-alt px-10 py-4">
            <div>
              {showBack && (
                <Button variant="secondary" onClick={handleBack}>
                  {t('onboarding.common.back')}
                </Button>
              )}
            </div>
            <div className="flex items-center gap-3">
              {showSkip && (
                <Button variant="secondary" onClick={handleSkip}>
                  {t('onboarding.common.skip')}
                </Button>
              )}
              {state.step === 's2' && (
                <Button variant="primary" onClick={handleFinish}>
                  {t('onboarding.common.finish')}
                </Button>
              )}
            </div>
          </footer>
        )}
      </main>
    </div>
  );
}
