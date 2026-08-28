/**
 * Product-level feature gates. v0.1.0 hides migration tools that are not yet production-ready.
 * Re-enable individually when each module reaches release quality.
 */
export const PRODUCT_FEATURES = {
  schemaDiff: false,
  dataSync: false,
  dataTransfer: true,
} as const;

export type ProductFeatureKey = keyof typeof PRODUCT_FEATURES;

export function isProductFeatureEnabled(key: ProductFeatureKey): boolean {
  return PRODUCT_FEATURES[key];
}
