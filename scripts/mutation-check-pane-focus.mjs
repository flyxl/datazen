/**
 * [tester] Mutation check for the BUG-001 pane-focus single-writer rule.
 *
 * `focusedPaneIdByPanel` has exactly one writer in production: `syncPaneFocus`
 * (plus the two whole-state literals at module scope and in `reset`). Reading
 * the code cannot tell you whether an action actually routes through it — only a
 * mutation can. This script reverts each focus-touching action in
 * `packages/frontend/src/stores/panelStore.ts` to its pre-BUG-001 shape and requires the **pane
 * suite** to go red: the 10 files in `TESTS` below, run together. Nothing here
 * looks at one file — a cell is RED when the whole `TESTS` run exits non-zero, so
 * a mutation that only some *other* file in that list can see still counts as
 * caught. "This file went red" is not a criterion this script evaluates.
 *
 * Usage:  node scripts/mutation-check-pane-focus.mjs [--keep-going]
 *   `--keep-going` is accepted and is also the default: the loop never stops at
 *   the first uncovered mutation, it reports every one of them.
 *
 * ## Is this a gate? Half of one, on purpose — read this before adding a CI step.
 *
 * TYPECHECKED: yes. This file is listed in `files` of `tsconfig.pack-ep.json`,
 *   so `pnpm typecheck` (and CI's "TypeScript typecheck" step) type-checks it. It
 *   is a program member, not a `// @ts-check` pragma, so deleting or renaming the
 *   file turns the gate red with TS6053 instead of quietly going unchecked.
 *
 * EXECUTED BY CI: **no, and deliberately not.** `grep -rn mutation-check-pane-focus
 *   .github/ package.json` returns nothing: no workflow, no npm script, no wrapper
 *   names or runs it. It is a manual tool you run when you touch the pane focus
 *   rule. The reason is measured, not guessed: a full run is 16 sequential vitest
 *   invocations (1 baseline + 15 cells), **78 s wall clock** on an otherwise idle
 *   machine. Each cell is a *separate* `vitest run` of the whole 10-file list
 *   rather than a rerun of a subset, because a mutation is only meaningful
 *   against the full pane suite. That does not parallelise, it does not share a
 *   transform cache across cells, and it lands on top of an already-slow CI job;
 *   on a shared runner, or on a machine with the rest of the suite running, the
 *   same run measured 13-15 min in practice. Spending that on every commit to
 *   re-derive "15/15" — a result that only changes when panelStore.ts or a pane
 *   test changes — is a bad trade. Run it by hand when you change either side.
 *
 * Do not add a CI step for it without re-measuring on the runner you intend to
 * use, and do not "fix" the 78 s by narrowing `TESTS`: the suite-level criterion
 * is the whole point of this harness.
 *
 * The restore is **anchored to git, not to a disk re-read**. `PRISTINE` is always
 * `git show HEAD:src/stores/panelStore.ts`, and every mutated file is built from
 * it — never from whatever happens to be on disk. Two guards follow from that,
 * and both are load-bearing; an earlier version had neither, and was measured to
 * be silently wrong on both:
 *
 *   - **INTERFERENCE** — if the file on disk already differs from HEAD *before*
 *     the first injection, this run would not be measuring the committed state.
 *     It says so on stdout AND stderr, refuses to run, and exits 4 without
 *     touching the file.
 *   - **VOID** — if after a mutation the file on disk is not byte-identical to
 *     what this run wrote, that mutation's result cannot be read at all: a
 *     write-back that silently failed used to be printed as `RED`. It is recorded
 *     as VOID and exits 5.
 *
 * That earlier version snapshotted the file from disk, restored the snapshot,
 * and then compared the file with **that same snapshot**: a tautology, blind to
 * git, which can only ever prove "the file equals what I just wrote". It printed
 * `restored byte-for-byte: true` even when the working tree had already been
 * drifted by someone else — measured: one pure-comment line added to
 * `panelStore.ts`, harness run to completion, the drifted bytes written back and
 * a clean restore reported, exit 0.
 *
 * The write-back target is the committed file, which on any run that got past the
 * interference assertion is byte-identical to what was found on disk (that
 * equality is exactly what the assertion proves). The single exception is at
 * `restoreTarget` below and is deliberately **not** a restore — read it before
 * "simplifying" the two arms into one. `<path>.mutation-backup` holds the startup
 * bytes. It is deleted when the run finishes having put the store back, and it is
 * deliberately left on disk in every other case — including a run that exits 0,
 * because the deletion itself can fail on a read-only directory. So a backup
 * still on disk after a run is the expected shape, not a surprise to chase.
 * No mutation is ever committed.
 *
 * Exit codes — 0 is the only value that means "this measurement is trustworthy":
 *   0 clean · 1 uncovered · 2 baseline not green · 3 write-back verification
 *   failed · 4 pre-injection interference with HEAD · 5 a mutation was VOID ·
 *   6 lost control of the store file (the loop threw, or the restore failed) ·
 *   7 the safety backup could not be written, so nothing was mutated at all.
 * If several apply, the highest wins, and **every** condition found is printed.
 * 6 sits above the rest on purpose: a tree left holding an injected mutation
 * voids every number in the run, so it must not be summarised as "1 uncovered".
 * 7 is above 6 only as a tiebreak, and they cannot co-occur for two independent
 * reasons. Mechanism: the NO_SAFETY_NET path calls process.exit, which ends the
 * process there, so the Math.max fold at the bottom never runs and never sees a
 * 7 — measured, with 6 already in the list the fold still does not execute, and
 * neutering that one call makes it run and yield 6. So the 7 above 6 in the table
 * is documentation, not something the fold is being asked to resolve. Ordering,
 * independently: 7 exits before the first mutation and 6 requires that a
 * mutation happened.
 * One caveat on the mechanism half, measured and worth keeping: the exit code is
 * not immutable after the call. A process.on('exit') listener still runs and can
 * set process.exitCode over it (set 7 -> 99 that way). The fold is bypassed
 * regardless; the number is only safe because nothing here registers such a
 * listener — checked, not assumed, by preloading a wrapper over process.on /
 * .addListener / .prependListener / .once / .off / .removeListener onto a real
 * PANE_MUTATION_FAULT=backup run: zero registrations. The coverage is deliberate.
 * A wrapper over process.on alone sees 2 of the 4 registrations in a control that
 * registers one through each of on/addListener/prependListener/once, so the
 * narrower spy would have reported a false 0 exactly as convincingly. Adding an
 * exit listener here would put 7 under this "cannot co-occur" claim with nothing
 * behind it.
 *
 * `PANE_MUTATION_FAULT` is a debug-only fault injector (off by default), the
 * counterpart of `NEW_WT_FORCE_FAIL` in scripts/new-feature-worktree.sh. Each
 * value fires once and produces exactly one reportable condition:
 *   write-back — corrupt the file right after the harness writes it back, so the
 *               VOID guard is exercised by a real external write.
 *   mid-run    — make the file read-only with a mutation on disk, so the restore
 *               fails. This is the LOST_CONTROL path that used to leave the
 *               injection in the tree behind a bare stack.
 *   restore    — make the file read-only only for the final restore, leaving the
 *               15 cells to complete, so the two LOST_CONTROL causes are
 *               separately observable.
 *   backup     — make the store's *directory* read-only, which stops the safety
 *               backup being written while leaving every rewrite of the file
 *               itself working. This is the NO_SAFETY_NET refusal, and the only
 *               value that needs a directory: a read-only FILE cannot express it.
 *               A read-only directory is not expressible by the two values above
 *               either, and pointing them at the directory does not make them
 *               stricter — it deletes the fault. Measured, on a scratch dir:
 *               dir 555 / file 644 lets an overwrite through and refuses only
 *               create+unlink; file 444 / dir 755 refuses the overwrite. So the
 *               file-scoped injectors work (their write fails, visibly, exit 6)
 *               but can only ever reach faults that break writing the FILE, and
 *               this one breaks nothing until the run is already over.
 *
 * `--testTimeout=30000` is passed deliberately: several tests here drive a
 * never-settling query stream, and a busy machine must not turn CPU contention
 * into a false RED. Every mutation below is expected to be caught by a specific
 * named assertion, not by a timeout.
 */

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, unlinkSync, existsSync, chmodSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const STORE_REL = 'packages/frontend/src/stores/panelStore.ts';
const STORE = resolve(ROOT, STORE_REL);

