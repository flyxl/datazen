import { useCallback, useState } from 'react';
import {
  Database,
  Gauge,
  Loader2,
  Plus,
  Sparkles,
  Upload,
  Workflow,
  type LucideIcon,
} from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { MenuBar } from '../../components/MenuBar';
import { ThemeToggle } from '../../components/ThemeToggle';
import { Button } from '../../components/ui/Button';
import { ResultMessageDialog } from '../../components/ui/ResultMessageDialog';
import { sampleDataCommands } from '../../commands/sampleData';
import { useI18n } from '../../hooks/useI18n';
import { openConnectionShareDialog } from '../../lib/connectionShare';
import { openNewConnectionDialog, PENDING_CONNECTION_KEY } from '../../lib/windowManager';
import { useConnectionStore } from '../../stores/connectionStore';
import { useOnboardingStore } from '../../stores/onboardingStore';

interface FeatureItemProps {
  icon: LucideIcon;
  title: string;
  description: string;
}

function FeatureItem({ icon: Icon, title, description }: Readonly<FeatureItemProps>) {
  return (
    <div className="rounded-xl border border-edge bg-surface-alt p-4">
      <div className="mb-3 inline-flex rounded-md bg-surface p-2 text-fg-secondary">
        <Icon className="h-4 w-4" />
      </div>
      <h3 className="text-sm font-medium text-fg">{title}</h3>
      <p className="mt-1 text-sm text-fg-muted">{description}</p>
    </div>
  );
}

export function WelcomePage() {
  const { t } = useI18n();
  const [sampleLoading, setSampleLoading] = useState(false);
  const [errorDialogOpen, setErrorDialogOpen] = useState(false);
  const [errorDialogText, setErrorDialogText] = useState('');

  const features: FeatureItemProps[] = [
    {
      icon: Database,
      title: t('welcome.feature.connections.title'),
      description: t('welcome.feature.connections.description'),
    },
    {
      icon: Gauge,
      title: t('welcome.feature.dashboard.title'),
      description: t('welcome.feature.dashboard.description'),
    },
    {
      icon: Workflow,
      title: t('welcome.feature.workflow.title'),
      description: t('welcome.feature.workflow.description'),
    },
    {
      icon: Sparkles,
      title: t('common.aiAssistant'),
      description: t('welcome.feature.ai.description'),
    },
  ];

  const handleOpenSampleDb = useCallback(async () => {
    if (sampleLoading) return;
    setSampleLoading(true);
    try {
      const config = await sampleDataCommands.initSampleDatabase();
      await useConnectionStore.getState().fetchConnections();
      localStorage.setItem(PENDING_CONNECTION_KEY, JSON.stringify({ connectionId: config.id }));
      useOnboardingStore.getState().startOnboarding(config.id);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setErrorDialogText(message);
      setErrorDialogOpen(true);
    } finally {
      setSampleLoading(false);
    }
  }, [sampleLoading]);

  const handleCreateConnection = useCallback(() => {
    if (useOnboardingStore.getState().status === 'not_started') {
      useOnboardingStore.getState().startOnboarding();
    }
    openNewConnectionDialog();
  }, []);

  const handleSkipOnboarding = useCallback(() => {
    useOnboardingStore.getState().skipOnboarding();
  }, []);

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface text-fg" data-testid="welcome-page">
      <TitleBar
        title={t('menu.appName')}
        leftContent={<MenuBar />}
        rightContent={<ThemeToggle />}
      />

      <div className="flex flex-1 items-center justify-center overflow-auto px-6 py-10">
        <div className="w-full max-w-2xl">
          <div className="text-center">
            <img
              src="./logo.png"
              alt=""
              data-testid="welcome-app-icon"
              className="mx-auto h-16 w-16"
            />
            <h1 className="mt-4 text-2xl font-semibold text-fg">{t('welcome.title')}</h1>
            <p className="mt-2 text-sm text-fg-muted">{t('welcome.subtitle')}</p>
          </div>

          <div
            className="mt-8 rounded-xl border border-accent/30 bg-accent/5 p-5"
            data-testid="welcome-onboarding-banner"
          >
            <h2 className="text-sm font-semibold text-fg">{t('welcome.onboarding.title')}</h2>
            <ol className="mt-3 space-y-1.5 text-sm text-fg-muted">
              <li>{t('welcome.onboarding.step1')}</li>
              <li>{t('welcome.onboarding.step2')}</li>
              <li>{t('welcome.onboarding.step3')}</li>
            </ol>
          </div>

          <div className="mt-6 flex flex-col items-center gap-3">
            <Button
              data-testid="welcome-open-sample"
              onClick={() => void handleOpenSampleDb()}
              disabled={sampleLoading}
              className="w-full sm:w-auto"
            >
              {sampleLoading ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Database className="h-4 w-4" />
              )}
              {t('welcome.openSampleDb')}
            </Button>
            <p className="max-w-md text-center text-xs text-fg-muted">
              {t('welcome.openSampleDbHint')}
            </p>
          </div>

          <div className="mt-6 flex flex-col items-center gap-3">
            <div className="flex flex-wrap justify-center gap-3">
              <Button
                variant="secondary"
                data-testid="welcome-create-connection"
                onClick={handleCreateConnection}
              >
                <Plus className="h-4 w-4" />
                {t('welcome.createConnection')}
              </Button>
              <Button
                variant="secondary"
                data-testid="welcome-import-connection"
                onClick={() => openConnectionShareDialog('import')}
              >
                <Upload className="h-4 w-4" />
                {t('common.importConnections')}
              </Button>
            </div>
            <p className="max-w-md text-center text-xs text-fg-muted">
              {t('welcome.importConnectionHint')}
            </p>
          </div>

          <button
            type="button"
            data-testid="welcome-skip-onboarding"
            onClick={handleSkipOnboarding}
            className="mt-4 w-full text-center text-xs text-fg-muted underline-offset-2 hover:text-fg hover:underline"
          >
            {t('welcome.skipOnboarding')}
          </button>

          <div className="mt-8 grid grid-cols-1 gap-3 sm:grid-cols-2">
            {features.map((feature) => (
              <FeatureItem key={feature.title} {...feature} />
            ))}
          </div>
        </div>
      </div>

      <ResultMessageDialog
        open={errorDialogOpen}
        kind="error"
        message={errorDialogText}
        onClose={() => setErrorDialogOpen(false)}
      />
    </div>
  );
}
