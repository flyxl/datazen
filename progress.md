# P5 comment cleanup — UI track (`feature/p5-comment-cleanup-ui`)

**Track scope:** strip spec/document/line-number citations from comments and string literals in
`src/**` and `e2e/**` (TypeScript only). No Rust, no `src-tauri/**`, no `packages/**`,
no `scripts/**`, no `docs/**`, no `*.config.ts`. **No cargo command was executed.**

> This file is a development ledger only. Per `AGENTS.md` it must be **deleted at merge
> acceptance** and must not survive onto `main`.

---

## 1. Result summary

| Item | Value |
|---|---|
| Branch | `feature/p5-comment-cleanup-ui` |
| Baseline HEAD | `630b464aa0281b8862a4f667d13829182ce7ac94` |
| Files modified | **161** (`src/**` 131, `e2e/**` 30) |
| Lines | **721 insertions / 727 deletions** |
| Scanned files | **1480** (1309 `src` `.ts`/`.tsx` excluding `src/extensions/generated*` + 171 `e2e/*.ts`) |
| Residual candidates | **577 → 18** |
| Executable code changed | **none** (Gate 1 `both` mode diff = 0 lines) |

---

## 2. Gate 1 — comment-stripped code is byte-identical

**Method.** The project's TypeScript compiler API is the parser. Comments are located by an
AST trivia pass **plus** a token-gap pass (scanning the gap between every pair of adjacent
real tokens), because a comment sitting between a union/intersection constituent and its `|`
separator is leading trivia of *no* node and is therefore missed by a pure
`getLeadingCommentRanges` walk. The token-gap pass is immune to `//` occurring inside strings
and templates. Each covered range is replaced by `" " + raw.replace(/[^\n\r]/g, "")`
(length-independent), then `dropBlankLines()` canonicalisation.

Two modes are produced per file:

- **`comments`** — only comments blanked. String payloads remain **verbatim**.
- **`both`** — comments blanked **and** every string/template literal replaced with `<str>`.

**Gate semantics.** Mode `comments` is the strictly stronger statement: an empty diff there
proves *only comments and string-literal text moved*. Mode `both` erases string payloads by
design, so string edits are invisible in it; every string edit is therefore declared per file
in §4.

### Verbatim conclusion lines (final run)

```
GATE1_EXIT=0
STRIP_EXIT[comments]=0 FILES=STRIPPED_FILES=1480
GATE1_DIFF_LINES[comments]=904
STRIP_EXIT[both]=0 FILES=STRIPPED_FILES=1480
GATE1_DIFF_LINES[both]=0
```

**`GATE1_DIFF_LINES[both]=0` — the code is byte-identical before and after.** No expression,
structure, control flow, type, attribute, or import was touched.

### The 904 `comments`-mode lines are string-literal payloads

50 files, **214 removed / 214 added** content lines. A targeted scan for risky string edits
over that diff:

```
RISK_COUNT=0        (for toBe( | toEqual( | toContain( | toStrictEqual( | assert | expect()
```

Sampled lines are cosmetic test labels only, e.g. `describe('planJourneys (F2 core)')`,
`it('CTX-001: …')`. These are the same e2e-id cosmetic edits mandated by the criterion
`CM-XX 断言N：` → `断言N：`.

> **Correction (added after review):** an earlier draft of this line cited
> `it('搜索框应能过滤连接 - 无匹配 (CM-008)')` as a surviving title. That was **wrong** —
> re-measured on the committed tree, `CM-008` has **0 occurrences** in `src`/`e2e`.
> The claim has been removed rather than left standing.

### Stripper self-check ("input unchanged, output unchanged")

Run on **both** the edited tree and the pristine baseline copy:

```
SELFCHECK_FILES=1480  DETERMINISM_FAIL=0  IDEMPOTENCE_FAIL=0  AST_SHAPE_FAIL=0  TOKEN_FAIL=0
```

`SELFCHECK_FILES=1480` = the checker read exactly as many files as the stripper wrote.
Determinism = same input ⇒ same output. Idempotence = stripping twice ≡ stripping once.
`AST_SHAPE` = node-kind sequence unchanged. `TOKEN_FAIL` = parser leaf-token
(`kind` + verbatim text) sequence unchanged.

### Negative control (proves the checks can fail)