const TESTS = [
  'packages/frontend/src/stores/__tests__/panelStore.test.ts',
  'packages/frontend/src/stores/__tests__/panelStore.panes.test.ts',
  'packages/frontend/src/stores/__tests__/paneFocusScope.tester.test.ts',
  'packages/frontend/src/stores/__tests__/paneFocusDifferentialRepro.tester.test.ts',
  'packages/frontend/src/stores/__tests__/paneFocusEntryLifecycle.tester.test.ts',
  'packages/frontend/src/stores/__tests__/paneP2ContractProbes.tester.test.ts',
  'packages/frontend/src/stores/__tests__/paneRealSequence.tester.test.ts',
  'packages/frontend/src/stores/__tests__/panelCloseNotifier.test.ts',
  'packages/frontend/src/hooks/__tests__/useQueryExec.pane.tester.test.tsx',
  'packages/frontend/src/windows/connection/__tests__/QueryPanel.paneRouting.test.tsx',
];

// The syncPaneFocus call site shared by removePanelsFor{Relation,Database}.
const REMOVE_SYNC = `      ...syncPaneFocus(
        s,
        remaining,
        activeStillExists ? activePanelId : (remaining.at(-1)?.id ?? null),
      ),`;
const REMOVE_PLAIN =
  '      activePanelId: activeStillExists ? activePanelId : (remaining.at(-1)?.id ?? null),';

