# Design: GitHub Community Health + Repo Settings Polish

**Date:** 2026-08-08  
**Branch:** `feature/github-community-health` (to create)  
**Status:** Approved  
**Baseline:** `health_percentage` ≈ 42% (`gh api repos/flyxl/datazen/community/profile`)

## Goals

1. Raise GitHub Community Health checklist coverage (description, README, LICENSE, CONTRIBUTING, CoC, issue/PR templates, SECURITY where applicable).
2. Fix incorrect public metadata (description still says MIT while repo is GPL-3.0).
3. Lightly polish README + existing issue templates; configure repo settings (topics, Discussions links, branch protection when permitted).

## Decisions (approved)

| Topic | Choice |
|-------|--------|
| Scope | Full polish: health files + repo settings + README/Issue template polish |
| Doc language | **English** for CoC / CONTRIBUTING / SECURITY / PR & Issue templates |
| Approach | **A** — standard health-file kit + `gh` settings + light README polish (not a heavy contributor handbook) |
| Code of Conduct | Contributor Covenant 2.1 |
| CoC / security contact | Existing email `wuxiaolongklws@gmail.com` |

## Current gaps

- Missing: `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, `SECURITY.md`, `.github/PULL_REQUEST_TEMPLATE.md`
- Repo `description` claims MIT; `license` is GPL-3.0
- Community profile reports `issue_template: null` despite `.github/ISSUE_TEMPLATE/*.yml` (re-check after polish; ensure templates remain valid)
- Topics sparse: `database`, `mysql`, `open-source`, `postgresql`, `redis`, `rust`, `tauri`
- README may need clearer community entry points; screenshot paths under `docs/screenshots/` exist — verify links still resolve

## Deliverables

### A. Health files (repo root / `.github/`)

| Path | Content |
|------|---------|
| `CODE_OF_CONDUCT.md` | Contributor Covenant 2.1; enforcement contact = project email |
| `CONTRIBUTING.md` | Dev setup (`pnpm`, Cargo), tests (`pnpm test:unit`, `cargo test -p datazen --lib`), PR workflow, plugin note, links to `AGENTS.md` / architecture docs / website |
| `SECURITY.md` | Private disclosure via email; expected response window; supported versions (latest release + main) |
| `.github/PULL_REQUEST_TEMPLATE.md` | Summary, Test plan, Checklist (tests, docs, no secrets) |

### B. Issue templates polish

- Keep `bug_report.yml`, `feature_request.yml`
- Update `config.yml`: keep email + releases; add Discussions link (`https://github.com/flyxl/datazen/discussions`)
- Ensure YAML remains valid so GitHub recognizes issue forms

### C. Repo settings via `gh` / API

- **Description** (example target):  
  `Lightweight GPL-3.0 desktop database client (PostgreSQL / MySQL / SQLite / Redis). Built with Tauri + Rust, under 10MB.`
- **Homepage:** keep `https://flyxl.github.io/datazen/`
- **Topics:** keep existing; add e.g. `sqlite`, `desktop`, `sql`, `ai`, `cross-platform`, `mariadb` (only accurate tags)
- **Branch protection on `main`:** if admin permissions allow — require PR before merge + require status check for CI workflow (`CI` / job that runs unit tests + site SEO check). If API returns 403, document manual Settings steps in the plan; do not block the rest of the work.

### D. README polish

- Ensure license badge/text says GPL-3.0 (already does in places — audit for MIT leftovers)
- Add links to Website, CONTRIBUTING, SECURITY, Code of Conduct (footer or “Contributing” section)
- Fix any broken screenshot / doc links discovered during audit
- Keep bilingual product blurb; policy docs remain English-primary

## Out of scope

- Rewriting AGENTS.md or full architecture docs
- Enabling Wiki
- Changing license SPDX from GPL-3.0
- Heavy multi-chapter contributor guide (Approach B)
- Paid GitHub features beyond what free public repos support

## Acceptance

- [ ] Community health files present and linked from README
- [ ] Repo description no longer mentions MIT; matches GPL-3.0
- [ ] Topics expanded with accurate tags
- [ ] PR template + polished issue templates live
- [ ] `gh api repos/flyxl/datazen/community/profile` shows higher `health_percentage` and non-null CoC / contributing / PR template (and issue template if GitHub counts forms)
- [ ] Branch protection applied **or** documented skip reason
- [ ] Work on dedicated branch; merge/push only when requested

## Notes

- GitHub’s community health score can lag after pushes; verify after merge to `main`.
- SECURITY.md is not always counted in the classic percentage but is best practice for a desktop app handling credentials.
