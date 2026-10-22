/**
 * Which control edits a cell, what a typed input means, and why a cell
 * cannot be edited at all.
 *
 * The three questions share **one** type judgement — the normalised
 * `CellType` of `src/lib/cellTypes.ts` — and they share it deliberately: a
 * cell that is painted as a number but edited as text is how values get
 * corrupted. Everything in this module is a pure function: no store, no IPC,
 * no DOM. That is what makes the rules testable on their own and reusable
 * by the paste path (DB-03), which coerces without opening an editor.
 *
 * ## The three states of a write
 *
 * A cell edit produces a **write intent**, not a value:
 *
 * - `undefined` — do not write this column (undo the change);
 * - `null` — write SQL `NULL`;
 * - anything else — write that value.
 *
 * `null` and `undefined` are different statements, so the two must never be
 * collapsed (00-contracts §3).
 */
import type { ColumnSchema, TableType, Value } from '../types';
import type { GridErrorCode } from './gridErrors';
import type { SetValueActionId, SetValueState } from './setValueActions';
import {
  isIntegerCellType,
  isReadOnlyCellType,
  resolveCellType,
  type CellType,
  type CellTypeHints,
} from './cellTypes';

/**
 * The control family of a cell editor.
 *
 * `date` covers the three temporal shapes; which one it is follows from the
 * cell type, not from a second control.
 */
export type CellEditorKind =
  | 'text'
  | 'number'
  | 'boolean'
  | 'enum'
  | 'date'
  | 'json'
  | 'binary'
  | 'longText';

export interface CellEditorKindInput {
  /** The normalised type of the column. */
  readonly cellType: CellType;
  /** The column's values carry a zone (`isZonedCellType`). */
  readonly zoned: boolean;
  /** The driver or the metadata already supplied the enum candidates. */
  readonly hasCandidates: boolean;
  /** The length of the value in **code points** (not UTF-16 units). */
  readonly valueCodePoints: number;
  /**
   * The value contains a line break.
   *
   * A single-line input would show it as a blank, which is worse than not
   * showing it at all (05 §7 #5), so such a value is edited in the
   * multi-line editor.
   */
  readonly hasLineBreak?: boolean;
}

/** Above this many code points a text value is edited in the pop-up editor. */
export const INLINE_TEXT_MAX_CODE_POINTS = 200;
/** Above this many bytes a value is shown but not edited in the grid. */
export const CELL_EDIT_MAX_BYTES = 64 * 1024;

/**
 * Why an input could not be turned into a write intent.
 *
 * Every reason has its own sentence in the interface (05 §8); the editor
 * shows it inline and the paste path reports it per cell.
 */
export type CellValueErrorReason =
  | 'emptyNotNull'
  | 'invalidNumber'
  | 'invalidInteger'
  | 'invalidBoolean'
  | 'invalidDate'
  | 'invalidTime'
  | 'invalidDateTime'
  | 'invalidJson'
  | 'invalidUuid'
  | 'tooLarge'
  | 'notEditable';

export type CoerceResult =
  | { readonly ok: true; readonly value: Value | null | undefined }
  | { readonly ok: false; readonly reason: CellValueErrorReason };

export interface CoerceInput {
  readonly kind: CellEditorKind;
  readonly cellType: CellType;
  readonly text: string;
  /** The column accepts SQL `NULL`. */
  readonly nullable: boolean;
  /** The value before the edit; the editor needs it for "nothing changed". */
  readonly originalValue: unknown;
  /**
   * The column holds whole numbers, so a fraction is refused.
   *
   * `CellType` folds integers and decimals into one `number`, so the
   * distinction has to come from the type name
   * (`isIntegerCellType`, `src/lib/cellTypes.ts`).
   */
  readonly integerOnly?: boolean;
}

/**
 * Which control edits this cell.
 *
 * The dispatch table (05 §3.5):
 *
 * | cell type            | control                                   |
 * | -------------------- | ----------------------------------------- |
 * | `bool`               | three-state select / checkbox             |
 * | `enum`               | searchable select                         |
 * | `date` / `time`      | temporal picker                           |
 * | `datetime` (no zone) | temporal picker                           |
 * | `datetime` (zone)    | plain text — a picker drops the offset     |
 * | `number`             | single-line input with a number check     |
 * | `json`               | pop-up text area                          |
 * | `bytes`              | not editable in place                     |
 * | `text` / `uuid`      | single-line input (long values: pop-up)   |
 */
