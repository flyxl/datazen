/**
 * Redis mode switchers now delegate their tab semantics to `@datazen/ui`'s
 * `Tabs` instead of hand-rolling `role="tab"` / `aria-selected` per file.
 *
 * Two things are being pinned here, and the second is the one that matters:
 *
 * 1. **Nothing visible or addressable changed.** The `redis-json-mode-*` /
 *    `redis-search-mode-*` test ids, the segment order and the labels all still
 *    resolve, because a dozen driver tests click them by id.
 * 2. **What they gained is real.** Before the consolidation neither switcher had
 *    a roving tabindex or any arrow-key handling, so the only way to change
 *    mode with a keyboard was Tab to each segment and press Enter. That is the
 *    regression risk in the other direction: a future edit that quietly drops
 *    the shared shell would take keyboard access away again, and only these
 *    assertions would notice.
 *
 * Driver-specific UI tests live here, inside the driver crate, per AGENTS.md.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

// The switchers take `useI18n` from the single @datazen/ui runtime; override
// only that hook so the assertions are locale-independent and the real `Tabs`
// still renders underneath.
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key, lang: 'en' }),
}));

import { JsonModeBar } from '../value-editors/JsonModeBar';
import { SearchModeTabs } from '../key-browser/SearchModeTabs';
import { JSON_DISPLAY_MODES } from '../value-editors/jsonModes';

afterEach(cleanup);

const SEARCH_REFS = [
  'redis-search-mode-key',
  'redis-search-mode-value',
  'redis-search-mode-all',
] as const;

/** Both strips are bar-only: the panel lives in the owning editor. */
function expectBarOnly() {
  expect(screen.queryByRole('tabpanel')).toBeNull();
  screen.getAllByRole('tab').forEach((tab) => {
    expect(tab).not.toHaveAttribute('aria-controls');
  });
}

describe('JsonModeBar tab semantics (converged onto Tabs)', () => {
  function renderBar(onSelect = vi.fn()) {
    render(<JsonModeBar modes={JSON_DISPLAY_MODES} active="pretty" onSelect={onSelect} />);
    return onSelect;
  }

  it('keeps every segment addressable by its existing test id and label', () => {
    renderBar();
    for (const mode of JSON_DISPLAY_MODES) {
      const segment = screen.getByTestId(`redis-json-mode-${mode}`);
      expect(segment.textContent).toBe(`redis.json.mode.${mode}`);
    }
    expect(screen.getAllByRole('tab')).toHaveLength(JSON_DISPLAY_MODES.length);
    expectBarOnly();
  });

  it('announces the active mode as selected and the rest as unselected', () => {
    renderBar();
    expect(screen.getByTestId('redis-json-mode-pretty')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByTestId('redis-json-mode-tree')).toHaveAttribute('aria-selected', 'false');
  });

  it('holds the strip at one tab stop and moves it with the selection', () => {
    renderBar();
    const tabIndexes = JSON_DISPLAY_MODES.map((mode) =>
      screen.getByTestId(`redis-json-mode-${mode}`).getAttribute('tabindex'),
    );
    expect(tabIndexes).toEqual(['-1', '-1', '0', '-1']);
  });

  it('changes mode from the keyboard and reports the real JsonDisplayMode', () => {
    const onSelect = renderBar();
    const pretty = screen.getByTestId('redis-json-mode-pretty');

    fireEvent.keyDown(pretty, { key: 'ArrowRight' });
    expect(document.activeElement).toBe(screen.getByTestId('redis-json-mode-minify'));
    expect(onSelect).toHaveBeenCalledWith('minify');

    fireEvent.keyDown(pretty, { key: 'ArrowLeft' });
    expect(onSelect).toHaveBeenLastCalledWith('raw');
  });

  it('still changes mode on click', () => {
    const onSelect = renderBar();
    fireEvent.click(screen.getByTestId('redis-json-mode-tree'));
    expect(onSelect).toHaveBeenCalledWith('tree');
  });

  it('keeps the bar an inline segmented control', () => {
    renderBar();
    // `inline-flex` is load-bearing here: the bar sits beside the other editor
    // toolbar controls and has to size to its segments instead of spanning the
    // editor. Nothing else can supply it — bar-only `Tabs` injects no layout
    // class by design, and `cn`/twMerge only ever drops conflicting classes, it
    // never adds a missing token back.
    expect(screen.getByTestId('redis-json-mode-bar')).toHaveClass('inline-flex');
  });
});

describe('SearchModeTabs tab semantics (converged onto Tabs)', () => {
  function renderStrip(onChange = vi.fn()) {
    render(<SearchModeTabs mode="key" onChange={onChange} />);
    return onChange;
  }

  it('keeps every segment addressable by its existing test id and label', () => {
    renderStrip();
    expect(screen.getByTestId('redis-search-mode-key').textContent).toBe('redis.search.modeKey');
    expect(screen.getByTestId('redis-search-mode-value').textContent).toBe(
      'redis.search.modeValue',
    );
    expect(screen.getByTestId('redis-search-mode-all').textContent).toBe('redis.search.modeAll');
    expect(screen.getAllByRole('tab')).toHaveLength(3);
    expectBarOnly();
  });

  it('announces the active scope and holds the strip at one tab stop', () => {
    renderStrip();
    expect(screen.getByTestId('redis-search-mode-key')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByTestId('redis-search-mode-value')).toHaveAttribute('aria-selected', 'false');
    expect(SEARCH_REFS.map((id) => screen.getByTestId(id).getAttribute('tabindex'))).toEqual([
      '0',
      '-1',
      '-1',
    ]);
  });

  it('changes scope from the keyboard and reports the real SearchMode', () => {
    const onChange = renderStrip();
    const key = screen.getByTestId('redis-search-mode-key');

    fireEvent.keyDown(key, { key: 'ArrowRight' });
    expect(document.activeElement).toBe(screen.getByTestId('redis-search-mode-value'));
    expect(onChange).toHaveBeenCalledWith('value');

    fireEvent.keyDown(key, { key: 'End' });
    expect(onChange).toHaveBeenLastCalledWith('all');
  });

  it('still changes scope on click', () => {
    const onChange = renderStrip();
    fireEvent.click(screen.getByTestId('redis-search-mode-all'));
    expect(onChange).toHaveBeenCalledWith('all');
  });

  it('keeps the three segments in one flex row', () => {
    renderStrip();
    // This is the regression that shipped once already. The tablist is the flex
    // *container*; each segment carries its own `flex h-7`. Drop `flex` here and
    // the three block-level segments stack vertically, and every one of them
    // grows the `border-l` that is only supposed to be the divider *between*
    // segments — so the control stops reading as three side-by-side options.
    // Asserted on the rendered tablist because the class lives in the component,
    // not in anything a `Tabs`-level test passes in.
    expect(screen.getByTestId('redis-search-mode-tabs')).toHaveClass('flex');
  });
});
