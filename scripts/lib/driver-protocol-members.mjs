/**
 * Member-level reading of a governed Rust type.
 *
 * `check-driver-protocol-compat.mjs` classifies a change by *span*: which
 * diff lines land between a governed item's declaration line and its closing
 * brace. That is enough to answer "was this contract touched", and it is
 * completely blind to *what* changed inside it. The whole of an enum's variant
 * list sits inside its span, so a commit that adds a variant diffs as one added
 * line — the same shape as a doc comment's worth of text — and reads as
 * `additive`.
 *
 * That blind spot is not theoretical: `NamespaceSwitch::PerRequest` was added
 * to `packages/driver-api/src/capabilities.rs` and the gate reported nothing.
 * It is the exact class of edit a `[MIN_PROTOCOL_VERSION, PROTOCOL_VERSION]`
 * window exists to force a decision about.
 *
 * ## The added-variant rule, and why it is `breaking`
 *
 * **An added enum variant is `breaking` unless the enum carries
 * `#[non_exhaustive]` at the base ref.** It is not `additive`, which is what
 * the class means everywhere else in this gate ("leaves existing implementors
 * compiling and declaring nothing new"), because without `#[non_exhaustive]` a
 * downstream `match` on the enum must be exhaustive. Adding a variant makes it
 * non-exhaustive and the old arm set stops compiling with rustc E0004. No
 * protocol window expresses that either; the driver has to be edited and
 * recompiled.
 *
 * The one exception is real and is honoured: with `#[non_exhaustive]` the
 * downstream `match` was already forced to carry a wildcard arm, and that arm
 * absorbs the new variant, so the addition genuinely is additive. Which side
 * decides is the point most easily got wrong. It is the **base** ref, not the
 * head: the question is not "is this enum wildcard-tolerant now" but "was a
 * driver written against the *previous* crate already holding a wildcard arm".
 * Marking an enum `#[non_exhaustive]` and adding a variant in the same commit is
 * therefore reported as breaking, which is correct — the attribute addition is
 * a break of its own, and drivers written against the base had no wildcard arm.
 *
 * In `packages/driver-api/src` the exception is currently unexercised: no
 * `non_exhaustive` attribute appears anywhere under `src/`, so every governed
 * enum resolves to `breaking` on an added variant. The branch exists because
 * the correct rule is the conditional one, not because this crate needs the
 * lenient arm today.
 *
 * ## Removals and renames
 *
 * A removed or renamed variant is `breaking` with no exception. A rename is
 * remove-plus-add and is reported as a removal, which is what it is. The gate
 * already treats any removed line inside a span as `breaking`; this module
 * exists to give that verdict a *reason that names the variant*, because
 * 「1 line(s) removed from pub enum NamespaceSwitch」 is not something an
 * out-of-tree author can act on.
 *
 * ## Structs
 *
 * Struct member comparison is deliberately reason-only. It never changes a
 * verdict: a new public field stays `additive` (the pinned behaviour, and a
 * correct one — `CapabilitySet`'s fields are filled by `Default`, and a driver
 * that declares nothing new keeps compiling), and a removed field is already
 * `breaking` through the generic removal branch. What it adds is the field name
 * in the report.
 *
 * @module lib/driver-protocol-members
 */

/**
 * @typedef {{ start: number, end: number }} LineSpan zero-based, `end` inclusive.
 */

/**
 * Replace the contents of string and char literals with spaces so brace
 * counting is not fooled by `"}"` or a `'{'` in a default value.
 *
 * Moved here from the gate: member extraction needs the same guarantee, and a
 * second copy of this function would be free to drift from the first.
 *
 * @param {string} line
 * @returns {string}
 */
