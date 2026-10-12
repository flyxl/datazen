/**
 * @vitest-environment node
 *
 * `LegacyResourceAdapter` is a governed contract, not a helper.
 *
 * It is the shim that lets a driver which has *not* implemented
 * `ResourceProvider` be driven through it anyway. The host calls
 * `LegacyResourceAdapter::new(driver, driver_id, driver_version, …)` once per
 * un-migrated path driver, so its constructor signature is a driver/host ABI
 * surface: change it and the host plus the whole un-migrated fleet stop
 * compiling, at the next protocol bump rather than at the moment of the edit.
 *
 * The decision this file exists to make unambiguous is *where* the rule is
 * anchored, because two anchors were plausible and only one of them works:
 *
 * - `pub struct LegacyResourceAdapter` → the six **private** fields. Governing
 *   that span would report "breaking" for edits that cannot break anyone
 *   outside driver-api, and it would still leave `new()` unwatched.
 * - `impl LegacyResourceAdapter` → lines 111-287, containing `new()` and
 *   `with_evidence()`. This is the host's actual binding point.
 *
 * The alternative is asserted to be insufficient rather than merely set aside,
 * so "both anchors work" can never quietly become true. And if a second rule
 * ever starts covering the same span, `only this rule governs it` fails and
 * forces the overlap to be resolved instead of left in force.
 */

import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import {
  CONTRACT_RULES,
  classifyContractChange,
  findItemSpan,
  parseDiffByFile,
} from '../check-driver-protocol-compat.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

const ADAPTER_FILE = 'packages/driver-api/src/resource_adapter.rs';

const read = (relPath: string): string => readFileSync(join(ROOT, relPath), 'utf8');

type Rule = (typeof CONTRACT_RULES)[number];

const ruleById = (id: string): Rule | undefined => CONTRACT_RULES.find((r) => r.id === id);

/** The governed span, as line numbers on the real file. */
const spanOf = (rule: Rule): { start: number; end: number } => {
  const span = findItemSpan(read(rule.file), rule.anchor);
  expect(span, `${rule.id} must resolve against ${rule.file}`).not.toBeNull();
  return span!;
};

const RULE = ruleById('legacy-resource-adapter')!;

/**
 * Build a `-U0` unified diff from an old/new pair.
 *
 * Copied rather than imported: the sibling suite keeps `diffOf` module-local
 * and a real LCS matters here for the same reason it does there — the
 * classification under test depends on which line a hunk lands on.
 */
