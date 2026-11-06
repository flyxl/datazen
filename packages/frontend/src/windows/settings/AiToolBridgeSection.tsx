import { useEffect, useState } from 'react';
import { Bot, Workflow } from 'lucide-react';
import { useSettingsStore } from '../../stores/settingsStore';
import { useWappStore } from '../../stores/wappStore';
import { aiCommands } from '../../commands/ai';
import { useI18n } from '../../hooks/useI18n';
import { toErrorMessage } from '../../lib/errors';
import type { WorkflowListItem } from '../../types';
import { SectionTitle, ToggleRow } from './settingsUi';
import { tid } from '../../lib/tid';

/**
 * Opt-in switches for model-facing bridge tools. A plugin/workflow is absent
 * from the Agent tool list until its id is stored here; the host re-resolves
 * both lists on every turn, so turning a switch off removes the tool at once.
 */
export function AiToolBridgeSection() {
  const { t } = useI18n();
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const wapps = useWappStore((s) => s.wapps) ?? [];
  const fetchWapps = useWappStore((s) => s.fetch);
  const [workflows, setWorkflows] = useState<WorkflowListItem[]>([]);
  const [workflowError, setWorkflowError] = useState<string | null>(null);

  useEffect(() => {
    void fetchWapps?.();
    void aiCommands
      .workflowList()
      .then((rows) => setWorkflows(rows ?? []))
      .catch((cause) => setWorkflowError(toErrorMessage(cause)));
  }, [fetchWapps]);

  const pluginIds = settings.aiPluginToolIds ?? [];
  const workflowIds = settings.aiWorkflowToolIds ?? [];
  const bridgePlugins = wapps.filter((wapp) => (wapp.aiTools?.length ?? 0) > 0);

  const togglePlugin = (pluginId: string, enabled: boolean) => {
    const next = enabled
      ? [...pluginIds.filter((id) => id !== pluginId), pluginId]
      : pluginIds.filter((id) => id !== pluginId);
    void updateSettings({ ...settings, aiPluginToolIds: next });
  };

  const toggleWorkflow = (workflowId: string, enabled: boolean) => {
    const next = enabled
      ? [...workflowIds.filter((id) => id !== workflowId), workflowId]
      : workflowIds.filter((id) => id !== workflowId);
    void updateSettings({ ...settings, aiWorkflowToolIds: next });
  };

  return (
    <div className="space-y-4" {...tid('ai-tool-bridge-section')}>
      <SectionTitle hint={t('settings.ai.toolsHint')}>
        {t('settings.ai.toolsTitle')}
      </SectionTitle>

      <div className="space-y-3 rounded-xl border border-edge bg-surface p-4">
        <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-fg-muted">
          <Bot className="h-3.5 w-3.5" />
          {t('settings.ai.toolsPlugins')}
        </div>
        {bridgePlugins.length === 0 ? (
          <p className="text-xs text-fg-muted">{t('settings.ai.toolsNoPlugins')}</p>
        ) : (
          bridgePlugins.map((wapp) => {
            const enabled = pluginIds.includes(wapp.id);
            return (
              <ToggleRow
                key={wapp.id}
                label={wapp.name}
                hint={
                  wapp.enabled
                    ? `${wapp.id} · ${(wapp.aiTools ?? [])
                        .map((tool) => `${tool.name}${tool.readOnly ? ' (read-only)' : ''}`)
                        .join(', ')}`
                    : t('settings.ai.toolsPluginDisabled')
                }
                checked={enabled}
                disabled={!wapp.enabled}
                onChange={(value) => togglePlugin(wapp.id, value)}
                testId={`ai-plugin-tool-${wapp.id}`}
              />
            );
          })
        )}

        <div className="flex items-center gap-2 border-t border-edge/60 pt-3 text-xs font-medium uppercase tracking-wide text-fg-muted">
          <Workflow className="h-3.5 w-3.5" />
          {t('settings.ai.toolsWorkflows')}
        </div>
        {workflowError && <p className="select-text text-xs text-red-500">{workflowError}</p>}
        {workflows.length === 0 && !workflowError ? (
          <p className="text-xs text-fg-muted">{t('settings.ai.toolsNoWorkflows')}</p>
        ) : (
          workflows.map((workflow) => (
            <ToggleRow
              key={workflow.id}
              label={workflow.name}
              hint={workflow.description || workflow.id}
              checked={workflowIds.includes(workflow.id)}
              onChange={(value) => toggleWorkflow(workflow.id, value)}
              testId={`ai-workflow-tool-${workflow.id}`}
            />
          ))
        )}
      </div>
    </div>
  );
}
