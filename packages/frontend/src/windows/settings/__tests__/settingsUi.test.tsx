import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SectionTitle, SettingRow, ToggleRow } from '../settingsUi';
import { JsonSchemaSettingsForm } from '../JsonSchemaSettingsForm';

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

function renderHint() {
  return render(
    <SettingRow label="Identifier quoting" hint="Long completion explanation">
      <select aria-label="Quoting policy">
        <option>Smart</option>
      </select>
    </SettingRow>,
  );
}

function advanceClose() {
  act(() => vi.advanceTimersByTime(150));
}

describe('settings hints', () => {
  it('keeps the long explanation out of the row and renders it in a portal on hover', () => {
    const { container } = renderHint();
    expect(screen.queryByText('Long completion explanation')).toBeNull();
    const trigger = screen.getByRole('button', { name: 'Identifier quoting' });
    expect(trigger.textContent).toBe('?');
    fireEvent.mouseEnter(trigger);
    const tooltip = screen.getByRole('tooltip');
    expect(tooltip.textContent).toBe('Long completion explanation');
    expect(container.contains(tooltip)).toBe(false);
    expect(trigger.getAttribute('aria-describedby')).toBe(tooltip.id);
    fireEvent.mouseLeave(trigger);
    fireEvent.mouseEnter(tooltip);
    advanceClose();
    expect(screen.getByRole('tooltip')).toBe(tooltip);
    fireEvent.mouseLeave(tooltip);
    advanceClose();
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('supports focus, Escape dismissal, reopening and blur', () => {
    renderHint();
    const trigger = screen.getByRole('button');
    act(() => trigger.focus());
    expect(screen.getByRole('tooltip')).toBeTruthy();
    fireEvent.mouseLeave(trigger);
    advanceClose();
    expect(screen.getByRole('tooltip')).toBeTruthy();
    fireEvent.keyDown(trigger, { key: 'Escape' });
    expect(screen.queryByRole('tooltip')).toBeNull();
    expect(trigger.hasAttribute('aria-describedby')).toBe(false);
    act(() => trigger.blur());
    act(() => trigger.focus());
    expect(screen.getByRole('tooltip')).toBeTruthy();
    act(() => trigger.blur());
    advanceClose();
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('dismisses mouse-only hints with Escape', () => {
    renderHint();
    fireEvent.mouseEnter(screen.getByRole('button'));
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('flips below near the top edge and repositions on scrolling and resizing', () => {
    renderHint();
    let top = 10;
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.tagName === 'BUTTON'
        ? new DOMRect(window.innerWidth - 20, top, 16, 16)
        : new DOMRect(0, 0, 320, 80);
    });
    fireEvent.mouseEnter(screen.getByRole('button'));
    const tooltip = screen.getByRole('tooltip');
    expect(tooltip.style.left).toBe(`${window.innerWidth - 328}px`);
    expect(tooltip.style.top).toBe('34px');
    top = 200;
    fireEvent.scroll(window);
    expect(tooltip.style.top).toBe('112px');
    top = 300;
    fireEvent.resize(window);
    expect(tooltip.style.top).toBe('212px');
  });

  it('uses tooltips for section explanations and does not show empty help icons', () => {
    render(
      <>
        <SectionTitle hint="Section explanation">Section</SectionTitle>
        <SettingRow label="Other">
          <input />
        </SettingRow>
      </>,
    );
    expect(screen.queryByText('Section explanation')).toBeNull();
    expect(screen.getAllByRole('button')).toHaveLength(1);
    fireEvent.mouseEnter(screen.getByRole('button', { name: 'Section' }));
    expect(screen.getByRole('tooltip').textContent).toBe('Section explanation');
  });

  it('shows toggle help without changing the value and still allows toggling', () => {
    const onChange = vi.fn();
    render(<ToggleRow label="Feature" hint="Feature explanation" checked onChange={onChange} />);
    expect(screen.queryByText('Feature explanation')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Feature' }));
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.focus(screen.getByRole('button', { name: 'Feature' }));
    expect(screen.getByRole('tooltip').textContent).toBe('Feature explanation');
    fireEvent.click(screen.getByRole('switch', { name: 'Feature' }));
    expect(onChange).toHaveBeenCalledWith(false);
  });

  it('uses the same tooltip contract for schema-generated extension settings', () => {
    const onChange = vi.fn();
    render(
      <JsonSchemaSettingsForm
        schema={{
          type: 'object',
          properties: {
            enabled: {
              type: 'boolean',
              title: 'Extension feature',
              description: 'Extension explanation',
              default: true,
            },
          },
        }}
        value={{}}
        onChange={onChange}
      />,
    );
    expect(screen.queryByText('Extension explanation')).toBeNull();
    fireEvent.mouseEnter(screen.getByRole('button', { name: 'Extension feature' }));
    expect(screen.getByRole('tooltip').textContent).toBe('Extension explanation');
    fireEvent.click(screen.getByRole('switch', { name: 'Extension feature' }));
    expect(onChange).toHaveBeenCalledWith({ enabled: false });
  });

  it('exposes an accessible prefix switch and passes the updated preference', () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <ToggleRow label="Include table prefix" checked onChange={onChange} />,
    );
    const toggle = screen.getByRole('switch', { name: 'Include table prefix' });
    expect(toggle.getAttribute('aria-checked')).toBe('true');
    fireEvent.click(toggle);
    expect(onChange).toHaveBeenLastCalledWith(false);
    rerender(<ToggleRow label="Include table prefix" checked={false} onChange={onChange} />);
    fireEvent.click(toggle);
    expect(onChange).toHaveBeenLastCalledWith(true);
  });
});
