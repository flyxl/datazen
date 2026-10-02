/**
 * @vitest-environment node
 *
 * The driver protocol / crate compatibility gate.
 *
 * Two independent things are pinned here, and they fail for opposite reasons:
 *
 * 1. **The rule table is true of the tree it claims to govern.** Every anchor in
 *    `CONTRACT_RULES` must actually resolve to an item of the declared kind in
 *    the real source, every version target must be where the matrix says it is,
 *    and every rule must require *something*. A table that drifts from the code
 *    is worse than no table, because it produces a clean `ok` for a contract
 *    nobody is watching.
 * 2. **The checker actually fires.** Each class in the matrix is driven through
 *    a synthetic diff against a synthetic base/new pair, so "breaking without a
 *    protocol bump" is asserted to be reported, not merely plausible.
 *
 * The real `git diff` plumbing is exercised too, but only read-only, by running
 * the shipped CLI against this repository: a checker that throws while parsing
 * its own diff would still be green in every unit test above.
 */

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import {
  CLASS_REQUIRES,
  COMPAT_MATRIX,
  CONTRACT_RULES,
  GOVERNED_CRATE_MANIFEST,
  MIN_PROTOCOL_VERSION_SYMBOL,
  PROTOCOL_VERSION_SYMBOL,
  STATIC_INVARIANTS,
  classifyContractChange,
  compareCrateVersions,
  findItemSpan,
  isCosmeticLine,
  isRequiredTraitItemAddition,
  parseDiffByFile,
  readConstValue,
  readCrateVersion,
} from '../check-driver-protocol-compat.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

const read = (relPath: string): string => readFileSync(join(ROOT, relPath), 'utf8');

/** A stand-in for a governed `trait`, small enough to reason about exactly. */
const BASE_TRAIT = `pub trait Demo: Send {
    /// Existing doc.
    fn existing(&self) -> u32;

    fn with_default(&self) -> u32 {
        0
    }
}
`;

/**
 * Build a `-U0` unified diff from an old/new pair for a single file.
 *
 * A real LCS rather than a greedy walk: the classification under test depends
 * on which line a change lands on, and a greedy walk produces a *valid* diff
 * that attributes the change to a different line than git would — which would
 * make these cases assert the parser's behaviour instead of the real one.
 */
const diffOf = (before: string, after: string): string => {
  const a = before.split('\n');
  const b = after.split('\n');
  const lcs: number[][] = Array.from({ length: a.length + 1 }, () =>
    new Array<number>(b.length + 1).fill(0),
  );
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }
  const body: string[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      body.push(' ' + a[i]);
      i += 1;
      j += 1;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      body.push('-' + a[i]);
      i += 1;
    } else {
      body.push('+' + b[j]);
      j += 1;
    }
  }
  while (i < a.length) body.push('-' + a[i++]);
  while (j < b.length) body.push('+' + b[j++]);

  // `-U0` emits no context lines, so each *contiguous* run of changed lines is
  // one hunk — removals first, then additions, exactly as git writes them. A
  // removal and the addition that replaces it belong to the same hunk; splitting
  // them would move one of the two to the wrong new-side line number and make
  // these cases assert against a diff git never produces.
  const header = `diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n`;
  const hunks: string[] = [];
  let oldNo = 1;
  let newNo = 1;
  let pending: string[] = [];
  const flush = () => {
    if (pending.length === 0) return;
    const addedCount = pending.filter((l) => l.startsWith('+')).length;
    const oldCount = pending.length - addedCount;
    hunks.push(`@@ -${oldNo},${oldCount} +${newNo},${addedCount} @@`, ...pending);
    pending = [];
  };
  for (const line of body) {
    if (line.startsWith('+') || line.startsWith('-')) {
      pending.push(line);
    } else {
      flush();
      oldNo += 1;
      newNo += 1;
    }
  }
  flush();
  return header + `${hunks.join('\n')}\n`;
};

/**
 * Classify a synthetic base/new pair for a synthetic rule, going through the
 * real diff parser rather than hand-built records — the parser is where a
 * plausible-looking classifier gets its input wrong.
 */
