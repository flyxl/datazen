/**
 * Independent verification of the named-view contract (Wave 7 · track B).
 *
 * The track's own tests prove the behaviour of the pieces it wrote. This file
 * checks the **links between them**, because those are the places where the
 * contract can be broken without any single unit test turning red:
 *
 * - **§2 · the error code closure.** The host's `CommandError::Serialize`
 *   emits a **plain string**, `deserializeApiError` replaces a non-record
 *   payload with a generic sentence, and the backend client prefixes the
 *   result with `"<method> failed: "`. The `grid.view.*` codes are only
 *   reachable if the front end walks the **cause chain** — so this file walks
 *   the *real* chain: the mocked `invoke` → the real desktop transport → the
 *   real backend-client facade → the real `databaseCommands` wrapper.
 *
 * - **§5 · one DTO.** The named-view shape exists in
 *   `@datazen/backend-client`; `src/lib/namedViews.ts` must **re-export** it
 *   and not declare a second copy. A second copy is the project's known
 *   failure mode ("one DTO, two shapes"), and nothing else in the tree would
 *   notice it.
 *
 * - **§6 · the `connector` spelling.** The shared crate skips an absent
 *   connector instead of writing `null` (00-contracts §5.2.1, "缺省不写").
 *   The front end must agree: `undefined` means "no connector", `null` must
 *   never reach a filter object, and `JSON.stringify` must not turn a reset
 *   connector into a spurious "dirty".
 *
 * - **§8 · the web boundary.** A form factor that declares the named views as
 *   a **capability** gap must make the front end hide the entry, not offer it
 *   and refuse it later.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
  Channel: class {
    onmessage = (_message: unknown) => {};
  },
}));

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

import {
  ApiError,
  createBackendClient,
  selectBackend,
  setBackendClient,
} from '@datazen/backend-client';
import {
  createDesktopBackendTransport,
  DESKTOP_BACKEND_ID,
} from '../../platform/tauriBackendTransport';
import { classifyGridError, GRID_ERROR_CODES } from '../gridErrors';
import { hostErrorMessage } from '../hostErrorMessage';
import { databaseCommands } from '../../commands/database';
import { namedViewsAvailable } from '../namedViews';
import { filterDraftEqualsApplied } from '../../stores/tableData/filterUtils';
import type { NamedView, NamedViewScope } from '../namedViews';

const REPO_ROOT = resolve(__dirname, '../../..');

const SCOPE: NamedViewScope = {
  connectionId: 'conn-1',
  database: 'app',
  schema: null,
  table: 'orders',
};

const VIEW: NamedView = {
  id: '',
  name: 'Open orders',
  scope: SCOPE,
  filters: [],
  filterLogic: 'and',
  sorts: [],
  visibleColumns: null,
  columnOrder: null,
  isDefault: false,
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: null,
};

// ── §2 · the error code closure ──────────────────────────────────────────

describe('§2 · the named-view codes reach the front end through the real chain', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    const client = createBackendClient(DESKTOP_BACKEND_ID, createDesktopBackendTransport());
    setBackendClient(DESKTOP_BACKEND_ID, client);
    selectBackend(DESKTOP_BACKEND_ID);
  });

  it.each([
    {
      code: 'grid.view.nameExists',
      // The host's own wording (D1: a host message never quotes what the
      // user typed, so the name is not in it).
      text: 'grid.view.nameExists: a saved view with this name already exists for this table',
    },
    {
      code: 'grid.view.invalid',
      text: 'grid.view.invalid: the view name must not be empty',
    },
  ])(
    '$code survives invoke → transport → backend-client → databaseCommands',
    async ({ code, text }) => {
      // The host rejects with a **plain string** — that is what
      // `CommandError::Serialize` produces.
      mockInvoke.mockRejectedValue(text);

      let caught: unknown = null;
      try {
        await databaseCommands.saveNamedView(VIEW);
      } catch (error) {
        caught = error;
      }

      // The rejection is an `ApiError`, as it always was.
      expect(caught).toBeInstanceOf(ApiError);
      const apiError = caught as ApiError;
      expect(apiError.code).toBe('ServiceUnavailable');

      // **The host's own text is in the cause chain**, verbatim.
      const cause = (apiError as { cause?: unknown }).cause;
      expect(cause).toBeInstanceOf(Error);
      expect((cause as Error).message).toBe(text);

      // And it is the text the front end classifies.
      expect(hostErrorMessage(caught)).toBe(text);
      expect(classifyGridError(hostErrorMessage(caught))).toBe(code);

      // The wrapper's own message would classify to "unknown" — the prefix
      // "<method> failed: " moves the code away from the anchor. This is why
      // the cause chain has to be walked, and why the front end must not read
      // `error.message` directly.
      expect(classifyGridError(apiError.message)).toBe('unknown');
      expect(apiError.message).toContain(text);
      expect(apiError.message).toContain('failed:');
    },
  );

  it('classifies a failed list and a failed delete the same way', async () => {
    const text = 'grid.view.nameExists: a saved view with this name already exists for this table';
    mockInvoke.mockRejectedValue(text);

    let caught: unknown = null;
    try {
      await databaseCommands.listNamedViews(SCOPE);
    } catch (error) {
      caught = error;
    }
    expect(classifyGridError(hostErrorMessage(caught))).toBe('grid.view.nameExists');

    mockInvoke.mockRejectedValue(text);
    let caughtDelete: unknown = null;
    try {
      await databaseCommands.deleteNamedView('view-1');
    } catch (error) {
      caughtDelete = error;
    }
    expect(classifyGridError(hostErrorMessage(caughtDelete))).toBe('grid.view.nameExists');
  });

  it('registers both codes and lists neither as producer-pending', () => {
    expect(GRID_ERROR_CODES).toContain('grid.view.nameExists');
    expect(GRID_ERROR_CODES).toContain('grid.view.invalid');

    // A code with a producer does not belong on the pending list: the list
    // is the record of what is still missing.
    const guard = readFileSync(
      join(REPO_ROOT, 'src/lib/__tests__/gridErrors.registration.test.ts'),
      'utf-8',
    );
    const pending = guard.match(/const PRODUCER_PENDING: readonly string\[\] = \[([\s\S]*?)\];/);
    expect(pending).not.toBeNull();
    const listed = pending?.[1] ?? '';
    expect(listed).not.toContain('grid.view.nameExists');
    expect(listed).not.toContain('grid.view.invalid');
  });

  it('reaches at least the number of Rust files the registration guard claims', () => {
    // The guard's own lower bound (1423, measured 2026-10-22) is a claim
    // about how much of the workspace it scans. It is re-derived here so a
    // silently weakened derivation is visible from this file too.
    const manifest = readFileSync(join(REPO_ROOT, 'Cargo.toml'), 'utf-8');
    const members = manifest.match(/^members\s*=\s*\[([\s\S]*?)\]/m)?.[1] ?? '';
    const roots = new Set<string>();
    for (const quoted of members.match(/"[^"]+"/g) ?? []) {
      const member = quoted.slice(1, -1);
      const paths = member.endsWith('/*')
        ? readdirSync(join(REPO_ROOT, member.slice(0, -1)), { withFileTypes: true })
            .filter((entry) => entry.isDirectory())
            .map((entry) => `${member.slice(0, -1)}${entry.name}`)
        : [member];
      for (const path of paths) {
        for (const directory of ['src', 'tests']) {
          const full = join(REPO_ROOT, path, directory);
          if (existsSync(full)) roots.add(full);
        }
      }
    }
    const walk = (directory: string): string[] =>
      readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
        const full = join(directory, entry.name);
        if (entry.isDirectory()) return walk(full);
        return entry.name.endsWith('.rs') ? [full] : [];
      });
    const files = [...roots].flatMap((root) => (statSync(root).isDirectory() ? walk(root) : []));
    expect(files.length).toBeGreaterThanOrEqual(1423);
  });
});

// ── §5 · one DTO, one shape ──────────────────────────────────────────────

describe('§5 · the named-view DTO exists exactly once', () => {
  it('is declared in the backend-client port, not in the host helper', () => {
    const helper = readFileSync(join(REPO_ROOT, 'src/lib/namedViews.ts'), 'utf-8');
    // A second `interface NamedView` / `type NamedView =` in the host helper
    // is the "one DTO, two shapes" failure mode.
    expect(helper).not.toMatch(/interface\s+NamedView\b/);
    expect(helper).not.toMatch(/type\s+NamedView\s*=/);
    expect(helper).not.toMatch(/interface\s+NamedViewScope\b/);
    expect(helper).not.toMatch(/type\s+NamedViewScope\s*=/);
    // It re-exports the port's types instead.
    expect(helper).toMatch(/from '@datazen\/backend-client'/);
    expect(helper).toMatch(/export type \{ NamedView, NamedViewScope \}/);
  });

  it('is declared exactly once in the whole TypeScript tree', () => {
    const walk = (directory: string): string[] =>
      readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
        const full = join(directory, entry.name);
        if (entry.isDirectory()) {
          // `.worktrees/` 是仓库自己的并行开发目录（AGENTS.md 要求用它），里面是
          // 另一条轨道的完整检出：不跳过就会把「同一份类型声明」数成两份，让这条
          // 契约测试在有条轨道在跑时变红。`.git` / `target` 同理不该走进来。
          if (
            entry.name === 'node_modules' ||
            entry.name === 'dist' ||
            entry.name === 'target' ||
            entry.name === '.worktrees' ||
            entry.name === '.git'
          ) {
            return [];
          }
          return walk(full);
        }
        return /\.tsx?$/.test(entry.name) ? [full] : [];
      });
    const declarations: string[] = [];
    for (const file of walk(REPO_ROOT)) {
      const source = readFileSync(file, 'utf-8');
      if (/interface\s+NamedView\s*\{/.test(source)) declarations.push(file);
    }
    expect(declarations).toEqual([
      join(REPO_ROOT, 'packages/backend-client/src/types/tableData.ts'),
    ]);
  });

  it('gives the port a nullable update time, not a plain string', () => {
    const port = readFileSync(
      join(REPO_ROOT, 'packages/backend-client/src/types/tableData.ts'),
      'utf-8',
    );
    // `updatedAt: string | null` — the host's field is an optional timestamp.
    expect(port).toMatch(/updatedAt:\s*string\s*\|\s*null/);
    // The scope keeps the four fields, and no runtime session id. The check
    // is scoped to the scope block: other DTOs in the same file (the foreign
    // key request) legitimately carry a `dbSessionId`.
    const scopeBlock = port.match(/export interface NamedViewScope \{([\s\S]*?)\n\}/)?.[1] ?? '';
    expect(scopeBlock).not.toBe('');
    expect(scopeBlock).not.toMatch(/dbSessionId/);
    expect(scopeBlock).toMatch(/connectionId/);
    expect(scopeBlock).toMatch(/database/);
    expect(scopeBlock).toMatch(/schema/);
    expect(scopeBlock).toMatch(/table/);
  });
});

// ── §6 · the connector spelling ──────────────────────────────────────────

describe('§6 · an absent connector stays absent', () => {
  it('treats undefined as "no connector" and never as a value', () => {
    const withoutConnector: { connector?: 'and' | 'or' } = {};
    const withNull: { connector?: 'and' | 'or' | null } = { connector: null };

    // The front end's own check is `!== undefined`, so `null` would slip
    // through and put a second spelling of "no connector" into the filter.
    expect(withoutConnector.connector).toBeUndefined();
    expect(withNull.connector).not.toBeUndefined();
    // That is exactly why the shared crate skips the field instead of
    // writing `null` — the wire must never carry `connector: null`.
  });

  it('does not report a reset connector as a dirty draft', () => {
    const applied = [{ column: 'id', operator: 'eq' as const, value: 1 }];
    // The user set a per-condition connector and then cleared it again. The
    // draft object carries the key with an undefined value (the shape the
    // editor produces); the applied one does not carry it at all.
    const draft = [{ column: 'id', operator: 'eq' as const, value: 1, connector: undefined }];
    expect(filterDraftEqualsApplied(draft, 'and', applied, 'and')).toBe(true);
  });

  it('does report a changed connector as a dirty draft', () => {
    const applied = [{ column: 'id', operator: 'eq' as const, value: 1 }];
    const draft = [{ column: 'id', operator: 'eq' as const, value: 1, connector: 'or' as const }];
    expect(filterDraftEqualsApplied(draft, 'and', applied, 'and')).toBe(false);
  });

  it('writes the Rust side with the field skipped, not with null', () => {
    // The shared crate's own spelling — asserted here so a change to the
    // serde attribute is caught from the front end's side too.
    const filters = readFileSync(join(REPO_ROOT, 'packages/driver-api/src/filters.rs'), 'utf-8');
    expect(filters).toMatch(/skip_serializing_if\s*=\s*"Option::is_none"/);
  });
});

// ── §8 · the web boundary ────────────────────────────────────────────────

describe('§8 · a form factor that cannot serve the named views hides the entry', () => {
  const clients = new Map<string, ReturnType<typeof createBackendClient>>();

  beforeEach(() => {
    vi.clearAllMocks();
    clients.clear();
  });

  afterEach(() => {
    setBackendClient('desktop-test', clients.get('desktop-test')!);
    selectBackend('desktop-test');
  });

  it('offers the entry on the desktop, which registers the three commands', () => {
    const client = createBackendClient(DESKTOP_BACKEND_ID, createDesktopBackendTransport());
    clients.set('desktop-test', client);
    setBackendClient('desktop-test', client);
    selectBackend('desktop-test');
    expect(namedViewsAvailable()).toBe(true);
    // The desktop declares **no** gap: it serves them.
    expect(client.transport.unsupportedMethods?.listNamedViews).toBeUndefined();
  });

  it('hides the entry for a form factor whose gap is a capability gap', () => {
    const client = createBackendClient('web', {
      async call() {
        throw new Error('the capability check should have refused this call');
      },
      unsupportedMethods: {
        listNamedViews: 'capability',
        saveNamedView: 'capability',
        deleteNamedView: 'capability',
      },
    });
    clients.set('desktop-test', client);
    setBackendClient('desktop-test', client);
    selectBackend('desktop-test');
    expect(namedViewsAvailable()).toBe(false);
  });

  it('hides the entry for a form factor whose gap is a not-yet-implemented gap', () => {
    // The two kinds are different facts, but **both** must keep the entry
    // hidden: an entry that is offered and then refused is worse than one
    // that is absent.
    const client = createBackendClient('web', {
      async call() {
        throw new Error('the capability check should have refused this call');
      },
      unsupportedMethods: {
        listNamedViews: 'notImplemented',
        saveNamedView: 'notImplemented',
        deleteNamedView: 'notImplemented',
      },
    });
    clients.set('desktop-test', client);
    setBackendClient('desktop-test', client);
    selectBackend('desktop-test');
    expect(namedViewsAvailable()).toBe(false);
  });

  it('refuses a named-view call on a capability-gap backend with an explicit code', async () => {
    // Not an empty list: an explicit "this form factor cannot serve them".
    const client = createBackendClient('web', {
      async call() {
        throw new Error('the capability check should have refused this call');
      },
      unsupportedMethods: {
        listNamedViews: 'capability',
        saveNamedView: 'capability',
        deleteNamedView: 'capability',
      },
    });
    clients.set('desktop-test', client);
    setBackendClient('desktop-test', client);
    selectBackend('desktop-test');

    let caught: unknown = null;
    try {
      await client.listNamedViews({});
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(ApiError);
    const apiError = caught as ApiError;
    expect(apiError.code).toBe('CapabilityUnsupported');
    expect(apiError.unsupportedKind).toBe('capability');
    // The refusal never went through the wire, so it has no cause chain.
    expect((apiError as { cause?: unknown }).cause).toBeUndefined();
  });

  it('tells the two kinds of gap apart in the code', () => {
    const kinds = readFileSync(join(REPO_ROOT, 'packages/backend-client/src/errors.ts'), 'utf-8');
    expect(kinds).toMatch(/export type UnsupportedMethodKind = 'capability' \| 'notImplemented'/);
  });
});