// The syncPaneFocus call site shared by closePanelsTo{Left,Right}.
const CLOSESYNC_TO_RIGHT = `        panels: kept,
        queryExec: nextExec,
        ...syncPaneFocus(s, kept, activeStillExists ? s.activePanelId : panelId),
      };
    });
  },

  closePanelsToTheLeft: (panelId) => {`;
const CLOSE_TO_RIGHT = `        panels: kept,
        queryExec: nextExec,
        activePanelId: activeStillExists ? s.activePanelId : panelId,
      };
    });
  },

  closePanelsToTheLeft: (panelId) => {`;
const CLOSESYNC_TO_LEFT = `        panels: kept,
        queryExec: nextExec,
        ...syncPaneFocus(s, kept, activeStillExists ? s.activePanelId : panelId),
      };
    });
  },

  // ── Panes ─`;
const CLOSE_TO_LEFT = `        panels: kept,
        queryExec: nextExec,
        activePanelId: activeStillExists ? s.activePanelId : panelId,
      };
    });
  },

  // ── Panes ─`;

const MUTATIONS = [
  [
    'addPanel: drop syncPaneFocus (revert to a plain activePanelId write)',
    `      panels: [...s.panels, panel],
      queryExec: nextExec,
      // A freshly opened tab starts on its own default pane, so activating it
      // must not inherit the pane id the previously active tab had focused.
      ...syncPaneFocus(s, [...s.panels, panel], activate ? panel.id : s.activePanelId),`,
    `      panels: [...s.panels, panel],
      queryExec: nextExec,
      activePanelId: activate ? panel.id : s.activePanelId,`,
  ],
  [
    'removePanel: drop syncPaneFocus (no map pruning)',
    `      panels: nextPanels,
      queryExec: nextExec,
      ...syncPaneFocus(s, nextPanels, nextActive),`,
    `      panels: nextPanels,
      queryExec: nextExec,
      activePanelId: nextActive,`,
  ],
  [
    'removeAllForConnection: drop syncPaneFocus',
    `      panels: remaining,
      queryExec: nextExec,
      ...syncPaneFocus(s, remaining, nextActive),`,
    `      panels: remaining,
      queryExec: nextExec,
      activePanelId: nextActive,`,
  ],
  [
    'removePanelsForRelation: drop syncPaneFocus',
    `${REMOVE_SYNC}
    }));
  },

  removePanelsForDatabase: (connectionId, database, sessionDatabase) => {`,
    `${REMOVE_PLAIN}
    }));
  },

  removePanelsForDatabase: (connectionId, database, sessionDatabase) => {`,
  ],
  [
    'removePanelsForDatabase: drop syncPaneFocus',
    `${REMOVE_SYNC}
    }));
  },

  setActivePanel: (panelId) => {`,
    `${REMOVE_PLAIN}
    }));
  },

  setActivePanel: (panelId) => {`,
  ],
  [
    'setActivePanel: drop syncPaneFocus (mirror not re-derived)',
    '    set((s) => syncPaneFocus(s, s.panels, panelId ?? null));',
    '    set({ activePanelId: panelId ?? null });',
  ],
  [
    'closeOtherPanels: drop syncPaneFocus',
    `      panels: kept,
      queryExec: nextExec,
      ...syncPaneFocus(s, kept, panelId),`,
    `      panels: kept,
      queryExec: nextExec,
      activePanelId: panelId,`,
  ],
  [
    'closeAllPanels: drop syncPaneFocus',
    '    set((s) => ({ panels: [], queryExec: nextExec, ...syncPaneFocus(s, [], null) }));',
    '    set({ panels: [], queryExec: nextExec, activePanelId: null });',
  ],
  ['closePanelsToTheRight: drop syncPaneFocus', CLOSESYNC_TO_RIGHT, CLOSE_TO_RIGHT],
  ['closePanelsToTheLeft: drop syncPaneFocus', CLOSESYNC_TO_LEFT, CLOSE_TO_LEFT],
  [
    'setFocusedPane: revert to a single global write',
    '    set((cur) => syncPaneFocus(cur, cur.panels, cur.activePanelId, { panelId: target, paneId }));',
    '    set({ focusedPaneId: paneId || null });',
  ],
  [
    'openPane: revert to a single global write (and no activation)',
    `      const focus = syncPaneFocus(s, s.panels, panelId, { panelId, paneId });
      if (s.queryExec.has(key)) return focus;
      return { queryExec: new Map(s.queryExec).set(key, emptyQueryExecState()), ...focus };`,
    `      if (s.queryExec.has(key)) return s;
      return { queryExec: new Map(s.queryExec).set(key, emptyQueryExecState()), focusedPaneId: paneId };`,
  ],
  [
    'closePane: always clear the closed tab focus (even for an unfocused pane)',
    '          clearsFocus ? { panelId, paneId: null } : undefined,',
    '          { panelId, paneId: null },',
  ],
  [
    'closePane: never clear the focus of the tab it closed',
    `        queryExec: nextExec,
        ...syncPaneFocus(
          s,
          s.panels,
          s.activePanelId,
          clearsFocus ? { panelId, paneId: null } : undefined,
        ),`,
    `        queryExec: nextExec,
        ...syncPaneFocus(s, s.panels, s.activePanelId),`,
  ],
  [
    'reset: forget to clear focusedPaneIdByPanel',
    `      queryExec: new Map(),
      focusedPaneIdByPanel: {},
      focusedPaneId: null,`,
    `      queryExec: new Map(),
      focusedPaneId: null,`,
  ],
];

