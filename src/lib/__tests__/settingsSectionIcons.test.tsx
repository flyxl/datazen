import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { SETTINGS_SECTIONS } from '../../windows/settings/settingsSections';
import { buildHostLucideById, settingsSectionIconId } from '../hostLucideMap';
import { createIconResolver, getActiveIconResolver, setActiveIconResolver } from '../iconResolver';
import { ThemedIcon } from '../../components/ThemedIcon';

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

describe('settings section icon chain (F7)', () => {
  it('maps settings.appearance to the Palette lucide name', () => {
    const lucideById = buildHostLucideById();
    expect(lucideById['settings.appearance']).toBe('Palette');
    expect(settingsSectionIconId('appearance')).toBe('settings.appearance');
  });

  it('resolves every registered settings section to a lucide icon (no ? placeholder)', () => {
    installDefaultResolver();
    for (const sec of SETTINGS_SECTIONS) {
      const id = settingsSectionIconId(sec.id);
      const resolved = getActiveIconResolver().resolve(id);
      expect(resolved.kind, `section ${sec.id} (icon id ${id})`).toBe('lucide');
    }
  });

  // BUG-F7-01 fixed: ThemedIcon's internal LUCIDE_MAP now includes `Palette`,
  // so the resolved lucide name renders as an svg glyph (no ? placeholder).
  it('renders settings.appearance Palette as an svg glyph (BUG-F7-01 fixed)', () => {
    installDefaultResolver();
    expect(getActiveIconResolver().resolve('settings.appearance')).toEqual({
      kind: 'lucide',
      name: 'Palette',
    });

    const { container } = render(<ThemedIcon id="settings.appearance" />);
    expect(container.querySelector('svg')).not.toBeNull();
    expect(screen.queryByText('?')).toBeNull();
  });
});
