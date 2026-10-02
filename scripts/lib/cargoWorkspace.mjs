import { spawnSync } from 'child_process';
import { join, relative, resolve, sep } from 'path';

/**
 * Cargo workspace facts for the platform boundary guards.
 *
 * Everything in here answers ONE question — *which workspace member is which
 * layer, and what does that layer actually link?* — because three different
 * callers need the same answer and must never disagree:
 *
 *   * `check-platform-crate-boundaries.mjs` (the gate) forbids edges;
 *   * `run-platform-crate-tests.mjs` (the CI test runner) picks crates to test;
 *   * `check-platform-crate-boundaries` unit fixtures reuse the classifier.
 *
 * Declaring the layer set twice is how `check-module-layers.mjs` and
 * `check-driver-import-boundaries.mjs` came to disagree about which files
 * exist, so the set is declared exactly once, here.
 *
 * ## Why the graph comes from `resolve` and not from `packages[].dependencies`
 *
 * `cargo metadata` (no `--no-deps`) returns `resolve`: the **feature-resolved**
 * graph. `packages[].dependencies` is the *declared* one, which lists optional
 * dependencies whether or not anything enables them. Measured on this repo at
 * 6175f1c1f: `packages/drivers/redis/Cargo.toml:27` declares
 * `tauri = { version = "2", optional = true }` behind a `tauri-plugin` feature
 * that no workspace member enables, so the declared form would report a Tauri
 * edge into the driver layer for a crate that is never linked. Only the
 * resolved graph is what the linker actually sees, and only the resolved graph
 * is what "独立构建 / builds standalone" means.
 *
 * The price, stated rather than hidden: a virtual workspace resolves **one**
 * feature set for the whole workspace, so an optional dependency enabled by
 * *some* member is visible from every member that shares its parent crate. That
 * can only over-report an edge, never drop one — the check errs toward red,
 * never toward a silent pass.
 *
 * ## Why layer membership comes from a path, never from a crate name
 *
 * A list of crate names is a list that rots: the day a crate is added under
 * `packages/application/` and the list is not updated, the new crate is
 * completely unguarded and the gate is green for the wrong reason. The layers
 * here are therefore keyed by the **directory** the spec names
 * (`shared-boundaries-and-ports.md` §2.2 / §2.4 / §2.5), joined against each
 * member's own `manifest_path`. A crate appearing under a classified path is
 * classified automatically; a member matching no path is an **error**, never a
 * pass (`classifyMemberDir`).
 */

/**
 * `cargo metadata` has to print the whole 937-node graph (4.9 MB measured at
 * 6175f1c1f). `spawnSync`'s default `maxBuffer` is 1 MB, so without this the
 * call fails with ENOBUFS on a perfectly healthy workspace.
 */
const METADATA_MAX_BUFFER = 128 * 1024 * 1024;

/**
 * The layer vocabulary, keyed by workspace-relative **directory**, exactly as
 * `shared-boundaries-and-ports.md` §2.4 / §2.5 names them.
 *
 * `kind: 'ts'` rows are not Cargo members at all (F-07 scans source text); they
 * are here so a single table answers "what layer is `packages/backend-client`?"
 * for every caller.
 *
 * A path ending in `/*` matches any directory strictly beneath it.
 */
export const LAYERS = Object.freeze([
  { id: 'host', path: 'src-tauri', kind: 'rust' },
  { id: 'application', path: 'packages/application', kind: 'rust' },
  { id: 'runtime', path: 'packages/runtime', kind: 'rust' },
  { id: 'platform-api', path: 'packages/platform-api', kind: 'rust' },
  { id: 'server', path: 'server', kind: 'rust' },
  { id: 'driver-api', path: 'packages/driver-api', kind: 'rust' },
  { id: 'ai-api', path: 'packages/ai-api', kind: 'rust' },
  { id: 'driver', path: 'packages/drivers/*', kind: 'rust' },
  { id: 'backend-client', path: 'packages/backend-client', kind: 'ts' },
]);