// Exit codes. 0 is the only one that means "this measurement is trustworthy".
const EXIT = {
  CLEAN: 0,
  UNCOVERED: 1,
  BASELINE: 2,
  WRITE_BACK: 3,
  INTERFERENCE: 4,
  VOID: 5,
  // The harness lost control of the store file: the mutation loop threw, or the
  // restore could not be written. 6 is above everything on purpose — a tree left
  // holding an injected mutation invalidates every number this run printed, which
  // is strictly worse than VOID (5, one unreadable cell) and worse than any
  // "the suite is not good enough" finding (1). It has to win the max(), or a
  // caller would read a trustworthy-looking "1 uncovered" for a run whose
  // working tree it must now go and check by hand.
  LOST_CONTROL: 6,
  // The safety backup could not be written, so the run refused to mutate anything
  // at all. Deliberately NOT 6: nothing has been written to the store, so the tree
  // does not hold an injection and there is nothing to go and check by hand —
  // reporting 6 here would overstate it. What 7 means is "this run produced no
  // measurement at all", the most complete absence of a result. It sits above 6
  // only as a tiebreak, and the tiebreak is never exercised: this path calls
  // process.exit, so the fold below never runs and never sees a 7 (measured —
  // with 6 already in the list the fold still does not execute, and neutering
  // that one call makes it run and yield 6). The ordering is a second, separate
  // reason: 7 exits before the first mutation, 6 needs one to have happened.
  // Header at the top says why the exit code itself is still safe to report.
  NO_SAFETY_NET: 7,
};
/**
 * A thrown value. Node's `catch` binding is `unknown` under this config, and the
 * only member read off it anywhere below is the ones a system error is defined
 * to have; anything else is stringified by `reason()`.
 * @typedef {{ code?: string, message?: string, stderr?: string | Buffer }} ThrownError
 */

/** @type {number[]} Every non-clean condition seen, folded by max() at the end. */
const exitReasons = [];
/**
 * Record a non-clean condition: printed on stderr, folded into the exit code.
 * @param {number} code
 * @param {string} why
 */
const fail = (code, why) => {
  exitReasons.push(code);
  console.error(`✖ ${why}`);
};