export function stripStringAndCharLiterals(line) {
  return line
    .replace(/r?#*"[^"]*"#/g, (m) => ' '.repeat(m.length))
    .replace(/'[^']*'/g, (m) => ' '.repeat(m.length));
}

/**
 * Net bracket nesting contributed by one line, literals already stripped.
 *
 * @param {string} line
 * @returns {number} positive = opens, negative = closes.
 */
function bracketDelta(line) {
  let delta = 0;
  for (const ch of line) {
    if (ch === '(' || ch === '[' || ch === '{') delta += 1;
    else if (ch === ')' || ch === ']' || ch === '}') delta -= 1;
  }
  return delta;
}

/**
 * Drop a trailing `//` comment. Literals are stripped first, so a `//` that
 * lives inside a string is already whitespace by the time this runs.
 *
 * @param {string} line
 * @returns {string}
 */
function stripTrailingComment(line) {
  const at = line.indexOf('//');
  return at === -1 ? line : line.slice(0, at);
}

/**
 * Is this line body-comment/doc/attribute noise rather than a member?
 *
 * @param {string} trimmed
 * @returns {boolean}
 */
function isNonMemberLine(trimmed) {
  if (trimmed === '') return true;
  if (trimmed.startsWith('//')) return true;
  if (trimmed.startsWith('/*')) return true;
  return trimmed.startsWith('*'); // block-comment body, or the closing `*/`
}

/**
 * The contiguous attribute block written immediately above an item.
 *
 * Rust puts `#[non_exhaustive]` *above* the `pub enum` declaration line, which
 * is outside the item's span — so a span-based reader can never see it. The
 * walk goes upwards until it meets a line that is neither an attribute, a doc
 * comment, nor the inside of a multi-line attribute such as `#[error(`.
 *
 * @param {string} source
 * @param {number} spanStart zero-based first line of the item.
 * @returns {string[]}
 */
function attributeBlockAbove(source, spanStart) {
  const lines = source.split('\n');
  const collected = [];
  let balance = 0;
  for (let i = spanStart - 1; i >= 0; i -= 1) {
    const trimmed = stripStringAndCharLiterals(lines[i]).trim();
    if (trimmed === '') break;
    // Walking upwards, a bracket closed here was opened on a line already
    // collected — so subtract, and keep going while something is still open.
    balance -= bracketDelta(trimmed);
    collected.unshift(trimmed);
    if (balance > 0) continue; // still inside a multi-line attribute
    if (trimmed.startsWith('#')) continue; // more attributes may sit above
    break;
  }
  return collected;
}

/**
 * Does the item at `span` carry `#[non_exhaustive]`?
 *
 * @param {string} source
 * @param {LineSpan} span
 * @returns {boolean}
 */
export function hasNonExhaustiveAttribute(source, span) {
  return attributeBlockAbove(source, span.start).some((line) =>
    /^#!?\[\s*non_exhaustive\s*[\],]/.test(line),
  );
}

/**
 * Variant names declared in an enum body, in declaration order.
 *
 * Handles the shapes that occur in real Rust rather than only the tutorial one:
 * doc comments above a variant, per-variant `#[cfg]`, an explicit discriminant,
 * a trailing comment, and a variant whose body spans several lines (which is
 * nested past, so its fields are never mistaken for variants).
 *
 * @param {string} source
 * @param {LineSpan} span
 * @returns {string[]}
 */
export function readEnumVariants(source, span) {
  const lines = source.split('\n');
  const names = [];
  let depth = 0;
  for (let i = span.start + 1; i < span.end; i += 1) {
    const code = stripTrailingComment(stripStringAndCharLiterals(lines[i]));
    const trimmed = code.trim();
    if (depth === 0) {
      if (isNonMemberLine(trimmed)) continue;
      const match = /^([A-Za-z_][A-Za-z0-9_]*)/.exec(trimmed);
      if (!match) continue;
      names.push(match[1]);
    }
    depth += bracketDelta(code);
  }
  return names;
}

/**
 * Public field names declared in a struct body, in declaration order.
 *
 * `pub fn helper(&self);` is not a field and does not match: after the name a
 * field is followed by `:`, an item signature by `(`.
 *
 * @param {string} source
 * @param {LineSpan} span
 * @returns {string[]}
 */
export function readStructFields(source, span) {
  const lines = source.split('\n');
  const names = [];
  let depth = 0;
  for (let i = span.start + 1; i < span.end; i += 1) {
    const code = stripTrailingComment(stripStringAndCharLiterals(lines[i]));
    const trimmed = code.trim();
    if (depth === 0) {
      if (isNonMemberLine(trimmed)) continue;
      const match = /^pub(?:\s*\([^)]*\))?\s+([A-Za-z_][A-Za-z0-9_]*)\s*:/.exec(trimmed);
      if (match) names.push(match[1]);
    }
    depth += bracketDelta(code);
  }
  return names;
}

