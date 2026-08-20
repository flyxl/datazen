# Architecture Review Progress — 2026-08-20

Branch: `feat/architecture-review-2026-08`  
Worktree: `../datazen-arch-review`  
Base: `main` @ c5e9c656

## Review source

Full-repo review against AGENTS.md + `docs/architecture/` (12 findings F1–F12).  
Large refactors (F1–F4 driver command migration) deferred; this branch targets quick, testable alignment fixes.

## Status legend

| Status | Meaning |
|--------|---------|
| pending | Not started |
| dev_done | Implemented + unit tests; awaiting QA agent |
| tested_fail | QA found bugs; needs fix |
| tested_pass | QA passed |
| deferred | Out of scope this branch |

---

## F11 — `--dt-binary` theme token

| Field | Value |
|-------|-------|
| Severity | low |
| Status | **dev_done** |
| Scope | Add `--dt-binary`; map `dataTypeColors` binary family; align Host + community packs |
| Unit tests | `dataTypeColors.test.ts`, `communityThemePacks.test.ts` |
| E2E | N/A (visual token) |

---

## F6 — Stale `query-*` window capabilities

| Field | Value |
|-------|-------|
| Severity | medium |
| Status | **dev_done** |
| Scope | Remove unused `query-*` / legacy `connection-*` labels from `default.json.host` |
| Unit tests | capability merge smoke / existing window tests |
| E2E | settings/backup sub-window smoke |

---

## F9 — Architecture doc drift (subset)

| Field | Value |
|-------|-------|
| Severity | medium |
| Status | **dev_done** |
| Scope | Fix store count, Tailwind 4, Redis UI path, IPC list in architecture docs |
| Unit tests | none |
| E2E | none |

---

## F8 — Legacy sync IPC surface

| Field | Value |
|-------|-------|
| Severity | medium |
| Status | deferred |
| Notes | Requires E2E migration `data-sync-real.ts`; separate PR |

---

## F1–F4 — Host driver hardcoding

| Field | Value |
|-------|-------|
| Status | **deferred** |
| Notes | Requires driver Command API migration; track as RFC |

---

## QA log

| Date | Agent | Feature | Result | Notes |
|------|-------|---------|--------|-------|
| — | — | — | — | — |

## Bugs

| ID | Feature | Steps | Status |
|----|---------|-------|--------|
| — | — | — | — |