/**
 * The committed bytes of `rel` — the only source of truth for the base file.
 * @param {string} rel
 * @returns {Buffer}
 */
function headBlob(rel) {
  try {
    return execFileSync('git', ['show', `HEAD:${rel}`], {
      cwd: ROOT,
      maxBuffer: 64 * 1024 * 1024,
    });
  } catch (err) {
    console.error(`✖ cannot read the committed blob: git show HEAD:${rel}`);
    console.error(String(/** @type {ThrownError} */ (err).stderr ?? reason(err)));
    process.exit(EXIT.INTERFERENCE);
  }
}

/**
 * `err` as one quotable line. Keeps the code (EACCES) and drops the stack.
 * @param {unknown} err
 */
function reason(err) {
  return String(/** @type {ThrownError} */ (err)?.message ?? err);
}

/**
 * A short fingerprint of `buf` — enough to name a file's bytes in a bug report.
 * @param {Buffer | null} buf
 */
function fingerprint(buf) {
  if (buf === null) return '(unreadable)';
  const sum = createHash('sha256').update(buf).digest('hex').slice(0, 16);
  return `${buf.length} bytes, sha256:${sum}`;
}

/** Make `STORE` read-only. Returns true only if it actually did. */
function chmodReadOnly() {
  try {
    chmodSync(STORE, 0o444);
    return true;
  } catch {
    return false;
  }
}

/**
 * Make `STORE`'s directory read-only, so creating a file *in* it fails while
 * rewriting `STORE` itself still succeeds. That asymmetry is what separates this
 * from `chmodReadOnly()`: read-only on the FILE breaks every write, read-only on
 * the DIRECTORY breaks only the unlink and the backup. Returns true only if it
 * actually did.
 */
function chmodStoreDirReadOnly() {
  try {
    chmodSync(dirname(STORE), 0o555);
    return true;
  } catch {
    return false;
  }
}

/**
 * One vitest run over the whole TESTS list. Never throws: a failing suite comes
 * back as a non-zero `code`, because "the tests went red" is a result here, not
 * an error. `status` is the child's own exit code, and the 1 is Node's own
 * convention for a child that died without one — which is also why an *unrelated*
 * crash would be indistinguishable from a genuine red; the exit-code table is
 * what keeps that honest.
 * @returns {{ code: number, out: string }}
 */
function runTests() {
  try {
    return {
      code: 0,
      out: execFileSync('npx', ['vitest', 'run', '--testTimeout=30000', ...TESTS], {
        cwd: ROOT,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
        maxBuffer: 64 * 1024 * 1024,
      }),
    };
  } catch (err) {
    const e = /** @type {{ status?: number, stdout?: string, stderr?: string }} */ (err);
    return {
      code: e.status ?? 1,
      out: `${e.stdout ?? ''}${e.stderr ?? ''}`,
    };
  }
}

/**
 * The failing test names in one vitest run's output.
 * @param {string} out
 * @returns {string[]}
 */
const redTests = (out) =>
  out
    .split('\n')
    .map((/** @type {string} */ l) => l.trim())
    .filter((/** @type {string} */ l) => l.startsWith('×') || l.startsWith('✗'));

// PRISTINE is the committed file; PRE_DISK is what the working tree actually
// held. They are equal in a clean tree, and *comparing* them is the whole point:
// without that comparison the harness cannot tell "I restored my own snapshot"
// (true for whatever the snapshot was) from "the tree is where it should be".
const head = headBlob(STORE_REL);
const pristine = head.toString('utf8');
const preDisk = readFileSync(STORE);
const driftedBeforeRun = !preDisk.equals(head);

const backup = `${STORE}.mutation-backup`;
if (process.env.PANE_MUTATION_FAULT === 'backup') {
  // Debug-only: a read-only *directory* is the one fault that stops the backup
  // write while leaving every rewrite of the file itself working, so it is the
  // only way to reach NO_SAFETY_NET without breaking something else as well.
  chmodStoreDirReadOnly();
}
try {
  writeFileSync(backup, preDisk);
} catch (err) {
  // The backup is this harness's promise that it can hand the tree back. Without
  // it, there is nothing to fall back on, so it declines to mutate at all rather
  // than mutate on a promise it cannot keep. Reported, never thrown: this runs
  // before the mutation loop, so an uncaught throw here would exit 1 — read as
  // UNCOVERED, with no RESTORE CHECK and no summary printed at all.
  if (process.env.PANE_MUTATION_FAULT === 'backup') {
    try {
      chmodSync(dirname(STORE), 0o755);
    } catch {
      /* a directory left as the injector set it is stated below, not hidden */
    }
  }
  console.error(`✖ could not write the safety backup ${backup}: ${reason(err)}`);
  console.error(
    `  Every mutation below rewrites ${STORE_REL} in place, and this backup is the only\n` +
      `  way the harness can hand your bytes back if something goes wrong. Rather than\n` +
      `  mutate on a promise it cannot keep, it stopped before the first mutation.`,
  );
  console.error(
    `  Your working tree was not touched. Make the directory writable and re-run:\n` +
      `    ${STORE_REL} lives in ${dirname(STORE)}`,
  );
  process.exit(EXIT.NO_SAFETY_NET);
}

