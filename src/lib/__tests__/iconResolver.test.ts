import { describe, expect, it } from 'vitest';
import { createIconResolver } from '../iconResolver';

describe('createIconResolver', () => {
  const resolver = createIconResolver({
    packIcons: { 'nav.settings': 'blob:pack-settings', 'db.postgresql': 'blob:pack-pg' },
    driverIcons: { 'db.postgresql': 'asset:driver-pg', 'db.mysql': 'asset:driver-mysql' },
    lucideById: { 'nav.settings': 'Settings', 'query.run': 'Play' },
    placeholderForDb: (t) => ({ label: t.slice(0, 2), bgClass: 'bg-slate-600' }),
  });

  it('prefers pack over lucide for UI icons', () => {
    expect(resolver.resolve('nav.settings')).toEqual({ kind: 'url', href: 'blob:pack-settings' });
  });

  it('falls back to lucide', () => {
    expect(resolver.resolve('query.run')).toEqual({ kind: 'lucide', name: 'Play' });
  });

  it('resolves db pack → driver → placeholder', () => {
    expect(resolver.resolve('db.postgresql')).toEqual({ kind: 'url', href: 'blob:pack-pg' });
    expect(resolver.resolve('db.mysql')).toEqual({ kind: 'url', href: 'asset:driver-mysql' });
    expect(resolver.resolve('db.unknown')).toEqual({
      kind: 'placeholder',
      label: 'un',
      bgClass: 'bg-slate-600',
    });
  });
});