/** @param {string} id */
export function layerById(id) {
  return LAYERS.find((l) => l.id === id) ?? null;
}

/** Workspace-relative POSIX path of the directory holding `manifestPath`. */
export function manifestDir(manifestPath, workspaceRoot) {
  return relative(workspaceRoot, resolve(join(manifestPath, '..'))).split(sep).join('/');
}

/**
 * Classify one workspace member by its directory.
 *
 * Returns `{ id, kind, path }` on a hit, and `{ unclassified: <why> }` on a
 * miss. The miss is the load-bearing branch: it is what turns "a crate nobody
 * thought about" into a red gate instead of an unexamined green one.
 *
 * @param {string} relDir workspace-relative POSIX directory
 */
export function classifyMemberDir(relDir) {
  for (const layer of LAYERS) {
    if (layer.path.endsWith('/*')) {
      const prefix = layer.path.slice(0, -2);
      // Segment-boundary match: `packages/drivers/foo` is a driver,
      // `packages/drivers-archive/foo` is not.
      if (relDir.startsWith(`${prefix}/`)) return { ...layer };
    } else if (relDir === layer.path) {
      return { ...layer };
    }
  }
  const known = LAYERS.map((l) => l.path).join(', ');
  return {
    unclassified:
      `workspace member at \`${relDir}\` matches no layer path (known: ${known}). ` +
      `Add it to LAYERS in scripts/lib/cargoWorkspace.mjs and to §2.4 of ` +
      `shared-boundaries-and-ports.md. This is an error on purpose: a member ` +
      `this guard cannot classify is a member no rule covers.`,
  };
}

/**
 * Prefix-family match on a Cargo package name.
 *
 * Cargo package names may separate words with `-` or `_`, so both are checked;
 * `tauri` must match `tauri-plugin-dialog` but must not match `taurium`.
 *
 * @param {string} name
 * @param {string} family
 */
export function matchesCrateFamily(name, family) {
  return name === family || name.startsWith(`${family}-`) || name.startsWith(`${family}_`);
}

/**
 * Run `cargo metadata` and return the parsed document.
 *
 * Throws (after reporting) on: cargo absent, non-zero exit, unparsable output.
 * It never returns a partial or synthesised document — a guard that invents an
 * empty graph would report "clean" for a workspace it never looked at.
 *
 * @param {{ cwd: string, env?: NodeJS.ProcessEnv, log?: (s: string) => void, error?: (s: string) => void }} options
 */
export function runCargoMetadata({ cwd, env, log = () => {}, error = console.error }) {
  const result = spawnSync('cargo', ['metadata', '--format-version', '1'], {
    cwd,
    encoding: 'utf8',
    maxBuffer: METADATA_MAX_BUFFER,
    env,
  });
  if (result.error) {
    throw new Error(
      `could not run \`cargo metadata\` in ${cwd}: ${result.error.message}. ` +
        `This guard needs a Cargo toolchain; it is wired into the CI \`rust\` job for that reason.`,
    );
  }
  if (result.status !== 0) {
    throw new Error(`\`cargo metadata\` exited ${result.status}: ${(result.stderr ?? '').trim()}`);
  }
  let parsed;
  try {
    parsed = JSON.parse(result.stdout);
  } catch (cause) {
    throw new Error(`\`cargo metadata\` produced unparsable JSON: ${cause.message}`);
  }
  if (!Array.isArray(parsed.packages) || !parsed.resolve?.nodes) {
    throw new Error('`cargo metadata` returned no `packages`/`resolve` — refusing to guard an unknown graph');
  }
  log(`[cargo] ${parsed.packages.length} package(s), ${parsed.resolve.nodes.length} resolve node(s)`);
  return parsed;
}

