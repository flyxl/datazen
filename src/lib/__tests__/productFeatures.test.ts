import { describe, expect, it } from 'vitest';
import { PRODUCT_FEATURES, isProductFeatureEnabled } from '../productFeatures';

describe('productFeatures', () => {
  it('exposes migration tools in menus', () => {
    expect(PRODUCT_FEATURES.schemaDiff).toBe(true);
    expect(PRODUCT_FEATURES.dataSync).toBe(true);
    expect(PRODUCT_FEATURES.dataTransfer).toBe(true);
    expect(isProductFeatureEnabled('schemaDiff')).toBe(true);
    expect(isProductFeatureEnabled('dataSync')).toBe(true);
    expect(isProductFeatureEnabled('dataTransfer')).toBe(true);
  });
});
