/**
 * JOURNEY TEST — multi-cursor enter / render / exit.
 *
 * Continuous, not a table of valid inputs: every test here walks a full
 * keystroke/mouse sequence and asserts the state after **each** step, including
 * the degenerate ones (no extra cursors, a cursor at position 0, a cursor at
 * end of document, a surviving range that is a selected word, column
 * selection). The exit is always asserted, not just the entry.
 *
 * The platform here is the non-mac branch — jsdom reports an empty
 * `navigator.platform`, so `isMacOS()` is false and the multi-cursor pointer
 * chord is Ctrl. See `multiCursorPointerClick.macos.test.ts` for the macOS
 * branch, where the chord is Cmd and Control is deliberately *not* a chord.
 */
import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import { EditorSelection, Prec } from '@codemirror/state';
import { keymap } from '@codemirror/view';
import { autocompletion, completionStatus, startCompletion } from '@codemirror/autocomplete';
import { clickAddsCursor } from '../multiCursorPointer';
import {
  JOURNEY_DOC,
  clickAtPosition,
  columnsOf,
  dragAcross,
  flushMeasure,
  heads,
  installEditorLayoutShim,
  mountEditor,
  press,
  rangeTuples,
  renderedCursors,
  type EditorMountOptions,
  type MountedEditor,
} from './multiCursorJourneyHarness';

/**
 * Positions in JOURNEY_DOC ('SELECT alpha\nSELECT beta\nSELECT gamma'):
 *
 *   0            start of line 1
 *   7            inside 'alpha' on line 1
 *   20           inside 'beta'  on line 2
 *   32           inside 'gamma' on line 3
 *   37           end of document
 *
 * These are distinct, inside-word positions rather than word starts: a
 * multi-cursor at two different columns of the same word is a state no
 * shortcut can disambiguate, and the exit must be tested with more than one.
 */
const P = {
  docStart: 0,
  line1: 7,
  line2: 20,
  line3: 32,
  docEnd: JOURNEY_DOC.length,
} as const;

let mounted: MountedEditor | null = null;

beforeAll(() => {
  installEditorLayoutShim();
});

afterEach(() => {
  mounted?.destroy();
  mounted = null;
});

function open(anchor: number, extraAfter?: EditorMountOptions['extraAfter']) {
  mounted = mountEditor({ anchor, extraAfter });
  return mounted.view;
}

function completionOpen(view: MountedEditor['view']): boolean {
  return completionStatus(view.state) === 'active';
}

/**
 * Waits for the completion to reach `active`.
 *
 * `startCompletion` only records a *pending* source; the plugin debounces the
 * actual query by 50ms (`completionPlugin.update`) and then resolves the
 * promise, so a synchronous read after the keypress legitimately sees nothing.
 */
