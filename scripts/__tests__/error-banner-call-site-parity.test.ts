/**
 * Class-set parity for every call site absorbed onto `ErrorBanner`.
 *
 * Lives here, in `scripts/__tests__/`, because that is where whole-repo
 * invariant tests live (module layers, driver import boundaries, id
 * terminology, ci-docs consistency, version consistency, and the Tailwind
 * colour guard). `packages/ui/` has no vitest config of its own, so a test
 * placed there only runs by accident of the Host's include list.
 *
 * It is `.ts`, not `.tsx`, on purpose: the Host's `include` list covers
 * `scripts/__tests__` tests ending in `.ts` or `.mjs` — **not** `.tsx`. A
 * `.tsx` file here would be silently never collected, which is the exact
 * failure this test exists to prevent. It therefore builds elements with
 * `createElement`.
 *
 * WHY THIS FILE EXISTS
 * --------------------
 * The extraction's two worst defects were both invisible to a behavioural
 * test, and both are layout defects:
 *
 *   1. A call site used the `danger` token, passed only its *text* colour, and
 *      silently inherited the variant's background and border. The banner still
 *      rendered, was still announced, still showed the right string — in the
 *      wrong red.
 *   2. A variant was hard-coded to literal Tailwind `red-*`, which reads no
 *      `--c-*` token and is therefore blind to the active theme.
 *
 * `src/windows/settings/__tests__/ErrorBannerCallSiteParity.test.tsx` pins
 * tag / role / visible text. This file pins the *class set*, which is the other
 * half. (Different basename on purpose: two files called
 * `ErrorBannerCallSiteParity.test.tsx` in one repo is a trap.)
 *
 * WHAT IS COMPARED, AND AGAINST WHAT
 * ----------------------------------
 * `base` below is the hand-written error bar's `className`, copied verbatim
 * out of the base blob:
 *
 *     git show 65143b133:<path>      # the per-site path is in the table
 *
 * They are **not** derived from the current implementation — deriving them from
 * head would make every assertion here vacuously true. Do not "helpfully"
 * regenerate them from the current `ErrorBanner` usage.
 *
 * COMPARISON BASIS (口径) — deliberately not a raw class-string diff:
 *
 *   * Sets, not strings. CSS rendering depends on stylesheet source order, not
 *     on the order of tokens inside a `class` attribute, so ordering is noise.
 *   * Split into LAYOUT vs COLOUR. Layout must match base exactly. Colour is
 *     compared separately, because the user-approved change ("全仓错误色统一到
 *     `danger` token") *intends* the colour classes to differ from base.
 *   * `extraAllowed` is an explicit per-site allow-list of layout classes
 *     present in head but not in base. Anything not on the list fails.
 *
 * COLOUR IS PINNED EXACTLY, NOT LOOSELY
 * -------------------------------------
 * An earlier revision asserted only "no native `red-*`" and "at least one
 * `danger` class". That is far weaker than it looks: adding `text-danger/50` to
 * a call site satisfies both, and all 38 tests in the repo still passed — while
 * the header comment claimed the colour classes were pinned. So `colour`
 * below is the **complete, exact** set of colour utilities expected on the
 * rendered element, asserted with `toEqual`. A wrong alpha now fails.
 *
 * `colour` is hand-derived from `variant` + the call site's own `className`
 * after tw-merge, NOT recorded from a current run — recording from a run would
 * re-introduce the same vacuity described above. The derivation for each site
 * is written out next to its entry.
 */
import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createElement } from 'react';
import { render } from '@testing-library/react';
import { twMerge } from 'tailwind-merge';
import { ErrorBanner } from '../../packages/ui/src/ErrorBanner';

/** Walk up from the test cwd until the workspace root (holds `packages/ui/src`) is found. */
function findRepoRoot(): string {
  let dir = process.cwd();
  for (;;) {
    if (existsSync(resolve(dir, 'packages/ui/src/ErrorBanner.tsx'))) return dir;
    const parent = resolve(dir, '..');
    if (parent === dir) throw new Error('repo root not found from ' + process.cwd());
    dir = parent;
  }
}

const REPO_ROOT = findRepoRoot();

/** Matches a colour utility: the semantic token or a literal Tailwind shade. */
const COLOUR =
  /^(?:[a-z-]+:)*(?:text|bg|border|ring|fill|stroke|from|via|to|outline|decoration|accent|caret|divide|placeholder)-(?:danger|red-\d+)(?:\/\d+)?$/;
const NATIVE_RED =
  /^(?:[a-z-]+:)*(?:text|bg|border|ring|fill|stroke|from|via|to|outline|decoration|accent|caret|divide|placeholder)-red-\d+(?:\/\d+)?$/;

type Site = {
  label: string;
  /** Repo-relative path of the call site. */
  file: string;
  /** How many `<ErrorBanner` opening tags this file must contain. */
  count: number;
  /** Verbatim `className` of the hand-written bar, from the base blob. */
  base: string;
  /**
   * The exact colour utilities expected on the rendered element. Hand-derived
   * from `variant` + this site's own `className` after tw-merge.
   */
  colour: string;
  /** Layout classes legitimately present in head but absent from base. */
  extraAllowed?: readonly string[];
};

