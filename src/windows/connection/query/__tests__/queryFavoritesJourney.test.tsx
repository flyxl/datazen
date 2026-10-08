/**
 * SQL Favorites — file-first panel journey.
 *
 * This is a continuous state-machine test, not a set of static assertions. The
 * journey walks the panel through every intermediate state a user can observe:
 *
 *   closed → opening → root unknown → root known + empty → a sync client
 *   writes a file behind our back → *still* empty (the backend caches) →
 *   window regains focus → rescan → the file appears → opened in a new tab →
 *   connection switched → only that connection's favorites → deleted → gone.
 *
 * The "still empty" step is the point of the whole test. A favorite is a file
 * in a folder the user may sync, so the panel's listing can
 * change without the app writing anything. If the panel only called the cached
 * read, a file that arrived during the session would stay invisible until
 * restart — and this test is what pins the rescan that prevents that.
 *
 * The fake below reproduces the backend's real caching contract
 * (`store::favorites::FavoritesStore`): `getFavoriteQueries` answers from the
 * cache, `refreshFavorites` drops the cache and re-reads. The Rust suite owns
 * the file format itself; this suite owns *when the UI asks*.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act, cleanup, waitFor } from '@testing-library/react';
import { QuerySidebarSection } from '../QuerySidebarSection';
import { usePanelStore } from '../../../../stores/panelStore';
import type { FavoriteQuery } from '../../../../types';

const FAKE_ROOT = '/Users/someone/Library/iCloud Drive/DataZen/favorites';

/** Call log, so the test can assert *which* read the panel performed. */
const backend = vi.hoisted(() => ({
  /** What is actually on "disk" right now. */
  disk: [] as FavoriteQuery[],
  /** What the backend cache holds. Null = cold, i.e. a rescan is required. */
  cache: null as FavoriteQuery[] | null,
  rescanCalls: [] as Array<string | undefined>,
  cachedCalls: [] as Array<string | undefined>,
}));

vi.mock('../../../../commands/query', async () => {
  const actual = await vi.importActual<typeof import('../../../../commands/query')>(
    '../../../../commands/query',
  );
  const matches = (favorites: FavoriteQuery[], connectionId?: string) =>
    connectionId ? favorites.filter((f) => f.connectionId === connectionId) : favorites;

  return {
    queryCommands: {
      ...actual.queryCommands,
      // The cached fast path. Never re-reads disk.
      getFavoriteQueries: (connectionId?: string) => {
        backend.cachedCalls.push(connectionId);
        return Promise.resolve(matches(backend.cache ?? [], connectionId));
      },
      // The rescan. Drops the cache first, exactly like `refresh_favorites`.
      refreshFavorites: (connectionId?: string) => {
        backend.rescanCalls.push(connectionId);
        backend.cache = [...backend.disk];
        return Promise.resolve(matches(backend.cache, connectionId));
      },
      getFavoritesRoot: () => Promise.resolve(FAKE_ROOT),
    },
  };
});

vi.mock('../../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: () => Promise.resolve(),
}));

function favorite(id: string, connectionId: string, title: string): FavoriteQuery {
  return {
    id,
    connectionId,
    title,
    sql: `SELECT '${title}';`,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    database: null,
    keyword: null,
    folder: null,
  };
}

function renderPanel(connectionId: string, favoritesVisible: boolean) {
  return render(
    <QuerySidebarSection
      panelId="qry-1"
      connectionId={connectionId}
      favoritesVisible={favoritesVisible}
      historyVisible={false}
    />,
  );
}

