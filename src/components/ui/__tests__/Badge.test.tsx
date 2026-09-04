import { describe, expect, it, afterEach } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { Badge } from '../Badge';

afterEach(cleanup);

describe('[tester] Badge semantic tone tokens', () => {
  it.each([
    ['neutral', 'bg-surface-raised', 'text-fg-secondary', 'border-edge'],
    ['success', 'bg-success/10', 'text-success', 'border-success/20'],
    ['warning', 'bg-warning/10', 'text-warning', 'border-warning/20'],
    ['danger', 'bg-danger/10', 'text-danger', 'border-danger/20'],
    ['accent', 'bg-accent/10', 'text-accent', 'border-accent/20'],
  ] as const)('renders %s tone with semantic token classes', (tone, bg, text, border) => {
    render(<Badge tone={tone}>{`${tone}-label`}</Badge>);
    const badge = screen.getByText(`${tone}-label`);
    expect(badge.className).toContain(bg);
    expect(badge.className).toContain(text);
    expect(badge.className).toContain(border);
    expect(badge.className).not.toMatch(/green-500|amber-500|red-500|blue-500/);
  });
});