const SITES: readonly Site[] = [
  {
    // `plain` variant, no `className` at either site.
    label: 'TunnelEditDialog hint',
    file: 'src/windows/settings/TunnelEditDialog.tsx',
    count: 2,
    base: 'text-xs text-red-400',
    colour: 'text-danger',
  },
  {
    // `plain`, no `className` at either site.
    label: 'TunnelSettingsSection store error',
    file: 'src/windows/settings/TunnelSettingsSection.tsx',
    count: 2,
    base: 'text-xs text-red-400',
    colour: 'text-danger',
  },
  {
    // `plain`, no `className`.
    label: 'TunnelDeleteDialog',
    file: 'src/windows/settings/TunnelDeleteDialog.tsx',
    count: 1,
    base: 'text-xs text-red-400',
    colour: 'text-danger',
  },
  {
    // `plain`, no `className`.
    label: 'TunnelTestDialog',
    file: 'src/windows/settings/TunnelTestDialog.tsx',
    count: 1,
    base: 'text-xs text-red-400',
    colour: 'text-danger',
  },
  {
    // `boxed` + `mt-3`; the call site restates no colour, so the variant's
    // background and border both survive — which is the point of defect 1.
    label: 'SaveTunnelDialog',
    file: 'src/components/connection/SaveTunnelDialog.tsx',
    count: 1,
    base: 'mt-3 rounded-md border border-red-500/20 bg-red-500/10 p-2 text-xs text-red-400',
    colour: 'border-danger/20 bg-danger/10 text-danger',
  },
  {
    // `plain`, no `className` at either site.
    label: 'ConnectionAdvancedSettings div',
    file: 'src/components/connection/ConnectionAdvancedSettings.tsx',
    count: 2,
    base: 'text-xs text-red-400',
    colour: 'text-danger',
  },
  {
    // `plain` + `basis-full px-1 py-1` — layout only, no colour override.
    label: 'FilterEditor',
    file: 'src/components/FilterEditor.tsx',
    count: 1,
    base: 'basis-full px-1 py-1 text-xs text-red-400',
    colour: 'text-danger',
  },
  {
    // `plain` + `pl-9`. This bar was already `text-danger` at base, and the
    // call site deliberately does NOT restate it: restating a class identical
    // to the variant's is what invites silent divergence later.
    label: 'NlFilterInput',
    file: 'src/components/ai/NlFilterInput.tsx',
    count: 1,
    base: 'pl-9 text-xs text-danger',
    colour: 'text-danger',
  },
  {
    // `strip` + `border-danger/30 py-1`. Both the variant (`/20`) and the call
    // site (`/30`) are in the border-colour group, so tw-merge keeps the
    // call site's and drops the variant's — `/30` must win, and the expected
    // set below proves the `/20` is gone rather than merely outranked.
    label: 'TableView quick-filter',
    // The quick filter moved out of `TableView` into `TableToolbar` (DB-06
    // track, GC budget). The call site was moved verbatim — class set
    // included — so only the file it lives in changed.
    file: 'src/windows/connection/TableToolbar.tsx',
    count: 1,
    base: 'border-b border-red-500/30 bg-red-500/10 px-3 py-1 text-xs text-red-400',
    colour: 'bg-danger/10 border-danger/30 text-danger',
  },
  {
    // `strip` + `py-2 text-danger`. Restating the variant's own value is inert
    // (tw-merge collapses the duplicate), so the rendered set is unchanged.
    label: 'WorkflowPage operation error',
    file: 'src/windows/workflow/WorkflowPage.tsx',
    count: 1,
    base: 'flex items-start gap-2 border-b border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-300',
    colour: 'border-danger/20 bg-danger/10 text-danger',
  },
  {
    // `boxed` + `px-2 py-1.5`; no colour override.
    label: 'StringEditor save error',
    file: 'packages/drivers/redis/ui/value-editors/StringEditor.tsx',
    count: 1,
    base: 'rounded-md border border-danger/20 bg-danger/10 px-2 py-1.5 text-danger',
    colour: 'border-danger/20 bg-danger/10 text-danger',
    // `p-2` survives only because no `className` member conflicts with it, and
    // it is provably a no-op: `px-2` already sets the x axis to the same 0.5rem
    // and `py-1.5` overrides the y axis. tw-merge cannot delete a group that
    // nothing conflicts with — it only ever overrides.
    // `text-xs` is the variant's own font size. It renders the same as base
    // *only because* the nearest font-size ancestor is `text-xs`
    // (KeyEditors' root). That is a cascade fact, not a class-set fact, so it
    // is proved by the rendered-ancestor test in
    // packages/drivers/redis/ui/__tests__/stringEditorFontInheritance.test.tsx
    // rather than asserted here.
    extraAllowed: ['p-2', 'text-xs'],
  },
];

