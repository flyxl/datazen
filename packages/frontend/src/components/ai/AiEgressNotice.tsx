import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import type { ContextItem } from '../../types';

interface AiEgressNoticeProps {
  contextItems?: ContextItem[];
  /** Host-provided tool egress scope, delivered on the first stream chunk. */
  egressSummary?: string | null;
  /** Database currently bound to the assistant, shown alongside the summary. */
  database?: string | null;
}

export function AiEgressNotice({ contextItems, egressSummary, database }: AiEgressNoticeProps) {
  const { t } = useI18n();
  const aiStrictEgress = useSettingsStore((s) => s.settings.aiStrictEgress);
  const contextFiles = contextItems?.filter((item) => item.kind === 'file' || item.kind === 'dir');
  const hasContextFiles = (contextFiles?.length ?? 0) > 0;
  const summary = egressSummary?.trim() ?? '';
  const showRelaxed = aiStrictEgress === false;
  const showContext = hasContextFiles;
  const showSummary = summary.length > 0;
  const databaseLabel = database?.trim() ?? '';
  const showDatabase = databaseLabel.length > 0;

  if (!showRelaxed && !showContext && !showSummary) {
    return null;
  }

  const messages: string[] = [];
  if (showRelaxed) {
    messages.push(t('chat.egressNoticeRelaxed'));
  }
  if (showContext) {
    messages.push(t('chat.egressNoticeContext'));
  }

  return (
    <div
      className="rounded border border-amber-500/30 bg-amber-500/10 px-2 py-1.5 text-[11px] text-amber-200/90"
      data-testid="ai-egress-notice"
    >
      {showSummary && (
        <p data-testid="ai-egress-summary" className="font-medium">
          {summary}
        </p>
      )}
      {showDatabase && (
        <p data-testid="ai-egress-database" className="text-amber-200/70">
          {t('chat.egressDatabase', { database: databaseLabel })}
        </p>
      )}
      {messages.map((message) => (
        <p key={message}>{message}</p>
      ))}
    </div>
  );
}
