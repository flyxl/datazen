import { describe, expect, it } from 'vitest';
import { PRODUCT_FEATURES, isProductFeatureEnabled } from '../productFeatures';

describe('productFeatures', () => {
  it('disables migration tools for v0.1.0', () => {
    expect(PRODUCT_FEATURES.schemaDiff).toBe(false);
    expect(PRODUCT_FEATURES.dataSync).toBe(false);
    expect(PRODUCT_FEATURES.dataTransfer).toBe(false);
  });

  it('isProductFeatureEnabled reflects flags', () => {
    expect(isProductFeatureEnabled('schemaDiff')).toBe(false);
    expect(isProductFeatureEnabled('dataSync')).toBe(false);
    expect(isProductFeatureEnabled('dataTransfer')).toBe(false);
  });
});
