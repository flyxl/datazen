/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import {
  checkManagedStubs,
  staticDriverDeclarationProblems,
  stripMarkerBlocks,
} from '../check-managed-stubs.mjs';

const REGISTRY = {
  postgres: { source: 'path', feature: 'driver-postgres' },
  redis: { source: 'path', feature: 'driver-redis' },
  kiwi: { source: 'git', feature: 'driver-kiwi' },
};

const CONSISTENT_TOML = `# <<driver-dependencies>>
# <</driver-dependencies>>
datazen-driver-postgres = { path = "../../../crates/drivers/postgres", optional = true }
datazen-driver-redis = { path = "../../../crates/drivers/redis", optional = true }
[features]
default = []
# <<driver-features>>
# <</driver-features>>
driver-postgres = ["dep:datazen-driver-postgres"]
driver-redis = ["dep:datazen-driver-redis"]
drivers = ["driver-postgres", "driver-redis"]
`;

describe('stripMarkerBlocks', () => {
  it('removes injected marker content but keeps the markers', () => {
    const injected = `# <<driver-dependencies>>\ndatazen-plugin-kiwi = { path = "../../../crates/drivers/kiwi", optional = true }\n# <</driver-dependencies>>\nstatic = 1\n`;
    const out = stripMarkerBlocks(injected);
    expect(out).not.toContain('datazen-plugin-kiwi');
    expect(out).toContain('static = 1');
    expect(out).toContain('# <<driver-dependencies>>');
  });
});

describe('staticDriverDeclarationProblems', () => {
  const log = () => {};
  const error = () => {};

  it('accepts a manifest consistent with the registry', () => {
    expect(
      staticDriverDeclarationProblems({
        registry: REGISTRY,
        cargoToml: CONSISTENT_TOML,
        log,
        error,
      }),
    ).toEqual([]);
  });

  it('rejects a missing static dep for a path driver', () => {
    const toml = CONSISTENT_TOML.replace(/^datazen-driver-redis = .*\n/m, '');
    expect(
      staticDriverDeclarationProblems({ registry: REGISTRY, cargoToml: toml, log, error }).some(
        (p) => p.includes('static optional dep') && p.includes('"redis"'),
      ),
    ).toBe(true);
  });

  it('rejects a missing aggregate seat for a path-driver feature', () => {
    const toml = CONSISTENT_TOML.replace('", "driver-redis"]', '"]');
    expect(
      staticDriverDeclarationProblems({ registry: REGISTRY, cargoToml: toml, log, error }).some(
        (p) => p.includes('"drivers" aggregate is missing') && p.includes('driver-redis'),
      ),
    ).toBe(true);
  });

  it('rejects a git driver statically declared outside the markers', () => {
    const toml = `${CONSISTENT_TOML}datazen-driver-kiwi = { path = "../../../crates/drivers/kiwi", optional = true }\n`;
    expect(
      staticDriverDeclarationProblems({ registry: REGISTRY, cargoToml: toml, log, error }).some(
        (p) => p.includes('git driver "kiwi" must NOT be statically declared'),
      ),
    ).toBe(true);
  });

  it('ignores git-driver lines that live inside the marker blocks', () => {
    const toml = `# <<driver-dependencies>>\ndatazen-driver-kiwi = { path = "../../../crates/drivers/kiwi", optional = true }\n# <</driver-dependencies>>\n${CONSISTENT_TOML}`;
    expect(
      staticDriverDeclarationProblems({ registry: REGISTRY, cargoToml: toml, log, error }),
    ).toEqual([]);
  });
});

describe('checkManagedStubs --check-static-drivers', () => {
  it('fails closed when the desktop manifest cannot be read', () => {
    const errors: string[] = [];
    const code = checkManagedStubs({
      root: '/nonexistent-root-for-w3c-test',
      files: [],
      checkStaticDrivers: true,
      log: () => {},
      error: (m) => errors.push(String(m)),
    });
    expect(code).toBe(1);
    expect(errors.some((e) => e.includes('cannot read static-declaration inputs'))).toBe(true);
  });
});