```
NEGCTRL_PASS=19 NEGCTRL_FAIL=0 NEGCTRL_RESULT=PASS
```

19 deliberately mutated files are correctly detected as changed; zero are falsely reported
identical.

> **Note on Gate 1's blind spot.** Gate 1 is structurally insensitive to an `eslint-disable`
> / `@ts-ignore` added *inside a comment*, because that is a comment-only change and Gate 1
> deliberately ignores comments in `both` mode. That is by design, not a loophole: the
> suppression counts are enforced separately in §5 (Gate-2 evidence) and are **unchanged from
> baseline**. Nothing was made invisible to the checker in order to pass.

---

## 3. Gate 2 — residual scan decreases, every survivor has a keep-reason

The residual scanner was **not** weakened: identical rule set, identical scope, identical file
list (`/tmp/dz-tools/files.txt`, 1480 files). Rule families unchanged —
`SECTION_SYM`, `CM_ID`, `DECISION_ID`, `INV_ID`, `DOCS_PATH`, `MD_FILE`, `RS_LINE`, `LNN`
(core 8) plus `DOC_STYLE`, `PHASE_NUM`, `F_NUM`, `RULE_NUM` (secondary).

### Verbatim conclusion lines

```
SCAN_EXIT=0
HIT_TOTAL=18  HIT_LINES=18  COMMENT_HITS=6  STRING_HITS=12  HIT_FILES=9
BY_RULE={"LNN":1,"MD_FILE":6,"F_NUM":11}
```

| | baseline | final |
|---|---|---|
| `HIT_TOTAL` | 577 | **18** |
| `HIT_LINES` | 514 | **18** |
| `COMMENT_HITS` | 407 | **6** |
| `STRING_HITS` | 170 | **12** |
| `HIT_FILES` | 165 | **9** |

`SECTION_SYM`, `CM_ID`, `DECISION_ID`, `INV_ID`, `DOCS_PATH`, `PHASE_NUM` are all **zero**.

> **On the ~295 figure.** The brief's residual scan measures **~295 lines**; this track
> measures **340 hits / 287 lines** for the same core-8 comment families under the
> comments-only 口径 the coordinator uses. The 287-line figure is the one that reconciles
> with the coordinator; 340 counts hits rather than distinct lines. Both numbers describe the
> *pre-cleanup* state. Final state is 18 under either 口径. Per the brief, the candidate count
> is **not** the violation count and was never used as a target.

### 3.1 Survivor classification — all 18, item by item

