import { useEffect, useMemo, useState } from 'react';
import { BookOpen } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { useSettings } from '../../hooks/useSettings';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import { getUrlParam } from '../../lib/windowKind';
import { cn } from '../../lib/cn';
import { getDocsSections, type DocsSectionId } from './content';

const VALID_IDS = new Set<DocsSectionId>([
  'overview',
  'features',
  'ai',
  'context',
  'workflows',
  'opsDashboard',
  'schemaDiff',
]);

export function DocsWindow() {
  useSettings();
  const { t } = useI18n();
  const language = useSettingsStore((s) => s.settings.language);
  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const sections = useMemo(() => getDocsSections(language || 'zh-CN'), [language]);

  const [activeId, setActiveId] = useState<DocsSectionId>(() => {
    const raw = getUrlParam('section');
    if (raw && VALID_IDS.has(raw as DocsSectionId)) return raw as DocsSectionId;
    return 'overview';
  });

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    if (!sections.some((s) => s.id === activeId) && sections[0]) {
      setActiveId(sections[0].id);
    }
  }, [sections, activeId]);

  const active = sections.find((s) => s.id === activeId) ?? sections[0];

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      <TitleBar
        title={
          <span className="inline-flex items-center gap-1.5">
            <BookOpen className="h-3.5 w-3.5 text-fg-muted" />
            {t('win.docs')}
          </span>
        }
      />

      <div className="flex min-h-0 flex-1">
        <aside className="flex w-48 shrink-0 flex-col border-r border-edge bg-surface-alt py-2">
          <div className="px-3 pb-2 text-[11px] font-medium uppercase tracking-wide text-fg-muted">
            {t('docs.nav')}
          </div>
          {sections.map((s) => (
            <button
              key={s.id}
              type="button"
              onClick={() => setActiveId(s.id)}
              className={cn(
                'mx-1 rounded px-2.5 py-1.5 text-left text-xs transition-colors',
                activeId === s.id
                  ? 'bg-accent/15 text-accent font-medium'
                  : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
              )}
            >
              {s.title}
            </button>
          ))}
        </aside>

        <main className="min-w-0 flex-1 overflow-y-auto px-6 py-5">
          {active && (
            <>
              <h1 className="mb-4 text-lg font-semibold text-fg">{active.title}</h1>
              <div
                className={cn(
                  'docs-prose text-sm leading-relaxed text-fg-secondary',
                  '[&_h2]:mt-6 [&_h2]:mb-3 [&_h2]:scroll-mt-4 [&_h2]:border-b [&_h2]:border-edge [&_h2]:pb-1.5 [&_h2]:text-base [&_h2]:font-semibold [&_h2]:text-fg',
                  '[&_h3]:mt-5 [&_h3]:mb-2 [&_h3]:scroll-mt-4 [&_h3]:text-[13px] [&_h3]:font-semibold [&_h3]:text-fg',
                  '[&_h4]:mt-4 [&_h4]:mb-1.5 [&_h4]:text-[13px] [&_h4]:font-medium [&_h4]:text-fg',
                  '[&_p]:mb-3',
                  '[&_ul]:mb-3 [&_ul]:list-disc [&_ul]:pl-5',
                  '[&_ol]:mb-3 [&_ol]:list-decimal [&_ol]:pl-5',
                  '[&_li]:mb-1',
                  '[&_strong]:text-fg [&_strong]:font-medium',
                  '[&_blockquote]:mb-3 [&_blockquote]:border-l-2 [&_blockquote]:border-edge [&_blockquote]:pl-3 [&_blockquote]:text-fg-muted',
                  '[&_hr]:my-6 [&_hr]:border-edge',
                  '[&_table]:mb-4 [&_table]:w-full [&_table]:border-collapse [&_table]:text-left [&_table]:text-[12px]',
                  '[&_th]:border [&_th]:border-edge [&_th]:bg-surface-alt [&_th]:px-2 [&_th]:py-1.5 [&_th]:font-medium [&_th]:text-fg',
                  '[&_td]:border [&_td]:border-edge [&_td]:px-2 [&_td]:py-1.5 [&_td]:align-top',
                  '[&_code]:rounded [&_code]:bg-surface-alt [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[12px]',
                  '[&_pre]:mb-4 [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:border [&_pre]:border-edge [&_pre]:bg-surface-alt [&_pre]:p-3 [&_pre]:font-mono [&_pre]:text-[12px] [&_pre]:text-fg-secondary',
                  '[&_pre_code]:bg-transparent [&_pre_code]:p-0',
                  '[&_a]:text-accent [&_a]:underline',
                )}
                dangerouslySetInnerHTML={{ __html: active.html }}
              />
            </>
          )}
        </main>
      </div>
    </div>
  );
}
