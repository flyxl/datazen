import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { PluginIcon } from '../PluginIcon';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

describe('PluginIcon', () => {
  it('falls back to the puzzle glyph when no icon is contributed', () => {
    const { container } = render(<PluginIcon pluginId="acme.bill-audit" />);
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('svg')).not.toBeNull();
  });

  it('renders the datazen:// asset image for a contributed icon', () => {
    const { container } = render(
      <PluginIcon pluginId="acme.bill-audit" icon="./assets/icon.svg" />,
    );
    const img = container.querySelector('img');
    expect(img).not.toBeNull();
    expect(img!.getAttribute('src')).toBe('datazen://acme.bill-audit/assets/icon.svg');
    expect(container.querySelector('svg')).toBeNull();
  });

  it('swaps to the puzzle glyph after the icon fails to load and recovers on icon change', () => {
    const { rerender, container } = render(
      <PluginIcon pluginId="acme.bill-audit" icon="assets/icon.svg" />,
    );
    const img = container.querySelector('img');
    expect(img).not.toBeNull();

    fireEvent.error(img!);
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('svg')).not.toBeNull();

    rerender(<PluginIcon pluginId="acme.bill-audit" icon="assets/other.svg" />);
    const refreshed = container.querySelector('img');
    expect(refreshed!.getAttribute('src')).toBe('datazen://acme.bill-audit/assets/other.svg');

    // Same broken icon again → fallback again.
    fireEvent.error(refreshed!);
    expect(container.querySelector('img')).toBeNull();
  });
});
