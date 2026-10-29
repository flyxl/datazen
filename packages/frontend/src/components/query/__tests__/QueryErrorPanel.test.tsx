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

  it('renders "Ask in Chat" button only when onAskInChat is provided', () => {
    const onAskInChat = vi.fn();
    const { unmount } = render(<QueryErrorPanel message="err" onAskInChat={onAskInChat} />);
    const button = screen.getByTestId('query-ask-in-chat');
    expect(button).toBeInTheDocument();
    expect(button).toHaveTextContent('query.editor.askInChat');
    fireEvent.click(button);
    expect(onAskInChat).toHaveBeenCalledTimes(1);
    unmount();
    render(<QueryErrorPanel message="err" />);
    expect(screen.queryByTestId('query-ask-in-chat')).toBeNull();
  });

  it('shows ask-in-chat button alongside other action buttons', () => {
    const onRetry = vi.fn();
    const onAskInChat = vi.fn();
    render(<QueryErrorPanel message="err" onRetry={onRetry} onAskInChat={onAskInChat} />);
    expect(screen.getByTestId('query-retry')).toBeInTheDocument();
    expect(screen.getByTestId('query-ask-in-chat')).toBeInTheDocument();
  });

  it('hides the entire action bar when no actions are provided', () => {
    const { container } = render(<QueryErrorPanel message="err" />);
    // No action bar should exist (no explain, fix, retry, or askInChat).
    expect(screen.queryByTestId('query-explain-error')).toBeNull();
    expect(screen.queryByTestId('query-fix-sql')).toBeNull();
    expect(screen.queryByTestId('query-retry')).toBeNull();
    expect(screen.queryByTestId('query-ask-in-chat')).toBeNull();
    // The container should not have the action bar div (mt-3 flex flex-wrap).
    const actionBars = container.querySelectorAll('.mt-3.flex.flex-wrap');
    expect(actionBars.length).toBe(0);
  });
});
