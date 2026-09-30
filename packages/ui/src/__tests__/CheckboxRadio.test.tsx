import { describe, expect, it, vi, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { Checkbox, Radio } from '../index';

afterEach(cleanup);

describe('@datazen/ui Checkbox', () => {
  it('renders a themed native checkbox', () => {
    render(<Checkbox aria-label="Enable" />);
    const box = screen.getByRole('checkbox', { name: 'Enable' });
    expect(box.tagName).toBe('INPUT');
    expect(box).toHaveAttribute('type', 'checkbox');
    expect(box.className).toContain('accent-accent');
    expect(box.className).toContain('cursor-pointer');
  });

  it('fires onChange with the native event', () => {
    const onChange = vi.fn();
    render(<Checkbox onChange={onChange} aria-label="Enable" />);
    fireEvent.click(screen.getByRole('checkbox'));
    expect(onChange).toHaveBeenCalledTimes(1);
    // Uncontrolled box: jsdom flips the native checked state on click.
    expect(onChange.mock.calls[0][0].target.checked).toBe(true);
  });

  it('passes data-* attributes through to the input', () => {
    render(<Checkbox data-testid="my-checkbox" aria-label="Enable" />);
    expect(screen.getByTestId('my-checkbox')).toHaveAttribute('type', 'checkbox');
  });

  it('honours disabled', () => {
    render(<Checkbox disabled aria-label="Enable" />);
    expect(screen.getByRole('checkbox')).toBeDisabled();
    expect(screen.getByRole('checkbox').className).toContain('disabled:opacity-50');
  });
});

describe('@datazen/ui Radio', () => {
  it('renders a themed native radio', () => {
    render(<Radio aria-label="Option A" />);
    const radio = screen.getByRole('radio', { name: 'Option A' });
    expect(radio.tagName).toBe('INPUT');
    expect(radio).toHaveAttribute('type', 'radio');
    expect(radio.className).toContain('accent-accent');
  });

  it('fires onChange with the native event and forwards value/name', () => {
    const onChange = vi.fn();
    render(<Radio name="mode" value="a" onChange={onChange} aria-label="Option A" />);
    const radio = screen.getByRole('radio', { name: 'Option A' });
    expect(radio).toHaveAttribute('name', 'mode');
    expect(radio).toHaveAttribute('value', 'a');
    fireEvent.click(radio);
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange.mock.calls[0][0].target.value).toBe('a');
  });

  it('does not fire onChange when clicking an already-checked radio', () => {
    const onChange = vi.fn();
    render(<Radio name="mode" value="a" checked onChange={onChange} aria-label="Option A" />);
    fireEvent.click(screen.getByRole('radio'));
    expect(onChange).not.toHaveBeenCalled();
  });

  it('passes data-* attributes through to the input', () => {
    render(<Radio data-testid="my-radio" aria-label="Option A" />);
    expect(screen.getByTestId('my-radio')).toHaveAttribute('type', 'radio');
  });
});
