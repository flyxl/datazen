import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen } from '@testing-library/react';
import { ErRelationLegend } from '../ErRelationLegend';
import { ER_DECLARED_COLOR, ER_PREDICTED_COLOR, ER_PREDICTED_DASH } from '../relationStyle';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

/** The `<line>` a legend swatch draws. */
function swatchStroke(testId: string): { stroke: string | null; dash: string | null } {
  const line = screen.getByTestId(testId).querySelector('line');
  expect(line).not.toBeNull();
  return {
    stroke: line!.getAttribute('stroke'),
    dash: line!.getAttribute('stroke-dasharray'),
  };
}

describe('ErRelationLegend', () => {
  it('names both kinds of line and counts them separately', () => {
    render(
      <ErRelationLegend
        declaredCount={7}
        predictedCount={3}
        predictionEnabled
        onTogglePrediction={() => {}}
      />,
    );

    expect(screen.getByTestId('er-legend-declared')).toHaveTextContent('erDiagram.legendDeclared');
    expect(screen.getByTestId('er-legend-declared')).toHaveTextContent('7');
    expect(screen.getByTestId('er-legend-predicted')).toHaveTextContent(
      'erDiagram.legendPredicted',
    );
    // The inferred count is disclosed on its own, never folded into the total.
    expect(screen.getByTestId('er-predicted-count')).toHaveTextContent('3');
  });

  it('draws the swatches with the strokes the canvas actually uses', () => {
    // A legend that lies about the line it describes is worse than no legend, so
    // the swatches are asserted against the shared constants, not against a copy.
    render(
      <ErRelationLegend
        declaredCount={1}
        predictedCount={1}
        predictionEnabled
        onTogglePrediction={() => {}}
      />,
    );

    expect(swatchStroke('er-legend-declared')).toEqual({
      stroke: ER_DECLARED_COLOR,
      dash: null,
    });
    expect(swatchStroke('er-legend-predicted')).toEqual({
      stroke: ER_PREDICTED_COLOR,
      dash: ER_PREDICTED_DASH,
    });
  });

  it('offers to turn prediction on when it is off, and reports it pressed when on', () => {
    const onToggle = vi.fn();
    render(
      <ErRelationLegend
        declaredCount={1}
        predictedCount={0}
        predictionEnabled={false}
        onTogglePrediction={onToggle}
      />,
    );

    const button = screen.getByTestId('er-diagram-toggle-prediction');
    expect(button).toHaveAttribute('aria-pressed', 'false');
    expect(button).toHaveTextContent('erDiagram.predictionOff');

    fireEvent.click(button);
    expect(onToggle).toHaveBeenCalledWith(true);
  });

  it('turns prediction back off when it is already on', () => {
    const onToggle = vi.fn();
    render(
      <ErRelationLegend
        declaredCount={1}
        predictedCount={4}
        predictionEnabled
        onTogglePrediction={onToggle}
      />,
    );

    const button = screen.getByTestId('er-diagram-toggle-prediction');
    expect(button).toHaveAttribute('aria-pressed', 'true');
    expect(button).toHaveTextContent('erDiagram.predictionOn');

    fireEvent.click(button);
    expect(onToggle).toHaveBeenCalledWith(false);
  });
});
