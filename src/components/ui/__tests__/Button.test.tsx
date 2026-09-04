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
});
