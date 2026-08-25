import { useCallback, useEffect } from 'react';
import { ArrowDownToLine, Loader2, Settings, Stethoscope, X } from 'lucide-react';
import { Button } from '../ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { openSettingsWindow } from '../../lib/windowManager';

interface DiagnosisPanelProps {
  connectionId: string;
  database: string;
  sql: string;
  errorMessage: string;
  onApplySql: (sql: string) => void;
  onClose: () => void;
}

export function DiagnosisPanel({
  connectionId,
  database,
  sql,
  errorMessage,
  onApplySql,
  onClose,
}: DiagnosisPanelProps) {
  const { t } = useI18n();
  const diagnosis = useAiStore((s) => s.diagnosis);
  const isDiagnosing = useAiStore((s) => s.isDiagnosing);
  const diagnosisError = useAiStore((s) => s.diagnosisError);
  const isConfigured = useAiStore((s) => s.isConfigured);
  const diagnoseError = useAiStore((s) => s.diagnoseError);
  const clearDiagnosis = useAiStore((s) => s.clearDiagnosis);

  const handleDiagnose = useCallback(() => {
    void diagnoseError({ dbSessionId: connectionId, database, sql, errorMessage });
  }, [diagnoseError, connectionId, database, sql, errorMessage]);

  useEffect(() => {
    if (!diagnosis && !isDiagnosing && !diagnosisError) {
      handleDiagnose();
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const handleApply = useCallback(() => {
    if (diagnosis?.suggestedSql) {
      onApplySql(diagnosis.suggestedSql);
      clearDiagnosis();
      onClose();
    }
  }, [diagnosis, onApplySql, clearDiagnosis, onClose]);

  const handleClose = useCallback(() => {
    clearDiagnosis();
    onClose();
  }, [clearDiagnosis, onClose]);

  if (!isConfigured) {
    return (
      <div className="border-t border-edge bg-surface-alt px-4 py-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 text-xs text-fg-muted">
            <Stethoscope className="h-3.5 w-3.5" />
            <span>{t('diagnosis.notConfigured')}</span>
            <Button variant="primary" className="h-5 gap-1 px-1.5 text-[10px]" onClick={() => openSettingsWindow('ai')}>
              <Settings className="h-2.5 w-2.5" />
              {t('settings.ai.goToConfigure')}
            </Button>
          </div>
          <button type="button" onMouseDown={(e) => e.preventDefault()} className="text-fg-muted hover:text-fg" onClick={handleClose}>
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="border-t border-blue-500/20 bg-blue-500/5">
      <div className="flex items-center justify-between border-b border-edge px-3 py-1.5">
        <div className="flex items-center gap-2">
          <Stethoscope className="h-3.5 w-3.5 text-blue-400" />
          <span className="text-xs font-medium text-fg">{t('diagnosis.title')}</span>
        </div>
        <div className="flex items-center gap-1.5">
          {!diagnosis && !isDiagnosing && (
            <Button
              variant="primary"
              className="h-6 gap-1 px-2 text-[11px]"
              onClick={handleDiagnose}
            >
              <Stethoscope className="h-3 w-3" />
              {t('diagnosis.diagnose')}
            </Button>
          )}
          <button type="button" onMouseDown={(e) => e.preventDefault()} className="text-fg-muted hover:text-fg" onClick={handleClose}>
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      <div className="max-h-[200px] overflow-auto px-3 py-2">
        {isDiagnosing && (
          <div className="flex items-center gap-2 py-3 text-xs text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('diagnosis.diagnosing')}
          </div>
        )}

        {diagnosisError && (
          <div className="rounded border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400">
            {diagnosisError}
          </div>
        )}

        {diagnosis && (
          <div className="space-y-2">
            <div>
              <div className="mb-1 text-[11px] font-medium uppercase tracking-wider text-fg-muted">
                {t('diagnosis.explanation')}
              </div>
              <p className="text-xs text-fg-secondary">{diagnosis.explanation}</p>
            </div>

            {diagnosis.changes.length > 0 && (
              <div>
                <div className="mb-1 text-[11px] font-medium uppercase tracking-wider text-fg-muted">
                  {t('diagnosis.changes')}
                </div>
                <ul className="list-inside list-disc text-xs text-fg-secondary">
                  {diagnosis.changes.map((change, i) => (
                    <li key={i}>{change}</li>
                  ))}
                </ul>
              </div>
            )}

            {diagnosis.suggestedSql && (
              <div>
                <div className="mb-1 text-[11px] font-medium uppercase tracking-wider text-fg-muted">
                  {t('diagnosis.suggestedSql')}
                </div>
                <pre className="rounded border border-edge bg-surface p-2 font-mono text-xs text-green-400">
                  {diagnosis.suggestedSql}
                </pre>
                <div className="mt-1.5">
                  <Button
                    variant="primary"
                    className="h-6 gap-1 px-2 text-[11px]"
                    onClick={handleApply}
                  >
                    <ArrowDownToLine className="h-3 w-3" />
                    {t('diagnosis.applySuggested')}
                  </Button>
                </div>
              </div>
            )}
          </div>
        )}

        {!diagnosis && !isDiagnosing && !diagnosisError && (
          <p className="py-2 text-xs text-fg-muted">
            {t('diagnosis.diagnose')}
          </p>
        )}
      </div>
    </div>
  );
}