export function resolveCellEditorKind(input: CellEditorKindInput): CellEditorKind {
  switch (input.cellType) {
    case 'bool':
      return 'boolean';
    case 'enum':
      return 'enum';
    case 'date':
      return 'date';
    case 'time':
      // A zoned time (`timetz`) carries an offset the picker cannot hold.
      return input.zoned ? 'text' : 'date';
    case 'datetime':
      return input.zoned ? 'text' : 'date';
    case 'number':
      return 'number';
    case 'json':
      return 'json';
    case 'bytes':
      return 'binary';
    case 'text':
    case 'unknown':
      return isLongText(input) ? 'longText' : 'text';
    case 'uuid':
      // A UUID is always short: it never needs the pop-up editor.
      return 'text';
  }
}

function isLongText(input: CellEditorKindInput): boolean {
  if (input.valueCodePoints > INLINE_TEXT_MAX_CODE_POINTS) return true;
  return input.hasLineBreak === true;
}

/** ASCII digits only — `١٢` (Arabic-Indic) and `0x10` are not numbers here. */
const NUMBER_RE = /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/;
const DATE_RE = /^(\d{4})-(\d{2})-(\d{2})$/;
const TIME_RE = /^(\d{1,2}):(\d{2})(?::(\d{2}))?$/;
const DATETIME_RE = /^(\d{4})-(\d{2})-(\d{2})[T ](\d{1,2}):(\d{2})(?::(\d{2}))?$/;
const ZONED_DATETIME_RE =
  /^(\d{4})-(\d{2})-(\d{2})[T ](\d{1,2}):(\d{2})(?::(\d{2}))?(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?$/;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** Whether a `YYYY-MM-DD` string is a real day of the calendar. */
function isRealCalendarDate(year: number, month: number, day: number): boolean {
  if (month < 1 || month > 12) return false;
  if (day < 1 || day > 31) return false;
  const date = new Date(Date.UTC(year, month - 1, day));
  return (
    date.getUTCFullYear() === year && date.getUTCMonth() === month - 1 && date.getUTCDate() === day
  );
}

/** Whether a `HH:MM(:SS)` string is a real time of the day. */
function isRealClockTime(hour: number, minute: number, second: number): boolean {
  return hour >= 0 && hour <= 23 && minute >= 0 && minute <= 59 && second >= 0 && second <= 59;
}

/**
 * The write intent of a typed input.
 *
 * The rules that matter:
 *
 * - an empty input on a `NOT NULL` column is **refused** (`emptyNotNull`) —
 *   it never becomes an empty string and never becomes `NULL`;
 * - an empty input on a nullable column is `NULL` (the behaviour the grid
 *   has always had);
 * - a number is validated by a **regular expression first** and only then
 *   converted, so `Number('')` (`0`), `Number('0x10')` (`16`) and
 *   `Number('١٢')` (`12`) can never reach the store;
 * - `NaN` and `Infinity` are refused — `valuesEqual` compares with
 *   `JSON.stringify`, which turns `NaN` into `null` and would silently
 *   write a `NULL`;
 * - a date without a zone is a **wall clock**: it is passed through
 *   verbatim, never through `new Date()` (05 §7 #8);
 * - invalid JSON is **refused**. The old editor committed the raw text,
 *   which put a non-JSON string into a JSON column.
 */
export function coerceValue(input: CoerceInput): CoerceResult {
  const text = input.text;
  if (text === '') {
    if (!input.nullable) return { ok: false, reason: 'emptyNotNull' };
    return { ok: true, value: null };
  }

  switch (input.kind) {
    case 'number':
      return coerceNumber(input, text);
    case 'boolean':
      return coerceBoolean(text);
    case 'date':
      return coerceTemporal(input, text);
    case 'json':
      return coerceJson(text);
    case 'enum':
      return { ok: true, value: text };
    case 'binary':
      return { ok: false, reason: 'notEditable' };
    case 'text':
    case 'longText':
      return coerceText(input, text);
  }
}

function coerceNumber(input: CoerceInput, text: string): CoerceResult {
  if (!NUMBER_RE.test(text)) return { ok: false, reason: 'invalidNumber' };
  const value = Number(text);
  if (!Number.isFinite(value)) return { ok: false, reason: 'invalidNumber' };
  if (input.integerOnly === true && !Number.isInteger(value)) {
    return { ok: false, reason: 'invalidInteger' };
  }
  return { ok: true, value };
}

function coerceBoolean(text: string): CoerceResult {
  const normalized = text.trim().toLowerCase();
  if (normalized === 'true' || normalized === '1') return { ok: true, value: true };
  if (normalized === 'false' || normalized === '0') return { ok: true, value: false };
  return { ok: false, reason: 'invalidBoolean' };
}

function coerceTemporal(input: CoerceInput, text: string): CoerceResult {
  if (input.cellType === 'date') {
    const match = DATE_RE.exec(text);
    if (!match) return { ok: false, reason: 'invalidDate' };
    const [, year, month, day] = match;
    if (!isRealCalendarDate(Number(year), Number(month), Number(day))) {
      return { ok: false, reason: 'invalidDate' };
    }
    // Wall clock: the text is the value.
    return { ok: true, value: text };
  }
  if (input.cellType === 'time') {
    const match = TIME_RE.exec(text);
    if (!match) return { ok: false, reason: 'invalidTime' };
    const hour = Number(match[1]);
    const minute = Number(match[2]);
    const second = match[3] === undefined ? 0 : Number(match[3]);
    if (!isRealClockTime(hour, minute, second)) return { ok: false, reason: 'invalidTime' };
    const canonical = `${String(hour).padStart(2, '0')}:${String(minute).padStart(2, '0')}:${String(second).padStart(2, '0')}`;
    return { ok: true, value: canonical };
  }
  // datetime without a zone.
  const match = DATETIME_RE.exec(text);
  if (!match) return { ok: false, reason: 'invalidDateTime' };
  const [, year, month, day, hour, minute, second] = match;
  if (!isRealCalendarDate(Number(year), Number(month), Number(day))) {
    return { ok: false, reason: 'invalidDateTime' };
  }
  if (!isRealClockTime(Number(hour), Number(minute), second === undefined ? 0 : Number(second))) {
    return { ok: false, reason: 'invalidDateTime' };
  }
  return { ok: true, value: text };
}

function coerceJson(text: string): CoerceResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return { ok: false, reason: 'invalidJson' };
  }
  if (
    parsed === null ||
    typeof parsed === 'string' ||
    typeof parsed === 'number' ||
    typeof parsed === 'boolean' ||
    Array.isArray(parsed) ||
    typeof parsed === 'object'
  ) {
    return { ok: true, value: parsed as Value };
  }
  return { ok: false, reason: 'invalidJson' };
}

