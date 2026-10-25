import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { TableDataSettingsSection } from '../TableDataSettingsSection';

/**
 * The store mock is minimal on purpose: the section reads seven settings and
 * writes through `updateSettings`; what the backend does with the write is
 * the store's own test suite, not this one's.
 */
const settingsState = vi.hoisted(() => ({
  rowCountMode: 'auto' as const,
  rowCountEstimateThreshold: 200_000,
  defaultTableSort: 'primaryKeyAsc' as const,
  gridDensity: 'standard' as const,
  gridAlternatingRows: true,
  gridRowNumbers: true,
  gridAutoHideScrollbar: false,
}));

const updateSettings = vi.fn().mockResolvedValue(undefined);

vi.mock('../../../stores/settingsStore', () => ({
  // The selector mock must answer BOTH reads: the settings object and the
  // write function the component pulls from the same store.
  useSettingsStore: Object.assign(
    (
      sel: (s: {
        settings: typeof settingsState;
        updateSettings: typeof updateSettings;
      }) => unknown,
    ) => sel({ settings: settingsState, updateSettings }),
    { getState: () => ({ settings: settingsState }) },
  ),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

/** Open the density Select (a listbox, not a native select) and pick a label. */
function pickDensity(optionLabel: string) {
  fireEvent.click(screen.getByTestId('settings-grid-density'));
  fireEvent.mouseDown(screen.getByText(optionLabel));
}

/**
 * The DB-17 appearance block of the data-browsing settings: one Select for
 * the density, three toggles — and every control must show the default the
 * grids had before the settings existed.
 */
describe('TableDataSettingsSection grid appearance (DB-17)', () => {
  beforeEach(() => {
    updateSettings.mockClear();
  });

  afterEach(() => {
    cleanup();
  });

  it('renders the density select with all three densities', () => {
    render(<TableDataSettingsSection />);
    fireEvent.click(screen.getByTestId('settings-grid-density'));
    // The options carry their locale keys verbatim (the i18n mock is the key).
    // `standard` matches twice — once as the trigger's selected label, once
    // as the option — so the presence check must accept multiplicity.
    expect(screen.getAllByText('settings.gridDensity.compact').length).toBeGreaterThan(0);
    expect(screen.getAllByText('settings.gridDensity.standard').length).toBeGreaterThan(0);
    expect(screen.getAllByText('settings.gridDensity.relaxed').length).toBeGreaterThan(0);
  });

  it('writes the density through updateSettings', () => {
    render(<TableDataSettingsSection />);
    pickDensity('settings.gridDensity.relaxed');
    expect(updateSettings).toHaveBeenCalledWith({ gridDensity: 'relaxed' });
  });

  it('renders the three toggles with their defaults', () => {
    render(<TableDataSettingsSection />);
    expect(screen.getByTestId('settings-grid-alternating-rows').getAttribute('aria-checked')).toBe(
      'true',
    );
    expect(screen.getByTestId('settings-grid-row-numbers').getAttribute('aria-checked')).toBe(
      'true',
    );
    expect(
      screen.getByTestId('settings-grid-auto-hide-scrollbar').getAttribute('aria-checked'),
    ).toBe('false');
  });

  it('toggles write through updateSettings with the flipped value', () => {
    render(<TableDataSettingsSection />);
    fireEvent.click(screen.getByTestId('settings-grid-alternating-rows'));
    expect(updateSettings).toHaveBeenCalledWith({ gridAlternatingRows: false });
    fireEvent.click(screen.getByTestId('settings-grid-row-numbers'));
    expect(updateSettings).toHaveBeenCalledWith({ gridRowNumbers: false });
    fireEvent.click(screen.getByTestId('settings-grid-auto-hide-scrollbar'));
    expect(updateSettings).toHaveBeenCalledWith({ gridAutoHideScrollbar: true });
  });
});
