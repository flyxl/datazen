/**
 * [tester] Round 2 — adversarial equivalence matrix for the `validate()` hoist
 * (commit `cfb63354`) and key-space isolation audit for the driver-validator
 * merge (commit `c9777246`, tunnel-form-BUG-001).
 *
 * Zero-trust goals:
 *  1. **Exhaustiveness** — every `connectionForm` variant reachable in this build
 *     must be exercised, not just `redis`. The variant list is derived from the
 *     live `DB_REGISTRY`, so a newly registered driver form fails the guard
 *     instead of silently escaping the matrix.
 *  2. **Equivalence** — a tunnel-free connection and a fully-filled inline tunnel
 *     must stay savable on every variant (the hoist must not mis-block).
 *  3. **Key-space isolation** — the only registered driver validator's key space
 *     must be disjoint from the tunnel domain, so `Object.assign` merging can
 *     never clobber a tunnel error.
 *  4. **Working exit** — switching a dangling reference to `none` must not only
 *     pass `validate()` but actually persist a tunnel-free config.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, render, screen, cleanup } from '@testing-library/react';
import { useConnectionForm } from '../useConnectionForm';
import { ConnectionAdvancedSettings } from '../ConnectionAdvancedSettings';
import { useTunnelStore } from '../../../stores/tunnelStore';
import { getDriverValidator } from '../../../extensions/generated';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import type { ConnectionConfig, SavedTunnelSummary } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/windowManager', () => ({
  openSettingsWindow: vi.fn(),
}));

const { saveConnectionMock, testConnectionMock } = vi.hoisted(() => ({
  saveConnectionMock: vi.fn(),
  testConnectionMock: vi.fn(),
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    testConnection: testConnectionMock,
    saveConnection: saveConnectionMock,
  },
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: Object.assign(
    vi.fn((selector: (s: { saveConnection: typeof saveConnectionMock }) => unknown) =>
      selector({ saveConnection: saveConnectionMock }),
    ),
    { getState: () => ({ saveConnection: saveConnectionMock }) },
  ),
}));

const mockTunnelCommands = vi.hoisted(() => ({
  getTunnels: vi.fn(),
  getTunnelSummaries: vi.fn(),
  getTunnel: vi.fn(),
  getTunnelUsage: vi.fn(),
  saveTunnel: vi.fn(),
  deleteTunnel: vi.fn(),
  testTunnel: vi.fn(),
}));

vi.mock('../../../commands/tunnel', () => ({ tunnelCommands: mockTunnelCommands }));

afterEach(cleanup);

/** The keys `validate()` owns in the tunnel domain (per the BUG-001/hoist fix). */
const TUNNEL_DOMAIN_KEYS = [
  'tunnelId',
  'httpProxyHost',
  'httpProxyPort',
  'wsUrl',
  'sshHost',
  'sshUsername',
] as const;

function editExisting(overrides: Partial<ConnectionConfig> = {}): ConnectionConfig {
  return {
    id: 'c-1',
    name: 'matrix',
    databaseType: 'postgresql',
    host: 'db.internal',
    port: 5432,
    database: 'postgres',
    sslMode: 'prefer',
    ...overrides,
  };
}

/**
 * `ConnectionConfig['databaseType']` is the codegen-derived union from
 * `generated.ts`, so it only admits the drivers the current `--drivers` set
 * injected — naming `sqlserver` under `--drivers=basic` is a type error even
 * though the matrix must cover it. This keeps the union and adds the ids the
 * matrix deliberately spans builds for; each one is still checked against the
 * live `DB_REGISTRY` at runtime (see the exhaustiveness guard and
 * `expectedFormVariant` below), so nothing here widens to an unchecked string.
 */
type CrossBuildDatabaseType = ConnectionConfig['databaseType'] | 'sqlserver';

interface Variant {
  label: string;
  databaseType: CrossBuildDatabaseType;
  formVariant: string;
  hasDriverValidator: boolean;
  supportsSSH: boolean;
  /** Valid connection-level fields so `validate()` passes with no tunnel. */
  base: Partial<ConnectionConfig>;
}