if (driftedBeforeRun) {
  console.log(
    `\nINTERFERENCE: ${STORE_REL} differs from HEAD before injection.\n` +
      `  The base file is taken from HEAD, so the mutations below would be applied to a\n` +
      `  code state nobody committed, and the resulting numbers would not be a measurement\n` +
      `  of the pane focus rule. Refusing to run; the file is left exactly as found.\n` +
      `  What is on disk right now was kept byte-for-byte here: ${backup}\n` +
      `  (manual restore: cp ${backup} ${STORE})`,
  );
  fail(
    EXIT.INTERFERENCE,
    `${STORE_REL} differs from HEAD before injection — see: git diff HEAD -- ${STORE_REL}`,
  );
}

// What every write-back in this script targets.
//
// Normal path: the committed file. The assertion above proved the startup bytes
// ARE the HEAD blob, so `head` and `preDisk` are the same bytes here — naming
// `head` states what is actually restored instead of leaving it implicit.
//
// INTERFERENCE path: `preDisk`, and **this arm is deliberately not a restore at
// all**. That run refused before injecting anything, so there is nothing to undo.
// A "unified" write-back of the HEAD blob here would delete the very drift the
// run just reported — reporting interference and then silently reverting it is a
// new way to destroy someone's uncommitted work. Writing `preDisk` is a no-op,
// and it is also the honest one. Do not collapse these two arms into one.
const restoreTarget = driftedBeforeRun ? preDisk : head;

