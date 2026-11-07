import { Sparkles } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { useI18n } from '../../../hooks/useI18n';
import { ER_DECLARED_COLOR, ER_PREDICTED_COLOR, ER_PREDICTED_DASH } from './relationStyle';

/**
 * A line swatch drawn with the exact stroke the canvas uses for that kind.
 *
 * Rendering the real stroke, dash included, is what makes the legend legible at a
 * glance: the user matches what they see here against what they see on the canvas
 * rather than translating a colour name into a line.
 */
function LineSwatch({ color, dashed = false }: { color: string; dashed?: boolean }) {
  return (
    <svg width="26" height="8" viewBox="0 0 26 8" aria-hidden="true" className="shrink-0">
      <line
        x1="1"
        y1="4"
        x2="25"
        y2="4"
        stroke={color}
        strokeWidth="2"
        strokeDasharray={dashed ? ER_PREDICTED_DASH : undefined}
      />
    </svg>
  );
}

export interface ErRelationLegendProps {
  declaredCount: number;
  predictedCount: number;
  /** Whether inference is on right now. */
  predictionEnabled: boolean;
  /** Requested new state for the shared "smart foreign key prediction" setting. */
  onTogglePrediction: (enabled: boolean) => void;
}

/**
 * The diagram's key to its own lines, and the switch that adds the inferred ones.
 *
 * The button and the Settings → Editor toggle are the same persisted setting, so
 * there is one answer to "is prediction on" rather than a page-local override
 * that could contradict the settings page.
 */
export function ErRelationLegend({
  declaredCount,
  predictedCount,
  predictionEnabled,
  onTogglePrediction,
}: ErRelationLegendProps) {
  const { t } = useI18n();

  return (
    <div
      data-testid="er-diagram-legend"
      className="w-56 rounded-lg border border-edge bg-surface/85 px-3 py-2 text-xs backdrop-blur"
    >
      <div className="mb-1.5 text-[11px] font-medium uppercase tracking-wide text-fg-muted">
        {t('erDiagram.legendTitle')}
      </div>
      <div className="flex items-center gap-2 text-fg-secondary" data-testid="er-legend-declared">
        <LineSwatch color={ER_DECLARED_COLOR} />
        <span className="flex-1 truncate" title={t('erDiagram.legendDeclared')}>
          {t('erDiagram.legendDeclared')}
        </span>
        <span className="tabular-nums text-fg-muted">{declaredCount}</span>
      </div>
      <div
        className={`mt-1 flex items-center gap-2 ${
          predictionEnabled ? 'text-fg-secondary' : 'text-fg-muted opacity-60'
        }`}
        data-testid="er-legend-predicted"
      >
        <LineSwatch color={ER_PREDICTED_COLOR} dashed />
        <span className="flex-1 truncate" title={t('erDiagram.legendPredicted')}>
          {t('erDiagram.legendPredicted')}
        </span>
        {/* The inferred count is disclosed separately rather than folded into the
            declared total, so a guess never reads as a constraint. */}
        <span className="tabular-nums text-fg-muted" data-testid="er-predicted-count">
          {predictedCount}
        </span>
      </div>
      <Button
        variant={predictionEnabled ? 'secondary' : 'ghost'}
        size="sm"
        className="mt-2 w-full gap-1.5"
        aria-pressed={predictionEnabled}
        title={t('erDiagram.predictionToggleHint')}
        data-testid="er-diagram-toggle-prediction"
        onClick={() => onTogglePrediction(!predictionEnabled)}
      >
        <Sparkles className={`h-3.5 w-3.5 ${predictionEnabled ? 'text-warning' : ''}`} />
        <span className="truncate">
          {predictionEnabled ? t('erDiagram.predictionOn') : t('erDiagram.predictionOff')}
        </span>
      </Button>
    </div>
  );
}
