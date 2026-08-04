import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { Button } from '../Button';

afterEach(cleanup);

describe('Button', () => {
  it('calls preventDefault on mousedown to preserve focus', () => {
    const { getByRole } = render(<Button>Test</Button>);
    const btn = getByRole('button');

    const event = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
    const prevented = !btn.dispatchEvent(event);
    expect(prevented).toBe(true);
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

  it('preserves input focus after mousedown on button', () => {
    const { container } = render(
      <div>
        <input placeholder="focused input" />
        <Button>Action</Button>
      </div>,
    );
    const input = container.querySelector('input')!;
    input.focus();

    fireEvent.mouseDown(container.querySelector('button')!);

    expect(document.activeElement).toBe(input);
  });

  it('invokes user-provided onMouseDown after preventDefault', () => {
    const onMouseDown = vi.fn();
    const { container } = render(<Button onMouseDown={onMouseDown}>Test</Button>);

    fireEvent.mouseDown(container.querySelector('button')!);
    expect(onMouseDown).toHaveBeenCalledTimes(1);
  });
});
