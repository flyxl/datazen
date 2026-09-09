import { act, render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { SqlCodeBlock } from '../SqlCodeBlock';
import { useSettingsStore } from '../../stores/settingsStore';

function getHighlightedTokenClass(container: HTMLElement, token: string): string {
  const element = Array.from(container.querySelectorAll('.cm-line span')).find(
    (span) => span.textContent === token,
  );
  if (!element) throw new Error(`Could not find highlighted token: ${token}`);
  return element.className;
}

describe('SqlCodeBlock', () => {
  it('updates syntax highlighting when the setting changes', () => {
    const previousSettings = useSettingsStore.getState().settings;
    useSettingsStore.setState({
      settings: { ...previousSettings, sqlSyntaxTheme: 'default' },
    });

    const { container, unmount } = render(<SqlCodeBlock code="SELECT 'hello';" />);

    try {
      const initialTokenClass = getHighlightedTokenClass(container, 'SELECT');

      act(() => {
        useSettingsStore.setState((state) => ({
          settings: { ...state.settings, sqlSyntaxTheme: 'monokai' },
        }));
      });

      expect(getHighlightedTokenClass(container, 'SELECT')).not.toBe(initialTokenClass);
    } finally {
      unmount();
      useSettingsStore.setState({ settings: previousSettings });
    }
  });
});
