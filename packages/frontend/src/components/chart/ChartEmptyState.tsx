import { BarChart3 } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';

interface ChartEmptyStateProps {
  reason: 'noNumericField' | 'noData' | 'noConfig';
}

export function ChartEmptyState({ reason }: ChartEmptyStateProps) {
  const { t } = useI18n();
  const messages: Record<string, string> = {
    noNumericField: t('chart.empty.noNumericField'),
    noData: t('chart.empty.noData'),
    noConfig: t('chart.empty.noConfig'),
  };

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 text-fg-muted">
      <BarChart3 className="h-12 w-12 opacity-30" />
      <p className="text-sm">{messages[reason]}</p>
    </div>
  );
}
