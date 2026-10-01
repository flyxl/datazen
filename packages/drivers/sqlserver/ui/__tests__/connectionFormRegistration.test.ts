/**
 * [driver:sqlserver] Being the export `sqlServerValidate` is not the same property
 * as being *registered*: `resolve-drivers.mjs` is what puts a driver form variant
 * into the generated `DRIVER_VALIDATORS` map, and without that entry
 * `getDriverValidator('sqlserver')` returns nothing and the form silently
 * degrades. So the registration needs its own evidence:
 *
 *  - **export** — what `ConnectionFields.tsx` provides. Asserted in
 *    `connectionFieldsValidate.test.ts`.
 *  - **registration** — whether codegen binds the `sqlserver` form variant to
 *    that export. Asserted here.
 *
 * The host matrix (`src/components/connection/__tests__/tester_tunnelValidationMatrix.test.tsx`)
 * cannot witness the registration in the pipeline that gates merges: PR CI
 * builds `--drivers=basic`, where `generated.ts` carries no `sqlserver` entry,
 * `DB_REGISTRY` has no `sqlserver`, and every host assertion about it is skipped.
 * Measuring it here works in every build because the question is asked of the
 * *generator* — "what would it emit for a set that includes this driver?" — not
 * of whatever the current checkout happens to have generated. That also means
 * this suite needs no codegen step and is unaffected by the active driver set.
 *
 * The two sides are deliberately read from different places. The emitted
 * registry says *which* symbol codegen wires up; the module namespace says
 * *whether that symbol exists and works*. So deleting the `validator` entry, or
 * renaming it to something that does not exist, or pointing it at another module
 * each fail here rather than quietly unregistering the form.
 *
 * The generator is driven through a Node child process rather than a static
 * `import ... from '../../../../../scripts/resolve-drivers.mjs'`: the build
 * scripts are plain `.mjs` with no declarations, and importing them from the
 * root tsc program raises TS7016 (see the note in `tsconfig.scripts.json`).
 * Adding a hand-written `.d.mts` would resolve, but it would also shadow the
 * real module for `tsconfig.scripts.json` and quietly downgrade that gate from
 * inferred to declared types. Loading the same file through Node costs one
 * spawn and weakens nothing.
 *
 * Scope — the layer this suite asserts on, and the layer it does not:
 *
 * 1. **Asserted here: the codegen contract**, with every party in it real. The real
 *    `renderFrontendRegistry` emits the registry text; these assertions parse that
 *    emitted text; the binding it names is resolved in the real `../ConnectionFields`
 *    namespace and called. Nothing here restates `FRONTEND_DRIVER_CONFIG`.
 * 2. **Not asserted here: that the emitted file is actually evaluated.** This suite
 *    never imports `src/extensions/generated.ts`, so it does not cover "the
 *    `generated.ts` of this checkout being really evaluated inside the Host". Left
 *    unwitnessed by this file: that the emitted text is valid TypeScript, that its
 *    relative specifiers resolve from `src/extensions/`, and that `getDriverValidator`
 *    returns what the emitted map says.
 * 3. **Who covers that remainder.** Host suites do, by importing the real, unmocked
 *    module: `tester_tunnelValidationMatrix.test.tsx`, `tester_tunnelRefIntegrity.test.ts`,
 *    `src/lib/__tests__/driverIconMap.test.ts`, `src/lib/__tests__/connectionViews.test.ts`.
 *    Their limit, so they are not read for more than they are: each only reaches the
 *    drivers the active `--drivers` set injected. Under `--drivers=basic` nobody
 *    evaluates an emitted `sqlserver` entry. This suite guarantees the generator emits
 *    one; those suites guarantee an emitted registry is loadable; for this driver on
 *    that build, the two guarantees are never joined, and neither file may be cited as
 *    evidence for the other's half.
 * 4. **Why neither side is reached by a plain `import`.** `generated.ts` is not imported
 *    because the PR CI set contains no `sqlserver`: importing it would turn every
 *    assertion here into a skip — the exact failure this suite exists to prevent. The
 *    generator is not imported statically because it is declaration-less `.mjs`
 *    (TS7016), and a hand-written `.d.mts` would shadow the real module for
 *    `tsconfig.scripts.json`, downgrading that gate from inferred to declared types;
 *    a child-process spawn costs one fork and weakens nothing.
 */
import { describe, it, expect } from 'vitest';
import { execFileSync } from 'child_process';
import { dirname, resolve } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import * as connectionFieldsModule from '../ConnectionFields';

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(TEST_DIR, '../../../../..');
const SCRIPT = resolve(ROOT, 'scripts/resolve-drivers.mjs');
/** `src/extensions/` — the directory the emitted import specifiers are relative to. */
const GENERATED_DIR = resolve(ROOT, 'src/extensions');
/** This module, extension-stripped, so emitted specifiers compare on identity. */
const OWN_MODULE = resolve(TEST_DIR, '../ConnectionFields');