/**
 * Index the metadata into what the rules need: member → name + layer, and
 * package id → package.
 *
 * Member identity comes from `workspace_members` joined on `packages[].id`.
 * `resolve.root` is **null** in a virtual workspace (measured at 6175f1c1f), so
 * it is never consulted.
 *
 * @param {object} metadata `cargo metadata --format-version 1` output
 */
export function buildWorkspaceIndex(metadata) {
  const byId = new Map(metadata.packages.map((p) => [p.id, p]));
  // cargo ≥1.77 keys resolve nodes by `id`; older shapes used `pkg`.
  const nodeById = new Map();
  for (const node of metadata.resolve.nodes) {
    const key = node.id ?? node.pkg;
    if (key) nodeById.set(key, node);
  }

  const members = new Map();
  const unclassified = [];
  for (const id of metadata.workspace_members ?? []) {
    const pkg = byId.get(id);
    if (!pkg) {
      unclassified.push({ id, reason: `workspace member id has no entry in packages[]: ${id}` });
      continue;
    }
    const relDir = manifestDir(pkg.manifest_path, metadata.workspace_root);
    const layer = classifyMemberDir(relDir);
    if (layer.unclassified) {
      unclassified.push({ id, name: pkg.name, relDir, reason: layer.unclassified });
      continue;
    }
    members.set(id, {
      id,
      name: pkg.name,
      relDir,
      manifestPath: pkg.manifest_path,
      layer: layer.id,
      kind: layer.kind,
    });
  }
  return { byId, nodeById, members, unclassified };
}

/**
 * Normal + build dependency closure of `id`, as package ids.
 *
 * `dev` edges are excluded on purpose: a `#[cfg(test)]` helper may reach for
 * anything without making the shipped crate depend on it, and F-02 / F-06 say
 * "normal + build". An edge is followed only when the `dep_kinds` entry that
 * produced it is `null` (normal) or `"build"` — a package reachable through two
 * kinds (normal and dev) is still reachable through the normal one.
 *
 * @param {{ nodeById: Map<string, any> }} index
 * @param {string} id
 * @returns {Set<string>}
 */
export function normalBuildClosure(index, id) {
  const seen = new Set();
  const stack = [id];
  while (stack.length > 0) {
    const current = stack.pop();
    if (seen.has(current)) continue;
    seen.add(current);
    const node = index.nodeById.get(current);
    if (!node) continue;
    for (const dep of node.deps ?? []) {
      const kinds = dep.dep_kinds ?? [];
      const shippable = kinds.length === 0 || kinds.some((k) => k.kind === null || k.kind === 'build');
      if (shippable) stack.push(dep.pkg);
    }
  }
  return seen;
}

/**
 * Split a dotted TOML key on `.`, ignoring dots inside quotes.
 *
 * A Cargo key can carry a quoted `cfg()` containing a string literal with a
 * dot in it — `target.'cfg(target_os = "macos")'.dependencies` is the form
 * `src-tauri/Cargo.toml:90` actually uses — so a plain `split('.')` puts
 * fragments of the predicate in the wrong place.
 */
function splitDottedKey(key) {
  const out = [];
  let cur = '';
  let quote = null;
  for (const ch of key) {
    if (quote !== null) {
      cur += ch;
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      cur += ch;
      continue;
    }
    if (ch === '.') {
      out.push(cur);
      cur = '';
      continue;
    }
    cur += ch;
  }
  out.push(cur);
  return out;
}

