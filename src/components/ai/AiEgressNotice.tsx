import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import type { ContextItem } from '../../types';

interface AiEgressNoticeProps {
  contextItems?: ContextItem[];
}

export function AiEgressNotice({ contextItems }: AiEgressNoticeProps) {
  const { t } = useI18n();
  const aiStrictEgress = useSettingsStore((s) => s.settings.aiStrictEgress);
  const hasContextFiles =
    contextItems?.some((item) => item.kind === 'file' || item.kind === 'dir') ?? false;
  const showRelaxed = aiStrictEgress === false;
  const showContext = hasContextFiles;

  if (!showRelaxed && !showContext) {
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
      {messages.map((message) => (
        <p key={message}>{message}</p>
      ))}
    </div>
  );
}