/** Locate `<ErrorBanner` opening tags, skipping the component's own definition. */
function callSiteTags(source: string): string[] {
  const out: string[] = [];
  let from = 0;
  for (;;) {
    const at = source.indexOf('<ErrorBanner', from);
    if (at === -1) break;
    from = at + 1;
    // Brace/quote-aware scan to the end of the opening tag: attribute values
    // legitimately contain `>` (e.g. `onDismiss={() => …}`), so a naive
    // `[^>]*` would truncate the tag.
    let i = at + '<ErrorBanner'.length;
    let depth = 0;
    let quote: string | null = null;
    for (; i < source.length; i += 1) {
      const ch = source[i];
      if (quote) {
        if (ch === quote) quote = null;
        continue;
      }
      if (ch === '"' || ch === "'" || ch === '`') quote = ch;
      else if (ch === '{') depth += 1;
      else if (ch === '}') depth -= 1;
      else if (ch === '>' && depth === 0) break;
    }
    out.push(source.slice(at, i + 1));
  }
  return out;
}

function attr(tag: string, name: string): string | undefined {
  const m = new RegExp(`\\b${name}="([^"]*)"`).exec(tag);
  return m?.[1];
}

const classes = (s: string) => s.split(/\s+/).filter(Boolean);
const nonColour = (cs: string[]) => cs.filter((c) => !COLOUR.test(c)).sort();
const colourOf = (cs: string[]) => cs.filter((c) => COLOUR.test(c)).sort();

/** Render the real component with the props parsed from a real call-site tag. */
function renderTag(tag: string): string[] {
  const { unmount } = render(
    createElement(ErrorBanner, {
      variant: (attr(tag, 'variant') ?? 'plain') as 'plain',
      className: attr(tag, 'className') ?? '',
      icon: /\bicon=/.test(tag) ? createElement('span') : undefined,
      // `children` is a required prop here, so it belongs in the props object —
      // `createElement`'s rest-children overload only applies when the props
      // object already satisfies the component's own type.
      children: 'x',
    }),
  );
  const el = document.querySelector('[role="alert"]');
  const head = el?.getAttribute('class')?.split(/\s+/).filter(Boolean) ?? [];
  unmount();
  return head;
}

describe('ErrorBanner call-site class-set parity', () => {
  it('covers exactly the 14 production call sites', () => {
    // 11 files, some of which hold more than one call site.
    expect(SITES.reduce((n, s) => n + s.count, 0)).toBe(14);
  });

  for (const site of SITES) {
    it(`${site.label}: layout matches base, colour matches exactly`, () => {
      const source = readFileSync(resolve(REPO_ROOT, site.file), 'utf8');
      const tags = callSiteTags(source);
      // A new or removed call site must fail loudly, not be skipped.
      expect(tags).toHaveLength(site.count);

      const layouts: string[][] = [];
      for (const tag of tags) {
        const head = renderTag(tag);

        // Colour, pinned exactly. A wrong alpha (e.g. `text-danger/50`) or a
        // surviving literal shade both fail here.
        expect(colourOf(head)).toEqual(classes(site.colour).sort());

        const base = classes(site.base);
        // Extra *layout* classes only — colour is pinned by the assertion above.
        const extraLayout = head.filter((c) => !base.includes(c) && !COLOUR.test(c)).sort();
        expect(extraLayout).toEqual([...(site.extraAllowed ?? [])].sort());
        // Layout must match base *modulo* the declared extras, which are pinned
        // by the assertion above.
        const allowed = site.extraAllowed ?? [];
        layouts.push(nonColour(head).filter((c) => !allowed.includes(c)));
      }

      expect(layouts).toEqual(
        Array.from({ length: site.count }, () => nonColour(classes(site.base))),
      );
    });
  }

  it('renders no literal Tailwind red at any call site', () => {
    // Redundant with the exact `colour` assertion above, but kept as a
    // standalone invariant: the reason `danger` exists is that literal shades
    // read no `--c-*` token and are blind to the active theme. If someone
    // widens `colour` to allow a new shade, this still objects.
    const offenders: string[] = [];
    for (const site of SITES) {
      const source = readFileSync(resolve(REPO_ROOT, site.file), 'utf8');
      for (const tag of callSiteTags(source)) {
        for (const c of renderTag(tag).filter((x) => NATIVE_RED.test(x))) {
          offenders.push(`${site.file} \`${c}\``);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it('keeps the row layout the icon prop is responsible for', () => {
    // The `flex items-start gap-2` row is added by `icon`, not by the variant;
    // WorkflowPage is the only site that uses it, and its base bar had it.
    const head = renderTag(
      '<ErrorBanner variant="strip" icon={<span />} className="py-2 text-danger" />',
    );
    expect(twMerge(head.join(' '))).toContain('flex');
    expect(head).toEqual(expect.arrayContaining(['flex', 'items-start', 'gap-2']));
  });
});