const diffOf = (before: string, after: string, file = 'x.rs'): string => {
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

  const header = `diff --git a/${file} b/${file}\n--- a/${file}\n+++ b/${file}\n`;
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

/** Classify a synthetic pair through the real diff parser. */
const classifyViaDiff = (rule: Rule, before: string, after: string) => {
  const records = parseDiffByFile(diffOf(before, after)).get('x.rs');
  expect(records, 'the synthetic diff must produce records').toBeDefined();
  return classifyContractChange(before, after, rule, records!);
};

/** The shape of the governed item, kept small enough to reason about exactly. */
const BASE_IMPL = `impl LegacyResourceAdapter {
    /// Existing doc.
    pub fn new(
        driver_id: impl Into<String>,
        driver_version: impl Into<String>,
    ) -> Self {
        let _ = driver_id;
        let _ = driver_version;
        Self { runtime_epoch: 0 }
    }

    pub fn open_resource_count(&self) -> usize {
        self.live.lock().len()
    }
}
`;

describe('the rule table entry for LegacyResourceAdapter', () => {
  it('exists, and is a struct rule on the adapter module', () => {
    expect(RULE, 'legacy-resource-adapter must be in CONTRACT_RULES').toBeDefined();
    expect(RULE.file).toBe(ADAPTER_FILE);
    expect(RULE.kind).toBe('struct');
  });

  it('explains itself, so the choice cannot rot into an accident', () => {
    expect(RULE.why ?? '').toMatch(/\S/);
    expect(RULE.implementedBy ?? '').toMatch(/\S/);
    // The comment on the rule in driver-protocol-rules.mjs records *why the
    // inherent impl and not the struct*. That reasoning is the decision; a
    // reader of the table alone must be able to reach it.
    expect(RULE.why).toMatch(/host/i);
  });
});

describe('the chosen anchor really covers what the decision claims', () => {
  it('resolves to the inherent impl', () => {
    expect(RULE.anchor).toBe('impl LegacyResourceAdapter {');
    const lines = read(ADAPTER_FILE).split('\n');
    const span = spanOf(RULE);
    expect(lines[span.start]).toBe('impl LegacyResourceAdapter {');
  });

  it('contains the constructor the host calls', () => {
    // The load-bearing assertion. If someone re-anchors the rule onto the
    // struct declaration, `new` silently leaves the governed span and every
    // breaking-change case below starts passing vacuously.
    const lines = read(ADAPTER_FILE).split('\n');
    const span = spanOf(RULE);
    const governed = lines.slice(span.start, span.end + 1).join('\n');
    expect(governed, 'the governed span must contain `pub fn new(').toContain('pub fn new(');
    expect(governed).toContain('driver_version');
  });

  it('stops before the ResourceProvider impl, which `resource-provider` already owns', () => {
    // The rule governs the adapter's *inherent* surface only. The trait
    // implementation is the `resource-provider` rule's business; overlapping
    // the two would make one edit produce two verdicts.
    const lines = read(ADAPTER_FILE).split('\n');
    const span = spanOf(RULE);
    expect(lines[span.end].trim()).toBe('}');
    // `#[async_trait]` and a blank line sit between the two impls, so the
    // boundary is located by searching forward rather than at a fixed offset.
    const providerImpl = lines.findIndex(
      (line, i) => i > span.end && /^impl ResourceProvider for LegacyResourceAdapter \{$/.test(line),
    );
    expect(providerImpl, 'the ResourceProvider impl must follow the inherent impl').toBeGreaterThan(
      span.end,
    );
    const governed = lines.slice(span.start, span.end + 1).join('\n');
    expect(governed, 'the two impls must not overlap').not.toContain(
      'impl ResourceProvider for LegacyResourceAdapter',
    );
  });

  it('is the only rule that governs the constructor', () => {
    const lines = read(ADAPTER_FILE).split('\n');
    const constructorLine = lines.findIndex((l) => l.includes('pub fn new('));
    expect(constructorLine).toBeGreaterThan(-1);

    const covering = CONTRACT_RULES.filter((rule) => {
      if (rule.file !== ADAPTER_FILE) return false;
      const span = findItemSpan(read(rule.file), rule.anchor);
      return span !== null && constructorLine >= span.start && constructorLine <= span.end;
    }).map((rule) => rule.id);

    expect(covering).toEqual(['legacy-resource-adapter']);
  });
});

describe('the anchor that was rejected would not have worked', () => {
  it('the struct declaration span does not contain the constructor', () => {
    // This is what makes "both approaches work" impossible rather than
    // merely unchosen: the alternative anchor provably misses `new()`.
    const source = read(ADAPTER_FILE);
    const lines = source.split('\n');
    const structSpan = findItemSpan(source, 'pub struct LegacyResourceAdapter')!;
    const structBody = lines.slice(structSpan.start, structSpan.end + 1).join('\n');
    expect(structBody).toContain('driver_id: String');
    expect(structBody, 'the struct span holds fields, not the constructor').not.toContain(
      'pub fn new(',
    );
  });

  it('and a break inside the constructor is invisible to that alternative', () => {
    // The same edit, classified against the rejected anchor: nothing is
    // reported. Proof that the anchor choice is the difference between a
    // governed contract and a decorative one.
    const alt: Rule = {
      id: 'rejected-alt',
      file: 'x.rs',
      anchor: 'pub struct LegacyResourceAdapter',
      kind: 'struct',
    };
    const before = `pub struct LegacyResourceAdapter {
    driver_id: String,
    driver_version: String,
}
impl LegacyResourceAdapter {
    pub fn new(
        driver_id: impl Into<String>,
        driver_version: impl Into<String>,
    ) -> Self {
        Self { driver_id: driver_id.into(), driver_version: driver_version.into() }
    }
}
`;
    const after = before.replace('        driver_version: impl Into<String>,\n', '');
    const finding = classifyViaDiff(alt, before, after);
    expect(finding.cls, 'the rejected anchor reports nothing for this break').not.toBe('breaking');
  });
});

describe('breaking changes to the adapter are caught', () => {
  it('rejects dropping a constructor parameter', () => {
    // The realistic break: `driver_version` is fed into CapabilitySnapshot at
    // resource_adapter.rs:142, so it is load-bearing, not decorative. Any edit
    // that removes it recompiles the host and every un-migrated driver.
    const after = BASE_IMPL.replace('        driver_version: impl Into<String>,\n', '');
    const finding = classifyViaDiff(RULE, BASE_IMPL, after);
    expect(finding.cls).toBe('breaking');
  });

  it('rejects renaming a constructor parameter', () => {
    const after = BASE_IMPL.replace(
      '        driver_id: impl Into<String>,',
      '        id: impl Into<String>,',
    );
    const finding = classifyViaDiff(RULE, BASE_IMPL, after);
    expect(finding.cls).toBe('breaking');
  });

  it('rejects deleting a whole method', () => {
    const after = BASE_IMPL.replace(
      `
    pub fn open_resource_count(&self) -> usize {
        self.live.lock().len()
    }
`,
      '',
    );
    const finding = classifyViaDiff(RULE, BASE_IMPL, after);
    expect(finding.cls).toBe('breaking');
  });

  it('names the rule, so the failure points at the adapter', () => {
    const after = BASE_IMPL.replace('        driver_version: impl Into<String>,\n', '');
    const finding = classifyViaDiff(RULE, BASE_IMPL, after);
    expect(finding.id).toBe('legacy-resource-adapter');
    expect(finding.reason).toContain('legacy-resource-adapter');
  });
});

describe('additive and cosmetic changes to the adapter are not over-reported', () => {
  it('treats a new inherent method as additive', () => {
    const after = BASE_IMPL.replace(
      '\n}\n',
      `
    pub fn open_resource_count(&self) -> usize {
        self.live.lock().len()
    }

    /// A brand-new helper nobody calls yet.
    pub fn epoch(&self) -> u64 {
        self.runtime_epoch
    }
}
`,
    );
    const finding = classifyViaDiff(RULE, BASE_IMPL, after);
    expect(finding.cls).not.toBe('breaking');
    expect(['additive', 'cosmetic']).toContain(finding.cls);
  });

  it('treats a doc-comment rewrite as cosmetic', () => {
    const after = BASE_IMPL.replace('    /// Existing doc.', '    /// Reworded doc.');
    const finding = classifyViaDiff(RULE, BASE_IMPL, after);
    expect(finding.cls).toBe('cosmetic');
  });
});

describe('the real tree', () => {
  it('still declares the constructor the host depends on', () => {
    // Anchors can survive a rename of the *thing* only if the thing survives.
    const source = read(ADAPTER_FILE);
    expect(source).toContain('pub struct LegacyResourceAdapter');
    expect(source).toContain('impl ResourceProvider for LegacyResourceAdapter');
    expect(source).toMatch(/pub fn new\(\s*\n\s*driver: Arc<dyn DatabaseDriver>/);
  });

  it('is not covered twice over by the resource-provider rule', () => {
    const rp = ruleById('resource-provider')!;
    expect(rp.file, 'the trait rule lives on the trait declaration').not.toBe(ADAPTER_FILE);
  });
});