const VARIANTS: Variant[] = [
  {
    label: 'standard (postgresql)',
    databaseType: 'postgresql',
    formVariant: 'standard',
    hasDriverValidator: false,
    supportsSSH: true,
    base: { host: 'db.internal', port: 5432, database: 'postgres' },
  },
  {
    label: 'file (sqlite)',
    databaseType: 'sqlite',
    formVariant: 'file',
    hasDriverValidator: false,
    supportsSSH: false,
    base: { host: '', port: 0, database: '/tmp/datazen-tester.sqlite' },
  },
  {
    label: 'driver-validator (redis)',
    databaseType: 'redis',
    formVariant: 'redis',
    hasDriverValidator: true,
    supportsSSH: true,
    base: { host: 'cache.internal', port: 6379, database: '0' },
  },
  {
    // Driver *form* that started out without a driver *validator*: `isDriverForm`
    // is true, so the generic `!isDriverForm` branch below the validator early
    // return was skipped and nothing checked host/port — blank host/port saved
    // cleanly while `ConnectionFields.tsx` renders both as `<Label required>`.
    // `sqlServerValidate` (driver side) now owns that check, like redis.
    label: 'driver-form (sqlserver)',
    databaseType: 'sqlserver',
    formVariant: 'sqlserver',
    hasDriverValidator: true,
    supportsSSH: true,
    base: { host: 'db.internal', port: 1433, database: 'master', username: 'sa' },
  },
];

function renderVariant(variant: Variant, tunnel: Partial<ConnectionConfig> = {}) {
  // Downcast, not a blanket escape hatch: when the driver is absent from this
  // build the id is still what reaches the hook, so the case keeps exercising the
  // `meta?.connectionForm ?? 'standard'` fallback for an unregistered driver type.
  const databaseType = variant.databaseType as ConnectionConfig['databaseType'];
  return renderHook(() =>
    useConnectionForm({
      editId: 'c-1',
      existingConnections: [editExisting({ databaseType, ...variant.base, ...tunnel })],
    }),
  ).result;
}

/**
 * Is this variant's driver part of *this* build? `DB_REGISTRY` is codegen output
 * (`resolve-drivers.mjs --drivers=<set>`), so `sqlserver` is simply absent under
 * `--drivers=basic`. Facts that only hold for a driver this build actually injected are
 * asserted only when this is true — the same decidable condition the `connectionForm`
 * guard above already used, extracted so the two conditionals cannot drift apart.
 */
function isInThisBuild(variant: Variant): boolean {
  return variant.databaseType in DB_REGISTRY;
}

beforeEach(() => {
  vi.clearAllMocks();
  useTunnelStore.setState({ summaries: [], loaded: false, loading: false, error: null });
  mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);
  mockTunnelCommands.getTunnel.mockResolvedValue(null);
});

describe('[tester] validate() variant matrix is exhaustive for this build', () => {
  it('test_tester covers every connectionForm variant present in DB_REGISTRY', () => {
    const registered = [...new Set(Object.values(DB_REGISTRY).map((m) => m.connectionForm))].sort();
    const covered = new Set(VARIANTS.map((v) => v.formVariant));
    // One direction only: every form this build registers needs a VARIANTS entry.
    // The reverse is deliberately NOT asserted. VARIANTS is a static manifest while
    // DB_REGISTRY is codegen output (`resolve-drivers.mjs --drivers=<set>`), so a
    // manifest entry for a form the current build did not inject — `sqlserver` under
    // `--drivers=basic` — is expected, not a defect. `toEqual` on both sides made the
    // result depend on which drivers the build happened to select instead of on real
    // coverage, so adding a variant could only ever move the failure between builds.
    const uncovered = registered.filter((form) => !covered.has(form));
    expect(
      uncovered,
      `connectionForm(s) registered in DB_REGISTRY with no VARIANTS entry: ${uncovered.join(', ') || '(none)'} — add a variant for each so the matrix stays exhaustive.`,
    ).toEqual([]);
  });

  it('test_tester every driver validator this build registers has a VARIANTS row', () => {
    const validatorsInBuild = [
      ...new Set(Object.values(DB_REGISTRY).map((m) => m.connectionForm)),
    ]
      .filter((formVariant) => !!getDriverValidator(formVariant))
      .sort();
    // The static manifest, which is codegen-independent: these are the variants the
    // matrix *declares* own a driver validator. Pinning it keeps the subset guard below
    // from going vacuous — emptying `hasDriverValidator` would shrink `declared` until
    // nothing could ever be uncovered while the matrix stopped exercising those forms.
    const declared = VARIANTS.filter((v) => v.hasDriverValidator).map((v) => v.formVariant);
    expect(declared).toEqual(['redis', 'sqlserver']);

    // One direction only, the same shape as the connectionForm guard above: every form
    // this build registers a validator for needs a VARIANTS row declaring it. The reverse
    // is deliberately NOT asserted. VARIANTS is a static manifest while `validatorsInBuild`
    // is codegen output (`resolve-drivers.mjs --drivers=<set>`), so a declared row for a
    // form the current build did not inject — `sqlserver` under `--drivers=basic`, where
    // `getDriverValidator('sqlserver')` is `undefined` by construction — is expected, not a
    // defect. `toEqual` on both sides made the result depend on which drivers the build
    // happened to select, so declaring a validator could only ever move the failure between
    // builds. Registering a validator for a form with no row (a new superset/olap/kiwi
    // validator) still goes red under `--drivers=all`.
    const undeclared = validatorsInBuild.filter((form) => !declared.includes(form));
    expect(
      undeclared,
      `driver validator(s) registered in this build with no VARIANTS row declaring hasDriverValidator: ${undeclared.join(', ') || '(none)'} — add a variant for each so the matrix stays exhaustive.`,
    ).toEqual([]);
  });
});

