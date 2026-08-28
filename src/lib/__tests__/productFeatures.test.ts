import { describe, expect, it } from 'vitest';
import { PRODUCT_FEATURES, isProductFeatureEnabled } from '../productFeatures';

describe('productFeatures', () => {
  it('keeps schema diff and data sync hidden until production-ready', () => {
    expect(PRODUCT_FEATURES.schemaDiff).toBe(false);
    expect(PRODUCT_FEATURES.dataSync).toBe(false);
  });

  it('exposes data transfer in menus', () => {
    expect(PRODUCT_FEATURES.dataTransfer).toBe(true);
    expect(isProductFeatureEnabled('dataTransfer')).toBe(true);
  });

  it('isProductFeatureEnabled reflects flags', () => {
    expect(isProductFeatureEnabled('schemaDiff')).toBe(false);
    expect(isProductFeatureEnabled('dataSync')).toBe(false);
  });
});
