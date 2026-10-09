#!/usr/bin/env node
/**
 * Driver type isolation guard — `docs/development/platform-development-plan.md:99`.
 *
 * The plan's D-layer exit gate says, verbatim:
 *
 *   退出门槛：D 层 CM-07～19、22～26、30、45、48；不同 driver 不共享实现库类型；新增能力缺失不会 no-op 成功。
 *
 * The middle clause is the one this file enforces: **不同 driver 不共享实现库类型** —
 * no driver crate may name a type that belongs to a different driver crate.
 *
 * ---------------------------------------------------------------------------
 * Why this is not already covered
 * ---------------------------------------------------------------------------
 *
 * Two guards exist and neither reaches the Rust half of the driver crates:
 *
 *   - `check-driver-import-boundaries.mjs` (588 lines, 4 rules) scans
 *     `SCAN_EXTENSIONS` from `scripts/lib/scanTargets.mjs:44`, which is
 *     `{.ts,.tsx,.js,.jsx,.mjs,.cjs}` — **no `.rs`**. It never opens a Rust
 *     file, so it cannot see a driver crate's `use` statements.
 *   - `check-platform-crate-boundaries.mjs` F-01 (`:96-110`) sets
 *     `allowedLayers: ['driver','driver-api']`. `LAYERS` in
 *     `scripts/lib/cargoWorkspace.mjs:66-76` has exactly ONE `driver` layer
 *     covering `packages/drivers/*`, so **every crate under that directory —
 *     every driver and the shared `http-support` implementation library alike —
 *     is the same layer, and F-01 explicitly permits driver → driver edges**.
 *     Counting the crates here is stated by population rather than by a fixed
 *     number on purpose: the number moves every time a driver lands, and a
 *     stale count in a header comment is indistinguishable from a stale gate.
 *     The driver/crates split that this file *does* measure is spelled out
 *     under "S1 ∩ S2" below.
 *
 * The consequence was a gap with no guard behind it: nothing stopped
 * `packages/drivers/postgres` from taking `use datazen_driver_redis::…`, or from
 * `#[path]`-including Redis's source outright.
 *
 * ---------------------------------------------------------------------------
 * What failure this prevents
 * ---------------------------------------------------------------------------
 *
 * Driver selection in this repo is compile-time (`scripts/resolve-drivers.mjs`
 * injects one `driver-<id>` Cargo feature per registered driver and rewrites
 * `src-tauri/Cargo.toml`'s `<<driver-features>>` block), and each driver is its
 * own crate with its own version. That design only holds if a driver's shipped
 * code is a function of that driver alone. A cross-driver type edge breaks
 * three things at once:
 *
 *   1. **Selection stops being a selection.** Building with `driver-postgres`
 *      and not `driver-redis` would drag Redis's implementation types into the
 *      binary through Postgres, so "which drivers are in this build" is no
 *      longer the feature list.
 *   2. **Two independently-versioned crates get a joint SemVer contract.**
 *      A rename in Redis's shared type breaks Postgres at a version boundary
 *      that Cargo cannot express, and the fix has to happen in two repos or two
 *      release trains.
 *   3. **The exit gate becomes unfalsifiable.** `:99`'s companion clause
 *      「新增能力缺失不会 no-op 成功」 is only meaningful if each driver owns its
 *      own behaviour; with a shared implementation library one driver can
 *      quietly satisfy (or mask) another's missing capability.
 *
 * ---------------------------------------------------------------------------
 * Why this guard is not a tautology
 * ---------------------------------------------------------------------------
 *
 * The obvious way to build this guard is one hand-written array holding the
 * driver ids, and a second array holding the paths to scan. Editing both to the
 * same wrong value keeps it green forever, and the edit that matters is the one
 * that *removes a driver from the list*. So the subject set here is assembled
 * from sources that are not editable in that way, and the sources are
 * cross-checked against each other rather than trusted individually:
 *
 *   - **S1 · `cargo metadata`** — workspace membership, each member's
 *     `manifest_path`, and the **feature-resolved** `resolve.nodes[].deps`.
 *     Cargo computes this; the guard only reads it.
 *   - **S2 · each crate's own Rust source** — whether the crate carries the
 *     driver contract marker `impl … DatabaseDriver for …`
 *     (`packages/driver-api/src/traits.rs:24`). This is what separates a
 *     *driver* from a shared support library, and it is read out of the files.
 *   - **S3 · `drivers-registry.json`** — the git-tracked build-time selection
 *     registry that `resolve-drivers.mjs` consumes, maintained by a completely
 *     different mechanism.
 *   - **S4 · the filesystem** — the real directory tree, which is what T-03
 *     resolves `#[path]` targets against. Cargo has no view of `#[path]`.
 *
 * S1 supplies the paths to scan; S2 supplies the "is this a driver" predicate.
 * They are different artifacts, so there is no single array to corrupt. S3 then
 * disagrees-by-construction with S1∩S2 in both directions, and S4 disagrees with
 * S1 whenever a member has no directory. Disagreement is an **error**, never a
 * silent pass.
 *
 * The `S1 ∩ S2` split is not a convenience. Measured on this tree,
 * `packages/drivers/http-support` is a Cargo member under `packages/drivers/*`,
 * has no `impl … DatabaseDriver for`, and is depended on by 10 drivers. It is
 * a *shared implementation library*, not a driver — the requirement is about
 * drivers not sharing types with each other, and a support library is exactly
 * the sanctioned way to share code. Treating every crate under the directory as
 * a driver would make 10 sanctioned edges read as violations and force someone
 * to add an exemption list, which is precisely the tautology this file refuses.
 * The classification is *measured*, and the registry cross-check is what stops
 * the measurement from quietly going stale.
 *
 * ---------------------------------------------------------------------------
 * The three rules
 * ---------------------------------------------------------------------------
 *
 *   - **T-01 · dependency edge.** No driver may reach another driver through
 *     its own normal/build/dev edges, nor through a non-driver intermediary.
 *     Dev edges are included and named: `:99` does not qualify the clause, a
 *     dev-dependency compiles the other driver's types into the test binary,
 *     and the fix is the same one the ten support-library edges already use.
 *     Transitive reach is reported separately from a direct edge, so one real
 *     defect is one finding rather than N.
 *   - **T-02 · source text.** No driver's Rust source may contain another
 *     driver's crate identifier. This catches names Cargo never sees: a type
 *     spelled in a `macro_rules!` body, in a `cfg_if!`, or behind a name that
 *     is not currently a dependency.
 *   - **T-03 · `#[path]` / `include!` resolution.** The repo's own idiom is to
 *     `#[path]`-include a shared template — `packages/drivers/postgres/tests/
 *     real_driver_contract.rs:16` pulls in
 *     `../../http-support/tests/support/real_driver_contract.rs`. That shape
 *     produces **no Cargo edge at all**, so it is invisible to T-01 by
 *     construction, and the path lives in a string literal so it is invisible
 *     to a blanked-source scan. T-03 reads the literal and resolves it against
 *     the real directory tree.
 *
 * ---------------------------------------------------------------------------
 * What a green run does not mean
 * ---------------------------------------------------------------------------
 *
 * The same honesty rule `check-platform-crate-boundaries.mjs:613-618` follows:
 * a subject that is **absent** and a subject that is **present but empty** are
 * different sentences, and neither is "pass".
 *
 *   - `absent` — a registered path driver with no directory on disk.
 *   - `empty`  — a driver's directory exists but holds no readable `.rs` file.
 *
 * And the structural case matters most: **fewer than two drivers is an error,
 * not a pass.** Every rule here compares a driver against *another* driver, so
 * with a single driver the guard is arithmetically incapable of firing. This
 * file refuses to exit 0 in that state rather than reporting a green that
 * carries no information. That is the direct answer to "change both arrays to
 * the same wrong value and it stays green": you cannot shrink the driver set
 * without the guard refusing to run.
 */

