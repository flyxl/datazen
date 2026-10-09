/**
 * Console batch helpers: split a multi-line command input into the logical
 * commands the server will run, then fold their danger levels into one verdict
 * (track `redis-console-safety`, PRD §4 I-7 + task book §6.1/§6.3).
 *
 * The split mirrors the server (`src/ops/exec.rs::split_redis_commands`:
 * newline-separated, trimmed, blanks dropped) with one addition: a line that
 * closes an argument quote opened on an earlier line is treated as a
 * *continuation* and merged into the previous logical command. That is what
 * keeps an `EVAL "…\nKEYS …\n" 0` script body out of the classification — the
 * body is never a command, so it can neither inflate nor relax a level.
 *
 * Pure module: no React, no i18n import — copy is resolved through an injected
 * `t` so every branch stays table-testable.
 */

import {
  assessCommand,
  dangerRank,
  isAtLeast,
  isBlockedLevel,
  isFlushCommand,
  worstLevel,
  type CommandAssessment,
  type DangerLevel,
} from './redisConsoleDanger';

export interface BatchAssessment {
  /** Every logical command, in execution order. */
  readonly commands: readonly CommandAssessment[];
  /** Strictest level across the batch (drives the badge and the gate). */
  readonly worst: DangerLevel;
  /** Commands the Console must refuse (`ultra-danger`, incl. unknown). */
  readonly blocked: readonly CommandAssessment[];
  /** Blocked commands whose name was not recognised. */
  readonly unknown: readonly CommandAssessment[];
  /** Blocked commands that are known-destructive. */
  readonly destructive: readonly CommandAssessment[];
  /** `danger`-tier commands that need the (single) confirmation dialog. */
  readonly confirmations: readonly CommandAssessment[];
  /** `ultra-danger` commands that survived the block policy and need the second confirmation. */
  readonly doubleConfirmations: readonly CommandAssessment[];
}

const EMPTY_BATCH: BatchAssessment = Object.freeze({
  commands: [],
  worst: 'safe',
  blocked: [],
  unknown: [],
  destructive: [],
  confirmations: [],
  doubleConfirmations: [],
});

/**
 * Split console input into logical commands. Newlines inside a quoted argument
 * do not start a new command; unbalanced quotes at end of input still yield the
 * command seen so far (partial input must classify, never throw).
 */
export function splitConsoleCommands(input: string): string[] {
  const out: string[] = [];
  let buffer = '';
  let openQuote: string | null = null;

  for (const line of input.split('\n')) {
    if (openQuote === null && line.trim() === '') continue;
    buffer = openQuote === null ? line.trim() : `${buffer}\n${line}`;
    openQuote = findUnterminatedQuote(buffer);
    if (openQuote === null) {
      out.push(buffer);
      buffer = '';
    }
  }

  if (buffer.trim() !== '') out.push(buffer);
  return out;
}

/** The quote character left open by `text`, or `null` when every quote closes. */
function findUnterminatedQuote(text: string): string | null {
  let open: string | null = null;
  for (let i = 0; i < text.length; i += 1) {
    const char = text[i];
    if (char === '\\') {
      i += 1;
      continue;
    }
    if (open === null) {
      if (char === '"' || char === "'") open = char;
    } else if (char === open) {
      open = null;
    }
  }
  return open;
}

/**
 * Assess a whole batch.
 *
 * `allowFlush` is the driver's own connection opt-in (Redis settings) and is the
 * only thing that keeps FLUSHDB/FLUSHALL out of the refuse list — that is their
 * pre-I-7 behaviour, so no existing workflow is broken. Nothing else can be
 * waved through: a blocked command stays blocked (task book §5 "不得放宽默认档").
 */
export function assessCommands(input: string, allowFlush = false): BatchAssessment {
  const lines = splitConsoleCommands(input);
  if (lines.length === 0) return EMPTY_BATCH;

  const commands = lines.map((line) => assessCommand(line));
  const blocked: CommandAssessment[] = [];
  const unknown: CommandAssessment[] = [];
  const destructive: CommandAssessment[] = [];
  const confirmations: CommandAssessment[] = [];
  const doubleConfirmations: CommandAssessment[] = [];

  for (const command of commands) {
    if (isBlockedLevel(command.level)) {
      if (allowFlush && isFlushCommand(command.name)) {
        doubleConfirmations.push(command);
        continue;
      }
      blocked.push(command);
      if (command.unknown) unknown.push(command);
      else destructive.push(command);
      continue;
    }
    if (isAtLeast(command.level, 'danger')) confirmations.push(command);
  }

  return {
    commands,
    worst: worstLevel(commands.map((command) => command.level)),
    blocked,
    unknown,
    destructive,
    confirmations,
    doubleConfirmations,
  };
}

/**
 * Compact one-line labels for the confirmation dialog / block message. Unknown
 * commands are labelled by their parsed name so the user sees *which* token the
 * classifier refused to recognise.
 */
export function describeCommands(commands: readonly CommandAssessment[], max = 48): string {
  return commands
    .map((command) => {
      const label = command.raw.length > max ? `${command.raw.slice(0, max - 1)}…` : command.raw;
      return label;
    })
    .join(', ');
}

/**
 * Refusal copy for a blocked batch (PRD I-7 "默认阻断"). Unknown commands get
 * their own line so they never read as "known destructive". `t` is injected so
 * this module keeps zero i18n dependency and stays table-testable.
 */
export function composeBlockedMessage(
  batch: BatchAssessment,
  t: (key: string, params?: Record<string, string>) => string,
): string {
  const lines: string[] = [];
  if (batch.destructive.length > 0) {
    lines.push(
      t('redis.consoleSafety.blockedDestructive', {
        commands: describeCommands(batch.destructive),
      }),
    );
  }
  if (batch.unknown.length > 0) {
    lines.push(
      t('redis.consoleSafety.blockedUnknown', { commands: describeCommands(batch.unknown) }),
    );
  }
  lines.push(t('redis.consoleSafety.blockedHint'));
  return lines.join('\n');
}

/**
 * Verdict for the toolbar badge: the strictest command in the batch wins, and
 * `unknown` is set when *any* command in the batch was unrecognised — that way a
 * half-typed last line can never hide behind a known-safe first line.
 */
export function badgeAssessment(input: string, allowFlush = false): CommandAssessment {
  const batch = assessCommands(input, allowFlush);
  if (batch.commands.length === 0) return { raw: '', name: '', level: 'safe', unknown: false };
  const strictest = batch.commands.reduce((worst, command) =>
    dangerRank(command.level) > dangerRank(worst.level) ? command : worst,
  );
  return { ...strictest, unknown: batch.unknown.length > 0 || strictest.unknown };
}