describe('[tester] inline tunnel validation equivalence matrix', () => {
  it.each(VARIANTS)('$label — a tunnel-free connection stays savable', (variant) => {
    const result = renderVariant(variant);

    // The static manifest stays authoritative for every driver this build registers —
    // a renamed form still fails here. Only a driver outside the current codegen set
    // has no `DB_REGISTRY` entry, and then the hook legitimately resolves the generic
    // form (`useConnectionForm.ts`: `meta?.connectionForm ?? 'standard'`).
    const expectedFormVariant = isInThisBuild(variant) ? variant.formVariant : 'standard';
    expect(result.current.formVariant).toBe(expectedFormVariant);
    // `hasDriverValidator` stays the static declaration of intent — "sqlserver *should*
    // own a validator" — and is never flipped to `false` just to make a build that lacks
    // the driver come out green; that would make the matrix lie about the product defect
    // it exists to guard. `getDriverValidator` reads the same codegen output as
    // `DB_REGISTRY`, so for a driver this build did not inject it is `undefined` by
    // construction and there is no honest assertion available there. The check is real
    // wherever the driver exists: it runs for `redis` in every build, and for `sqlserver`
    // under `--drivers=basic,sqlserver` / `--drivers=all`, where dropping the
    // `validator:` registration from `resolve-drivers.mjs` turns it red.
    if (isInThisBuild(variant)) {
      expect(!!getDriverValidator(variant.formVariant)).toBe(variant.hasDriverValidator);
    }
    expect(result.current.tunnelSource).toBe('none');
    expect(result.current.tunnelRefMissing).toBe(false);
    act(() => {
      expect(result.current.validate()).toBe(true);
    });
    expect(result.current.validationErrors).toEqual({});
  });

  it.each(VARIANTS)('$label — a blank inline HTTP proxy is rejected', (variant) => {
    const result = renderVariant(variant, { tunnelKind: 'httpProxy' });
    act(() => {
      result.current.setHttpProxyHost('');
      result.current.setHttpProxyPort('');
    });

    expect(result.current.tunnelSource).toBe('inline');
    expect(result.current.effectiveTunnelKind).toBe('httpProxy');
    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    expect(result.current.validationErrors.httpProxyHost).toBe('newConn.required');
    expect(result.current.validationErrors.httpProxyPort).toBe('newConn.required');
  });

  it.each(VARIANTS)('$label — a blank inline WebSocket is rejected', (variant) => {
    const result = renderVariant(variant, { tunnelKind: 'websocket' });
    act(() => {
      result.current.setWsUrl('');
    });

    expect(result.current.effectiveTunnelKind).toBe('websocket');
    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    expect(result.current.validationErrors.wsUrl).toBe('newConn.required');
  });

  it.each(VARIANTS.filter((v) => v.supportsSSH))(
    '$label — a blank inline SSH is rejected',
    (variant) => {
      const result = renderVariant(variant, { tunnelKind: 'ssh' });
      act(() => {
        result.current.setSshHost('');
        result.current.setSshUsername('');
      });

      expect(result.current.effectiveTunnelKind).toBe('ssh');
      act(() => {
        expect(result.current.validate()).toBe(false);
      });
      expect(result.current.validationErrors.sshHost).toBe('newConn.required');
      expect(result.current.validationErrors.sshUsername).toBe('newConn.required');
    },
  );

  it.each(VARIANTS)('$label — a fully-filled inline HTTP proxy stays savable', (variant) => {
    const result = renderVariant(variant, { tunnelKind: 'httpProxy' });
    act(() => {
      result.current.setHttpProxyHost('proxy.internal');
      result.current.setHttpProxyPort('8080');
    });

    expect(result.current.effectiveTunnelKind).toBe('httpProxy');
    act(() => {
      expect(result.current.validate()).toBe(true);
    });
    expect(result.current.validationErrors).toEqual({});
  });

  it.each(VARIANTS)(
    '$label — a resolved `saved` reference is never judged by inline fields',
    (variant) => {
      const summary: SavedTunnelSummary = { id: 'tun_ok', name: 'Ok', kind: 'httpProxy' };
      useTunnelStore.setState({
        summaries: [summary],
        loaded: true,
        loading: false,
        error: null,
      });

      const result = renderVariant(variant, { tunnelId: 'tun_ok', tunnelKind: 'httpProxy' });
      // Blank the inline fields on purpose: with a valid reference the `!tunnelId`
      // guard must keep the inline requirements switched off.
      act(() => {
        result.current.setHttpProxyHost('');
        result.current.setHttpProxyPort('');
      });

      expect(result.current.tunnelSource).toBe('saved');
      expect(result.current.savedTunnel?.id).toBe('tun_ok');
      expect(result.current.tunnelRefMissing).toBe(false);
      act(() => {
        expect(result.current.validate()).toBe(true);
      });
      expect(result.current.validationErrors).toEqual({});
    },
  );
});

