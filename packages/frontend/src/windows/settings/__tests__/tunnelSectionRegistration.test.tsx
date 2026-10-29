import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setBackendClient, clearBackendClients } from '@datazen/backend-client';
import { cleanup, render, screen } from '@testing-library/react';
import { SETTINGS_SECTIONS, parseSettingsSection } from '../settingsSections';
import {
  SETTINGS_SECTION_LUCIDE_MAP,
  buildHostLucideById,
  settingsSectionIconId,
} from '../../../lib/hostLucideMap';
import { createIconResolver, setActiveIconResolver } from '../../../lib/iconResolver';
import { ThemedIcon } from '../../../components/ThemedIcon';

const { mockTunnelCommands } = vi.hoisted(() => ({
  mockTunnelCommands: {
    getTunnels: vi.fn(),
    getTunnelSummaries: vi.fn(),
    getTunnel: vi.fn(),
    getTunnelUsage: vi.fn(),
    saveTunnel: vi.fn(),
    deleteTunnel: vi.fn(),
    testTunnel: vi.fn(),
  },
}));

vi.mock('../../../commands/tunnel', () => ({ tunnelCommands: mockTunnelCommands }));
vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));
vi.mock('../../../hooks/useLocaleDomains', () => ({
  useLocaleDomains: () => true,
}));

import { SettingsContent } from '../SettingsContent';

afterEach(() => {
  cleanup();
  clearBackendClients();
});

beforeEach(() => {
  // Desktop settings behavior (form-split F2): bind the desktop facade so the
  // desktop-only sections (tunnels among them) are in the available set.
  setBackendClient('desktop', { backendId: 'desktop' } as never);
  vi.clearAllMocks();
  mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);
  mockTunnelCommands.getTunnelUsage.mockResolvedValue({ connectionIds: [], connectionNames: [] });
});

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

describe('tunnels settings section registration (G2)', () => {
  it('accepts "tunnels" as a deep link instead of falling back to general', () => {
    expect(parseSettingsSection('tunnels')).toBe('tunnels');
  });

  it('registers the section with its own label key', () => {
    const entry = SETTINGS_SECTIONS.find((s) => s.id === 'tunnels');
    expect(entry).toBeDefined();
    expect(entry?.labelKey).toBe('settings.tunnels.title');
    // Still falls back for genuinely unknown ids.
    expect(parseSettingsSection('nope')).toBe('general');
  });

  it('resolves the section icon through the host lucide chain (no undefined)', () => {
    expect(SETTINGS_SECTION_LUCIDE_MAP.tunnels).toBe('Cable');
    const lucideById = buildHostLucideById();
    expect(lucideById[settingsSectionIconId('tunnels')]).toBe('Cable');
    installDefaultResolver();
    expect(lucideById['settings.tunnels']).toBeDefined();
  });

  // The map has no fallback: a lucide name the renderer does not know silently
  // becomes a "?" placeholder. This guards the whole chain, not just the map.
  it('renders the tunnels nav icon as an svg glyph (not a ? placeholder)', () => {
    installDefaultResolver();
    const { container } = render(<ThemedIcon id={settingsSectionIconId('tunnels')} />);
    expect(container.querySelector('svg')).not.toBeNull();
    expect(screen.queryByText('?')).toBeNull();
  });

  it('renders the tunnel management surface when deep-linked', async () => {
    render(<SettingsContent initialSection="tunnels" />);
    expect(await screen.findByTestId('settings-section-tunnels')).toBeInTheDocument();
    expect(screen.getByTestId('settings-tunnels-section')).toBeInTheDocument();
    // It must not have silently fallen back to the general section.
    expect(screen.queryByTestId('settings-section-general')).toBeNull();
  });
});
