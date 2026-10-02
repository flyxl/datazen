/**
 * The reconciliation matrix, stated as data.
 *
 * `requires` names the version numbers that must have gone *up* relative to the
 * base when a change of `class` lands inside a governed span.
 *
 * Lives in `lib/` beside the source-break vocabulary it shares a conclusion
 * with; `check-driver-protocol-compat.mjs` re-exports it so the import path
 * every gate and test already uses keeps working.
 */
/**
 * The reconciliation matrix, stated as data.
 *
 * `requires` names the version numbers that must have gone *up* relative to the
 * base when a change of `class` lands inside a governed span.
 *
 * @type {Readonly<{
 *   protocol: string,
 *   minProtocol: string,
 *   crateVersion: string,
 *   classes: Readonly<Record<'breaking' | 'additive' | 'cosmetic' | 'source-breaking', { requires: string[], rationale: string }>>,
 * }>}
 */
export const COMPAT_MATRIX = Object.freeze({
  protocol: 'PROTOCOL_VERSION (packages/driver-api/src/lib.rs)',
  minProtocol: 'MIN_PROTOCOL_VERSION (packages/driver-api/src/lib.rs)',
  crateVersion: 'version (packages/driver-api/Cargo.toml)',
  // The rule table below is not invented here: it is the mechanical form of
  // `docs/architecture/platform/driver-capability-migration.md` §5.3
  // 「哪些改动是 breaking」, whose stated decision principle is 「老驱动在新宿主上
  // 是否仍能安全运行」. That table's six rows map onto the three classes below
  // as follows.
  //
  //   §5.3 新增 trait 方法且带 fail-closed 默认体   -> additive
  //   §5.3 新增 trait 方法没有默认体 / 改 DTO 字段必填性 -> breaking
  //   §5.3 改能力枚举取值或语义                       -> breaking
  //   §5.3 放宽已声明 unsupported 的行为              -> cosmetic (no bump)
  //   §5.3 收紧已声明 supported 的行为                 -> breaking
  //
  // §5.3's 「升 MIN + 升 PROTOCOL」 is encoded as a required `PROTOCOL_VERSION`
  // bump plus the standing `min-protocol-never-lowered` invariant, not as a
  // required MIN bump: §5.1 records `MIN = 1` against `PROTOCOL = 4` as a
  // deliberate surviving window, and §5.4's own step 1 is the PROTOCOL bump. A
  // rule that demanded a MIN bump on every breaking change would have failed
  // the crate's own 1 -> 2 -> 3 -> 4 history.
  sourceDoc: 'docs/architecture/platform/driver-capability-migration.md §5.3, §5.4',
  classes: Object.freeze({
    breaking: Object.freeze({
      requires: ['protocol'],
      rationale:
        'A removed or altered line inside a governed span changes what a driver compiled against the old crate must send or expect; no [MIN_PROTOCOL_VERSION, PROTOCOL_VERSION] window expresses that, so the protocol generation must move (migration doc §5.4 step 1).',
    }),
    additive: Object.freeze({
      requires: ['crateVersion'],
      rationale:
        'A new struct field, enum variant or defaulted trait method leaves existing implementors compiling and declaring nothing new, so the crate moves without the protocol moving (migration doc §5.3 row 1).',
    }),
    cosmetic: Object.freeze({
      requires: [],
      rationale:
        'Comment-only and whitespace-only edits carry no contract meaning and move no version number; §5.3 row 5 (放宽已声明 unsupported 的行为) is the runtime counterpart.',
    }),
    // A fourth class, for the one kind of edit the three above cannot name:
    // it leaves a built old driver working, and stops anyone building a new
    // one. `additive` would promise the opposite ("existing implementors keep
    // compiling"); `breaking` would demand a PROTOCOL_VERSION bump for a wire
    // change that never happens. So it is neither, it moves the crate version
    // like `additive`, and the gate prints the migration recipe with it.
    'source-breaking': Object.freeze({
      requires: ['crateVersion'],
      rationale:
        'Adding #[non_exhaustive] to a governed struct stops out-of-crate callers from constructing it at all, so "existing implementors keep compiling" is false and a protocol bump would be a lie — the break is visible only to `cargo build`, never to a host. Detected by scripts/lib/sourceBreak.mjs.',
    }),
  }),
});