import { existsSync, readFileSync, readdirSync, statSync } from 'fs';
import { dirname, join, relative, resolve, sep } from 'path';

import { buildWorkspaceIndex, layerById, runCargoMetadata } from './lib/cargoWorkspace.mjs';

export const PREFIX = '[check-driver-type-isolation]';
export const REGISTRY_PATH = 'drivers-registry.json';

/**
 * The driver contract marker, and the only thing that makes a crate a driver.
 *
 * `packages/driver-api/src/traits.rs:24` declares `trait DatabaseDriver`, and
 * every driver implements it for its own type: `postgres/src/postgres.rs:59`,
 * `clickhouse/src/clickhouse.rs:188`, `duckdb/src/duckdb.rs:125`,
 * `sqlite/src/sqlite.rs:181`. A shared helper library implements nothing, so
 * the marker is exactly the distinction `:99` is about.
 *
 * Anchored on `impl … for` rather than on the bare trait name so that a crate
 * which merely *mentions* `DatabaseDriver` — a re-export, a doc comment, a
 * `where` clause naming the trait object — is not promoted to a driver. The
 * trade-off is deliberate and is covered by the registry cross-check: if a real
 * driver ever stops carrying the marker, the registry still expects it and the
 * run is an **error**, not a silent downgrade to "support library".
 */
