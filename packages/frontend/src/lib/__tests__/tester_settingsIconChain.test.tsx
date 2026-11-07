import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { cleanup, render } from '@testing-library/react';
import { ThemedIcon } from '../../components/ThemedIcon';
import { createIconResolver, setActiveIconResolver } from '../iconResolver';
import {
  HOST_LUCIDE_MAP,
  SETTINGS_SECTION_LUCIDE_MAP,
  buildHostLucideById,
  settingsSectionIconId,
} from '../hostLucideMap';

afterEach(cleanup);

function installDefaultResolver(): void {
  setActiveIconResolver(
    createIconResolver({
      packIcons: {},
      driverIcons: {},
      lucideById: buildHostLucideById(),
      placeholderForDb: (dbType) => ({ label: dbType.slice(0, 2), bgClass: 'bg-slate-600' }),
    }),
  );
}

/**
 * [tester] (a)2 — the settings sidebar's icon chain has no fallback:
 * `SETTINGS_SECTION_LUCIDE_MAP` is a bare `Record<string,string>`, and
 * `ThemedIcon` silently renders a "?" span when the resolved lucide name is not
 * in its own `LUCIDE_MAP`. A typo or a missing registration therefore produces an
 * invisible defect that only a render assertion can catch.
 *
 * This walks *every* section id (not just `tunnels`) through the full chain:
 * map → `settingsSectionIconId` → `buildHostLucideById` → resolver → `<svg>`.
 */
describe('[tester] settings section icon chain (a)', () => {
  beforeEach(installDefaultResolver);

  it('resolves every settings section (including tunnels) to a real svg glyph', () => {
    for (const section of Object.keys(SETTINGS_SECTION_LUCIDE_MAP)) {
      const id = settingsSectionIconId(section);
      const { container } = render(<ThemedIcon id={id} />);
      expect(
        container.querySelector('svg'),
        `${section} (${id}) must render an svg`,
      ).not.toBeNull();
      expect(container.textContent, `${section} (${id}) must not be a "?" placeholder`).not.toBe(
        '?',
      );
      cleanup();
    }
  });

  it('registers tunnels in both maps consistently', () => {
    expect(SETTINGS_SECTION_LUCIDE_MAP.tunnels).toBe('Cable');
    expect(buildHostLucideById()[settingsSectionIconId('tunnels')]).toBe('Cable');
    // The nav id is derived, not hard-coded, so it must not collide with a v1 id.
    expect(settingsSectionIconId('tunnels')).toBe('settings.tunnels');
    expect(Object.keys(HOST_LUCIDE_MAP)).not.toContain('settings.tunnels');
  });
});
