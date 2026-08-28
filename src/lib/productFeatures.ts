/**
 * Product-level feature gates. v0.1.0 hides schema diff until production-ready.
 */
export const PRODUCT_FEATURES = {
  schemaDiff: false,
  dataSync: true,
  dataTransfer: true,
} as const;

export type ProductFeatureKey = keyof typeof PRODUCT_FEATURES;

export function isProductFeatureEnabled(key: ProductFeatureKey): boolean {
  return PRODUCT_FEATURES[key];
}
