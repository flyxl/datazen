import { useEffect, useRef, useState } from 'react';
import { CheckCircle2, Database, FlaskConical } from 'lucide-react';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import { sampleCommands } from '../../../commands/sample';
import { buildConnectionConfig } from '../../../lib/connectionFormModel';
import { newId } from '../../../components/connection/shared';
import { useConnectionStore } from '../../../stores/connectionStore';
import { StepTag } from './WelcomeStep';

/** Fixed identity of the seeded playground connection (see the journey proposal §4.3). */
export const SAMPLE_CONNECTION_NAME = 'Sample Playground';
export const SAMPLE_CONNECTION_GROUP = 'Sample';

interface SampleStepProps {
  onReady: (payload: { path: string; connectionName: string }) => void;
  onFailed: () => void;
  /** Existing connection name when step 1 was already completed once. */
  readyConnectionName?: string;
}

type SampleStatus = 'preparing' | 'ready' | 'failed';

/* ---------- S1 (sample entry): seed the bundled SQLite playground ---------- */

export function SampleStep({ onReady, onFailed, readyConnectionName }: Readonly<SampleStepProps>) {
  const { t } = useI18n();
  const connections = useConnectionStore((s) => s.connections);
  const saveConnection = useConnectionStore((s) => s.saveConnection);
  const [status, setStatus] = useState<SampleStatus>('preparing');
  const [samplePath, setSamplePath] = useState('');
  const [error, setError] = useState<string | null>(null);
  const seedingRef = useRef<Promise<string> | null>(null);
  // Keep the latest callbacks in refs: the seeding effect must run exactly once
  // per mount and must not re-fire when the shell re-renders with fresh inline
  // callbacks (that loop would keep dispatching SAMPLE_READY).
  const onReadyRef = useRef(onReady);
  const onFailedRef = useRef(onFailed);
  onReadyRef.current = onReady;
  onFailedRef.current = onFailed;

  useEffect(() => {
    let cancelled = false;

    // Seed once per mounted step, but keep the *promise* as the guard: the app
    // runs under <StrictMode>, which mounts → cleans up → mounts again. A
    // "did I start already?" boolean would make the remount skip the work and
    // throw away the first run's (cancelled) result, leaving the panel stuck on
    // "preparing the sample dataset" forever.
    seedingRef.current ??= seedSamplePlayground();

    void seedingRef.current
      .then((path) => {
        if (cancelled) return;
        setSamplePath(path);
        setStatus('ready');
        onReadyRef.current({ path, connectionName: SAMPLE_CONNECTION_NAME });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setStatus('failed');
        setError(typeof e === 'string' ? e : e instanceof Error ? e.message : String(e));
        onFailedRef.current();
      });

    return () => {
      cancelled = true;
    };
    // Mount-scoped by design: `seedSamplePlayground` reads mount-time state.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function seedSamplePlayground(): Promise<string> {
    const path = await sampleCommands.seedSampleDb();
    // Reuse an existing playground connection (re-entering step 1 must not pile
    // up duplicates).
    const existing = connections.find((c) => c.name === SAMPLE_CONNECTION_NAME);
    if (!existing) {
      await saveConnection(
        buildConnectionConfig({
          newId,
          unnamedLabel: SAMPLE_CONNECTION_NAME,
          name: SAMPLE_CONNECTION_NAME,
          databaseType: 'sqlite',
          host: '',
          port: '',
          database: path,
          schema: '',
          username: '',
          password: '',
          sslMode: 'disable',
          group: SAMPLE_CONNECTION_GROUP,
          colorTag: '#4fc3f7',
          readOnly: false,
          connectionOptions: {},
        }),
      );
    }
    return path;
  }

  const ready = status === 'ready' || Boolean(readyConnectionName);

  return (
    <div className="w-full max-w-[640px]" data-testid="onboarding-step-s1-sample">
      <StepTag label={t('onboarding.s1.stepLabel')} />
      <h2 className="mb-2.5 text-[25px] font-bold tracking-tight leading-tight text-fg">
        {t('onboarding.s1.sampleTitle')}
      </h2>
      <p className="mb-7 max-w-[56ch] text-[13.5px] leading-[1.65] text-fg-secondary">
        {t('onboarding.s1.sampleSubtitle')}
      </p>

      <div
        className={cn(
          'flex items-start gap-3 rounded-xl border p-4',
          ready ? 'border-green/25 bg-green/8' : 'border-edge bg-surface',
        )}
        data-testid="onboarding-sample-status"
      >
        <div
          className={cn(
            'flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border',
            ready
              ? 'border-green/25 bg-green/12 text-green'
              : 'border-accent/18 bg-accent/12 text-accent',
          )}
        >
          {ready ? (
            <CheckCircle2 className="h-[18px] w-[18px]" />
          ) : status === 'failed' ? (
            <Database className="h-[18px] w-[18px]" />
          ) : (
            <FlaskConical className="h-[18px] w-[18px] animate-pulse" />
          )}
        </div>
        <div className="min-w-0 flex-1">
          <b className="block text-[13.5px] font-semibold tracking-tight text-fg">
            {status === 'preparing'
              ? t('onboarding.s1.samplePreparing')
              : status === 'failed'
                ? t('onboarding.s1.sampleFailed')
                : t('onboarding.s1.sampleReady')}
          </b>
          <span className="mt-0.5 block text-[12.5px] leading-relaxed text-fg-secondary">
            {t('onboarding.s1.sampleReadyDesc')}
          </span>
          {samplePath && (
            <span
              className="mt-1 block truncate font-mono text-[11.5px] text-fg-muted"
              title={samplePath}
              data-testid="onboarding-sample-path"
            >
              {samplePath}
            </span>
          )}
          <span className="mt-1 block font-mono text-[11.5px] text-fg-muted">
            {t('onboarding.s1.sampleConnection', { name: SAMPLE_CONNECTION_NAME })}
          </span>
          {error && (
            <span
              className="mt-1 block select-text text-[12px] text-red"
              data-testid="onboarding-sample-error"
            >
              {error}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