| # | File:line | Rule / kind | Matched | Keep-reason (why this is not a violation) |
|---|---|---|---|---|
| 1 | `e2e/lib/schemaDiffFixtures.ts:447` | `LNN` / string | `"L4"` | A **job-tier code** (`'L4'`) inside a SQL `INSERT` fixture's JSON. `L4` is the data being inserted, not a line-number citation. Deleting it would corrupt the fixture. |
| 2 | `e2e/specs/ai-context.ts:44` | `MD_FILE` / string | `/relations.md` | The test **writes** this file (`fs.writeFileSync(..., '/relations.md')`). It is a fixture path created by the test, not a documentation pointer. |
| 3 | `e2e/specs/ai-context.ts:67` | `MD_FILE` / string | `relations.md` | `expect(names).toContain('relations.md')` — an **assertion payload**. Removing the substring deletes the assertion's meaning and breaks the test. |
| 4 | `src/components/ai/__tests__/ContextPicker.test.tsx:320` | `MD_FILE` / comment | `.md` | `.md` is the **real file extension** the icon-selection logic branches on (`.ctx.yaml` → Layers icon, `.md` → File icon). It explains the code; it cites nothing. |
| 5 | `src/components/ai/__tests__/ContextPicker.test.tsx:294` | `MD_FILE` / string | `notes.md` | Value of a mock `ContextEntry` fixture — sample data, not a citation. |
| 6 | `src/components/ai/__tests__/ContextPicker.test.tsx:311` | `MD_FILE` / string | `notes.md` | Same mock fixture, second entry. |
| 7 | `src/components/sql-editor/paste/__tests__/tester_multiCursorMatrix.test.ts:539` | `F_NUM` / comment | `F5：` | `F5` is a **physical keyboard function key**. The comment explains *why* the test dispatches F5 (Ctrl+A would be swallowed by `defaultKeymap`). Same entity as the `key: 'F5'` literals on lines 544–545. |
| 8 | `tester_multiCursorMatrix.test.ts:544` | `F_NUM` / string | `F5` | `key: 'F5'` — a literal `KeyboardEvent` constructor argument. |
| 9 | `tester_multiCursorMatrix.test.ts:545` | `F_NUM` / string | `F5` | `code: 'F5'` — same literal, second field. |
| 10 | `src/lib/__tests__/keymap.test.ts:34` | `F_NUM` / string | `F5` | `'F5'` shortcut string the keymap parser consumes. |
| 11 | `src/lib/__tests__/keymap.test.ts:35` | `F_NUM` / string | `F5` | Same, second case. |
| 12 | `src/lib/__tests__/keymap.test.ts:45` | `F_NUM` / string | `-F3` | `'Ctrl-F3'` — shortcut string. |
| 13 | `src/lib/__tests__/keymap.test.ts:54` | `F_NUM` / string | `-F3` | `'ctrl+f3'` — normalised form of the same shortcut. |
| 14 | `src/lib/__tests__/wappThemes.test.ts:218` | `MD_FILE` / string | `README.md` | `readFileSync(join(WAPPS_ROOT, 'README.md'))` — the test **reads this repository file** under `describe('extensions README contract')`. It is an I/O target, not a citation. |
| 15 | `src/lib/keymap.ts:63` | `F_NUM` / comment | `-F3"` | Docstring **format sample**: `"Mod-Enter", "Ctrl-F3"`. Documents the accepted shortcut grammar — substantive documentation, not a citation. |
| 16 | `src/windows/workspace/WappPageShell.tsx:100` | `F_NUM` / comment | `F6` | **PENDING ADJUDICATION** — `// F6 RPC bridge: one attach per iframe instance (key changes on reload).` See §6.1. |
| 17 | `src/windows/workspace/WappPageShell.tsx:205` | `F_NUM` / comment | `F6` | **PENDING ADJUDICATION** — `{/* F6 (message bridge): attached in the effect above; … */}`. See §6.1. |
| 18 | `src/windows/workspace/WorkspaceView.tsx:79` | `F_NUM` / comment | `F6.` | **PENDING ADJUDICATION** — `` // `params` is stored with the tab by the bridge consumer in F6. `` See §6.1. |

**Tally: 15 verified keeps + 3 pending adjudication.** All 15 keeps are genuine code/data
payloads — a job-tier literal, fixture paths the tests create and assert on, keyboard-key
literals, a repository file the test reads, and a docstring format sample. **None was kept to
improve a number.**

---

## 4. Mandatory gates (run on final HEAD, tree sha unchanged across the run)

Recorded before and after: `HEAD=630b464aa0281b8862a4f667d13829182ce7ac94`,
`TREE_SHA=39b0d01e70aeb059f3f0ac4ed573367c4a87e8d0` — **identical before and after every gate
run**, proving no validator mutated the tree and no submitter moved underneath the validator.

```
$ pnpm typecheck
ok: all 1 finally block(s) of scripts/mutation-check-pane-focus.mjs are guarded, and nothing there throws.
$ tsc -p tsconfig.scripts.json --noEmit
$ tsc -p tsconfig.pack-ep.json --noEmit
TYPECHECK_EXIT=0        (error TS count = 0)
```

```
$ npx vitest run src e2e
 Test Files  522 passed (522)
      Tests  5225 passed (5225)
VITEST_EXIT=0
```

---

## 5. Supporting audits

### Suppression audit (proof no gate was turned off)

```
SUPP_EXIT=0
ESLINT_DISABLE=45   TS_IGNORE=2   AS_ANY=119   ALLOW_ATTR=0   EXPECT_CALLS=14581
```

| Metric | baseline (HEAD, old stripper) | HEAD (current stripper) | worktree |
|---|---|---|---|
| `ESLINT_DISABLE` | 45 | 45 | 45 |
| `TS_IGNORE` | — | 2 | 2 |
| `AS_ANY` | 119 | 119 | 119 |
| `ALLOW_ATTR` | — | 0 | 0 |
| `EXPECT_CALLS` | 14582 | **14581** | **14581** |