export const DRIVER_CONTRACT_RE = /\bimpl\s+(?:\w+::)*DatabaseDriver\s+for\b/;

/** Source extensions the driver scan reads. Deliberately local. */
const RUST_EXTENSIONS = new Set(['.rs']);

/** Directories never walked. `target` is build output; the rest are not Rust. */
const SKIP_DIR_NAMES = new Set(['target', 'node_modules', '.git', 'ui', 'e2e']);

/** Dependency kinds T-01 follows. `null` is cargo's spelling for "normal". */
const FOLLOWED_KINDS = new Set([null, 'normal', 'build', 'dev']);

// ---------------------------------------------------------------------------
// Rust source scanner
// ---------------------------------------------------------------------------

const IDENT = /[A-Za-z0-9_]/;

/**
 * Tokenise Rust source into a comment/string-blanked copy plus the literals.
 *
 * This is deliberately **not** `scanCode` from `scripts/lib/scanSourceCode.mjs`.
 * That tokenizer is written for JS/TS, and applying it to Rust hides real
 * violations: it reads a lifetime as an unterminated string, so
 * `fn f<'a>(x: &datazen_driver_redis::T, y: &'a U)` has the span from the first
 * `'` to the second blanked out — including the `use` that T-02 exists to
 * find. It also has no raw-string handling, so `r#"…"` opens a phantom string.
 * A guard whose tokenizer eats the evidence is a guard that reports clean
 * because it could not read the file.
 *
 * Lifetime-vs-char-literal is settled by shape: a char literal matches
 * `'(?:\.|[^\\'])'`, and `'a>` / `'a ` / `'static` do not. An ambiguous quote is
 * therefore left as code, which can over-report but can never hide a type name.
 *
 * @param {string} source
 * @returns {{ code: string, strings: Array<{ value: string, line: number, raw: boolean }> }}
 */
