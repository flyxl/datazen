# Task 8 Report — Onboarding continuous lifecycle journey tests

**Status:** Complete

## Summary

Added `onboardingJourney.test.tsx` covering the full first-run wizard state machine: store-level lifecycle, premature skip, DOM-level guide bar interactions, end-to-end WelcomePage-to-completion journey, and persistence recovery after module reload.

## Changes

| Area | Detail |
|------|--------|
| `onboardingJourney.test.tsx` | Store lifecycle: `not_started` → `active` step 2 → query → explore → `completed` |
| Skip path | Premature skip at step 2 leaves `skipped`; subsequent `markQueryExecuted` does not reactivate |
| DOM guide bar | Quick-run button → step 3 → complete button → guide bar unmounts |
| End-to-end journey | `WelcomePage` click `welcome-open-sample` → `initSampleDatabase` → `startOnboarding('sample_sqlite')` → render `OnboardingGuideBar` → quick run → explore → complete → unmount |
| Persistence recovery | After `vi.resetModules()`, dynamically re-import **both** `onboardingStore` and `OnboardingGuideBar` so the component binds to the freshly hydrated store from `localStorage`; completed/skipped statuses do not re-open the wizard |

## Test coverage

| Test | Validates |
|------|-----------|
| Store lifecycle | Full state transitions without DOM |
| Premature skip | Clean `skipped` terminal state |
| DOM guide bar journey | Button clicks drive step 2 → 3 → completion |
| End-to-end multi-step | WelcomePage CTA through guide bar DOM to completion |
| Persistence (completed) | Module reload + guide bar stays hidden |
| Persistence (skipped) | Module reload + guide bar stays hidden |

## Tests

```bash
npx vitest run src/windows/connection/__tests__/onboardingJourney.test.tsx
→ 6 passed
```

## Concerns / follow-ups

- End-to-end test simulates WelcomePage and OnboardingGuideBar as sequential renders (not a single ConnectionPage mount); ConnectionPage integration is covered separately in `ConnectionPageOnboarding.test.tsx`.
- Chart/AI explore step is asserted via `markAiOrChartExplored()` store call; full chart/AI DOM interaction is out of scope for this journey file.
- Persistence tests must remain last in the file (or re-import store in `beforeEach`) because `vi.resetModules()` invalidates top-level static imports.

## Commit

```
test(onboarding): expand end-to-end journey test and fix dynamic module hydration test
```