const summary = [];
// The debug-only fault injectors fire once, so a single VOID or LOST_CONTROL
// report stays readable instead of turning a whole run into noise.
let faultFired = false;
let injectedMidRunFault = false;
let abortError = null;
try {
  if (driftedBeforeRun) {
    // Reported above. Running the baseline here would spend minutes measuring a
    // code state nobody committed, and the numbers could not be trusted
    // afterwards anyway, so the run stops here on purpose.
    console.log('(no baseline run, no mutation run — see INTERFERENCE above)');
  } else {
    const base = runTests();
    if (base.code !== 0) {
      console.error('!! baseline is not green — refusing to mutate. Stop and investigate.');
      console.error(base.out.slice(-4000));
      fail(EXIT.BASELINE, 'baseline (unmutated) pane suite is not green');
    } else {
      console.log('BASELINE (unmutated) exit=0 — all pane tests green\n');

      for (const [name, old, next] of MUTATIONS) {
        const hits = pristine.split(old).length - 1;
        if (hits !== 1) {
          console.log(`SKIP  ${name}: anchor matched ${hits} times (refusing to guess)`);
          summary.push([name, 'SKIP']);
          continue;
        }
        writeFileSync(STORE, pristine.replace(old, next));
        if (!faultFired && process.env.PANE_MUTATION_FAULT === 'mid-run') {
          // Debug-only: make the file read-only *with a mutation on disk*, so the
          // restore at the end of this cell throws. That is the case that used to
          // end in a bare stack with the injection still in the tree; it fires
          // once, like the write-back fault, so the report stays readable.
          faultFired = true;
          injectedMidRunFault = chmodReadOnly();
        }
        const { code, out } = runTests();
        const failed = redTests(out);
        const status = code !== 0 ? 'RED' : 'GREEN(!)';
        console.log(`${status.padEnd(8)} ${name}`);
        for (const f of failed.slice(0, 8)) console.log(`           ${f}`);
        if (code !== 0 && failed.length === 0) {
          console.log('           (non-zero exit with no test name captured — suspect a timeout)');
        }
        writeFileSync(STORE, restoreTarget);
        if (!faultFired && process.env.PANE_MUTATION_FAULT === 'write-back') {
          // Debug-only: impersonate an external writer that lands right after the
          // harness writes the file back, so the guard below is exercised by a
          // real external write instead of only by reasoning that one would break it.
          faultFired = true;
          writeFileSync(
            STORE,
            `${restoreTarget.toString('utf8')}// PANE_MUTATION_FAULT=write-back\n`,
          );
        }
        if (!readFileSync(STORE).equals(restoreTarget)) {
          // The write-back did not land (or something else touched the file right
          // after it did). A restore that silently failed must never be reported as
          // a caught regression, so this cell is VOID and the run exits non-zero.
          writeFileSync(STORE, restoreTarget);
          const recovered = readFileSync(STORE).equals(restoreTarget);
          console.log(
            `VOID      ${name} — the file was not what this run wrote back (re-written: ${recovered});` +
              ` the ${status} above cannot be read.`,
          );
          fail(EXIT.VOID, `write-back after "${name}" did not land`);
          summary.push([name, 'VOID']);
          continue;
        }
        summary.push([name, status]);
      }

      const voids = summary.filter(([, s]) => s === 'VOID');
      const holes = summary.filter(([, s]) => s !== 'RED' && s !== 'VOID');
      const caught = summary.length - holes.length - voids.length;
      console.log('\n===== SUMMARY =====');
      for (const [name, status] of summary) console.log(`  ${status.padEnd(8)} ${name}`);
      console.log(`\n${caught}/${summary.length} mutations caught`);
      if (voids.length) {
        console.log(
          `${voids.length} VOID — write-back verification failed, those results cannot be read.`,
        );
      }
      if (holes.length) {
        console.log('UNCOVERED (a defense hole — the suite cannot see this regression):');
        for (const [name] of holes) console.log(`  - ${name}`);
        fail(EXIT.UNCOVERED, `${holes.length} mutation(s) uncovered`);
      } else if (!voids.length) {
        console.log('0 uncovered — every focus-touching action is pinned.');
      }
    }
  }
} catch (err) {
  // The mutation loop threw — a failed write is the realistic case. Whatever was
  // on disk at this instant is still on disk, and the block below is the only
  // thing that can put it back. Swallowing here is deliberate: an exception
  // escaping the module would print a bare stack and force exit 1, which is
  // EXIT.UNCOVERED — a caller would read "1 mutation uncovered" when the truth
  // is "this run stopped early and may have left a mutation in the tree".
  abortError = err;
  console.log('\n!! the mutation loop threw — this run is INCOMPLETE and its numbers are void.');
  console.log(`   ${reason(err)}`);
  console.log('   The store file is put back by the block below, if that is still possible.');
} finally {
  // Last thing this process does to the working tree, and the one step whose
  // failure is worst, because everything above it assumes the file ends up as
  // `restoreTarget`.
  //
  // INVARIANT (local, and worth keeping): no statement in this block may throw.
  // A throw anywhere here escapes the module, so node prints a bare stack and
  // forces exit 1 regardless of process.exitCode — which is EXIT.UNCOVERED, so a
  // caller reads "1 mutation uncovered" for a run it must instead go and check by
  // hand. It also runs before the exit-code fold at the bottom, so a throw here
  // silently discards every exitReason pushed so far, including LOST_CONTROL.
  // Both were measured, not assumed: read-only on the file breaks the restore and
  // the mutations (bare stack, mutation left in the tree); read-only on the
  // *directory* breaks only the unlink, so the whole run succeeds and then dies
  // anyway, with a clean tree. So: every fs call below is individually guarded,
  // and a new bare one re-opens exactly the hole this is here to close.
  //
  // This is a statement about THIS BLOCK ONLY. It is not a claim about the file:
  // an exception raised anywhere else in this script still becomes a bare stack
  // and a forced exit 1. That is left as-is on purpose — a manual measurement
  // tool dying loudly on an unforeseen exception is the right default, and
  // guarding every fs call everywhere is unbounded work with no measured payoff.
  let injectedRestoreFault = false;
  if (process.env.PANE_MUTATION_FAULT === 'restore') {
    // Debug-only, and deliberately a *real* permission change: the writeFileSync
    // below then throws a real EACCES, so the guard is exercised by the same
    // failure it exists for rather than by a stubbed fs.
    injectedRestoreFault = chmodReadOnly();
  }

  let restoreError = null;
  try {
    writeFileSync(STORE, restoreTarget);
  } catch (err) {
    restoreError = err;
  }
  if (injectedMidRunFault || injectedRestoreFault) {
    // A fault injector may not leave a read-only file behind: the recovery
    // command printed below writes to this file, and a later run of the harness
    // would fail for an unrelated reason. (Measured, before this line existed:
    // the report said `cp <backup> <file>` and that `cp` died with EACCES.)
    // A file the *user* made read-only is left alone — that is not ours to undo.
    try {
      chmodSync(STORE, 0o644);
    } catch {
      /* the byte state below is the report */
    }
  }

  // Read back defensively: "cannot even read it" is one of the states this
  // report exists to describe, so it gets described instead of thrown.
  let now = null;
  try {
    now = readFileSync(STORE);
  } catch (err) {
    restoreError = restoreError ?? err;
  }
  const wroteBack = now !== null && now.equals(restoreTarget);
  const atHead = now !== null && now.equals(head);

  console.log('\n===== RESTORE CHECK (anchored to git, not to a disk re-read) =====');
  // When the final restore throws, `true` here means only that the last cell had
  // already put the file back — so the line must not read as "the restore worked".
  // Gated on `wroteBack` itself: annotating a `false` with a sentence about what
  // "the last cell already wrote" would print `false` and `true` on one line, and
  // the one place that matters most is the case where the tree is still holding an
  // injected mutation.
  const wroteBackNote = wroteBack && restoreError
    ? ' (true only because the last cell already wrote it — the final restore never landed)'
    : '';
  console.log(`  on disk == the bytes this run wrote back     : ${wroteBack}${wroteBackNote}`);
  console.log(`  on disk == git HEAD (${STORE_REL}) : ${atHead}`);
  console.log(`  on disk right now                           : ${fingerprint(now)}`);
  if (restoreError) {
    console.log(`  ✖ THE RESTORE ITSELF FAILED: ${reason(restoreError)}`);
    console.log(
      `    ${STORE_REL} was left exactly as it was when the write failed. It may be a MUTATED\n` +
        `    file rather than the committed one — check it before trusting this worktree.`,
    );
  }
  if (abortError) {
    console.log(`  ✖ the loop stopped early, so the counts above do not cover all ${MUTATIONS.length} cells.`);
  }
  if (!wroteBack) {
    // A failed restore must never delete the only copy of the good bytes.
    console.log(`  the file is NOT what this run wrote — its byte backup was kept: ${backup}`);
    console.log(`  recover with: cp ${backup} ${STORE}`);
  } else if (existsSync(backup)) {
    // Guarded like the rest, and for the same reason: this runs *before* the
    // exit-code fold below, so a throw here discards every exitReason pushed so
    // far and node forces exit 1 — i.e. UNCOVERED. Measured, not theorised: with
    // this one call left bare, making `packages/frontend/src/stores` read-only (the file itself
    // still writable, so the whole run succeeds) produced exactly that, a bare
    // `Error: EACCES ... unlink` stack and exit 1, for a run whose tree was
    // perfectly clean. A leftover backup is harmless — it is the startup bytes,
    // and the recovery instructions above point at it — so keep it and say so.
    try {
      unlinkSync(backup);
    } catch (err) {
      console.log(`  could not delete the now-redundant backup ${backup}: ${reason(err)}`);
      console.log(`  it was left on disk deliberately — delete it yourself when you are done.`);
    }
  }
  if (!atHead) {
    if (restoreError || abortError) {
      // Already described above, and calling it INTERFERENCE would be a lie: this
      // run is what put the difference there.
    } else if (driftedBeforeRun) {
      console.log(
        `  INTERFERENCE: ${STORE_REL} differs from HEAD — it was already drifted before the run; this run changed nothing.`,
      );
    } else {
      console.log(`  INTERFERENCE: ${STORE_REL} differs from HEAD.`);
    }
  }
  if (restoreError) {
    fail(EXIT.LOST_CONTROL, `${STORE_REL} could not be restored: ${reason(restoreError)}`);
  } else if (abortError) {
    fail(EXIT.LOST_CONTROL, `the mutation loop threw: ${reason(abortError)}`);
  } else if (!wroteBack) {
    fail(EXIT.WRITE_BACK, `${STORE_REL} is not the content this run wrote back`);
  }
  process.exitCode = exitReasons.length ? Math.max(...exitReasons) : EXIT.CLEAN;
}