export function scanRust(source) {
  const out = [...source];
  const n = source.length;
  const strings = [];
  let line = 1;
  let i = 0;

  const blank = (from, to) => {
    for (let k = from; k < to && k < n; k += 1) if (out[k] !== '\n') out[k] = ' ';
  };
  const countLines = (from, to) => {
    for (let k = from; k < to && k < n; k += 1) if (source[k] === '\n') line += 1;
  };
  const startsIdentifier = (c) => c !== undefined && /[A-Za-z_]/.test(c);

  while (i < n) {
    const ch = source[i];

    if (ch === '\n') {
      line += 1;
      i += 1;
      continue;
    }

    if (ch === '/' && source[i + 1] === '/') {
      const start = i;
      while (i < n && source[i] !== '\n') i += 1;
      blank(start, i);
      continue;
    }

    if (ch === '/' && source[i + 1] === '*') {
      const start = i;
      let depth = 0;
      while (i < n) {
        if (source[i] === '/' && source[i + 1] === '*') {
          depth += 1;
          i += 2;
          continue;
        }
        if (source[i] === '*' && source[i + 1] === '/') {
          depth -= 1;
          i += 2;
          if (depth === 0) break;
          continue;
        }
        if (source[i] === '\n') line += 1;
        i += 1;
      }
      blank(start, i);
      continue;
    }

    // Raw string: r"…", r#"…"#, br#"…"#. An identifier character in front means
    // this is a suffix like `my_var"`, not a raw string.
    if ((ch === 'r' || ch === 'b') && !startsIdentifier(source[i - 1])) {
      const raw = /^(b?r)(#*)"/.exec(source.slice(i, i + 16));
      if (raw) {
        const start = i;
        const startLine = line;
        const close = `"${raw[2]}`;
        const end = source.indexOf(close, i + raw[0].length);
        const stop = end === -1 ? n : end + close.length;
        i = stop;
        strings.push({ value: source.slice(start, stop), line: startLine, raw: true });
        countLines(start, stop);
        blank(start, stop);
        continue;
      }
    }

    if (ch === '"' || (ch === 'b' && source[i + 1] === '"')) {
      const start = i;
      const startLine = line;
      const open = ch === 'b' ? i + 1 : i;
      i = open + 1;
      while (i < n) {
        const c = source[i];
        if (c === '\\') {
          i += 2;
          continue;
        }
        if (c === '"') {
          i += 1;
          break;
        }
        i += 1;
      }
      strings.push({ value: source.slice(open + 1, Math.max(open + 1, i - 1)), line: startLine, raw: false });
      countLines(start, i);
      blank(start, i);
      continue;
    }

    if (ch === "'") {
      const char = /^'(\\.|[^\\'])'/.exec(source.slice(i, i + 8));
      if (char) {
        const start = i;
        i += char[0].length;
        blank(start, i);
        continue;
      }
      // Lifetime or loop label: ordinary code, and left visible on purpose.
      i += 1;
      continue;
    }

    i += 1;
  }

  return { code: out.join(''), strings };
}

/**
 * Offsets of `ident` in `code` that are whole identifiers.
 *
 * Both edges are checked. `findCodeNeedle` in `scanSourceCode.mjs:206` checks
 * only the left edge because its needles end in `(` — that shortcut is wrong
 * here, where the needles are crate names: with drivers `redis` and
 * `rediswire` on disk, `datazen_driver_redis` is a prefix of
 * `datazen_driver_rediswire`, and a left-only check reports the shorter
 * driver's identifier inside the longer driver's.
 *
 * @param {string} code comment/string-blanked source
 * @param {string} ident
 * @returns {number[]}
 */
export function findWholeIdentifier(code, ident) {
  const hits = [];
  for (let at = code.indexOf(ident); at !== -1; at = code.indexOf(ident, at + 1)) {
    const before = at === 0 ? '' : code[at - 1];
    const after = code[at + ident.length] ?? '';
    if (!IDENT.test(before) && !IDENT.test(after)) hits.push(at);
  }
  return hits;
}

/** 1-based line of `offset` in `code`. `blank` never shifts a newline. */
export function lineAtOffset(code, offset) {
  let line = 1;
  for (let k = 0; k < offset && k < code.length; k += 1) if (code[k] === '\n') line += 1;
  return line;
}

/**
 * `#[path = "…"]` and `include!("…")` targets, from the **unblanked** source.
 *
 * A separate pass on purpose: these targets *are* string literals, so a scan
 * that blanks literals — the correct thing for T-02 — has already thrown them
 * away. Reading them back from the raw text is the only way to see the one
 * include mechanism Cargo is structurally blind to.
 *
 * @param {string} source
 * @returns {Array<{ value: string, line: number, kind: 'path-attr'|'include-macro' }>}
 */
export function findFileIncludes(source) {
  const out = [];
  const re = /#\[\s*path\s*=\s*"([^"\n]+)"\s*\]|include!\s*\(\s*"([^"\n]+)"\s*\)/g;
  for (let m = re.exec(source); m !== null; m = re.exec(source)) {
    out.push({
      value: m[1] ?? m[2],
      line: lineAtOffset(source, m.index),
      kind: m[1] !== undefined ? 'path-attr' : 'include-macro',
    });
  }
  return out;
}

// ---------------------------------------------------------------------------
// Filesystem inputs
// ---------------------------------------------------------------------------