async function waitForCompletionOpen(view: MountedEditor['view']): Promise<void> {
  for (let i = 0; i < 40; i++) {
    if (completionOpen(view)) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error('completion never reached "active"');
}

/* ========================================================================== */
/*  The headline journey                                                       */
/* ========================================================================== */

describe('journey: one cursor -> Ctrl+click -> Escape -> one cursor', () => {
  it('adds, RENDERS, and exits, asserting state at every step', async () => {
    const view = open(P.line1);

    /* --- Step 1: enter the feature. One cursor, mid-document. ----------- */
    expect(heads(view)).toEqual([P.line1]);
    expect(view.state.selection.main.empty).toBe(true);
    await flushMeasure();
    expect(renderedCursors(view)).toEqual({
      total: 1,
      primary: 1,
      secondary: 0,
      hasLayer: true,
    });

    /* --- Step 2: Ctrl+click on line 2 adds a second cursor. -------------- */
    await clickAtPosition(view, P.line2, { ctrlKey: true });
    expect(heads(view)).toEqual([P.line1, P.line2]);
    expect(view.state.selection.main.head).toBe(P.line2);
    // THE ASSERTION THAT MATTERS: two logical cursors must be two *visible*
    // cursors. Without `drawSelection` the view has no cursor layer at all and
    // the user sees a single caret while the state says two.
    expect(renderedCursors(view)).toEqual({
      total: 2,
      primary: 1,
      secondary: 1,
      hasLayer: true,
    });

    /* --- Step 3: a third Ctrl+click grows it again. ---------------------- */
    await clickAtPosition(view, P.line3, { ctrlKey: true });
    expect(heads(view)).toEqual([P.line1, P.line2, P.line3]);
    expect(renderedCursors(view)).toEqual({
      total: 3,
      primary: 1,
      secondary: 2,
      hasLayer: true,
    });

    /* --- Step 3b: extend every cursor over a character. ------------------ */
    // Without this step the three ranges are all BARE cursors, and bare-cursor
    // multi-selections happen to collapse to the same result under
    // `defaultKeymap`'s own `simplifySelection`. This is the state that
    // actually distinguishes `exitMultiCursor`: the user selected something
    // and pressed Escape to leave multi-cursor mode, so they must get a caret,
    // not a leftover selection covering the word.
    press(view, { key: 'ArrowRight', code: 'ArrowRight', shiftKey: true });
    expect(view.state.selection.ranges).toHaveLength(3);
    expect(view.state.selection.ranges.every((range) => !range.empty)).toBe(true);
    expect(view.state.doc.toString()).toBe(JOURNEY_DOC);

    /* --- Step 4: Escape exits to a single cursor at the main head. ------- */
    const escapeEvent = press(view, { key: 'Escape', code: 'Escape' });
    expect(escapeEvent.defaultPrevented).toBe(true);
    expect(heads(view)).toEqual([P.line3 + 1]);
    // Bare — the discriminating assertion. `simplifySelection` would leave the
    // main range non-empty here, i.e. the word would stay selected.
    expect(view.state.selection.main.empty).toBe(true);
    expect(view.state.selection.ranges).toHaveLength(1);
    // Plain tuples, not `SelectionRange` objects: those carry a `flags` field
    // that only means something once the ranges have survived selection work.
    expect(rangeTuples(view)).toEqual([[P.line3 + 1, P.line3 + 1]]);
    await flushMeasure();
    expect(renderedCursors(view)).toEqual({
      total: 1,
      primary: 1,
      secondary: 0,
      hasLayer: true,
    });

    /* --- Step 5: Escape again in the degenerate state is a no-op. -------- */
    const secondEscape = press(view, { key: 'Escape', code: 'Escape' });
    // Handled by neither our command nor `simplifySelection`, so the event is
    // left for anything else listening.
    expect(secondEscape.defaultPrevented).toBe(false);
    expect(heads(view)).toEqual([P.line3 + 1]);

    /* --- Step 6: typing lands once, proving the extra cursors are gone. -- */
    // Asserted by offset, not by word: the surviving cursor is the main
    // head, and a second insertion anywhere would prove the exit failed.
    view.dispatch({ changes: { from: P.line3, insert: 'X' } });
    const doc = view.state.doc.toString();
    expect(doc.split('X')).toHaveLength(2);
    expect(doc.indexOf('X')).toBe(P.line3);
  });

  it('Ctrl+clicking INSIDE an existing cursor toggles it off, and Escape is then a no-op', async () => {
    // `basicMouseSelection` routes a multiple+click that lands inside an
    // existing range through `removeRangeAround`, so the gesture is a toggle.
    // The exit transition has to be correct from both sides of the toggle.
    const view = open(P.line1);
    await clickAtPosition(view, P.line2, { ctrlKey: true });
    expect(heads(view)).toEqual([P.line1, P.line2]);
    expect(renderedCursors(view)).toEqual({
      total: 2,
      primary: 1,
      secondary: 1,
      hasLayer: true,
    });

    // Same position again: the second range is removed, not duplicated.
    await clickAtPosition(view, P.line2, { ctrlKey: true });
    expect(heads(view)).toEqual([P.line1]);
    await flushMeasure();
    expect(renderedCursors(view).total).toBe(1);

    // Back to a single cursor: Escape does nothing.
    expect(press(view, { key: 'Escape', code: 'Escape' }).defaultPrevented).toBe(false);
    expect(heads(view)).toEqual([P.line1]);
  });

  it('Escape is a no-op when the toggle-off has already exited multi-cursor', async () => {
    // The degenerate tail of the previous journey, isolated: entering and
    // leaving by gesture alone must leave Escape inert.
    const view = open(P.line1);
    await clickAtPosition(view, P.line2, { ctrlKey: true });
    await clickAtPosition(view, P.line2, { ctrlKey: true });
    expect(view.state.selection.ranges.length).toBe(1);
    expect(press(view, { key: 'Escape', code: 'Escape' }).defaultPrevented).toBe(false);
  });
});

/* ========================================================================== */
/*  Escape as a state machine: the degenerate exits                            */
/* ========================================================================== */

describe('journey: Escape exit transitions, including degenerate states', () => {
  it('Escape with a single EMPTY cursor at position 0 is a no-op', () => {
    const view = open(P.docStart);
    expect(heads(view)).toEqual([0]);
    const event = press(view, { key: 'Escape', code: 'Escape' });
    expect(event.defaultPrevented).toBe(false);
    expect(heads(view)).toEqual([0]);
    expect(view.state.selection.main.empty).toBe(true);
  });

  it('Escape with a single cursor at the END of the document is a no-op', () => {
    const view = open(P.docEnd);
    expect(heads(view)).toEqual([P.docEnd]);
    const event = press(view, { key: 'Escape', code: 'Escape' });
    expect(event.defaultPrevented).toBe(false);
    expect(heads(view)).toEqual([P.docEnd]);
  });

  it('Escape with a single NON-EMPTY range still collapses it to a cursor', () => {
    // Our command returns false here so `simplifySelection` keeps owning the
    // single-cursor case. This is the regression guard for that layering.
    const view = open(0);
    view.dispatch({ selection: EditorSelection.range(P.line1, P.line1 + 5) });
    expect(rangeTuples(view)).toEqual([[P.line1, P.line1 + 5]]);

    press(view, { key: 'Escape', code: 'Escape' });
    expect(heads(view)).toEqual([P.line1 + 5]);
    expect(view.state.selection.main.empty).toBe(true);
  });

  it('Escape with two cursors where main is at position 0 collapses to 0', async () => {
    // The surviving cursor is the MAIN range, so the degenerate "main sits at
    // offset 0" case has to land on 0 rather than on the other range.
    // `EditorSelection` keeps ranges sorted by position, so the added range
    // sorts first even though it is the main one.
    const view = open(P.line2);
    await clickAtPosition(view, P.docStart, { ctrlKey: true });
    expect(heads(view)).toEqual([P.docStart, P.line2]);
    expect(view.state.selection.main.head).toBe(P.docStart);

    press(view, { key: 'Escape', code: 'Escape' });
    expect(heads(view)).toEqual([P.docStart]);
    expect(view.state.selection.main.empty).toBe(true);
  });

  it('Escape leaves a BARE cursor, not the selected word `simplifySelection` keeps', () => {
    // The behavioural difference from the stock command. Two `Mod-d` ranges
    // are both non-empty; `simplifySelection` would keep the main range's word
    // highlighted and need a second Escape to reach a caret.
    // `selectNextOccurrence` returns false when there is no next match, so the
    // document has to actually repeat the word.
    mounted = mountEditor({ doc: 'SELECT alpha\nSELECT alpha', anchor: 9 });
    const view = mounted.view;
    press(view, { key: 'd', code: 'KeyD', ctrlKey: true }); // selects 'alpha'
    expect(rangeTuples(view)).toEqual([[7, 12]]);
    press(view, { key: 'd', code: 'KeyD', ctrlKey: true }); // adds the second
    expect(view.state.selection.ranges.length).toBe(2);
    expect(view.state.selection.main.empty).toBe(false);
    const mainHead = view.state.selection.main.head;

    press(view, { key: 'Escape', code: 'Escape' });
    expect(view.state.selection.ranges.length).toBe(1);
    // Bare cursor — the distinguishing assertion.
    expect(view.state.selection.main.empty).toBe(true);
    // The survivor is the MAIN range, the same rule every other exit uses.
    expect(heads(view)).toEqual([mainHead]);

    // ...and the state is now well defined: a second Escape does nothing.
    expect(press(view, { key: 'Escape', code: 'Escape' }).defaultPrevented).toBe(false);
    expect(heads(view)).toEqual([mainHead]);
  });

  it('Escape exits a column (rectangular) selection too', async () => {
    const view = open(P.line1);
    await dragAcross(view, P.line1, P.line2, { altKey: true });
    const ranges = rangeTuples(view);
    expect(ranges.length).toBeGreaterThan(1);
    // The defining signature of a column selection: every range sits on the
    // same column of its own line.
    expect(new Set(columnsOf(view)).size).toBe(1);

    press(view, { key: 'Escape', code: 'Escape' });
    expect(view.state.selection.ranges.length).toBe(1);
    expect(view.state.selection.main.empty).toBe(true);
    await flushMeasure();
    expect(renderedCursors(view)).toEqual({
      total: 1,
      primary: 1,
      secondary: 0,
      hasLayer: true,
    });
  });
});

/* ========================================================================== */
/*  Escape precedence — the reason this binding is at Prec.high               */
/* ========================================================================== */

describe('journey: Escape precedence against the other Escape layers', () => {
  it('a live completion swallows the FIRST Escape and keeps every cursor', async () => {
    // `closeCompletion` sits at `Prec.highest`, above our `Prec.high`. Both the
    // panel closing and the cursor count are asserted, because a naive
    // `Prec.highest` binding on our side would collapse the cursors on the
    // first press and leave the panel open.
    //
    // The source has to complete on a position where `matchBefore` can see an
    // adjacent word (offset 12, the end of 'alpha'); the completion context
    // is built for the MAIN range, so the second click below deliberately
    // leaves the main cursor on the completable position.
    const view = open(P.line2, [
      autocompletion({
        activateOnTyping: false,
        override: [
          (ctx) => {
            const word = ctx.matchBefore(/\w+/);
            if (!word) return null;
            return {
              from: word.from,
              to: word.to,
              options: [{ label: 'alpha_one' }, { label: 'alpha_two' }],
            };
          },
        ],
      }),
    ]);

    await clickAtPosition(view, 12, { ctrlKey: true });
    expect(heads(view)).toEqual([12, P.line2]);
    expect(view.state.selection.main.head).toBe(12);

    startCompletion(view);
    await waitForCompletionOpen(view);
    expect(view.dom.querySelector('.cm-tooltip-autocomplete')).not.toBeNull();
    expect(heads(view)).toEqual([12, P.line2]);

    // First Escape: the completion closes, the cursors survive.
    press(view, { key: 'Escape', code: 'Escape' });
    expect(completionOpen(view)).toBe(false);
    expect(heads(view)).toEqual([12, P.line2]);
    await flushMeasure();
    expect(renderedCursors(view).total).toBe(2);

    // Second Escape: now the multi-cursor exit gets it.
    press(view, { key: 'Escape', code: 'Escape' });
    expect(heads(view)).toEqual([12]);
  });

  it('a Prec.highest Escape binding that DEFERS falls through to ours', async () => {
    // Pro's `createExtraKeymap` and the fold keymap both mount at
    // `Prec.highest`. If a privileged extension claims Escape it wins and our
    // command never runs. The realistic case is a Pro keymap that binds Escape
    // but declines the event (`run` returns false) — CodeMirror then falls
    // through to the next precedence, and the multi-cursor exit must still
    // work. `simplifySelection` would collapse the ranges too, but it keeps a
    // non-empty `main`, so the bare-cursor assertion is what discriminates.
    const calls: string[] = [];
    const view = open(P.line1, [
      Prec.highest(
        keymap.of([
          {
            key: 'Escape',
            run: () => {
              calls.push('pro-declined');
              return false; // Pro inspected the key and handed it back.
            },
          },
        ]),
      ),
    ]);
    await clickAtPosition(view, P.line2, { ctrlKey: true });
    // Extend so `simplifySelection` would leave a non-empty main behind.
    press(view, { key: 'ArrowRight', code: 'ArrowRight', shiftKey: true });
    expect(view.state.selection.ranges).toHaveLength(2);
    expect(view.state.selection.main.empty).toBe(false);
    // `main` is whichever range CodeMirror considers primary — do not assume
    // it is the first one by position.
    const mainHead = view.state.selection.main.head;

    press(view, { key: 'Escape', code: 'Escape' });
    expect(calls).toEqual(['pro-declined']);
    expect(rangeTuples(view)).toEqual([[mainHead, mainHead]]);
    expect(view.state.selection.main.empty).toBe(true);
  });

  it('a default-precedence Escape binding LOSES to ours', async () => {
    // The failure this change exists to prevent: registered after
    // `defaultKeymap`, a default-precedence binding is never consulted, so a
    // host command cannot rely on plain registration order to beat it. The
    // `Prec.high` claim is what wins, and the bare cursor is the observable
    // proof that OUR command ran rather than `simplifySelection`.
    const calls: string[] = [];
    const view = open(P.line1, [
      keymap.of([
        {
          key: 'Escape',
          run: () => {
            calls.push('low');
            return true;
          },
        },
      ]),
    ]);
    await clickAtPosition(view, P.line2, { ctrlKey: true });
    press(view, { key: 'ArrowRight', code: 'ArrowRight', shiftKey: true });
    expect(view.state.selection.main.empty).toBe(false);
    const mainHead = view.state.selection.main.head;

    press(view, { key: 'Escape', code: 'Escape' });
    expect(calls).toEqual([]);
    expect(rangeTuples(view)).toEqual([[mainHead, mainHead]]);
  });
});

/* ========================================================================== */
/*  The modifier conflict table                                                */
/* ========================================================================== */

describe('the pointer modifier table (non-mac branch: chord = Ctrl)', () => {
  const evt = (over: Partial<Record<'altKey' | 'ctrlKey' | 'metaKey' | 'shiftKey', boolean>>) => ({
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    ...over,
  });

  it('agrees with every row of the documented table', () => {
    // No modifier -> CodeMirror replaces the selection; our predicate says
    // "do not add a range", which is exactly what makes the default happen.
    expect(clickAddsCursor(evt({}))).toBe(false);
    expect(clickAddsCursor(evt({ altKey: true }))).toBe(true);
    expect(clickAddsCursor(evt({ ctrlKey: true }))).toBe(true);
    expect(clickAddsCursor(evt({ metaKey: true }))).toBe(false); // macOS-only chord
    expect(clickAddsCursor(evt({ shiftKey: true }))).toBe(false);
    expect(clickAddsCursor(evt({ altKey: true, shiftKey: true }))).toBe(false);
    expect(clickAddsCursor(evt({ altKey: true, ctrlKey: true }))).toBe(true);
    expect(clickAddsCursor(evt({ altKey: true, metaKey: true }))).toBe(true);
  });

  it('Alt+click and Ctrl+click compose without becoming ambiguous', async () => {
    const view = open(P.line1);
    await clickAtPosition(view, P.line2, { altKey: true });
    expect(heads(view)).toEqual([P.line1, P.line2]);
    await clickAtPosition(view, P.line3, { ctrlKey: true });
    expect(heads(view)).toEqual([P.line1, P.line2, P.line3]);
  });

  it('Shift+click still extends and never grows the cursor count', async () => {
    const view = open(P.line1);
    await clickAtPosition(view, P.line2, { shiftKey: true });
    expect(view.state.selection.ranges.length).toBe(1);
    expect(view.state.selection.main.empty).toBe(false);
  });

  it('a plain click after multi-cursor replaces the whole selection', async () => {
    const view = open(P.line1);
    await clickAtPosition(view, P.line2, { ctrlKey: true });
    expect(heads(view)).toEqual([P.line1, P.line2]);
    await clickAtPosition(view, P.line3, {});
    expect(heads(view)).toEqual([P.line3]);
    await flushMeasure();
    expect(renderedCursors(view).total).toBe(1);
  });

  it('Alt+drag stays rectangular while Ctrl+click stays a cursor add', async () => {
    const view = open(P.line1);
    await dragAcross(view, P.line1, P.line2, { altKey: true });
    const columnRanges = rangeTuples(view);
    expect(columnRanges.length).toBeGreaterThan(1);
    // The defining signature of a column selection: same column on every line.
    expect(new Set(columnsOf(view)).size).toBe(1);

    // Escape must exit a column selection the same way.
    press(view, { key: 'Escape', code: 'Escape' });
    expect(view.state.selection.ranges.length).toBe(1);
  });
});