**The `EXPECT_CALLS` 14582 → 14581 delta is resolved and is NOT a deleted assertion.**
Re-running the *current* stripper against the pristine baseline tree yields **14581** — the
same number as the worktree. The baseline figure 14582 was produced by the earlier AST-only
stripper, which failed to see one comment. Confirmation that no assertion was removed: the
per-file **raw** (unstripped) `expect(` counts across all 1480 files are **identical** between
HEAD and the worktree — total delta **0**. What changed is only how many comments the checker
can now see. `ALLOW_ATTR=0`: no `allowJs` / `allowSyntheticDefaultImports` /
`allowImportingTsExtensions` attribute was added anywhere.

### i18n invariant

```
I18N_EXIT=0
I18N_FILES=166  I18N_KEYS_BASELINE=24075  I18N_FILES_WITH_KEY_DRIFT=0  I18N_RESULT=PASS
```

### Prohibitions observed

No `.gitignore` edit, no file moved, no scan-scope change, no `sandbox_permissions` used,
`src/extensions/generated*.ts` never touched, no `.env` / `.env.test` contents read, no cargo
command executed. `src-tauri/`, `packages/`, `scripts/`, `docs/`, `*.config.ts` verified clean
via `git status --porcelain` (empty).

---

## 6. Pending adjudication

### 6.1 `F_NUM` — the frozen decision (3 sites, all `F6`)

**The coordinator froze the `F1–F12` question and the arbiter never reported.** Verbatim
from the coordinator: *"F1–F12 那条不归我裁。判据自身在 main 的提交正文之间就是冲突的
(`38c659667`/`0bcbf4334` 要求删内部编号并点名 `F1/F4/F9`,`2aac39c7b` 要求 `F1–F12 首行契约`
原样保留)…在它回来之前:**不要按你那条暂定读法继续扩面**."* The arbiter and all batch agents
failed, so no ruling arrived. These 3 sites were therefore **left untouched**.

**This track's own reading, stated but NOT acted on:** all three are the same class as the 111
`F_NUM` hits already removed elsewhere — a track/wave label whose sentence stands alone without
it. They differ from the `F5` / `Ctrl-F3` sites (kept, §3.1) which are genuine keyboard-key
payloads. If the arbiter rules that track labels must go, these 3 are a 3-line follow-up;
if it rules the exemption extends to TypeScript first-line contracts, they stay as-is.
**No change is pending either way — the tree is in the frozen, unexpanded state.**

`WappPageShell.tsx` was never opened by batch 1 (audited), which is precisely why sites 16–17
survive; `WorkspaceView.tsx:79` likewise.

### 6.2 Carried from the batch-4 report

| | Item |
|---|---|
| **A** | `src/stores/schemaStore.ts` — `F7` / `F1` removal. The `F1–F12` exemption was argued to have **no object** in `src`/`e2e` (it lives in the Rust `catalog_guard.rs`, which parses first-line `F1`..`F12` doc comments). Needs the arbiter. |
| **B** | `src-tauri/.../scope.rs` pointer kept as a **code-file** pointer with no line number — outside this track's editable scope regardless. |
| **C** | `e2e/specs/multi-database.ts` — `TEST_ID` `'F2-E2E'→'MySQL-E2E'`, `'F4-E2E'→'PostgreSQL-E2E'`. String-literal edit confined to `describe`/`it` titles; no repository reference involved. Not applied — awaiting the same `F` ruling. |
| **D** | `src/test/enCopy.ts` — a `tsconfig.json` statement is now stale. Pre-existing; not touched. |
| **E** | `P5` prefix removed as a wave-number class. |

### 6.3 Documented inconsistencies (not silently fixed)

- **Compartment labels left in place:** `S4-A/B/C/D`, `S5-A`, `S6-D` (11 sites) remain in
  `src/components/sql-editor/editorExtensions.ts`. They are internal labels the criterion
  would remove, but the same `F`-family freeze applies to expanding deletions at this stage.