const classifyViaDiff = (rule: (typeof CONTRACT_RULES)[number], before: string, after: string) => {
  const byFile = parseDiffByFile(diffOf(before, after));
  const records = byFile.get('x.rs');
  expect(records, 'the synthetic diff must produce records').toBeDefined();
  return classifyContractChange(before, after, rule, records!);
};

// `as const` keeps `kind` a literal type. Without it a mutable object literal
// widens `kind` to `string`, which no longer matches the rule table's union.
const TRAIT_RULE = { id: 'demo', file: 'x.rs', anchor: 'pub trait Demo', kind: 'trait' } as const;
const STRUCT_RULE = {
  id: 'demo',
  file: 'x.rs',
  anchor: 'pub struct Demo',
  kind: 'struct',
} as const;
const ENUM_RULE = { id: 'demo', file: 'x.rs', anchor: 'pub enum Demo', kind: 'enum' } as const;

describe('rule table — every governed contract exists in the tree', () => {
  it('resolves every anchor to an item of the declared kind', () => {
    const sources = new Map<string, string>();
    for (const rule of CONTRACT_RULES) {
      if (!sources.has(rule.file)) {
        sources.set(rule.file, read(rule.file));
      }
      const span = findItemSpan(sources.get(rule.file)!, rule.anchor);
      expect(span, `${rule.file}: anchor "${rule.anchor}" must resolve`).not.toBeNull();
    }
  });

  it('anchors a trait rule to a trait and a struct rule to a struct', () => {
    for (const rule of CONTRACT_RULES) {
      const lines = read(rule.file).split('\n');
      const span = findItemSpan(read(rule.file), rule.anchor);
      expect(span).not.toBeNull();
      const declaration = lines[span!.start];
      if (rule.kind === 'trait') {
        expect(declaration, `${rule.id}`).toContain('pub trait');
      } else if (rule.kind === 'struct') {
        expect(declaration, `${rule.id}`).toContain('pub struct');
      } else if (rule.kind === 'enum') {
        expect(declaration, `${rule.id}`).toContain('pub enum');
      }
    }
  });

  it('uses each anchor at most once per file, so spans cannot alias', () => {
    const seen = new Set<string>();
    for (const rule of CONTRACT_RULES) {
      const key = `${rule.file}::${rule.anchor}`;
      expect(seen.has(key), `duplicate governed anchor ${key}`).toBe(false);
      seen.add(key);
    }
  });

  it('covers all six optional-capability domains of the plan', () => {
    // platform-development-plan.md:93 lists namespace / session / transaction /
    // snapshot / data / backup. The `data` and `backup` domains arrived with
    // CapabilitySet gaining `data` and `backup`; if a future refactor drops
    // them, this anchor stops resolving and the table above fails first.
    const capabilitySet = read('packages/driver-api/src/capabilities.rs');
    for (const field of ['stateful_session', 'snapshots', 'transactions', 'data', 'backup']) {
      expect(capabilitySet, `CapabilitySet must still declare ${field}`).toContain(`pub ${field}:`);
    }
  });

  it('states why each contract is governed, so the table is not a bare list', () => {
    for (const rule of CONTRACT_RULES) {
      // `why` / `implementedBy` are optional on the typedef because the
      // classifier never reads them. They are required here: a rule that cannot
      // explain itself is exactly the table that rots the next time somebody
      // edits a governed contract.
      expect(rule.why ?? '', rule.id).toMatch(/\S/);
      expect(rule.implementedBy ?? '', rule.id).toMatch(/\S/);
    }
  });
});

