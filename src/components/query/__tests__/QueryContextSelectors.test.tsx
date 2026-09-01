import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { QueryContextSelectors } from '../QueryContextSelectors';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

describe('QueryContextSelectors', () => {
  it('does not reserve trailing toolbar space for a compact database selector', () => {
    render(
      <QueryContextSelectors
        isMultiDb
        isPathHierarchy={false}
        databases={['datazen_demo']}
        currentDatabase="datazen_demo"
        namespaceTree={{}}
        pathAliases={{}}
        contextPath={[]}
        onSelectLevel={vi.fn()}
      />,
    );

    const host = screen.getByTestId('query-context-selectors');
    expect(host).toHaveClass('min-w-0');
    expect(host).not.toHaveClass('min-w-[9rem]');
    expect(screen.getByRole('textbox').parentElement).toHaveClass('!max-w-[7rem]');
  });
});