describe('[tester] generic (non-driver) connection checks kept their scope', () => {
  it('test_tester keeps the host/port checks on a standard non-driver variant', () => {
    const result = renderVariant(VARIANTS[0]);
    // `standard` has no driver validator, so these errors can only come from the
    // generic `!isDriverForm` branch that stayed below the driver early return.
    expect(getDriverValidator('standard')).toBeUndefined();

    act(() => {
      result.current.setHost('');
      result.current.setPort('');
    });
    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    expect(result.current.validationErrors.host).toBe('newConn.required');
    expect(result.current.validationErrors.port).toBe('newConn.required');
  });

  it('test_tester keeps the file-mode database check and skips host/port for it', () => {
    const result = renderVariant(VARIANTS[1]);
    expect(getDriverValidator('file')).toBeUndefined();

    act(() => {
      result.current.setDatabase('');
    });
    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    // `else if` scoping preserved: file mode reports the path, never host/port.
    expect(result.current.validationErrors.database).toBe('newConn.required');
    expect(result.current.validationErrors.host).toBeUndefined();
    expect(result.current.validationErrors.port).toBeUndefined();
  });
});

describe('[tester] driver-validator key space vs the tunnel domain', () => {
  /** Every key the real redis validator can emit, across all of its topologies. */
  function redisValidatorKeySpace(): string[] {
    const validator = getDriverValidator('redis');
    expect(validator).toBeDefined();
    const translate = (key: string) => key;
    const base = { host: '', port: '', database: '', username: '', password: '', schema: '' };
    const keys = new Set<string>();
    for (const options of [
      {},
      { topology: 'cluster' },
      { topology: 'cluster', clusterNodes: ['not-a-host-port'] },
      { topology: 'sentinel' },
      { topology: 'sentinel', sentinelNodes: ['bad'], sentinelMasterName: 'master' },
    ]) {
      Object.keys(validator!({ ...base, options }, translate)).forEach((key) => keys.add(key));
    }
    return [...keys].sort();
  }

  it('test_tester emits no key that collides with the tunnel domain', () => {
    const keys = redisValidatorKeySpace();
    // Pinned so a future redis validator key addition is a conscious decision.
    expect(keys).toEqual(['clusterNodes', 'host', 'port', 'sentinelMasterName', 'sentinelNodes']);
    expect(keys.filter((key) => (TUNNEL_DOMAIN_KEYS as readonly string[]).includes(key))).toEqual(
      [],
    );
  });

  it('test_tester keeps tunnel and driver errors side by side on a redis form', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);

    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c-redis',
        existingConnections: [
          editExisting({
            id: 'c-redis',
            databaseType: 'redis',
            port: 6379,
            sslMode: 'disable',
            tunnelId: 'tun_gone',
          }),
        ],
      }),
    );
    await act(async () => {
      await useTunnelStore.getState().load();
    });

    act(() => {
      result.current.setHost('');
      result.current.setPort('');
    });
    act(() => {
      expect(result.current.validate()).toBe(false);
    });

    // Tunnel domain error survives the driver merge...
    expect(result.current.validationErrors.tunnelId).toBe('newConn.tunnelMissing');
    // ...and the driver validator's own errors survive the tunnel domain.
    expect(result.current.validationErrors.host).toBe('newConn.required');
    expect(result.current.validationErrors.port).toBe('newConn.required');

    await act(async () => {
      await result.current.onSave();
    });
    expect(saveConnectionMock).not.toHaveBeenCalled();
  });
});