const FORM_VARIANT = 'sqlserver';

/** Runs in the child: resolve both driver sets, then render the registry for each. */
const BRIDGE = `
import { readFileSync } from 'fs';
import { resolve } from 'path';
const ROOT = ${JSON.stringify(ROOT)};
const m = await import(${JSON.stringify(pathToFileURL(SCRIPT).href)});
const registry = JSON.parse(readFileSync(resolve(ROOT, 'drivers-registry.json'), 'utf-8'));
const all = m.resolveDrivers('all', registry);
const basic = m.resolveDrivers('basic', registry);
process.stdout.write(JSON.stringify({
  all,
  basic,
  allSource: m.renderFrontendRegistry(all, 'community'),
  basicSource: m.renderFrontendRegistry(basic, 'community'),
}));
`;

interface RenderedRegistries {
  all: string[];
  basic: string[];
  allSource: string;
  basicSource: string;
}

const rendered: RenderedRegistries = JSON.parse(
  execFileSync(process.execPath, ['--input-type=module', '--eval', BRIDGE], {
    cwd: ROOT,
    encoding: 'utf-8',
    maxBuffer: 32 * 1024 * 1024,
  }),
) as RenderedRegistries;

interface EmittedRegistry {
  /** local binding name -> the module specifier it is imported from */
  imports: Map<string, string>;
  /** form variant -> the local binding name it is mapped to */
  validators: Map<string, string>;
}

/** Read back the two structures this test cares about from a generated file body. */
function parseEmitted(content: string): EmittedRegistry {
  const imports = new Map<string, string>();
  for (const m of content.matchAll(/^import \{([^}]*)\} from '([^']+)';$/gm)) {
    for (const raw of m[1]!.split(',')) {
      const name = raw.trim();
      if (name) imports.set(name, m[2]!);
    }
  }

  const validators = new Map<string, string>();
  const block = /const DRIVER_VALIDATORS[^=]*= \{([^}]*)\};/.exec(content);
  for (const line of block?.[1]?.split('\n') ?? []) {
    const m = /^\s*([A-Za-z0-9_]+):\s*([A-Za-z0-9_]+),?\s*$/.exec(line);
    if (m) validators.set(m[1]!, m[2]!);
  }
  return { imports, validators };
}

/** An emitted specifier as an absolute, extension-stripped module path. */
function resolveEmitted(specifier: string): string {
  return resolve(GENERATED_DIR, specifier).replace(/\.(tsx?|jsx?)$/, '');
}

const withSqlServer = parseEmitted(rendered.allSource);
const basicBuild = parseEmitted(rendered.basicSource);

describe('[driver:sqlserver] the driver set these assertions rest on', () => {
  it('expands to a set containing sqlserver, so the checks below are not vacuous', () => {
    expect(rendered.all).toContain(FORM_VARIANT);
  });

  it('excludes sqlserver from the PR CI set, which is why the host matrix skips it', () => {
    expect(rendered.basic).not.toContain(FORM_VARIANT);
  });
});

describe('[driver:sqlserver] connectionForm registration', () => {
  it('binds the sqlserver form variant in the generated validator registry', () => {
    expect(withSqlServer.validators.has(FORM_VARIANT)).toBe(true);
  });

  it("binds it to a symbol imported from this crate's ConnectionFields", () => {
    const binding = withSqlServer.validators.get(FORM_VARIANT);
    expect(binding).toBeDefined();

    const specifier = withSqlServer.imports.get(binding!);
    expect(specifier).toBeDefined();
    expect(resolveEmitted(specifier!)).toBe(OWN_MODULE);
  });

  it('resolves that binding to a named export of this module', () => {
    const binding = withSqlServer.validators.get(FORM_VARIANT)!;
    expect(Object.keys(connectionFieldsModule)).toContain(binding);
    expect(typeof (connectionFieldsModule as Record<string, unknown>)[binding]).toBe('function');
  });

  it('registers a validator that rejects the empty form, under the registered name', () => {
    const binding = withSqlServer.validators.get(FORM_VARIANT)!;
    const validate = (connectionFieldsModule as Record<string, unknown>)[binding] as (
      fields: Record<string, string>,
      t: (key: string) => string,
    ) => Record<string, string>;
    const t = (key: string) => key;

    expect(validate({ host: '', port: '' }, t)).toEqual({
      host: 'newConn.required',
      port: 'newConn.required',
    });
    expect(validate({ host: 'db.internal', port: '1433', database: 'master' }, t)).toEqual({});
  });

  it('emits no sqlserver entry for the PR CI set, so nothing here depends on codegen', () => {
    expect(basicBuild.validators.has(FORM_VARIANT)).toBe(false);
  });
});
