import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { CopyableError } from '../CopyableError';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

describe('CopyableError', () => {
  let clipboardSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    Object.defineProperty(window.navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn() },
    });
    clipboardSpy = vi.spyOn(window.navigator.clipboard, 'writeText').mockResolvedValue(undefined);
  });

  it('renders the full message with selectable styling', () => {
    const longMessage = 'line one\nline two with more detail';
    render(<CopyableError message={longMessage} />);
    const el = screen.getByTestId('copyable-error-message');
    expect(el).toHaveClass('selectable', 'whitespace-pre-wrap', 'break-words');
    expect(el.textContent).toBe(longMessage);
  });

  it('copies the message when copyButton is enabled', () => {
    render(<CopyableError message="connection refused" copyButton />);
    fireEvent.click(screen.getByTestId('copyable-error-copy'));
    expect(clipboardSpy).toHaveBeenCalledWith('connection refused');
    expect(screen.getByText('common.copied')).toBeInTheDocument();
  });
});