function coerceText(input: CoerceInput, text: string): CoerceResult {
  if (input.cellType === 'uuid') {
    const trimmed = text.trim();
    if (!UUID_RE.test(trimmed)) return { ok: false, reason: 'invalidUuid' };
    return { ok: true, value: trimmed };
  }
  if (input.cellType === 'datetime') {
    // A zoned column is edited as text: the offset must survive untouched.
    const match = ZONED_DATETIME_RE.exec(text);
    if (!match) return { ok: false, reason: 'invalidDateTime' };
    const [, year, month, day, hour, minute, second] = match;
    if (!isRealCalendarDate(Number(year), Number(month), Number(day))) {
      return { ok: false, reason: 'invalidDateTime' };
    }
    if (!isRealClockTime(Number(hour), Number(minute), second === undefined ? 0 : Number(second))) {
      return { ok: false, reason: 'invalidDateTime' };
    }
    return { ok: true, value: text };
  }
  return { ok: true, value: text };
}

/**
 * Why a cell cannot be edited.
 *
 * Every reason has one sentence in the interface — "read-only" on its own
 * tells the user nothing (00-contracts §8.2).
 */
export type CellReadOnlyReason =
  | 'connectionReadOnly' // the connection or the driver is read-only
  | 'driverCapability' // the driver declares that it cannot write
  | 'view' // a view, a materialized view or a system table
  | 'noPrimaryKey' // no primary key, so the row cannot be addressed
  | 'ambiguousRowIdentity' // the key is NULL or duplicated
  | 'autoIncrement' // an auto-increment column of an existing row
  | 'generated' // a generated column
  | 'binaryColumn' // a binary column
  | 'largeValue' // too large to edit in the grid
  | 'typeNotEditable'; // the driver says this type is not writable in place

