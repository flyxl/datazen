/**
 * Source-level breakage: edits that leave a *built* old driver perfectly able
 * to run, and stop anyone from *building* one against the new crate.
 *
 * The compat matrix's three classes all answer one question — 「老驱动在新宿主上
 * 是否仍能安全运行」 (can an already-built old driver keep running safely against a
 * new host). A fourth kind of edit does not answer that question at all, which
 * is exactly why it needs its own class rather than a coarser verdict bolted
 * onto one of the three.
 *
 * Two of the existing classes are not merely imprecise here, they are wrong:
 *
 * - **Not `additive`.** The matrix defines `additive` as the change that
 *   "leaves existing implementors compiling and declaring nothing new".
 *   `#[non_exhaustive]` is precisely the attribute that makes them stop
 *   compiling, so classifying it `additive` asserts the opposite of the truth.
 *   That is not a wording problem: `additive` is the only class that tells an
 *   out-of-tree driver "you are fine, carry on", which is the one message
 *   that must not be sent.
 * - **Not `breaking`.** `breaking` obliges a `PROTOCOL_VERSION` bump, on the
 *   ground that no `[MIN, PROTOCOL]` window expresses the change. True, and
 *   useless: nothing on the wire changed, so no host can ever observe the
 *   break, and a host running an already-built old driver is genuinely fine.
 *   A protocol bump would advertise a wire incompatibility that does not
 *   exist while doing nothing about the real failure, which is a `cargo build`
 *   in someone else's repository.
 *
 * Hence the fourth class, `source-breaking`: it moves the crate version like
 * `additive` does, and it carries an explicit migration instruction, because
 * the protocol window has no vocabulary for "you must recompile" and never
 * will.
 */

/**
 * What an out-of-tree driver actually has to do. Printed verbatim on failure,
 * because the gate is often the only thing an out-of-tree author ever sees
 * before their build breaks.
 *
 * The load-bearing correction is the second half: the obvious one-line patch
 * does not compile. `#[non_exhaustive]` rejects *every* struct expression,
 * including the functional-update form, so `..Default::default()` is not a
 * migration — it is the same error in a different costume.
 */
export const OUT_OF_TREE_MIGRATION_NOTE = [
  'This cannot be expressed as a [MIN_PROTOCOL_VERSION, PROTOCOL_VERSION] window:',
  'nothing on the wire changed, so a host cannot detect an out-of-tree driver that',
  'no longer builds. Every out-of-tree driver must be recompiled and each',
  'construction site rewritten by hand.',
  '',
  'The rewrite is NOT "add ..Default::default()". `#[non_exhaustive]` rejects every',
  'struct expression, including the functional-update form `CapabilitySet { f: v,',
  '..base }`, with rustc E0639 — so functional update does not survive either. The',
  "whole literal must become `CapabilitySet::default()` followed by one explicit",
  "assignment per field. A driver that drops an assignment then silently inherits",
  "that field's `Default` instead of declaring it, so each driver needs a test",
  'asserting the full grid by value.',
].join('\n');

/**
 * Attributes that restrict how code *outside the defining crate* may use the
 * item, keyed by the `kind` of contract rule they restrict.
 *
 * Only `non_exhaustive` is listed, and only for `struct`. Three exclusions are
 * deliberate:
 *
 * - `serde(...)` is absent, and must stay absent. A serde attribute changes
 *   what is written on the wire, not what compiles. It is governed by the
 *   serde rows of the migration doc, never by this table. Filing a wire
 *   concern as a build concern — or the reverse — is the confusion this table
 *   exists to prevent, so `#[serde(...)]` deliberately falls through to the
 *   ordinary `additive` verdict.
 * - `non_exhaustive` on an `enum` is absent because on an enum it restricts
 *   *matching* (a downstream `match` must grow a `_` arm), not construction.
 *   That is a real source break with a different signature and a different
 *   rule kind; folding it in here would describe it with the wrong remedy.
 * - The patterns are whole-line on purpose. `isCosmeticLine` already drops a
 *   comment-only line before this table ever sees it, but a trailing
 *   `// ... #[non_exhaustive] ...` rides in on a line that really does declare
 *   a field, and a substring match would gate on that TODO and report an
 *   ordinary field addition as a source break.
 */
const SOURCE_BREAKING_ATTRIBUTES = Object.freeze({
  struct: Object.freeze([
    Object.freeze({
      name: 'non_exhaustive',
      line: /^\s*#\s*\[\s*non_exhaustive\s*\]\s*$/,
      effect: 'rejects every struct expression built outside this crate',
    }),
  ]),
  trait: Object.freeze([]),
});

/**
 * @param {string[]} addedLines non-cosmetic lines added inside a governed span
 * @param {{ kind?: string } | null | undefined} rule the contract rule in force
 * @returns {{ attributes: string[], effect: string, migration: string } | null}
 *   a description of the source break, or `null` when the addition does not
 *   restrict downstream construction.
 */
export function detectSourceBreak(addedLines, rule) {
  const table = SOURCE_BREAKING_ATTRIBUTES[rule?.kind];
  if (!table || table.length === 0) return null;

  const hits = table.filter((entry) => addedLines.some((raw) => entry.line.test(raw)));
  if (hits.length === 0) return null;

  return {
    attributes: [...new Set(hits.map((h) => h.name))],
    effect: hits[0].effect,
    migration: OUT_OF_TREE_MIGRATION_NOTE,
  };
}
