import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { EditableCell } from '../EditableCell';

afterEach(cleanup);

describe('EditableCell component', () => {
  it('renders a compact grid-cell editor instead of the generic form input', () => {
    const { container } = render(
      <EditableCell value="x" type="varchar" onCommit={vi.fn()} onCancel={vi.fn()} />,
    );
    const input = container.querySelector('input')!;
    // The editor must not inherit the heavy form `Input` styling that
    // padded/clipped it inside the tight grid cell.
    const classes: string = input.getAttribute('class') ?? '';
    expect(classes).toContain('h-7');
    expect(classes).toContain('font-mono');
    // The generic <Input> widget carries px-3 + rounded-md + focus:ring-2; a
    // grid cell editor should stay minimal and edge-aligned.
    expect(classes).not.toContain('px-3');
    expect(classes).not.toContain('focus:ring-2');
  });

  it('commits string value on Enter', () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const { container } = render(
      <EditableCell value="hello" type="varchar" onCommit={onCommit} onCancel={onCancel} />,
    );
    const input = container.querySelector('input')!;
    fireEvent.change(input, { target: { value: 'world' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCommit).toHaveBeenCalledWith('world');
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('cancels on Escape', () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const { container } = render(
      <EditableCell value="hello" type="varchar" onCommit={onCommit} onCancel={onCancel} />,
    );
    const input = container.querySelector('input')!;
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onCommit).not.toHaveBeenCalled();
  });

  it('cancels on blur when value unchanged', () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const { container } = render(
      <EditableCell value="hello" type="varchar" onCommit={onCommit} onCancel={onCancel} />,
    );
    const input = container.querySelector('input')!;
    fireEvent.blur(input);
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onCommit).not.toHaveBeenCalled();
  });

  it('coerces integer type', () => {
    const onCommit = vi.fn();
    const { container } = render(
      <EditableCell value={1} type="integer" onCommit={onCommit} onCancel={vi.fn()} />,
    );
    const input = container.querySelector('input')!;
    fireEvent.change(input, { target: { value: '99' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCommit).toHaveBeenCalledWith(99);
  });

  it('coerces boolean type', () => {
    const onCommit = vi.fn();
    const { container } = render(
      <EditableCell value={false} type="boolean" onCommit={onCommit} onCancel={vi.fn()} />,
    );
    const input = container.querySelector('input')!;
    fireEvent.change(input, { target: { value: 'true' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCommit).toHaveBeenCalledWith(true);
  });

  it('coerces numeric and json types', () => {
    const onCommit = vi.fn();
    const { container } = render(
      <EditableCell value={1.5} type="numeric" onCommit={onCommit} onCancel={vi.fn()} />,
    );
    const input = container.querySelector('input')!;
    fireEvent.change(input, { target: { value: '3.14' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCommit).toHaveBeenCalledWith(3.14);

    cleanup();
    const onCommitJson = vi.fn();
    const { container: c2 } = render(
      <EditableCell value={{ a: 1 }} type="jsonb" onCommit={onCommitJson} onCancel={vi.fn()} />,
    );
    const jsonInput = c2.querySelector('input')!;
    fireEvent.change(jsonInput, { target: { value: '{"x":1}' } });
    fireEvent.keyDown(jsonInput, { key: 'Enter' });
    expect(onCommitJson).toHaveBeenCalledWith({ x: 1 });
  });

  it('commits null for empty string when value was non-null', () => {
    const onCommit = vi.fn();
    const { container } = render(
      <EditableCell value="text" type="varchar" onCommit={onCommit} onCancel={vi.fn()} />,
    );
    const input = container.querySelector('input')!;
    fireEvent.change(input, { target: { value: '' } });
    fireEvent.blur(input);
    expect(onCommit).toHaveBeenCalledWith(null);
  });

  it('cancels when empty and original was null', () => {
    const onCancel = vi.fn();
    const { container } = render(
      <EditableCell value={null} type="varchar" onCommit={vi.fn()} onCancel={onCancel} />,
    );
    const input = container.querySelector('input')!;
    fireEvent.blur(input);
    expect(onCancel).toHaveBeenCalled();
  });
});