describe('rule table — the version targets exist where the matrix says', () => {
  it('reads both protocol constants out of lib.rs', () => {
    const lib = read('packages/driver-api/src/lib.rs');
    expect(readConstValue(lib, PROTOCOL_VERSION_SYMBOL)).toBeTypeOf('number');
    expect(readConstValue(lib, MIN_PROTOCOL_VERSION_SYMBOL)).toBeTypeOf('number');
    // The constants' *values* are the host's business, not this track's; the
    // gate only asserts they are readable literals, never that they equal 4/1.
    expect(readConstValue(lib, MIN_PROTOCOL_VERSION_SYMBOL)).toBeLessThanOrEqual(
      readConstValue(lib, PROTOCOL_VERSION_SYMBOL)!,
    );
  });

  it('reads the crate SemVer out of the governed manifest', () => {
    expect(readCrateVersion(read(GOVERNED_CRATE_MANIFEST))).toMatch(/^\d+\.\d+\.\d+$/);
  });

  it('maps every change class onto a real version number', () => {
    // The targets are spelled as short keys in `requires` and documented in
    // prose in the matrix; a typo between the two is what would silently turn
    // a mandatory bump into a no-op.
    const documented = [
      COMPAT_MATRIX.protocol,
      COMPAT_MATRIX.minProtocol,
      COMPAT_MATRIX.crateVersion,
    ];
    const symbolOf = (target: string): string | undefined =>
      ({
        protocol: PROTOCOL_VERSION_SYMBOL,
        minProtocol: MIN_PROTOCOL_VERSION_SYMBOL,
        crateVersion: 'version',
      })[target];
    for (const [cls, requires] of Object.entries(CLASS_REQUIRES)) {
      for (const target of requires) {
        const symbol = symbolOf(target);
        expect(symbol, `${cls} requires unknown target "${target}"`).toBeDefined();
        const documentedHere = documented.find((d) => d.startsWith(symbol!));
        expect(
          documentedHere,
          `${cls} -> ${target} (${symbol}) must appear in COMPAT_MATRIX`,
        ).toBeDefined();
      }
    }
  });

  it('escalates: breaking obliges the protocol, additive only the crate', () => {
    expect(CLASS_REQUIRES.breaking).toContain('protocol');
    expect(CLASS_REQUIRES.additive).toContain('crateVersion');
    expect(CLASS_REQUIRES.additive).not.toContain('protocol');
    // Only a measured lowering is forbidden; nothing obliges MIN to move
    // upward, because keeping the degraded window open is the safe default.
    expect(CLASS_REQUIRES.breaking).not.toContain('minProtocol');
  });

  it('documents every static invariant with a reason', () => {
    expect(STATIC_INVARIANTS.length).toBeGreaterThan(0);
    for (const inv of STATIC_INVARIANTS) {
      expect(inv.id.length, inv.id).toBeGreaterThan(0);
      expect(inv.because.length, inv.id).toBeGreaterThan(20);
    }
    const ids = STATIC_INVARIANTS.map((i) => i.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe('classification — a breaking change is breaking', () => {
  it('reports an altered signature as breaking', () => {
    const after = BASE_TRAIT.replace(
      'fn existing(&self) -> u32;',
      'fn existing(&self) -> DriverType;',
    );
    const finding = classifyViaDiff(TRAIT_RULE, BASE_TRAIT, after);
    expect(finding.cls).toBe('breaking');
    expect(finding.reason).toContain('removed');
  });

  it('reports a deleted method as breaking', () => {
    const after = BASE_TRAIT.replace('    fn existing(&self) -> u32;\n', '');
    expect(classifyViaDiff(TRAIT_RULE, BASE_TRAIT, after).cls).toBe('breaking');
  });

  it('reports a required method with no default body as breaking', () => {
    const after = BASE_TRAIT.replace(
      '    fn with_default(&self) -> u32 {',
      '    fn added_required(&self) -> u32;\n    fn with_default(&self) -> u32 {',
    );
    const finding = classifyViaDiff(TRAIT_RULE, BASE_TRAIT, after);
    expect(finding.cls).toBe('breaking');
    expect(finding.reason).toContain('added_required');
  });

  it('reports an item that disappeared entirely as breaking', () => {
    const finding = classifyContractChange(BASE_TRAIT, '', TRAIT_RULE, new Map());
    expect(finding.cls).toBe('breaking');
  });
});

describe('classification — an additive change is additive, not breaking', () => {
  it('treats a defaulted trait method as additive', () => {
    const after = BASE_TRAIT.replace(
      '    fn with_default(&self) -> u32 {',
      '    fn added_defaulted(&self) -> u32 {\n        0\n    }\n\n    fn with_default(&self) -> u32 {',
    );
    const finding = classifyViaDiff(TRAIT_RULE, BASE_TRAIT, after);
    expect(finding.cls).toBe('additive');
    expect(finding.reason).toContain('added_defaulted');
  });

  it('treats a new struct field as additive', () => {
    const before = 'pub struct Demo {\n    pub a: u32,\n}\n';
    const after =
      'pub struct Demo {\n    pub a: u32,\n    /// A new domain.\n    pub b: String,\n}\n';
    const finding = classifyViaDiff(STRUCT_RULE, before, after);
    expect(finding.cls).toBe('additive');
    // The doc comment above the field is additive-adjacent noise, not a
    // contract change in its own right; it must not be counted.
    expect(finding.reason).toMatch(/1 line\(s\) added .*1 cosmetic ignored/);
    expect(finding.reason).toContain('pub b: String,');
  });

  it('does not mistake a required method in a struct for one in a trait', () => {
    // `pub fn` inside a struct is a plain field-less function, not a trait
    // item; the rule kind, not the syntax alone, decides.
    const before = 'pub struct Demo {\n    pub a: u32,\n}\n';
    const after = 'pub struct Demo {\n    pub a: u32,\n    pub fn helper(&self);\n}\n';
    expect(classifyViaDiff(STRUCT_RULE, before, after).cls).toBe('additive');
  });
});

describe('classification — a source break is neither additive nor breaking', () => {
  // A doc line above the struct, so the attribute lands where it really lands
  // in `capabilities.rs`: above the span's first line but inside the old span,
  // because inserting it shifts every line below down by one.
  const BASE_STRUCT = '/// Demo docs.\npub struct Demo {\n    pub a: u32,\n}\n';

  it('judges #[non_exhaustive] a source break, not an additive change', () => {
    const after = '/// Demo docs.\n#[non_exhaustive]\npub struct Demo {\n    pub a: u32,\n}\n';
    const finding = classifyViaDiff(STRUCT_RULE, BASE_STRUCT, after);
    // Not `additive`: that class exists to mean "existing implementors keep
    // compiling and declare nothing new", and this is the one attribute that
    // stops them compiling. Reporting it as additive is what let the change
    // through in the first place.
    expect(finding.cls).toBe('source-breaking');
    expect(finding.cls).not.toBe('additive');
    // Not `breaking` either: `breaking` obliges PROTOCOL_VERSION on the ground
    // that no [MIN, PROTOCOL] window expresses the change. Nothing on the wire
    // changed, so that bump would advertise a break no host can observe.
    expect(CLASS_REQUIRES['source-breaking']).toContain('crateVersion');
    expect(CLASS_REQUIRES['source-breaking']).not.toContain('protocol');
  });

  it('carries the migration recipe, and the recipe must not lie about ..base', () => {
    const after = '/// Demo docs.\n#[non_exhaustive]\npub struct Demo {\n    pub a: u32,\n}\n';
    const note = classifyViaDiff(STRUCT_RULE, BASE_STRUCT, after).sourceBreak?.migration ?? '';
    // The obvious one-line patch does not compile: `#[non_exhaustive]` rejects
    // the functional-update form too (rustc E0639), so `..Default::default()`
    // is not a migration. A note that offered it would send every out-of-tree
    // author into a second failure.
    expect(note).toContain('E0639');
    expect(note).toMatch(/default\(\).*explicit\s+assignment per field/s);
    expect(note).not.toMatch(/add \.\.Default::default\(\) to keep/i);
  });

  it('does not mistake a serde attribute for a source break', () => {
    const after =
      '/// Demo docs.\n#[serde(rename_all = "camelCase")]\npub struct Demo {\n    pub a: u32,\n}\n';
    const finding = classifyViaDiff(STRUCT_RULE, BASE_STRUCT, after);
    // A serde attribute changes what goes on the wire, not what compiles. It
    // is deliberately absent from the source-break table, so it keeps exactly
    // the verdict it had before this class existed.
    expect(finding.cls).not.toBe('source-breaking');
    expect(finding.cls).toBe('additive');
    expect(finding.sourceBreak).toBeUndefined();
  });

  it('still judges a removed field breaking, not source-breaking', () => {
    const before = '/// Demo docs.\npub struct Demo {\n    pub a: u32,\n    pub b: u32,\n}\n';
    const after = '/// Demo docs.\npub struct Demo {\n    pub a: u32,\n}\n';
    const finding = classifyViaDiff(STRUCT_RULE, before, after);
    // The pre-existing verdict must not regress: a removal is a wire break
    // whatever else it is, and the new branch sits below the removal ones.
    expect(finding.cls).toBe('breaking');
    expect(finding.sourceBreak).toBeUndefined();
  });

  it('does not fire when a real code line merely mentions the attribute', () => {
    const after =
      '/// Demo docs.\npub struct Demo {\n    pub a: u32,\n' +
      '    pub b: u32, // TODO: drop once #[non_exhaustive] lands\n}\n';
    // A genuine *addition* — rewriting `pub a` in place would be a removal
    // instead, and would never reach this table. Whole-line patterns, not
    // substring ones: a comment-only line is already dropped upstream by
    // `isCosmeticLine`, but this trailing comment rides in on a line that
    // really does declare a field, so a substring match would gate on the
    // TODO and report an ordinary field addition as a source break.
    expect(classifyViaDiff(STRUCT_RULE, BASE_STRUCT, after).cls).toBe('additive');
  });

  it('leaves an enum alone, because there it restricts matching, not building', () => {
    const base = 'pub enum Demo {\n    One,\n}\n';
    const after = '#[non_exhaustive]\npub enum Demo {\n    One,\n}\n';
    // On an enum the remedy is a wildcard arm, not `default()` plus
    // assignments, so folding it into the struct table would misdescribe it.
    expect(classifyViaDiff(ENUM_RULE, base, after).cls).not.toBe('source-breaking');
  });
});

describe('classification — an enum is judged by its members, not its lines', () => {
  // A governed enum's span says only that the contract was touched. A new
  // variant diffs as one added line — byte-for-byte the shape of a doc comment
  // — which is why the variant list used to be invisible to this gate, and why
  // `NamespaceSwitch::PerRequest` could be added (commit c749e6dbb) while the
  // gate reported a clean `ok`. The rules below are what it now reads instead.
  const BASE_ENUM = `pub enum Demo {
    One,
    Two,
}
`;

  it('treats an added variant of an exhaustive enum as breaking', () => {
    const after = BASE_ENUM.replace('    Two,\n', '    Two,\n    Three,\n');
    const finding = classifyViaDiff(ENUM_RULE, BASE_ENUM, after);
    // Not `additive`. `additive` means "existing implementors keep compiling and
    // declare nothing new", and an out-of-tree `match` over `Demo` has no arm
    // for `Three`: rustc E0004. That is a recompile requirement, which no
    // [MIN_PROTOCOL_VERSION, PROTOCOL_VERSION] window can express — so the class
    // has to be `breaking`, which is exactly what obliges PROTOCOL_VERSION.
    expect(finding.cls).toBe('breaking');
    expect(CLASS_REQUIRES[finding.cls]).toContain('protocol');
    expect(finding.reason).toContain('Three');
    expect(finding.reason).toContain('E0004');
  });

  it('treats a removed variant as breaking and names it', () => {
    const after = BASE_ENUM.replace('    Two,\n', '');
    const finding = classifyViaDiff(ENUM_RULE, BASE_ENUM, after);
    // The verdict predates the member comparison; what is new is that the
    // reason names the variant, because that is the identifier an out-of-tree
    // author has to go and search for.
    expect(finding.cls).toBe('breaking');
    expect(finding.reason).toContain('Two');
    expect(finding.sourceBreak).toBeUndefined();
  });

  it('treats a renamed variant as breaking, not as an addition', () => {
    const after = BASE_ENUM.replace('    Two,', '    Renamed,');
    // This is the shape that made the blind spot worth closing: a rename diffs
    // as one line out and one line in, so a checker reading only line counts
    // sees a removal and an addition and has to pick. Removal wins, because it
    // is the one that cannot be un-done by the matching addition.
    const finding = classifyViaDiff(ENUM_RULE, BASE_ENUM, after);
    expect(finding.cls).toBe('breaking');
    expect(finding.reason).toContain('Two');
  });

  it('treats an added variant as additive when the base enum is #[non_exhaustive]', () => {
    const before = '#[non_exhaustive]\npub enum Demo {\n    One,\n    Two,\n}\n';
    const after = '#[non_exhaustive]\npub enum Demo {\n    One,\n    Two,\n    Three,\n}\n';
    const finding = classifyViaDiff(ENUM_RULE, before, after);
    // The genuine exception, and the only one: `#[non_exhaustive]` forces every
    // downstream `match` to carry a wildcard arm, and that arm absorbs `Three`.
    // So the addition really is additive here — which is what makes it worth
    // marking the enum rather than leaving it off.
    expect(finding.cls).toBe('additive');
    expect(finding.reason).toContain('Three');
  });

  it('decides non_exhaustive at the base ref, not the head', () => {
    const after = '#[non_exhaustive]\npub enum Demo {\n    One,\n    Two,\n    Three,\n}\n';
    const finding = classifyViaDiff(ENUM_RULE, BASE_ENUM, after);
    // The question is what a driver written against the *previous* crate was
    // already forced to do, and at BASE_ENUM it was forced to do nothing: an
    // exhaustive `match` compiled then and does not compile now. Adding the
    // attribute in the same change that adds the variant does not retroactively
    // grant the wildcard arm — and if it did, the check would be satisfiable by
    // shipping the break and the exemption together.
    expect(finding.cls).toBe('breaking');
    expect(finding.reason).toContain('Three');
  });

  it('leaves a comment-only edit inside an enum alone', () => {
    const after = BASE_ENUM.replace('    One,\n', '    /// A new doc.\n    One,\n');
    // The member comparison must not turn every doc line into a contract
    // change; `One` and `Two` are both still there.
    expect(classifyViaDiff(ENUM_RULE, BASE_ENUM, after).cls).toBe('cosmetic');
  });

  it('does not reclassify a struct field addition, which stays additive', () => {
    // The struct member comparison exists to name the field, never to change the
    // verdict: `#[derive(Default)]` still fills a new field, which is the
    // invariant that makes a DTO growth a crate bump rather than a protocol one.
    const before = 'pub struct Demo {\n    pub a: u32,\n}\n';
    const after = 'pub struct Demo {\n    pub a: u32,\n    pub b: u32,\n}\n';
    const finding = classifyViaDiff(STRUCT_RULE, before, after);
    expect(finding.cls).toBe('additive');
    expect(finding.reason).toContain('pub b: u32,');
  });

  it('has no real #[non_exhaustive] enum to exercise, and says so', () => {
    // The exemption above is provable only against a synthetic fixture, because
    // nothing in the governed crate marks an enum. Asserting that here keeps the
    // gap visible: the day someone marks one, this test fails and the reason to
    // decide its rule by hand rather than by fixture.
    const enumFiles = new Set(
      CONTRACT_RULES.filter((rule) => rule.kind === 'enum').map((rule) => rule.file),
    );
    expect(enumFiles.size).toBeGreaterThan(0);
    for (const file of enumFiles) {
      expect(read(file), `${file} must stay free of #[non_exhaustive]`).not.toContain(
        'non_exhaustive',
      );
    }
  });
});

describe('classification — cosmetics and scope', () => {
  it('ignores a doc-comment rewrite inside a governed item', () => {
    const after = BASE_TRAIT.replace('    /// Existing doc.', '    /// Rewritten doc.');
    expect(classifyViaDiff(TRAIT_RULE, BASE_TRAIT, after).cls).toBe('cosmetic');
  });

  it('ignores an addition made above the governed item', () => {
    // This is the whole reason the checker resolves spans instead of anchoring
    // to line numbers: a doc comment above a trait is the single most common
    // edit in this codebase and it is not a contract change.
    const before = '/// Crate level docs.\n' + BASE_TRAIT;
    const after = '/// Crate level docs, expanded.\n/// Second line.\n' + BASE_TRAIT;
    expect(classifyViaDiff(TRAIT_RULE, before, after).cls).toBe('cosmetic');
  });

  it('does not attribute an edit to a different item in the same file', () => {
    const before = `${BASE_TRAIT}\npub struct Neighbour {\n    pub a: u32,\n}\n`;
    const after = `${BASE_TRAIT}\npub struct Neighbour {\n    pub a: u64,\n}\n`;
    expect(classifyViaDiff(TRAIT_RULE, before, after).cls).toBe('cosmetic');
  });

  it('recognises the comment forms it claims to ignore', () => {
    expect(isCosmeticLine('    // note')).toBe(true);
    expect(isCosmeticLine('')).toBe(true);
    expect(isCosmeticLine('   ')).toBe(true);
    expect(isCosmeticLine('    /// doc')).toBe(true);
    expect(isCosmeticLine(' * block comment body')).toBe(true);
    expect(isCosmeticLine('    fn real_code(&self);')).toBe(false);
    expect(isCosmeticLine('    pub field: u32,')).toBe(false);
  });

  it('recognises required trait items by their absence of a default body', () => {
    expect(isRequiredTraitItemAddition('    fn foo(&self);')).toBe(true);
    expect(isRequiredTraitItemAddition('    const N: u32;')).toBe(true);
    expect(isRequiredTraitItemAddition('    type Output;')).toBe(true);
    expect(isRequiredTraitItemAddition('    fn foo(&self) -> u32 {')).toBe(false);
    // A statement, not an item declaration.
    expect(isRequiredTraitItemAddition('    let x = 1;')).toBe(false);
  });

  it('does not read the ---/+++ file headers as changed lines', () => {
    // Regression: the file headers are shaped exactly like a removal and an
    // addition. Parsed as change lines they sit on line 0, which is inside the
    // span of every item declared at the top of a file — so the checker used to
    // report a spurious edit to a governed trait on every run, whether or not
    // anything had changed.
    const records = parseDiffByFile(
      [
        'diff --git a/x.rs b/x.rs',
        '--- a/x.rs',
        '+++ b/x.rs',
        '@@ -40,1 +40,1 @@',
        '-old',
        '+new',
      ].join('\n') + '\n',
    ).get('x.rs')!;
    expect(records.has(0)).toBe(false);
    expect([...records.keys()]).toEqual([40]);
    expect(records.get(40)).toEqual({ removed: 'old', added: 'new' });
  });

  it('resolves spans without letting braces inside literals confuse it', () => {
    const source = 'pub enum E {\n    A,\n    B,\n}\n\npub trait After {\n    fn f(&self);\n}\n';
    const span = findItemSpan(source, 'pub enum E');
    expect(span).toEqual({ start: 0, end: 3 });
    const withLiterals =
      'pub struct S {\n    pub k: K,\n}\nimpl S {\n    fn f(&self) { let s = "}"; }\n}\n';
    expect(findItemSpan(withLiterals, 'pub struct S')).toEqual({ start: 0, end: 2 });
  });
});

describe('crate version comparison', () => {
  it('orders versions numerically, not lexically', () => {
    // The string comparison that would say "0.0.9" < "0.0.10" is exactly the
    // bug that lets a required bump pass unnoticed for a year.
    expect(compareCrateVersions('0.0.10', '0.0.9')).toBe(1);
    expect(compareCrateVersions('0.0.9', '0.0.10')).toBe(-1);
    expect(compareCrateVersions('0.1.0', '0.0.99')).toBe(1);
    expect(compareCrateVersions('1.0.0', '0.9.9')).toBe(1);
    expect(compareCrateVersions('0.0.9', '0.0.9')).toBe(0);
  });

  it('treats a missing version as no change rather than a crash', () => {
    expect(compareCrateVersions(null, '0.0.9')).toBe(0);
    expect(compareCrateVersions('0.0.9', null)).toBe(0);
  });
});

describe('the shipped CLI', () => {
  it('runs against this repository and exits 0 with the working tree clean of drift', () => {
    // The unit tests above use synthetic diffs; this one runs the real
    // `git diff` path so a crash while parsing this repo's own output is not
    // invisible to CI.
    const res = spawnSync(process.execPath, ['scripts/check-driver-protocol-compat.mjs'], {
      cwd: ROOT,
      encoding: 'utf8',
    });
    expect(res.status, res.stdout + res.stderr).toBe(0);
    expect(res.stdout).toContain('[check-driver-protocol-compat] ok');
  });

  it('reports the base it measured against, so the run is auditable', () => {
    const res = spawnSync(process.execPath, ['scripts/check-driver-protocol-compat.mjs'], {
      cwd: ROOT,
      encoding: 'utf8',
    });
    expect(res.stdout).toMatch(/base [0-9a-f]{7,40};/);
  });
});
