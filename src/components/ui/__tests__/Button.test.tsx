import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { Button } from '../Button';
import { ToolbarButton } from '../ToolbarButton';

afterEach(cleanup);

describe('Button', () => {
  it('does not preventDefault on mousedown so native focus works', () => {
    const { getByRole } = render(<Button>Test</Button>);
    const btn = getByRole('button');

    const event = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
    const prevented = !btn.dispatchEvent(event);
    expect(prevented).toBe(false);
  });

  it('includes keyboard focus-visible ring classes in base styles', () => {
    const { getByRole } = render(<Button>Test</Button>);
    const btn = getByRole('button');

    expect(btn.className).toContain('focus-visible:outline-none');
    expect(btn.className).toContain('focus-visible:ring-2');
    expect(btn.className).toContain('focus-visible:ring-accent/60');
  });

  it('fires onClick when clicked while an input has focus', () => {
    const onClick = vi.fn();
    const { container } = render(
      <div>
        <input placeholder="focused input" />
        <Button onClick={onClick}>Action</Button>
      </div>,
    );
    const input = container.querySelector('input')!;
    input.focus();
    expect(document.activeElement).toBe(input);

    const btn = container.querySelector('button')!;
    fireEvent.mouseDown(btn);
    fireEvent.mouseUp(btn);
    fireEvent.click(btn);

    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('invokes user-provided onMouseDown', () => {
    const onMouseDown = vi.fn();
    const { container } = render(<Button onMouseDown={onMouseDown}>Test</Button>);

    fireEvent.mouseDown(container.querySelector('button')!);
    expect(onMouseDown).toHaveBeenCalledTimes(1);
  });

  it('[tester] receives programmatic focus for keyboard navigation', () => {
    const { getByRole } = render(<Button>Focus me</Button>);
    const btn = getByRole('button', { name: 'Focus me' });
    btn.focus();
    expect(document.activeElement).toBe(btn);
  });

  it('[tester] renders secondary and danger variant classes', () => {
    const { rerender, getByRole } = render(<Button variant="secondary">Secondary</Button>);
    expect(getByRole('button').className).toContain('border-edge');

    rerender(<Button variant="danger">Danger</Button>);
    expect(getByRole('button').className).toContain('bg-red-500/90');
  });
});

describe('ToolbarButton', () => {
  it('defaults to ghost variant styling', () => {
    const { getByRole } = render(
      <ToolbarButton label="Refresh" icon={<span data-testid="icon">↻</span>} />,
    );
    const btn = getByRole('button', { name: 'Refresh' });

    expect(btn.className).toContain('bg-transparent');
    expect(btn.className).toContain('text-fg-secondary');
    expect(btn.className).not.toContain('bg-accent');
  });

  it('[tester] honors explicit primary variant override', () => {
    const { getByRole } = render(
      <ToolbarButton
        label="Run"
        variant="primary"
        icon={<span data-testid="icon">▶</span>}
      />,
    );
    const btn = getByRole('button', { name: 'Run' });
    expect(btn.className).toContain('bg-accent');
  });

  it('[tester] compact mode visually hides label but keeps aria-label', () => {
    const { getByRole, getByText } = render(
      <ToolbarButton compact label="Refresh" icon={<span>↻</span>} />,
    );
    const btn = getByRole('button', { name: 'Refresh' });
    expect(btn.className).toContain('h-7');
    expect(getByText('Refresh').className).toContain('sr-only');
  });

  it('[tester] uses explicit title when provided', () => {
    const { getByRole } = render(
      <ToolbarButton
        label="Refresh"
        title="Reload data"
        icon={<span>↻</span>}
      />,
    );
    expect(getByRole('button', { name: 'Refresh' })).toHaveAttribute('title', 'Reload data');
  });
});
