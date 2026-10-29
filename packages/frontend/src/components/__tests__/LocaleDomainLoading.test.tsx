import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { LocaleDomainLoading } from '../LocaleDomainLoading';

describe('LocaleDomainLoading', () => {
  it('renders page variant with role=status and aria-busy', () => {
    const { getByTestId, getByRole } = render(<LocaleDomainLoading testId="x-locale" />);
    const node = getByTestId('x-locale');
    expect(getByRole('status')).toBe(node);
    expect(node.getAttribute('aria-busy')).toBe('true');
    expect(node.className).toContain('h-screen');
  });

  it('renders section variant with bounded height', () => {
    const { container } = render(
      <LocaleDomainLoading variant="section" testId="s-locale" />,
    );
    const node = container.querySelector('[data-testid="s-locale"]')!;
    expect(node.className).toContain('min-h-[8rem]');
    expect(node.className).not.toContain('h-screen');
  });
});