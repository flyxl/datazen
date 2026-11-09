/**
 * Import column mapping (DBR-01).
 *
 * The wizard needs a plan for how source columns land on target columns, and
 * it needs the conflicts to be loud: two source columns resolving onto one
 * target column is a data-corrupting guess, so the plan reports it and the
 * wizard refuses to start until the user resolves it (PRD 4.2: "冲突时要求
 * 用户处理").
 *
 * Matching modes mirror the PRD's three options:
 * - `name`     — case-insensitive name match only; name matching records the
 *                actual target column name so the UI shows the real name.
 * - `position` — source i lands on target i; surplus sources stay unmapped.
 * - `auto`     — name matches first, then unmatched sources fill still-free
 *                targets in order ("按列名和顺序").
 */

export type ImportMappingMode = 'auto' | 'name' | 'position';

export type ImportMatchKind = 'name' | 'position' | 'manual' | 'none';

export interface ImportMappingEntry {
  /** Source column name exactly as parsed. */
  source: string;
  /** Target column name, or `null` for "don't import this column". */
  target: string | null;
  /** How this entry got its target (manual beats everything else). */
  matchedBy: ImportMatchKind;
}

export interface ImportMappingConflict {
  target: string;
  sources: string[];
}

export interface ImportMappingPlan {
  entries: ImportMappingEntry[];
  conflicts: ImportMappingConflict[];
  /** Source columns with no target (never imported) — surfaced, not hidden. */
  unmapped: string[];
  /** Target columns no source writes — informational (they keep defaults). */
  untargeted: string[];
}

export interface PlanImportMappingInput {
  sourceColumns: readonly string[];
  targetColumns: readonly string[];
  mode: ImportMappingMode;
  /** User overrides from the mapping table: source → target (or null for
   * "don't import"). Overrides win over every automatic rule. */
  overrides?: Readonly<Record<string, string | null>>;
}

/** Source columns whose duplicate names the parser flagged are pre-empted
 * here: the parser already reported them; the mapping treats each source
 * column by its exact name, duplicates included, so the user decides. */
export function planImportMapping({
  sourceColumns,
  targetColumns,
  mode,
  overrides = {},
}: PlanImportMappingInput): ImportMappingPlan {
  const entries: ImportMappingEntry[] = [];
  const usedTargets = new Set<string>();

  // 1. Manual overrides first — the user's word is final.
  const manualTargets = new Set<string>();
  for (const source of sourceColumns) {
    if (!(source in overrides)) continue;
    const target = overrides[source] ?? null;
    entries.push({ source, target, matchedBy: target == null ? 'manual' : 'manual' });
    if (target != null) {
      usedTargets.add(target);
      manualTargets.add(target);
    }
  }

  // 2. Name matching (auto and name modes): case-insensitive, and the
  //    recorded target is the real spelled name. When two sources claim the
  //    same target (e.g. `id` and `ID`), BOTH keep the claim — the conflict
  //    detector below flags it and the wizard blocks the start until the user
  //    resolves it. Silently dropping the second claim would lose a column.
  const targetByLower = new Map<string, string>();
  for (const target of targetColumns) {
    if (!targetByLower.has(target.toLowerCase())) targetByLower.set(target.toLowerCase(), target);
  }
  if (mode === 'auto' || mode === 'name') {
    for (const source of sourceColumns) {
      if (entries.some((entry) => entry.source === source)) continue;
      const target = targetByLower.get(source.toLowerCase());
      if (target == null) continue;
      entries.push({ source, target, matchedBy: 'name' });
      if (!usedTargets.has(target)) usedTargets.add(target);
    }
  }

  // 3. Positional fill (auto and position modes): unmatched sources take the
  //    next still-free target in target order.
  if (mode === 'auto' || mode === 'position') {
    const freeTargets = targetColumns.filter((target) => !usedTargets.has(target));
    let freeIndex = 0;
    for (const source of sourceColumns) {
      if (entries.some((entry) => entry.source === source)) continue;
      const target = freeTargets[freeIndex] ?? null;
      freeIndex += 1;
      if (target != null) usedTargets.add(target);
      entries.push({ source, target, matchedBy: target == null ? 'none' : 'position' });
    }
  }

  // 4. Anything still unassigned (name mode leftovers) lands unmapped.
  for (const source of sourceColumns) {
    if (entries.some((entry) => entry.source === source)) continue;
    entries.push({ source, target: null, matchedBy: 'none' });
  }

  // 5. Conflict detection: two or more active sources on one target. Manual
  //    targets are exempt from being counted as conflicting with automatic
  //    ones only when they are the single writer — a shared target is a
  //    conflict no matter how each side got there.
  const writersByTarget = new Map<string, string[]>();
  for (const entry of entries) {
    if (entry.target == null) continue;
    const list = writersByTarget.get(entry.target) ?? [];
    list.push(entry.source);
    writersByTarget.set(entry.target, list);
  }
  const conflicts: ImportMappingConflict[] = [];
  for (const [target, sources] of writersByTarget) {
    if (sources.length > 1) conflicts.push({ target, sources });
  }

  const unmapped = entries.filter((entry) => entry.target == null).map((entry) => entry.source);
  const untargeted = targetColumns.filter((target) => !usedTargets.has(target));

  // Keep the entries in source order — the mapping table mirrors the file.
  entries.sort((a, b) => sourceColumns.indexOf(a.source) - sourceColumns.indexOf(b.source));

  return { entries, conflicts, unmapped, untargeted };
}

/** A plan may only start an import when nothing conflicts and at least one
 * column is imported. Empty mappings are the wizard's own error. */
export function mappingIsExecutable(plan: ImportMappingPlan): boolean {
  return plan.conflicts.length === 0 && plan.entries.some((entry) => entry.target != null);
}
