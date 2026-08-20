import {
  Database,
  LayoutDashboard,
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
import { useI18n } from '../../hooks/useI18n';
import { openConnectionShareDialog } from '../../lib/connectionShare';
import { openNewConnectionDialog } from '../../lib/windowManager';

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

  const features: FeatureItemProps[] = [
    {
      icon: Database,
      title: t('welcome.feature.connections.title'),
      description: t('welcome.feature.connections.description'),
    },
    {
      icon: LayoutDashboard,
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
      title: t('welcome.feature.ai.title'),
      description: t('welcome.feature.ai.description'),
    },
  ];

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

          <div className="mt-8 grid grid-cols-1 gap-3 sm:grid-cols-2">
            {features.map((feature) => (
              <FeatureItem key={feature.title} {...feature} />
            ))}
          </div>

          <div className="mt-8 flex flex-col items-center gap-3">
            <div className="flex flex-wrap justify-center gap-3">
              <Button
                data-testid="welcome-create-connection"
                onClick={() => openNewConnectionDialog()}
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
                {t('welcome.importConnection')}
              </Button>
            </div>
            <p className="max-w-md text-center text-xs text-fg-muted">
              {t('welcome.importConnectionHint')}
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