- **e2e suite-id inconsistency:** some agents kept ids while others stripped equivalents.
  Re-measured on the committed tree (`grep -rEo` over `src`/`e2e`):

  | id | status on committed tree |
  |---|---|
  | `DTJ-001`, `OPS-FILTER-*`, `CTX-001` | **kept** (4 / 7 / 3 occurrences) |
  | `DI-001`, `SS-CLN-001`, `DB-001` | **kept** (1 / 1 / 3 occurrences) |
  | `CM-008`, `NCM-001`, `CM-007`, `OPS-DDL-00N` | **stripped** (0 occurrences) |

  **No script or CI consumes these ids**, and retitling is behaviour-neutral. Left as-is
  and disclosed rather than half-fixed.

  > **Correction (added after review):** the previous version of this bullet claimed
  > `CM-008` was *kept* and `DB-001` was *stripped*. Re-measurement shows both claims were
  > inverted. The table above replaces the prose.
- **`D-n` decision numbers** (`D-1/D-2/D-4/D-10`, 8 sites in
  `src/windows/data-transfer/__tests__/DataTransferWindow.test.tsx`) were treated as
  removable one-shot requirement numbers and rewritten to describe the behaviour. If the
  arbiter rules they belong to the frozen family, 4 of the 8 are a trivial `it()`-title revert.
- **Commit-hash citations** (`787f0e6fe`, `299b7562b`, `e883f834`) were removed while keeping
  the commands and the real identifiers around them — consistent with the reference sample
  `src/lib/migrationJobVerdict.ts`.

---

## 7. Pre-existing drift (not introduced, deliberately not fixed)

`npx prettier --check` reports files non-compliant, **including files this track never
touched** (e.g. `src/lib/migrationJobVerdict.ts`, `src/windows/data-transfer/DataTransferWindow.tsx`,
`src/platform/tauriBackendTransport.ts`). Spot-checking the prettier diff on this track's own
files showed the reported lines are all ones this track did not modify. Running `--write`
would change many lines outside the cleanup's scope. The repo's `package.json` has **no eslint
or prettier lint script** — only `typecheck*` variants — so there is no lint step to run.

`e2e/specs/*.ts` changes were **not executed** at runtime: they require a full
`pnpm tauri:build:webdriver` build, which is outside this track. Those files are covered by
Gate 1 only.

---

## 8. Cleanup state

- Codegen artefacts (`src/extensions/generated*.ts`, `src/extensions/generated-locales.ts`,
  `src/extensions/generated-pro.ts`, `src/locales/builtinLocales.ts`) are **gitignored** and
  were generated locally to unblock `pnpm typecheck`. They are not part of this commit.
- All tooling lives in `/tmp/dz-tools/` and **never** in the repository.
- **Not done, by instruction:** this branch is **not** merged into `codex/p5-integration`; the
  branch and worktree are **not** deleted.

---

## 9. Q1 check — "does any executable code match the token I deleted?"

Gate 2 counts hits before/after. It **cannot** see a deletion that turns an assertion
vacuous. This section records the separate check that was run, and its method — not just
its result.

### The criterion used, and why it is not the CE-1 "shape" trap

The rule applied is **Q1/Q2/Q3 against comment-stripped code**, *not* "it looks like an
exempted form". A pure shape test (e.g. "`/// F7:` prefix ⇒ keep") is known to wrongly keep
tokens that nothing parses. Nothing in this track keys on the *form* of a token: the
decision is made by asking whether the literal has a **resolvable referent inside this
repository**. Concretely a token is deleted when it resolves only outside the repo (specs,
plans, external docs), and kept when in-repo code reads it at runtime, it is a row-label of
an in-repo table, or it *is* the asserted value.

### Method (four passes, results below)

1. **Pre-state grep.** For every token core slated for deletion, grep the whole repo
   (`*.ts`/`*.tsx`, excluding the file itself) and inspect whether each hit **matches** the
   literal or merely **mentions** it.
2. **Comment-stripped grep.** Strip comments from all 1480 in-scope files
   (`strip.mjs --mode comments`) into a scratch copy and grep *that*. Comment-stripped code
   is precisely where a runtime `match`/`includes`/`RegExp` could live — a grep of raw text
   would be answered by the comment being deleted, which proves nothing.
3. **Full id inventory.** Count every distinct id-shaped token, then compute
   `only_in_comments = raw_count - stripped_count`. Tokens where the two counts are equal
   live in **string literals**, i.e. they are *mentions* (test titles), not matches.