/** Every readable `.rs` file under `dir`, as workspace-relative POSIX paths. */
export function listRustFiles(root, dir) {
  const abs = resolve(root, dir);
  const found = [];
  const walk = (current) => {
    let entries;
    try {
      entries = readdirSync(current, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (entry.name.startsWith('.') && entry.name !== '.') continue;
      const child = join(current, entry.name);
      if (entry.isDirectory()) {
        if (SKIP_DIR_NAMES.has(entry.name)) continue;
        walk(child);
        continue;
      }
      if (!entry.isFile()) continue;
      const dot = entry.name.lastIndexOf('.');
      if (dot === -1 || !RUST_EXTENSIONS.has(entry.name.slice(dot))) continue;
      let size = 0;
      try {
        size = statSync(child).size;
      } catch {
        continue;
      }
      if (size > 4 * 1024 * 1024) continue;
      found.push(relative(root, child).split(sep).join('/'));
    }
  };
  walk(abs);
  return found.sort();
}

/** Cargo package name → the identifier Rust code spells it with. */
export function rustIdentOf(packageName) {
  return packageName.replace(/-/g, '_');
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/**
 * Registered **path** driver directories, as workspace-relative POSIX paths.
 *
 * Git drivers are deliberately not part of this. `kiwi`, `olap` and `superset`
 * have no `path` and no Cargo manifest in a clean tree — they are cloned into
 * `packages/drivers/<id>/` at build time and are not workspace members, so
 * there is nothing for cargo to report and nothing for this guard to compare.
 * They are named in the header as a known blind spot rather than silently
 * skipped, because a guard that quietly drops a third of its subject set is the
 * failure mode this file was written to avoid.
 *
 * @param {object} parsed `drivers-registry.json`
 * @returns {Array<{ id: string, path: string }>}
 */
export function readRegistry(parsed) {
  const out = [];
  for (const [id, entry] of Object.entries(parsed ?? {})) {
    if (entry === null || typeof entry !== 'object') continue;
    if (entry.source !== 'path') continue;
    if (typeof entry.path !== 'string' || entry.path.length === 0) continue;
    out.push({ id, path: entry.path.replace(/\/+$/, '') });
  }
  return out;
}

// ---------------------------------------------------------------------------
// The check
// ---------------------------------------------------------------------------

/** Findings are per (from, to, file) so one defect is one line of output. */
const MAX_HITS_PER_PAIR = 3;

/**
 * @param {{
 *   root: string,
 *   metadata?: object,
 *   registry?: object,
 *   env?: NodeJS.ProcessEnv,
 *   log?: (s: string) => void,
 *   error?: (s: string) => void,
 * }} options
 */
export function checkDriverTypeIsolation(options) {
  const { root, env, log = () => {} } = options;
  const violations = [];
  const errors = [];
  const vacuous = [];
  const advisories = [];
  const evaluated = [];
  const supportCrates = [];

  const metadata =
    options.metadata ??
    runCargoMetadata({ cwd: root, env, log, error: options.error ?? console.error });

  const index = buildWorkspaceIndex(metadata);
  for (const bad of index.unclassified) {
    errors.push(
      `unclassified workspace member ${bad.name ?? bad.id} (${bad.relDir ?? '?'}): ${bad.reason}`,
    );
  }

  // --- S1 + S2: which crates under packages/drivers/* are drivers? ---------
  const memberByRelDir = new Map();
  for (const member of index.members.values()) memberByRelDir.set(member.relDir, member);

  const driverMembers = [...index.members.values()].filter(
    (m) => m.layer === layerById('driver')?.id,
  );

  /** @type {Array<{ name: string, ident: string, relDir: string, files: Array<{ rel: string, code: string, source: string }> }>} */
  const drivers = [];

  for (const member of driverMembers) {
    if (!existsSync(resolve(root, member.relDir))) {
      vacuous.push({ rule: 'T-01..T-03', driver: member.name, relDir: member.relDir, reason: 'absent' });
      continue;
    }
    const files = listRustFiles(root, member.relDir);
    if (files.length === 0) {
      vacuous.push({ rule: 'T-01..T-03', driver: member.name, relDir: member.relDir, reason: 'empty' });
      continue;
    }
    const read = files.map((rel) => {
      const source = readFileSync(resolve(root, rel), 'utf8');
      return { rel, source, code: scanRust(source).code };
    });
    const carriesContract = read.some((f) => DRIVER_CONTRACT_RE.test(f.code));
    if (!carriesContract) {
      supportCrates.push({ name: member.name, relDir: member.relDir, files: read.length });
      continue;
    }
    drivers.push({
      name: member.name,
      ident: rustIdentOf(member.name),
      relDir: member.relDir,
      files: read,
    });
  }

  // --- S3: the registry disagrees with S1 ∩ S2, in both directions ---------
  const registryPathDrivers = readRegistry(options.registry ?? loadRegistry(root, errors));
  const registeredPaths = new Set(registryPathDrivers.map((r) => r.path));
  const driverPaths = new Set(drivers.map((d) => d.relDir));
  const supportPaths = new Set(supportCrates.map((s) => s.relDir));

  for (const registered of registryPathDrivers) {
    // A registered path driver that was scanned and classified is the normal
    // case; the three branches below are each a way for the registry and the
    // measured source to have parted ways.
    if (driverPaths.has(registered.path)) continue;
    const member = memberByRelDir.get(registered.path);
    if (member === undefined) {
      errors.push(
        `registered path driver \`${registered.path}\` is not a Cargo workspace member. ` +
          `cargo cannot see it, so T-01/T-02 cannot check it.`,
      );
      continue;
    }
    if (supportPaths.has(registered.path)) {
      errors.push(
        `registered path driver \`${registered.path}\` (${registered.id}) carries no ` +
          `\`impl … DatabaseDriver for\`. Either the driver lost its implementation or ` +
          `DRIVER_CONTRACT_RE no longer matches this tree — a green run here would be a ` +
          `guard that stopped reading the driver.`,
      );
      continue;
    }
    errors.push(
      `registered path driver \`${registered.path}\` (${registered.id}) has no Rust source to ` +
        `scan (reported as empty above), so no rule covers it.`,
    );
  }

  for (const driver of drivers) {
    if (registeredPaths.has(driver.relDir)) continue;
    errors.push(
      `\`${driver.name}\` is a driver by its own source but is not a registered path driver in ` +
        `${REGISTRY_PATH}. \`scripts/resolve-drivers.mjs\` injects no feature for it, so it can ` +
        `never be linked — and this guard would otherwise judge it while nothing can ship it.`,
    );
  }

  // --- Arming: fewer than two drivers means the rules cannot fire ----------
  if (drivers.length < 2) {
    errors.push(
      `only ${drivers.length} driver(s) resolved from \`cargo metadata\` + the contract marker. ` +
        `T-01..T-03 compare a driver against *another* driver, so below two they are ` +
        `arithmetically incapable of firing. Refusing to exit 0 on a run that carries no ` +
        `information.`,
    );
  }

  // --- T-01: dependency edges, direct and transitive -----------------------
  const memberByName = new Map([...index.members.values()].map((m) => [m.name, m]));
  const driverNameSet = new Set(drivers.map((d) => d.name));

  for (const driver of drivers) {
    const member = memberByName.get(driver.name);
    if (member === undefined) continue;
    const node = index.nodeById.get(member.id);
    if (node === undefined) continue;

    /** @type {Map<string, { kind: string, depth: number }>} */
    const reach = new Map();
    const queue = [];
    for (const dep of node.deps ?? []) {
      const pkg = index.byId.get(dep.pkg);
      if (pkg === undefined) continue;
      for (const dk of dep.dep_kinds ?? [{ kind: null }]) {
        if (!FOLLOWED_KINDS.has(dk.kind)) continue;
        if (reach.has(pkg.name)) continue;
        reach.set(pkg.name, { kind: dk.kind ?? 'normal', depth: 0 });
        queue.push(pkg.id);
      }
    }
    // Only *path* members get walked further: a third-party crate cannot route
    // back into a workspace driver, and BFS over 900+ external nodes per driver
    // is the one part of this that would cost real time.
    let depth = 1;
    while (queue.length > 0) {
      const next = [];
      for (const id of queue) {
        const pkg = index.byId.get(id);
        if (pkg === undefined) continue;
        const child = index.nodeById.get(id);
        if (child === undefined) continue;
        for (const dep of child.deps ?? []) {
          const depPkg = index.byId.get(dep.pkg);
          if (depPkg === undefined || reach.has(depPkg.name)) continue;
          for (const dk of dep.dep_kinds ?? [{ kind: null }]) {
            // Dependency libraries' dev-dependencies are not linked into the
            // consuming driver. Only the root driver's own dev edges apply.
            if (!FOLLOWED_KINDS.has(dk.kind) || dk.kind === 'dev') continue;
            reach.set(depPkg.name, { kind: dk.kind ?? 'normal', depth });
            if (driverNameSet.has(depPkg.name) === false && index.nodeById.has(dep.pkg)) {
              next.push(dep.pkg);
            }
            break;
          }
        }
      }
      queue.length = 0;
      queue.push(...next);
      depth += 1;
    }

    for (const [target, how] of reach) {
      if (target === driver.name) continue;
      if (!driverNameSet.has(target)) continue;
      const kind = how.depth === 0 ? `${how.kind} dependency` : `transitive reach (depth ${how.depth})`;
      violations.push(
        `T-01: \`${driver.name}\` (${driver.relDir}) reaches driver \`${target}\` by ${kind}. ` +
          `不同 driver 不共享实现库类型 — a driver must own its own implementation types. ` +
          `Move the shared code into \`datazen-driver-api\` or a support library instead.`,
      );
    }
  }

  // --- T-02: another driver's identifier in this driver's source ----------
  for (const driver of drivers) {
    for (const other of drivers) {
      if (other.name === driver.name) continue;
      const hits = [];
      for (const file of driver.files) {
        for (const at of findWholeIdentifier(file.code, other.ident)) {
          hits.push({ file: file.rel, line: lineAtOffset(file.code, at) });
        }
      }
      if (hits.length === 0) continue;
      const shown = hits.slice(0, MAX_HITS_PER_PAIR);
      const more = hits.length - shown.length;
      violations.push(
        `T-02: \`${driver.name}\` (${driver.relDir}) names \`${other.ident}\`, driver ` +
          `\`${other.name}\`, in its own source: ` +
          shown.map((h) => `${h.file}:${h.line}`).join(', ') +
          (more > 0 ? ` (+${more} more)` : '') +
          `. 不同 driver 不共享实现库类型.`,
      );
    }
  }

  // --- T-03: #[path] / include! reaching into another driver's tree -------
  for (const driver of drivers) {
    for (const file of driver.files) {
      for (const include of findFileIncludes(file.source)) {
        const targetAbs = resolve(root, dirname(file.rel), include.value);
        const targetRel = relative(root, targetAbs).split(sep).join('/');
        for (const other of drivers) {
          if (other.name === driver.name) continue;
          if (!targetRel.startsWith(`${other.relDir}/`)) continue;
          const exists = existsSync(targetAbs);
          violations.push(
            `T-03: \`${driver.name}\` (${driver.relDir}) ${include.kind} at ` +
              `${file.rel}:${include.line} resolves to \`${targetRel}\`, inside driver ` +
              `\`${other.name}\`${exists ? '' : ' (target does not exist)'}. ` +
              `A \`#[path]\` include compiles another driver's source with no Cargo edge at all, ` +
              `so no dependency guard can see it. 不同 driver 不共享实现库类型.`,
          );
        }
      }
    }
  }

  for (const driver of drivers) {
    evaluated.push({
      rule: 'T-01..T-03',
      driver: driver.name,
      files: driver.files.length,
      comparedWith: drivers.length - 1,
    });
  }

  if (supportCrates.length > 0) {
    advisories.push(
      `classified as shared support, not drivers (no \`impl … DatabaseDriver for\`): ` +
        supportCrates.map((s) => `${s.name} (${s.files} .rs)`).join(', '),
    );
  }

  const summary =
    `${drivers.length} driver(s) x ${Math.max(0, drivers.length - 1)} peer(s); ` +
    `${violations.length} violation(s), ${errors.length} error(s), ${vacuous.length} vacuous`;

  return { violations, errors, vacuous, advisories, evaluated, supportCrates, summary };
}

function loadRegistry(root, errors) {
  const file = resolve(root, REGISTRY_PATH);
  if (!existsSync(file)) {
    errors.push(`missing ${REGISTRY_PATH}; refusing to guess which crates are drivers.`);
    return {};
  }
  try {
    return JSON.parse(readFileSync(file, 'utf8'));
  } catch (cause) {
    errors.push(`unparsable ${REGISTRY_PATH}: ${cause.message}`);
    return {};
  }
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

/**
 * The single definition of "did the run fail", shared by the CLI and the tests.
 *
 * Both the vacuous and the advisory channels are exit 0 on purpose, and both
 * are printed loudly. Refusing to pass on a repo that has not started a track
 * would block every unrelated PR; refusing to *print* it would be how the
 * subject set quietly rots until nothing is checked.
 *
 * @param {{ violations: unknown[], errors: unknown[] }} result
 * @returns {0|1}
 */
export function exitCodeFor(result) {
  return result.violations.length === 0 && result.errors.length === 0 ? 0 : 1;
}

function parseArgs(argv) {
  const args = { root: process.cwd(), help: false };
  for (const raw of argv) {
    if (raw === '--help' || raw === '-h') {
      args.help = true;
      continue;
    }
    if (raw.startsWith('--root=')) {
      args.root = resolve(raw.slice('--root='.length));
      continue;
    }
    throw new Error(`unknown argument: ${raw}`);
  }
  return args;
}

export function runCli({ argv = process.argv.slice(2), env = process.env } = {}) {
  const out = (s) => process.stdout.write(`${s}\n`);
  const err = (s) => process.stderr.write(`${s}\n`);

  let args;
  try {
    args = parseArgs(argv);
  } catch (cause) {
    err(`${PREFIX} ${cause.message}`);
    return 2;
  }
  if (args.help) {
    out(
      `${PREFIX} usage: node scripts/check-driver-type-isolation.mjs [--root=<dir>]\n` +
        `${PREFIX} enforces platform-development-plan.md §6「P2：Driver 固定资源与可选能力契约」退出门槛 「不同 driver 不共享实现库类型」.\n` +
        `${PREFIX}   T-01  no driver depends on, or transitively reaches, another driver\n` +
        `${PREFIX}   T-02  no driver's Rust source names another driver's crate identifier\n` +
        `${PREFIX}   T-03  no driver #[path]/include! resolves into another driver's tree\n` +
        `${PREFIX}   needs a Cargo toolchain; wired into the CI \`rust\` job for that reason.`,
    );
    return 0;
  }

  let result;
  try {
    result = checkDriverTypeIsolation({ ...args, env, log: out, error: err });
  } catch (cause) {
    err(`${PREFIX} ${cause.message}`);
    return 2;
  }

  for (const v of result.vacuous) {
    // Two different sentences for two different situations, and the difference
    // is the whole point: "absent" means the armed subject is not on disk,
    // "empty" means it is on disk with nothing in it to read. Conflating them
    // is how a guard that stopped reading anything reports itself as passing.
    const what =
      v.reason === 'absent'
        ? 'rule armed, subject not on disk'
        : 'rule armed, directory present but no readable .rs file';
    out(`${PREFIX} VACUOUS ${v.rule} \`${v.relDir}\` (${v.driver}): ${what}`);
  }
  for (const a of result.advisories) out(`${PREFIX} ADVISORY ${a}`);
  for (const v of result.violations) err(`${PREFIX} VIOLATION ${v}`);
  for (const e of result.errors) err(`${PREFIX} ERROR ${e}`);

  if (result.violations.length === 0 && result.errors.length === 0) {
    out(`${PREFIX} ok ${result.summary}`);
    for (const e of result.evaluated) {
      out(`${PREFIX} ok ${e.rule} \`${e.driver}\`: ${e.files} .rs file(s), compared with ${e.comparedWith} peer(s)`);
    }
  }
  return exitCodeFor(result);
}

if (process.argv[1] && process.argv[1].endsWith('check-driver-type-isolation.mjs')) {
  process.exitCode = runCli();
}
