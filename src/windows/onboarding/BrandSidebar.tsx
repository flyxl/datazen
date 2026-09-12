import { useEffect, useState } from 'react';
import { Database, Download, Gauge, Sparkles } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import {
  WIZARD_ASIDE_PX_CLASS,
  WIZARD_CHROME_TEXT_CLASS,
  WIZARD_FOOTER_HEIGHT_CLASS,
} from './onboardingLayout';

/**
 * Brand sidebar (left 352px) — matches the design prototype.
 *
 * The version row uses the same fixed height as the wizard footer
 * ({@link WIZARD_FOOTER_HEIGHT_CLASS}) and the sidebar has no bottom padding,
 * so `v{version}` and the step indicator end on the same bottom line.
 */
export function BrandSidebar() {
  const { t } = useI18n();
  const [version, setVersion] = useState('…');

  useEffect(() => {
    import('@tauri-apps/api/app')
      .then((m) => m.getVersion())
      .then(setVersion)
      .catch(() => {});
  }, []);

  return (
    <aside
      className={cn(
        'relative flex w-[352px] shrink-0 flex-col overflow-hidden border-r border-edge',
        WIZARD_ASIDE_PX_CLASS,
        'pt-[40px] pb-0',
      )}
    >
      {/* Gradient background — always dark as a brand anchor. */}
      <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(120%_70%_at_0%_0%,rgba(79,195,247,0.13)_0%,transparent_55%)] bg-[linear-gradient(180deg,#0e131c,#0a0e15)]" />

      {/* Brand */}
      <div className="relative mb-[38px] flex items-center gap-2.5">
        <div className="flex h-[21px] w-[21px] items-center justify-center text-accent">
          <Database className="h-[21px] w-[21px]" />
        </div>
        <span className="text-base font-bold tracking-tight text-fg">DataZen</span>
      </div>

      {/* Heading */}
      <h1 className="relative mb-3 text-[26px] font-bold leading-tight tracking-tight text-fg">
        {t('onboarding.sidebar.heading')}
      </h1>

      {/* Description */}
      <p className="relative mb-[34px] max-w-[32ch] text-[13.5px] leading-[1.65] text-fg-secondary">
        {t('onboarding.sidebar.desc')}
      </p>

      {/* Feature highlights */}
      <div className="relative flex flex-col">
        <FeatureItem
          icon={<Download className="h-[15px] w-[15px]" />}
          title={t('onboarding.sidebar.importTitle')}
          desc={t('onboarding.sidebar.importDesc')}
        />
        <FeatureItem
          icon={<Sparkles className="h-[15px] w-[15px]" />}
          title={t('onboarding.sidebar.aiTitle')}
          desc={t('onboarding.sidebar.aiDesc')}
        />
        <FeatureItem
          icon={<Gauge className="h-[15px] w-[15px]" />}
          title={t('onboarding.sidebar.dashboardTitle')}
          desc={t('onboarding.sidebar.dashboardDesc')}
        />
      </div>

      {/* Version — same bottom line as the wizard footer's step indicator. */}
      <div
        className={cn(
          'relative mt-auto flex shrink-0 items-center',
          WIZARD_FOOTER_HEIGHT_CLASS,
          WIZARD_CHROME_TEXT_CLASS,
        )}
        data-testid="onboarding-sidebar-foot"
      >
        <span data-testid="onboarding-version-label">v{version}</span>
      </div>
    </aside>
  );
}

function FeatureItem({
  icon,
  title,
  desc,
}: {
  icon: React.ReactNode;
  title: string;
  desc: string;
}) {
  return (
    <div className="flex gap-3 border-t border-edge/70 py-3 first:border-t-0 first:pt-0">
      <div className="flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-lg bg-accent/12 text-accent border border-accent/16">
        {icon}
      </div>
      <div>
        <b className="block text-[13px] font-semibold tracking-tight text-fg">{title}</b>
        <span className="mt-px block text-xs text-fg-muted leading-relaxed">{desc}</span>
      </div>
    </div>
  );
}
