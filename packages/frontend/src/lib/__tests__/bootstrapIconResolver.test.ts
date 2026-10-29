import { describe, expect, it } from 'vitest';
import { bootstrapDefaultIconResolver } from '../bootstrapIconResolver';
import { getActiveIconResolver } from '../iconResolver';

describe('bootstrapDefaultIconResolver', () => {
  it('seeds default driver icons and db placeholders', () => {
    bootstrapDefaultIconResolver();
    const resolver = getActiveIconResolver();
    const lucide = resolver.resolve('query.run');
    expect(lucide.kind === 'lucide' || lucide.kind === 'url').toBe(true);
    const unknownDb = resolver.resolve('db.unknown-type');
    expect(unknownDb.kind).toBe('placeholder');
  });
});
