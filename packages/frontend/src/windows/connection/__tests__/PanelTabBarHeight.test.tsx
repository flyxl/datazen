/**
 * The panel tab bar and the navigator's title row are the same height.
 *
 * They are the window's two column headers, side by side, and nothing in the
 * layout forces them to agree. The navigator's row was pinned to `h-12`; the
 * tab bar was sized by its own buttons (`py-2` plus a `pb-0.5` on the
 * container) and landed several pixels short. The drift is invisible in a diff
 * and immediately obvious on screen.
 *
 * The navigator row is the reference — "以左边为准" — so the assertion is
 * directional rather than a restatement of the number: whatever the navigator
 * declares, the tab bar must declare too. Changing one and not the other fails
 * here instead of in a screenshot.
 *
 * A shared constant cannot express this: Tailwind only extracts size tokens it
 * sees as literals in a scanned file, so an exported `HEADER_H` would silently
 * stop styling anything. Both sides name the literal, and this file is the
 * coupling.
 *
 * jsdom does no layout, so what is pinned is the declared contract, not a
 * measured box. That is the honest seam: the part a future edit can break is
 * that the two authors declared the same height, and that neither row adds
 * vertical padding back on top of it.
 */
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Panel } from '../../../stores/panelStore';
import { PanelTabBar } from '../PanelTabBar';
import { NavigatorToolbar } from '../navigator/NavigatorToolbar';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string>) =>
      key === 'panel.closeTab' ? `panel.closeTab ${params?.title ?? ''}` : key,
  }),
}));

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: (selector: (state: { schemas: Map<string, unknown> }) => unknown) =>
    selector({ schemas: new Map() }),
}));

vi.mock('../../../components/ThemedIcon', () => ({
  ThemedIcon: () => <span aria-hidden="true" />,
}));

afterEach(cleanup);

/** The Tailwind height token that decides the row's rendered height, if any. */
function heightToken(el: HTMLElement): string | undefined {
  return el.className.split(/\s+/).find((c) => /^h-/.test(c));
}

const panels: Panel[] = [
  {
    id: 'panel-one',
    type: 'query',
    title: 'Query 1',
    connectionId: 'connection-1',
    dbSessionId: 'session-1',
    connectionName: 'Local',
    databaseType: 'postgresql',
    database: 'app',
    schema: null,
  },
];

function renderNavigator() {
  return render(
    <NavigatorToolbar
      t={((key: string) => key) as never}
      searchQuery=""
      setSearchQuery={vi.fn()}
      onNewConnection={vi.fn()}
      onNewGroup={vi.fn()}
      onCollapseAll={vi.fn()}
    />,
  );
}

function renderTabBar() {
  return render(
    <PanelTabBar
      panels={panels}
      activePanelId="panel-one"
      onSelectPanel={vi.fn()}
      onClosePanel={vi.fn()}
      onContextMenu={vi.fn()}
    />,
  );
}

describe('column header height contract', () => {
  it('the panel tab bar matches the navigator title row', () => {
    const navigator = renderNavigator();
    const reference = heightToken(screen.getByTestId('navigator-title-row'));
    navigator.unmount();

    renderTabBar();
    const tabBar = heightToken(screen.getByTestId('panel-tab-bar'));

    // The reference must be a pinned token, not an implicit content height —
    // otherwise "matching" it is meaningless.
    expect(reference).toBe('h-12');
    expect(tabBar).toBe(reference);
  });

  it('neither header adds vertical padding on top of its own height', () => {
    renderTabBar();
    renderNavigator();

    // `py-*` or a `pb-*` on the container would push the rendered box past the
    // declared height, or leave the height decided by content — both of which
    // are how the two drifted apart in the first place. `px-*` is fine: it is
    // horizontal, and both rows are meant to breathe sideways.
    for (const testId of ['panel-tab-bar', 'navigator-title-row']) {
      const classes = screen.getByTestId(testId).className.split(/\s+/);
      expect(classes.filter((c) => /^p[yt]-/.test(c))).toEqual([]);
    }
  });

  it('an individual panel tab fills the bar rather than padding itself to size', () => {
    renderTabBar();

    // The active-tab underline is positioned against the tab's own box, so the
    // tab has to reach the bar's bottom edge. A `py-*` on the tab would both
    // re-add the height coupling and float the underline mid-tab.
    const tab = screen.getByTestId('panel-tab');
    expect(tab.className.split(/\s+/)).toContain('h-full');
    expect(tab.className.split(/\s+/).filter((c) => /^py-/.test(c))).toEqual([]);
  });
});
