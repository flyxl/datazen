import type { DriverFixtureDefinition, HostContractJourneyId } from '../fixtures';
import { journeyAllowed, skipReason } from '../fixtures';

/** Core journeys delivered in F2. */
export const F2_CORE_JOURNEYS: readonly HostContractJourneyId[] = [
  'HC-DATA',
  'HC-FILTER',
  'HC-QUERY',
] as const;

/** Full Host connection contract (F3). */
export const ALL_CONTRACT_JOURNEYS: readonly HostContractJourneyId[] = [
  'HC-CONN',
  'HC-QUERY',
  'HC-DATA',
  'HC-FILTER',
  'HC-EDIT',
  'HC-STRUCT',
  'HC-INDEX',
  'HC-EXPORT',
  'HC-OBJ',
  'HC-EXPLAIN',
] as const;

export interface PlannedJourney {
  readonly id: HostContractJourneyId;
  readonly status: 'run' | 'skip';
  readonly reason: string | null;
}

/**
 * Plan which Host-contract journeys to run for a fixture.
 * Pure function — unit-tested; WDIO matrix consumes the result.
 */
export function planJourneys(
  fixture: DriverFixtureDefinition,
  journeys: readonly HostContractJourneyId[] = ALL_CONTRACT_JOURNEYS,
): PlannedJourney[] {
  return journeys.map((id) => {
    if (journeyAllowed(fixture, id)) {
      return { id, status: 'run', reason: null };
    }
    return { id, status: 'skip', reason: skipReason(fixture, id) };
  });
}

export function journeysToRun(plan: readonly PlannedJourney[]): HostContractJourneyId[] {
  return plan.filter((p) => p.status === 'run').map((p) => p.id);
}

export function describeMatrixTitle(fixture: DriverFixtureDefinition): string {
  return `Host contract @ ${fixture.id} (${fixture.displayName})`;
}

/** Pure assertion helpers used by journeys (also unit-tested). */
export function bodyContainsAll(body: string, fragments: readonly string[]): boolean {
  return fragments.every((f) => body.includes(f));
}

export function bodyContainsNone(body: string, fragments: readonly string[]): boolean {
  return fragments.every((f) => !body.includes(f));
}

export function paginationRangeVisible(body: string): boolean {
  return /\d+\s*-\s*\d+\s*\/\s*\d+/.test(body);
}