4. **AST-precise pass.** Using the TS compiler API on the raw worktree text, locate tokens
   occurring **outside** string/template literals (270 sites / 156 distinct), then inspect
   the 10 sites where **code actually parses the id shape**. All 10 read live runtime DOM
   text, not source.

### Result

- Comment-stripped grep across all 58 token cores: **exactly one** hit — the
  `ORD-2026-002` false positive (an order id inside a SQL fixture in
  `e2e/specs/zz-screenshots.ts`). Untouched; it is data, not a citation.
- `§` symbols in comment-stripped code: **0**.
- Of the 10 id-shape-parsing sites, all read runtime DOM text, e.g.
  `e2e/specs/homepage-features.ts:160` `text.match(/\((\d+)\)/)` running inside
  `browser.execute()` over `[data-group-header]` `textContent`, and
  `e2e/helpers.ts:2887` `countText.match(/(\d+)\s*\/\s*(\d+)/)` over a DOM element's text.
  The remaining 8 are counts, `rgba()`, `${n:}` interpolation and plain number literals.

**Conclusion: no deleted token is matched by any executable code in this repo; no assertion
was made vacuous.** This is the CE-2 hazard, checked rather than assumed.

**Known limitation of this method:** it proves absence of a *match*, not absence of a
*semantic* dependency. If code built an id by concatenation or `import` at runtime rather
than by literal, this grep would not see it.

---

## 10. Measured coverage gap — `HIT_TOTAL=18` does **not** mean the class is exhausted

The residual scanner has 12 rules; none of them matches a general internal id. A separate
inventory was therefore run over the committed tree, classifying id-shaped tokens by
`raw_count > stripped_count` (i.e. resident in comments) versus string literals.

| population | distinct tokens | occurrences | files |
|---|---|---|---|
| Comment-resident internal ids | **154** | **267** | 95 |
| String-literal (test-title) ids | **594** | **743** | 97 |

Families in the comment-resident set include `AC-*`, `ADB-001..008`, `AI-001..012`,
`BACKUP-011/012`, `BKU-*`, `BUG-001..006`, `CM-SUB-*`, `DP-001..004`, `DSW-*`, `DTJ-*`,
`EI-BE-001`, `ER-*`, `PIH-001..006`, `QLIMIT-001..006`, `SE-PROD-001..031`, `SQ-CTX-001`,
`SS-*`, `SW-01..06`, `SYS-001..004`, `TF-*`, `UIX-001..005`, `ZERO-001/005`, `P1-5/6/8`.

**None of these is matched by code** (§9), so deleting them would not weaken any assertion.
They are nevertheless **left in place**, deliberately, for two reasons:

1. **The governing question is frozen.** The `F1–F12` / label-family ruling is pending with
   the arbiter, and the same "position vs criterion" contradiction is unresolved here. A
   blanket delete would unilaterally settle a question that has been explicitly escalated.
2. **The string-literal population is mixed.** It is not all citations: it also contains
   legitimate names such as `E2E-PG` (the Postgres suite) and `MY-TYPE-013`. A positional
   sweep would damage real labels. Classifying these needs a per-token judgement, not a rule.

This is stated here so that "18 survivors" is **not** read as "the internal-id class is
finished". It is not finished; it is measured, Q1-verified as safe, and pending adjudication.

### 10.1 Batch 9 was attempted and reverted

A mechanical pass was written to delete the 154 comment-resident ids (71 files, 194
occurrences reached) and was then **reverted in full** (`git checkout -- src e2e`; tree
restored byte-identical, `git status --porcelain` empty, `git diff HEAD` = 0 lines).

Reason: the removal produced **mangled comments** — orphaned fragments and dangling
punctuation where the id sat mid-sentence, e.g.

- `(~ )` left from a parenthetical whose only content was the id,
- `(/006/007, )` — a partially-consumed token leaving the numeric tail,
- `tunnel-form- fix` — a trailing hyphen from a hyphen-joined id.

An id in a comment is a citation (removable). A **wrong** comment is a content defect
(worse than the status quo) and would misrepresent the code. The tree is therefore left in
the state that passed every gate, and the gap is reported rather than half-fixed.

**Process note recorded for the next pass:** a token-deletion pass must clean the
punctuation *around* a match, not just the match, and must be audited on all added lines
for orphan fragments before it is kept.