/** Series colors remain fixed hex values; chart chrome uses --c-* CSS variables. */
export const COLOR_PALETTES: Record<string, string[]> = {
  default: ['#6366f1', '#f59e0b', '#10b981', '#ef4444', '#8b5cf6', '#06b6d4', '#f97316', '#84cc16'],
  ocean: ['#0ea5e9', '#0284c7', '#0369a1', '#075985', '#0c4a6e', '#164e63', '#155e75', '#0e7490'],
  forest: ['#22c55e', '#16a34a', '#15803d', '#166534', '#14532d', '#365314', '#3f6212', '#4d7c0f'],
  warm: ['#f97316', '#ef4444', '#f59e0b', '#ec4899', '#e11d48', '#be123c', '#9f1239', '#881337'],
  mono: ['#6b7280', '#4b5563', '#374151', '#1f2937', '#9ca3af', '#d1d5db', '#e5e7eb', '#f3f4f6'],
};

let chartPaletteOverride: Record<string, string[]> | null = null;

export function setChartPaletteOverride(palettes: Record<string, string[]> | null): void {
  chartPaletteOverride = palettes;
}

export function clearChartPaletteOverride(): void {
  chartPaletteOverride = null;
}

export function getColorPalette(name: string): string[] {
  const override = chartPaletteOverride?.[name];
  if (override?.length) return override;
  return COLOR_PALETTES[name] ?? COLOR_PALETTES.default;
}

/** Stroke dash patterns cycled across series so overlapping lines stay distinguishable. */
export const STROKE_DASH_PATTERNS: (string | undefined)[] = [
  undefined, // solid
  '8 4', // dashed
  '2 4', // dotted
  '8 4 2 4', // dash-dot
  '12 4', // long dash
  '4 4', // short dash
];

/** Read a CSS custom property from :root, with fallback when unset or empty. */
export function readCssColor(
  varName: string,
  fallback: string,
  el: Element = document.documentElement,
): string {
  const value = getComputedStyle(el).getPropertyValue(varName).trim();
  return value || fallback;
}