/**
 * @param {string[]} head
 * @param {string[]} base
 * @returns {string[]}
 */
const onlyIn = (head, base) => {
  const seen = new Set(base);
  return head.filter((name) => !seen.has(name));
};

/**
 * A trait item declaration, reduced to what an out-of-tree implementor has to
 * match against.
 *
 * @typedef {{ name: string, sig: string, required: boolean }} TraitItem
 */

/**
 * The start of a `fn` / `const` / `type` item at the top level of a trait body.
 * Visibility, `default`, `const`, `async`, `unsafe` and `extern "C"` are all
 * allowed in front of it; a doc comment or an ordinary expression is not.
 */
const TRAIT_ITEM_START =
  /^(?:pub(?:\s*\([^)]*\))?\s+)?(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?(?:fn|const|type)\b/;

/**
 * The identifier an implementor has to write: the one after `fn`, `const` or
 * `type`. A Rust trait cannot declare the same name twice, so the name is a
 * sound key for reconciling one trait across a change.
 *
 * @param {string} head
 * @returns {string}
 */
function traitItemName(head) {
  const match = /\b(?:fn|const|type)\s+([A-Za-z_][A-Za-z0-9_]*)/.exec(head);
  return match === null ? '<unnamed>' : match[1];
}

/**
 * Read the items of a trait body: what an implementor writes, not what the
 * default bodies contain.
 *
 * `sig` is the declaration text up to the terminator with whitespace collapsed,
 * so splitting one parameter list across several lines is not read as a
 * signature change, and the default body is excluded entirely so that editing a
 * body is not read as one either. `required` records the one distinction that
 * does force a downstream edit: a declaration ending in a bare `;` must be
 * written by every implementor, one carrying a body need not be.
 *
 * @param {string} source
 * @param {LineSpan} span
 * @returns {TraitItem[]}
 */
export function readTraitItems(source, span) {
  const lines = source.split('\n');
  /** @type {TraitItem[]} */
  const items = [];
  let depth = 0;
  /** @type {{ head: string, rel: number } | null} */
  let decl = null;

  for (let i = span.start + 1; i < span.end; i += 1) {
    const code = stripTrailingComment(stripStringAndCharLiterals(lines[i]));
    const trimmed = code.trim();

    // Inside a default body: nothing here is part of any signature.
    if (depth > 0) {
      depth += bracketDelta(code);
      continue;
    }
    if (decl === null) {
      if (isNonMemberLine(trimmed)) continue;
      if (!TRAIT_ITEM_START.test(trimmed)) continue;
      decl = { head: '', rel: 0 };
    }

    // Walk this line up to whichever comes first: the `;` that ends a required
    // item, or the `{` that opens a default body. Everything from there on is
    // body, so it must never reach the signature.
    let cut = code.length;
    let rel = decl.rel;
    let terminator = '';
    for (let k = 0; k < code.length; k += 1) {
      const ch = code[k];
      if (ch === ';' || ch === '{') {
        cut = k;
        terminator = ch;
        break;
      }
      if (ch === '(' || ch === '[') rel += 1;
      else if (ch === ')' || ch === ']') rel -= 1;
    }

    decl.head += (decl.head === '' ? '' : ' ') + code.slice(0, cut).trim();
    if (terminator === '' || rel > 0) {
      decl.rel = rel;
      continue;
    }

    items.push({
      name: traitItemName(decl.head),
      sig: decl.head.replace(/\s+/g, ' ').trim(),
      required: terminator === ';',
    });
    decl = null;
    // `code.slice(cut)` starts *at* the terminator, so a `{` opening a body
    // that runs on is counted here and a one-line body is counted as balanced.
    depth += bracketDelta(code.slice(cut));
  }
  return items;
}

/**
 * Reconcile one trait's items across a change, in the gate's own terms: an
 * out-of-tree implementor is only affected if it has to be edited and
 * recompiled.
 *
 * So a signature that disappears, changes shape, or loses its default body all
 * land in `removed`; and a new item only separates the two ways of arriving if
 * it carries a body, since a bodiless one must be written by every implementor.
 *
 * @param {TraitItem[]} baseItems
 * @param {TraitItem[]} headItems
 * @returns {{ removed: string[], addedRequired: string[], addedDefaulted: string[], baseTotal: number, total: number }}
 */
export function reconcileTraitItems(baseItems, headItems) {
  const headByName = new Map(headItems.map((item) => [item.name, item]));
  /** @type {string[]} */
  const removed = [];

  for (const item of baseItems) {
    const now = headByName.get(item.name);
    if (now === undefined) {
      removed.push(`${item.name} (removed from ${item.sig})`);
    } else if (now.sig !== item.sig) {
      removed.push(`${item.name} (${item.sig} -> ${now.sig})`);
    } else if (now.required !== item.required) {
      removed.push(
        now.required
          ? `${item.name} (no longer defaulted: ${item.sig})`
          : `${item.name} (gained a required body: ${item.sig})`,
      );
    }
  }

  const baseNames = new Set(baseItems.map((item) => item.name));
  const added = headItems.filter((item) => !baseNames.has(item.name));

  return {
    removed,
    addedRequired: added.filter((item) => item.required).map((item) => item.name),
    addedDefaulted: added.filter((item) => !item.required).map((item) => item.name),
    baseTotal: baseItems.length,
    total: headItems.length,
  };
}

/**
 * Compare the members of one governed item across the change.
 *
 * Returns the two things the gate needs and nothing else: a precise reason for
 * a removal (which the gate substitutes into its own removal branch, so no
 * existing verdict or message moves), and the verdict for an addition.
 *
 * @param {{ id: string, kind: string, anchor: string }} rule
 * @param {string} baseSource
 * @param {string} newSource
 * @param {LineSpan} baseSpan
 * @param {LineSpan} newSpan
 * @returns {{
 *   removed: string[],
 *   added: string[],
 *   removalReason: string | null,
 *   addedBreaking: string | null,
 *   addedNote: string | null,
 *   trait: {
 *     removed: string[],
 *     addedRequired: string[],
 *     addedDefaulted: string[],
 *     baseTotal: number,
 *     total: number,
 *   } | null,
 * }}
 */
export function inspectMemberDiff(rule, baseSource, newSource, baseSpan, newSpan) {
  const isEnum = rule.kind === 'enum';
  const read = isEnum ? readEnumVariants : readStructFields;
  const baseMembers = read(baseSource, baseSpan);
  const headMembers = read(newSource, newSpan);

  const removed = onlyIn(baseMembers, headMembers);
  const added = onlyIn(headMembers, baseMembers);
  // Reconciled before the early return below, because it is computed off a
  // different reader and the early return must not hide it.
  const trait =
    rule.kind === 'trait'
      ? reconcileTraitItems(
          readTraitItems(baseSource, baseSpan),
          readTraitItems(newSource, newSpan),
        )
      : null;
  const empty = {
    removed,
    added,
    removalReason: null,
    addedBreaking: null,
    addedNote: null,
    trait,
  };
  if (removed.length === 0 && added.length === 0) return empty;

  const subject = isEnum ? 'variant(s)' : 'public field(s)';

  if (removed.length > 0) {
    return {
      ...empty,
      removalReason: `${rule.id}: ${subject} ${removed.join(', ')} removed from ${rule.anchor} — an out-of-tree driver naming ${removed.length === 1 ? 'it' : 'them'} no longer compiles${isEnum ? ' (a rename reads as exactly this: one name gone, another arrived)' : ''}`,
    };
  }

  // Only an enum can be broken by gaining a member. A struct gaining a public
  // field is the pinned `additive` case and must not be reclassified here.
  if (added.length > 0 && !isEnum) {
    return { ...empty, addedNote: `new public field(s) ${added.join(', ')}` };
  }
  if (added.length === 0) return empty;

  const nonExhaustiveAtBase = hasNonExhaustiveAttribute(baseSource, baseSpan);
  if (nonExhaustiveAtBase) {
    return {
      ...empty,
      addedNote: `variant(s) ${added.join(', ')} added to ${rule.anchor}, which is #[non_exhaustive]: every downstream match already had to carry a wildcard arm, and that arm absorbs the new variant`,
    };
  }
  return {
    ...empty,
    addedBreaking: `${rule.id}: variant(s) ${added.join(', ')} added to ${rule.anchor}, which is not #[non_exhaustive] — an out-of-tree driver that matches it exhaustively stops compiling (rustc E0004); no [MIN_PROTOCOL_VERSION, PROTOCOL_VERSION] window expresses a recompile requirement`,
  };
}