export interface CellEditabilityInput {
  /** The connection (or the driver) is read-only. */
  readonly connectionReadOnly: boolean;
  /**
   * A more precise reason the mount point already knows.
   *
   * The view panel mounts the grid with `readOnly` hard-coded, so without
   * this the user would be told "the connection is read-only" for a view —
   * a different problem with a different fix.
   */
  readonly mountReason?: CellReadOnlyReason;
  /** What kind of relation the grid shows. */
  readonly relationKind: TableType;
  /** `DatabaseTypeMeta.dataGrid?.editable`; absent means editable. */
  readonly capabilityEditable?: boolean;
  /** The table has a primary key. */
  readonly hasPrimaryKey: boolean;
  /** The row's identity addresses exactly one row. */
  readonly rowIdentityUnique: boolean;
  /** The column being edited. */
  readonly column: Pick<ColumnSchema, 'name' | 'nullable' | 'isAutoIncrement'>;
  /** The normalised type of the column. */
  readonly cellType: CellType;
  /** The size of the value in the editor's memory, in bytes. */
  readonly inMemoryByteSize: number;
  /** The driver declared this type as not writable in place. */
  readonly typeReadOnly: boolean;
  /** Whether the row exists (`update`) or is a new-row draft (`insert`). */
  readonly rowKind: 'update' | 'insert';
}

export interface CellEditability {
  /** Whether the cell may be edited. */
  readonly editable: boolean;
  /** Why not — `null` when it may. */
  readonly reason: CellReadOnlyReason | null;
  /** The error code of a refusal (00-contracts §8.2). */
  readonly code: GridErrorCode | null;
  /** The i18n key of the sentence that explains the refusal. */
  readonly messageKey: string;
}

/**
 * The sentence that explains a refusal.
 *
 * The **only** mapping from a reason to a key: every caller reads it from
 * here, so a reason can never end up without an explanation.
 */
export function readOnlyReasonMessageKey(reason: CellReadOnlyReason): string {
  switch (reason) {
    case 'connectionReadOnly':
      return 'cellEditor.readOnly.connection';
    case 'driverCapability':
      return 'cellEditor.readOnly.driverCapability';
    case 'view':
      return 'cellEditor.readOnly.view';
    case 'noPrimaryKey':
      return 'cellEditor.readOnly.noPrimaryKey';
    case 'ambiguousRowIdentity':
      return 'cellEditor.readOnly.ambiguousRowIdentity';
    case 'autoIncrement':
      return 'cellEditor.readOnly.autoIncrement';
    case 'generated':
      return 'cellEditor.readOnly.generated';
    case 'binaryColumn':
      return 'cellEditor.readOnly.binaryColumn';
    case 'largeValue':
      return 'cellEditor.readOnly.largeValue';
    case 'typeNotEditable':
      return 'cellEditor.readOnly.typeNotEditable';
  }
}

/**
 * Whether a cell may be edited, and why not.
 *
 * The order of the checks **is** the order of the explanation: the first
 * match decides what the user is told. The mount point's own reason comes
 * first, because it knows more than this function does (the view panel
 * mounts the grid read-only, and "a view is read-only" is a different
 * sentence than "the connection is read-only").
 */
export function resolveCellEditable(input: CellEditabilityInput): CellEditability {
  const reason = readOnlyReasonOf(input);
  if (reason === null) {
    return { editable: true, reason: null, code: null, messageKey: '' };
  }
  return {
    editable: false,
    reason,
    code: 'grid.cell.readOnly',
    messageKey: readOnlyReasonMessageKey(reason),
  };
}

function readOnlyReasonOf(input: CellEditabilityInput): CellReadOnlyReason | null {
  if (input.mountReason !== undefined) return input.mountReason;
  if (input.connectionReadOnly) return 'connectionReadOnly';
  if (input.capabilityEditable === false) return 'driverCapability';
  if (input.relationKind !== 'table') return 'view';
  if (!input.hasPrimaryKey) return 'noPrimaryKey';
  if (!input.rowIdentityUnique) return 'ambiguousRowIdentity';
  if (input.column.isAutoIncrement === true && input.rowKind === 'update') return 'autoIncrement';
  if (input.typeReadOnly) return 'typeNotEditable';
  if (input.cellType === 'bytes') return 'binaryColumn';
  if (input.inMemoryByteSize > CELL_EDIT_MAX_BYTES) return 'largeValue';
  return null;
}

/**
 * What the editor of one column needs to know.
 *
 * It is computed **once per column** — not once per cell, and not once per
 * render: a virtualised grid would otherwise re-normalise every type of
 * every visible cell on every frame (05 §4.5 #3).
 */
export interface ColumnEditorInfo {
  /** The normalised type of the column. */
  readonly cellType: CellType;
  /** The column accepts SQL `NULL`. */
  readonly nullable: boolean;
  /** The column holds whole numbers. */
  readonly integerOnly: boolean;
  /** Why this column cannot be edited, if it cannot. */
  readonly readOnlyReason: CellReadOnlyReason | null;
}

/**
 * The editor facts of the table's own columns.
 *
 * This is the host's path: the metadata comes from the schema, so the
 * nullability is the **real** one and the driver's type rules apply.
 */
