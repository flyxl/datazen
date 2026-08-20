import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { QueryErrorPanel } from '../QueryErrorPanel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

describe('QueryErrorPanel', () => {
  let clipboardSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    // jsdom lacks navigator.clipboard; define a stub, then spy on its writer.
    Object.defineProperty(window.navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn() },
    });
    clipboardSpy = vi.spyOn(window.navigator.clipboard, 'writeText').mockResolvedValue(undefined);
  });

  it('renders the full error message as selectable text', () => {
    const longMessage =
      'syntax error at or near "FROM"\nmore details that must remain fully visible over many lines';
    render(<QueryErrorPanel message={longMessage} />);
    const pre = screen.getByTestId('query-error-message');
    expect(pre).toBeInTheDocument();
    expect(pre).toHaveClass('selectable');
    expect(pre.textContent).toBe(longMessage);
    // Not rendered as a table: no error column.
    expect(screen.queryByRole('table')).toBeNull();
  });

  it('copies the error message when the copy action is clicked', async () => {
    render(<QueryErrorPanel message="boom: constraint violated" />);
    const button = screen.getByTestId('query-copy-error');
    fireEvent.click(button);
    expect(clipboardSpy).toHaveBeenCalledWith('boom: constraint violated');
    // Feedback switches to "copied".
    expect(screen.getByText('common.copied')).toBeInTheDocument();
  });

  it('renders a diagnose action only when onDiagnose is provided', () => {
    const onDiagnose = vi.fn();
    const { unmount } = render(<QueryErrorPanel message="err" onDiagnose={onDiagnose} />);
    expect(screen.getByText('diagnosis.diagnose')).toBeInTheDocument();
    fireEvent.click(screen.getByText('diagnosis.diagnose'));
    expect(onDiagnose).toHaveBeenCalled();
    unmount();
    render(<QueryErrorPanel message="err" />);
    expect(screen.queryByText('diagnosis.diagnose')).toBeNull();
  });
});