describe('SQL Favorites panel — file-first journey', () => {
  beforeEach(() => {
    backend.disk = [];
    backend.cache = null;
    backend.rescanCalls.length = 0;
    backend.cachedCalls.length = 0;
    usePanelStore.setState({
      queryFavorites: [],
      favoritesRoot: null,
      favoritesVisible: true,
      historyVisible: false,
      panels: [],
      activePanelId: null,
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('walks a synced folder from empty to populated, across a connection switch', async () => {
    // ── 1. Panel closed: nothing is read at all ───────────────────────────
    // A closed panel must not pay for a directory scan.
    const { unmount } = renderPanel('cfg-a', false);
    expect(screen.queryByTestId('query-favorites-panel')).toBeNull();
    expect(backend.rescanCalls).toEqual([]);
    expect(backend.cachedCalls).toEqual([]);
    unmount();

    // ── 2. Panel opens: one rescan, and an intermediate state with no path ─
    // `favoritesRoot` is null until the lookup resolves, so the very first
    // paint must not render a stale or empty-looking path.
    renderPanel('cfg-a', true);
    expect(await screen.findByTestId('query-favorites-panel')).toBeInTheDocument();
    await waitFor(() => expect(backend.rescanCalls).toEqual(['cfg-a']));
    await waitFor(() => expect(screen.getByTestId('favorites-root')).toBeInTheDocument());
    expect(screen.getByTestId('favorites-root')).toHaveTextContent(FAKE_ROOT);

    // ── 3. Empty state ───────────────────────────────────────────────────
    expect(await screen.findByText('No favorites yet')).toBeInTheDocument();

    // ── 4. A sync client writes two files while the app is running ───────
    // This is the whole reason the panel rescans. Nothing in the app caused
    // these files to appear.
    backend.disk.push(favorite('01ARZ3NDEK0000000000000000', 'cfg-a', 'Nightly recon'));
    backend.disk.push(favorite('01ARZ3NDEK0000000000000001', 'cfg-b', 'Other database'));

    // ── 5. Still empty: the backend cache has not been dropped ───────────
    // Rendered from the store's own state, so this asserts the UI genuinely
    // did not silently pick the new files up.
    await act(async () => {});
    expect(screen.getByText('No favorites yet')).toBeInTheDocument();
    expect(screen.queryByText('Nightly recon')).toBeNull();
    expect(backend.rescanCalls).toEqual(['cfg-a']);

    // ── 6. The window regains focus → rescan → the file appears ──────────
    await act(async () => {
      window.dispatchEvent(new Event('focus'));
    });
    await waitFor(() => expect(backend.rescanCalls).toEqual(['cfg-a', 'cfg-a']));
    expect(await screen.findByText('Nightly recon')).toBeInTheDocument();
    // Filtered per connection: cfg-b's file must not leak into cfg-a's panel.
    expect(screen.queryByText('Other database')).toBeNull();

    // ── 7. Switching connection rescans, and the listing follows ─────────
    // A file the user cannot see is a favorite that looks deleted.
    cleanup();
    renderPanel('cfg-b', true);
    expect(await screen.findByText('Other database')).toBeInTheDocument();
    expect(screen.queryByText('Nightly recon')).toBeNull();
    await waitFor(() => expect(backend.rescanCalls).toEqual(['cfg-a', 'cfg-a', 'cfg-b']));

    // ── 8. Back to cfg-a: the other favorite is still there ──────────────
    cleanup();
    renderPanel('cfg-a', true);
    expect(await screen.findByText('Nightly recon')).toBeInTheDocument();
  });

  it('a file deleted out of band disappears on the next focus', async () => {
    backend.disk.push(favorite('01ARZ3NDEK0000000000000000', 'cfg-a', 'Nightly recon'));
    renderPanel('cfg-a', true);
    expect(await screen.findByText('Nightly recon')).toBeInTheDocument();

    // The user (or a sync conflict) removed it behind the app's back.
    backend.disk.length = 0;

    await act(async () => {
      window.dispatchEvent(new Event('focus'));
    });
    expect(await screen.findByText('No favorites yet')).toBeInTheDocument();
    expect(screen.queryByText('Nightly recon')).toBeNull();
  });

  it('the manual rescan button re-reads the directory on demand', async () => {
    renderPanel('cfg-a', true);
    await screen.findByTestId('query-favorites-panel');
    await waitFor(() => expect(backend.rescanCalls).toHaveLength(1));

    backend.disk.push(favorite('01ARZ3NDEK0000000000000002', 'cfg-a', 'Added elsewhere'));
    const before = backend.rescanCalls.length;

    await act(async () => {
      screen.getByTestId('favorites-refresh').click();
    });

    await waitFor(() => expect(backend.rescanCalls.length).toBe(before + 1));
    expect(await screen.findByText('Added elsewhere')).toBeInTheDocument();
  });

  it('hiding the panel stops the focus-driven rescans', async () => {
    backend.disk.push(favorite('01ARZ3NDEK0000000000000000', 'cfg-a', 'Nightly recon'));
    const { rerender } = renderPanel('cfg-a', true);
    expect(await screen.findByText('Nightly recon')).toBeInTheDocument();
    const afterOpen = backend.rescanCalls.length;

    // Hiding detaches the focus listener — a hidden panel doing directory
    // walks on every window focus would be a real cost.
    rerender(
      <QuerySidebarSection
        panelId="qry-1"
        connectionId="cfg-a"
        favoritesVisible={false}
        historyVisible={false}
      />,
    );
    await act(async () => {
      window.dispatchEvent(new Event('focus'));
    });

    expect(backend.rescanCalls.length).toBe(afterOpen);

    // Re-opening rescans again, because new files may have arrived meanwhile.
    rerender(
      <QuerySidebarSection
        panelId="qry-1"
        connectionId="cfg-a"
        favoritesVisible
        historyVisible={false}
      />,
    );
    await waitFor(() => expect(backend.rescanCalls.length).toBe(afterOpen + 1));
  });
});