export function buildColumnEditorInfo(
  columns: readonly Pick<ColumnSchema, 'name' | 'dataType' | 'nullable'>[],
  hints?: CellTypeHints,
): Map<string, ColumnEditorInfo> {
  const info = new Map<string, ColumnEditorInfo>();
  for (const column of columns) {
    const cellType = resolveCellType(column.dataType, hints);
    info.set(column.name, {
      cellType,
      nullable: column.nullable,
      integerOnly: isIntegerCellType(column.dataType, hints),
      readOnlyReason: columnReadOnlyReason(cellType, column.dataType, hints),
    });
  }
  return info;
}

/**
 * The editor facts of the grid's column definitions.
 *
 * The fallback for a grid that has no schema behind it (the result grid):
 * the type comes from the driver's type name, the nullability is assumed
 * (the grid cannot know better) and an integer column is recognised by its
 * name.
 */
export function buildColumnEditorInfoFromDefs(
  columns: readonly { name: string; type?: string }[],
): Map<string, ColumnEditorInfo> {
  const info = new Map<string, ColumnEditorInfo>();
  for (const column of columns) {
    const cellType = resolveCellType(column.type);
    info.set(column.name, {
      cellType,
      nullable: true,
      integerOnly: isIntegerCellType(column.type),
      readOnlyReason: columnReadOnlyReason(cellType, column.type),
    });
  }
  return info;
}

/** Why a **column** cannot be edited — the reasons that do not depend on a row. */
function columnReadOnlyReason(
  cellType: CellType,
  dataType: string | undefined,
  hints?: CellTypeHints,
): CellReadOnlyReason | null {
  if (isReadOnlyCellType(dataType, hints)) return 'typeNotEditable';
  if (cellType === 'bytes') return 'binaryColumn';
  return null;
}

/**
 * Everything the grid needs to edit a cell (DB-05).
 *
 * It is **one** object on purpose: the grid threads it through three
 * components, and eight loose props would mean eight chances to forget one
 * — and eight chances to break the line budget of the files that carry
 * them. Every field is optional, so a grid that does not offer the editing
 * keeps exactly the behaviour it had before (00-contracts R4).
 */
export interface CellEditingContext {
  /** The editor facts of every column, computed once by the owner. */
  readonly columnEditorInfo: ReadonlyMap<string, ColumnEditorInfo>;
  /** The whole grid is read-only, and this is why. */
  readonly readOnlyReason: CellReadOnlyReason | null;
  /** The panel the grid belongs to. */
  readonly panelId?: string;
  /** Read the candidate values of an enumeration column, on request only. */
  readonly loadCandidates?: (columnName: string) => Promise<readonly string[]>;
  /**
   * Write one cell with an explicit intent.
   *
   * `undefined` = do not write this column, `null` = write SQL `NULL`.
   */
  readonly onCellValue?: (row: number, col: string, value: Value | null | undefined) => void;
  /** What the "Set Value" menu may offer for the cell under the pointer. */
  readonly setValueState?: (row: number, columnName: string) => SetValueState | null;
  /** Write the cell without opening an editor. */
  readonly onSetValue?: (row: number, col: string, action: SetValueActionId) => void;
  /** Ask for the candidate values of the right-clicked column. */
  readonly onLoadCandidates?: (row: number, columnName: string) => void;
}

/**
 * The sentence that explains a refused input.
 *
 * The **only** mapping from a reason to a key, next to
 * {@link readOnlyReasonMessageKey}: every caller reads it from here, so a
 * reason can never end up without an explanation.
 */
export function cellValueErrorMessageKey(reason: CellValueErrorReason): string {
  switch (reason) {
    case 'emptyNotNull':
      return 'cellEditor.notNull';
    case 'invalidNumber':
      return 'cellEditor.invalidNumber';
    case 'invalidInteger':
      return 'cellEditor.invalidInteger';
    case 'invalidBoolean':
      return 'cellEditor.invalidBoolean';
    case 'invalidDate':
      return 'cellEditor.invalidDate';
    case 'invalidTime':
      return 'cellEditor.invalidTime';
    case 'invalidDateTime':
      return 'cellEditor.invalidDateTime';
    case 'invalidJson':
      return 'cellEditor.invalidJson';
    case 'invalidUuid':
      return 'cellEditor.invalidUuid';
    case 'tooLarge':
      return 'cellEditor.valueTooLarge';
    case 'notEditable':
      return 'cellEditor.binaryNotEditable';
  }
}
