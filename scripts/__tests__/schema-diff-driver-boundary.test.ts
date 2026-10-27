/** @vitest-environment node */
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { expect, it } from 'vitest';

interface CargoPackage {
  name: string;
  dependencies: { name: string; kind: 'dev' | 'build' | null }[];
}

it('keeps the shared modules crate independent of concrete drivers at runtime', () => {
  const root = fileURLToPath(new URL('../../', import.meta.url));
  // Cargo resolves renamed, workspace, optional and target-specific declarations.
  // --no-deps avoids building the graph or requiring any registry/network access.
  const metadata = JSON.parse(
    execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps', '--offline'], {
      cwd: root,
      encoding: 'utf8',
    }),
  ) as { packages: CargoPackage[] };
  const domain = metadata.packages.find((pkg) => pkg.name === 'datazen');
  expect(domain).toBeDefined();
  expect(
    domain?.dependencies.filter(
      (dependency) =>
        dependency.kind !== 'dev' &&
        dependency.name.startsWith('datazen-driver-') &&
        dependency.name !== 'datazen-driver-api',
    ),
  ).toEqual([]);
});
