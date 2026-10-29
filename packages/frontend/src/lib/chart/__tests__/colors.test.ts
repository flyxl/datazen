import { beforeEach, describe, expect, it } from 'vitest';
import {
  clearChartPaletteOverride,
  getColorPalette,
  readCssColor,
  setChartPaletteOverride,
  COLOR_PALETTES,
} from '../colors';

describe('getColorPalette', () => {
  beforeEach(() => {
    clearChartPaletteOverride();
  });

  it('returns built-in palette by name', () => {
    expect(getColorPalette('ocean')).toEqual(COLOR_PALETTES.ocean);
  });

  it('falls back to default for unknown name', () => {
    expect(getColorPalette('unknown')).toEqual(COLOR_PALETTES.default);
  });

  it('uses pack override when set', () => {
    setChartPaletteOverride({ custom: ['#111111', '#222222'] });
    expect(getColorPalette('custom')).toEqual(['#111111', '#222222']);
    clearChartPaletteOverride();
    expect(getColorPalette('custom')).toEqual(COLOR_PALETTES.default);
  });
});

describe('readCssColor', () => {
  it('reads CSS variable with fallback', () => {
    document.documentElement.style.setProperty('--c-accent', '#ff00ff');
    expect(readCssColor('--c-accent', '#000')).toBe('#ff00ff');
    expect(readCssColor('--c-missing', '#abc')).toBe('#abc');
  });
});
