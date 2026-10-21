import { useCallback, useState } from 'react';
import { aiCommands } from '../../commands/ai';
import { useI18n } from '../../hooks/useI18n';
import { buildCompareReportText } from './compareReport';
import type { CompareSummaryStats } from './mappingView';
import type { DataSyncEndpoints } from './useDataSyncEndpoints';
import type { DataSyncFeedback } from './useDataSyncFeedback';

export interface UseDataSyncExplainOptions {
  endpoints: DataSyncEndpoints;
  feedback: DataSyncFeedback;
  isAiConfigured: boolean;
  mappingResults: Parameters<typeof buildCompareReportText>[0];
  compareStats: CompareSummaryStats;
}

/**
 * AI explanation of the current Data Sync comparison.
 *
 * The compare report is sent to the configured AI provider; the answer is
 * rendered in a dialog and never leaves the window otherwise.
 */
export function useDataSyncExplain({
  endpoints,
  feedback,
  isAiConfigured,
  mappingResults,
  compareStats,
}: UseDataSyncExplainOptions) {
  const { t } = useI18n();
  const { setErrorMsg, setErrorOpen } = feedback;
  const { sourceSession, targetSession, sourceDatabase, targetDatabase } = endpoints;

  const [explainOpen, setExplainOpen] = useState(false);
  const [explainLoading, setExplainLoading] = useState(false);
  const [explainText, setExplainText] = useState('');

  const handleExplainDiff = useCallback(() => {
    if (!isAiConfigured) {
      setErrorMsg(t('sync.explainDiffNoAi'));
      setErrorOpen(true);
      return;
    }
    const report = buildCompareReportText(mappingResults, compareStats);
    const prompt = t('sync.explainDiffPrompt', { report });
    setExplainOpen(true);
    setExplainLoading(true);
    setExplainText('');
    void (async () => {
      try {
        const connectionId = sourceSession?.dbSessionId ?? targetSession?.dbSessionId;
        const text = await aiCommands.chat({
          dbSessionId: connectionId,
          database: sourceDatabase || targetDatabase || undefined,
          messages: [{ role: 'user', content: prompt }],
          requestId: crypto.randomUUID(),
          includeSchema: false,
        });
        setExplainText(text);
      } catch (e) {
        setExplainText('');
        setExplainOpen(false);
        setErrorMsg(
          `${t('sync.explainDiffFailed')}: ${e instanceof Error ? e.message : String(e)}`,
        );
        setErrorOpen(true);
      } finally {
        setExplainLoading(false);
      }
    })();
  }, [
    isAiConfigured,
    mappingResults,
    compareStats,
    sourceSession,
    targetSession,
    sourceDatabase,
    targetDatabase,
    t,
  ]);

  return {
    explainOpen,
    setExplainOpen,
    explainLoading,
    explainText,
    handleExplainDiff,
  };
}

export type DataSyncExplain = ReturnType<typeof useDataSyncExplain>;