const unquoteKey = (segment) => segment.trim().replace(/^["']|["']$/g, '');
const DEP_KINDS = new Set(['dependencies', 'build-dependencies', 'dev-dependencies']);

/**
 * Classify a TOML table header as a dependency table, or `null` if it is not one.
 *
 * Handles the bare form (`[dependencies]`) and the target-scoped form
 * (`[target.'cfg(windows)'.build-dependencies]`). The kind is always the LAST
 * segment, which is why a target-scoped table is recognised regardless of how
 * many segments the `cfg()` predicate has.
 *
 * @param {string} key inside the brackets, e.g. `target.'cfg(windows)'.build-dependencies`
 * @returns {{kind: string, target: string|null}|null}
 */
export function dependencyTableOf(key) {
  const segments = splitDottedKey(key).map(unquoteKey);
  const kindIndex = segments.findIndex((s) => DEP_KINDS.has(s));
  if (kindIndex === -1) return null;
  if (kindIndex !== segments.length - 1) return null;
  if (kindIndex === 0) return { kind: segments[kindIndex], target: null };
  if (segments[0] !== 'target') return null;
  return { kind: segments[kindIndex], target: segments.slice(1, kindIndex).join('.') };
}

/**
 * Every dependency a `Cargo.toml` *declares*, read from the manifest text.
 *
 * This is the complement of `normalBuildClosure`, not a replacement for it.
 * They answer different questions and neither can answer the other's:
 *
 *   * the resolve graph is **feature-resolved**, so a dependency behind a
 *     feature nobody enables is absent from it entirely. Measured at 2530161ac:
 *     `packages/drivers/redis/Cargo.toml:27` declares
 *     `tauri = { version = "2", optional = true }`, yet `tauri` does not appear
 *     in redis's `resolve.deps[]` at all, while the sibling build-dependency
 *     `tauri-plugin` on line 31 does. A closure walk cannot see line 27.
 *   * the manifest is **direct-only and resolution-blind**, so a crate pulled in
 *     transitively — `tauri-utils` for redis, nine crates for the host — does
 *     not appear here. A manifest scan cannot see those.
 *
 * So: enforce against the closure (the linker only ever sees what resolved), and
 * use this to *report* the edges the graph structurally cannot represent. A
 * guard that only reads `resolve` is not wrong about what ships; it is silent
 * about what the manifest says, and silence is what let a Tauri edge sit in a
 * driver for this long.
 *
 * Handles all four declaration forms cargo accepts: `name = "1.0"`,
 * `name = { … }`, the `[dependencies.name]` sub-table, and a renamed
 * dependency (`alias = { package = "real-crate" }`, where the *key* is the alias
 * and `package` is the crate that would actually be linked).
 *
 * @param {string} text contents of one `Cargo.toml`
 * @returns {Array<{name: string, kind: string, target: string|null, form: string, renamed: boolean}>}
 */
export function declaredDependencies(text) {
  const found = [];
  let open = null;
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (line === '' || line.startsWith('#')) continue;
    const header = line.match(/^\[([^[\]]+)\]$/);
    if (header) {
      // Split twice on purpose: `raw` keeps the original quoting so a container
      // header can be rebuilt byte-for-byte, `segments` is unquoted for lookup.
      const raw = splitDottedKey(header[1]);
      const segments = raw.map(unquoteKey);
      const kindIndex = segments.findIndex((s) => DEP_KINDS.has(s));
      if (kindIndex === -1) {
        open = null;
        continue;
      }
      // `[dependencies.foo]` names ONE dependency rather than opening a section:
      // it carries a segment after the kind, and that segment is the name. Keys
      // following it are that dependency's options, not more edges.
      if (kindIndex < segments.length - 1) {
        const container = dependencyTableOf(raw.slice(0, kindIndex + 1).join('.'));
        if (container !== null) {
          found.push({ name: segments[kindIndex + 1], ...container, form: 'table', renamed: false });
        }
        open = null;
        continue;
      }
      open = dependencyTableOf(header[1]);
      continue;
    }
    if (open === null) continue;
    const entry = line.match(/^([A-Za-z0-9_.-]+|"[^"]*"|'[^']*')\s*=\s*(.+)$/);
    if (entry === null) continue;
    const renamed = entry[2].match(/(?:^|[,{\s])package\s*=\s*["']([^"']+)["']/);
    found.push({
      name: renamed !== null ? renamed[1] : unquoteKey(entry[1]),
      ...open,
      form: 'inline',
      renamed: renamed !== null,
    });
  }
  return found;
}