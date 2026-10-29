import { describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/react';
import { DetailPanelToggle } from '../DetailPanelToggle';

describe('DetailPanelToggle', () => {
  it('marks button as not pressed when panel is closed', () => {
    const { container } = render(<DetailPanelToggle open={false} onToggle={() => {}} />);
    const btn = container.querySelector('button')!;
    expect(btn.getAttribute('aria-pressed')).toBe('false');
  });

  it('marks button as pressed when panel is open', () => {
    const { container } = render(<DetailPanelToggle open onToggle={() => {}} />);
    const btn = container.querySelector('button')!;
    expect(btn.getAttribute('aria-pressed')).toBe('true');
  });

  it('applies active visual styling when panel is open', () => {
    const { container } = render(<DetailPanelToggle open onToggle={() => {}} />);
    const btn = container.querySelector('button')!;
    expect(btn.className).toMatch(/text-accent|text-blue/);
  });

  it('uses muted visual styling when panel is closed', () => {
    const { container } = render(<DetailPanelToggle open={false} onToggle={() => {}} />);
    const btn = container.querySelector('button')!;
    expect(btn.className).not.toMatch(/text-accent|text-blue/);
  });

  it('calls onToggle when clicked', () => {
    const onToggle = vi.fn();
    const { container } = render(<DetailPanelToggle open={false} onToggle={onToggle} />);
    fireEvent.click(container.querySelector('button')!);
    expect(onToggle).toHaveBeenCalledTimes(1);
  });
});
