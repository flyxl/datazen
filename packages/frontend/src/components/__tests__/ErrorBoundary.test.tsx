import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { ErrorBoundary } from '../ErrorBoundary';
import { enCopy } from '../../test/enCopy';

// Form-split F2: ErrorBoundary renders the chrome SLOT, not a host component.
vi.mock('@datazen/extension-points', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/extension-points')>()),
  WindowChromeSlot: () => <div data-testid="title-bar" />,
}));

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

    // The contract is "heading + two actionable buttons, each carrying a real
    // localised accessible name". The wording belongs to the dictionary, so the
    // expected names are read from it instead of being hard-coded here — and
    // they go through enCopy(), which fails the moment a key is renamed,
    // deleted or emptied, instead of degrading the locator into `name: undefined`
    // (which testing-library reads as "any name" and matches unconditionally).
    expect(screen.getByTestId('title-bar')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: enCopy('common.error') })).toBeInTheDocument();
    expect(screen.getByText('render failed')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: enCopy('common.close') })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: enCopy('common.retry') })).toBeInTheDocument();
    expect(consoleError).toHaveBeenCalled();
    consoleError.mockRestore();
  });
});