describe('[tester] a dangling reference has a working exit that persists', () => {
  it.each(VARIANTS)(
    '$label — switching the source to `none` saves a tunnel-free config',
    async (variant) => {
      mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);

      const result = renderVariant(variant, { tunnelId: 'tun_gone' });
      await act(async () => {
        await useTunnelStore.getState().load();
      });

      expect(result.current.tunnelRefMissing).toBe(true);
      act(() => {
        expect(result.current.validate()).toBe(false);
      });

      act(() => {
        result.current.setTunnelSource('none');
      });
      expect(result.current.tunnelId).toBeNull();
      expect(result.current.tunnelRefMissing).toBe(false);
      act(() => {
        expect(result.current.validate()).toBe(true);
      });

      await act(async () => {
        await result.current.onSave();
      });
      expect(saveConnectionMock).toHaveBeenCalledTimes(1);
      const saved = saveConnectionMock.mock.calls[0][0] as ConnectionConfig;
      expect(saved.tunnelId).toBeUndefined();
      expect(saved.tunnelKind).toBeUndefined();
    },
  );
});

describe('[tester] dangling-state panel rendering (real hook + real component)', () => {
  it('test_tester hides the dead-end unbind button and only advertises reachable actions', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);

    const result = renderVariant(VARIANTS[0], { tunnelId: 'tun_gone' });
    await act(async () => {
      await useTunnelStore.getState().load();
    });
    expect(result.current.tunnelRefMissing).toBe(true);

    render(<ConnectionAdvancedSettings form={result.current} />);

    const alert = screen.getByTestId('new-conn-tunnel-missing');
    expect(alert).toHaveTextContent('newConn.tunnelMissing');
    // Empty collection: no alternative is reachable, so it must not be offered.
    expect(alert).not.toHaveTextContent('newConn.tunnelMissingAlt');
    expect(alert).not.toHaveTextContent('newConn.tunnelUnbind');
    expect(screen.queryByTestId('new-conn-tunnel-unbind')).not.toBeInTheDocument();
    // The reachable exit named by the alert is present.
    expect(screen.getByTestId('new-conn-tunnel-source')).toBeInTheDocument();
  });

  it('test_tester offers the alternative only when another tunnel is actually listed', async () => {
    const other: SavedTunnelSummary = { id: 'tun_other', name: 'Other', kind: 'ssh' };
    useTunnelStore.setState({ summaries: [other], loaded: true, loading: false, error: null });

    const result = renderVariant(VARIANTS[0], { tunnelId: 'tun_gone' });
    expect(result.current.tunnelRefMissing).toBe(true);
    expect(result.current.savedTunnels).toHaveLength(1);

    render(<ConnectionAdvancedSettings form={result.current} />);

    const alert = screen.getByTestId('new-conn-tunnel-missing');
    expect(alert).toHaveTextContent('newConn.tunnelMissing');
    expect(alert).toHaveTextContent('newConn.tunnelMissingAlt');
    expect(screen.queryByTestId('new-conn-tunnel-unbind')).not.toBeInTheDocument();
  });

  it('test_tester still renders the unbind button for a resolvable reference', async () => {
    const summary: SavedTunnelSummary = { id: 'tun_ok', name: 'Ok', kind: 'httpProxy' };
    useTunnelStore.setState({ summaries: [summary], loaded: true, loading: false, error: null });

    const result = renderVariant(VARIANTS[0], { tunnelId: 'tun_ok' });
    expect(result.current.tunnelRefMissing).toBe(false);

    render(<ConnectionAdvancedSettings form={result.current} />);

    expect(screen.queryByTestId('new-conn-tunnel-missing')).not.toBeInTheDocument();
    expect(screen.getByTestId('new-conn-tunnel-unbind')).toBeInTheDocument();
  });
});
