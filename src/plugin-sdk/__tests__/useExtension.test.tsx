import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, screen, act, cleanup } from '@testing-library/react';
import React from 'react';
import {
  createExtensionPoint,
  extensionRegistry,
  type ExtensionPoint,
} from '@datazen/extension-points';
import { useExtension, useIsExtensionEnhanced } from '@datazen/extension-points';

interface FormatterFeature {
  format(val: string): string;
}

const defaultFormatter: FormatterFeature = {
  format: (s) => `basic:${s}`,
};

const proFormatter: FormatterFeature = {
  format: (s) => `PRO:${s.toUpperCase()}`,
};

const testPoint: ExtensionPoint<FormatterFeature> = createExtensionPoint({
  id: 'test.formatter',
  name: 'Formatter Feature',
  getDefault: () => defaultFormatter,
});

function TestComponent({ input }: { input: string }) {
  const feature = useExtension(testPoint);
  const isEnhanced = useIsExtensionEnhanced(testPoint);

  return (
    <div>
      <span data-testid="output">{feature.format(input)}</span>
      <span data-testid="enhanced">{isEnhanced ? 'yes' : 'no'}</span>
    </div>
  );
}

describe('useExtension & useIsExtensionEnhanced', () => {
  beforeEach(() => {
    extensionRegistry.reset();
  });

  afterEach(() => {
    cleanup();
  });

  it('initially renders with default fallback', () => {
    render(<TestComponent input="hello" />);

    expect(screen.getByTestId('output').textContent).toBe('basic:hello');
    expect(screen.getByTestId('enhanced').textContent).toBe('no');
  });

  it('dynamically updates when extension is registered and unregistered', () => {
    render(<TestComponent input="world" />);

    expect(screen.getByTestId('output').textContent).toBe('basic:world');
    expect(screen.getByTestId('enhanced').textContent).toBe('no');

    // Register Pro extension
    let unregister: () => void = () => {};
    act(() => {
      unregister = extensionRegistry.register(testPoint, proFormatter);
    });

    expect(screen.getByTestId('output').textContent).toBe('PRO:WORLD');
    expect(screen.getByTestId('enhanced').textContent).toBe('yes');

    // Unregister Pro extension
    act(() => {
      unregister();
    });

    expect(screen.getByTestId('output').textContent).toBe('basic:world');
    expect(screen.getByTestId('enhanced').textContent).toBe('no');
  });
});
