import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { ErrorBoundary } from '../ErrorBoundary';

vi.mock('../TitleBar', () => ({ TitleBar: () => <div data-testid="title-bar" /> }));

afterEach(cleanup);

function BrokenView(): never {
  throw new Error('render failed');
}

describe('ErrorBoundary', () => {
  it('renders a localized, actionable fallback without crashing the shell', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

    render(
      <ErrorBoundary>
        <BrokenView />
      </ErrorBoundary>,
    );

    expect(screen.getByTestId('title-bar')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Error' })).toBeInTheDocument();
    expect(screen.getByText('render failed')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Close' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    expect(consoleError).toHaveBeenCalled();
    consoleError.mockRestore();
  });
});
