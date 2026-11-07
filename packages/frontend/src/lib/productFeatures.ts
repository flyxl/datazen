/**
 * Product-level feature gates.
 */
export const PRODUCT_FEATURES = {
  schemaDiff: true,
  dataSync: true,
  dataTransfer: true,
} as const;

export type ProductFeatureKey = keyof typeof PRODUCT_FEATURES;

export function isProductFeatureEnabled(key: ProductFeatureKey): boolean {
  return PRODUCT_FEATURES[key];
}
