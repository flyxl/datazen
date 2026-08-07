# GitHub Community Health Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Raise GitHub community health (from ~42%) by adding standard English health files, fixing MIT-vs-GPL description metadata, polishing README/issue templates, and updating repo settings (topics, Discussions link, branch protection when permitted).

**Architecture:** Mostly documentation + GitHub metadata. Files live at repo root and `.github/`. Settings are applied with `gh api` against `flyxl/datazen`. No application runtime code changes.

**Tech Stack:** Markdown, GitHub Issue Forms YAML, `gh` CLI, Contributor Covenant 2.1.

**Spec:** `docs/superpowers/specs/2026-08-08-github-community-health-design.md`  
**Branch:** `feature/github-community-health` (already created; design commit may already be on tip)

## Global Constraints

- Community policy docs (**CODE_OF_CONDUCT**, **CONTRIBUTING**, **SECURITY**, PR/Issue templates) are **English**.
- Contact / CoC enforcement / security email: `wuxiaolongklws@gmail.com`
- License remains **GPL-3.0** (never MIT in description or docs).
- Target description string: `Lightweight GPL-3.0 desktop database client (PostgreSQL / MySQL / SQLite / Redis). Built with Tauri + Rust, under 10MB.`
- Homepage: `https://flyxl.github.io/datazen/`
- Discussions URL: `https://github.com/flyxl/datazen/discussions`
- Approach A only — no heavy contributor handbook; no Wiki; do not rewrite `AGENTS.md`.
- Merge/push only when the user asks.

---

## File map

| Path | Role |
|------|------|
| `CODE_OF_CONDUCT.md` | Contributor Covenant 2.1 |
| `CONTRIBUTING.md` | How to build, test, PR |
| `SECURITY.md` | Vulnerability disclosure |
| `.github/PULL_REQUEST_TEMPLATE.md` | PR form |
| `.github/ISSUE_TEMPLATE/config.yml` | Contact links + Discussions |
| `.github/ISSUE_TEMPLATE/bug_report.yml` | Light polish |
| `.github/ISSUE_TEMPLATE/feature_request.yml` | Light polish |
| `README.md` | Community links + link audit |
| (remote) repo description / topics / branch protection | Via `gh` |

---

### Task 1: Add `CODE_OF_CONDUCT.md`

**Files:**
- Create: `CODE_OF_CONDUCT.md`

- [ ] **Step 1: Create the file**

Write `CODE_OF_CONDUCT.md` using **Contributor Covenant v2.1** full text from https://www.contributor-covenant.org/version/2/1/code_of_conduct/ (Markdown).

Set enforcement contact placeholders to:

- `wuxiaolongklws@gmail.com`

If fetching the official Markdown, replace `[INSERT CONTACT METHOD]` / similar with that email. Do not invent a different CoC.

Minimum acceptable structure if offline: paste the standard Covenant 2.1 sections (Our Pledge, Our Standards, Enforcement Responsibilities, Scope, Enforcement, Enforcement Guidelines, Attribution) with the email above and Attribution linking to https://www.contributor-covenant.org/version/2/1/code_of_conduct.html.

- [ ] **Step 2: Commit**

```bash
git add CODE_OF_CONDUCT.md
git commit -m "$(cat <<'EOF'
docs: add Contributor Covenant code of conduct

Establish community standards and a clear enforcement contact.
EOF
)"
```

---

### Task 2: Add `CONTRIBUTING.md` and `SECURITY.md`

**Files:**
- Create: `CONTRIBUTING.md`
- Create: `SECURITY.md`

- [ ] **Step 1: Write `CONTRIBUTING.md`**

```markdown
# Contributing to DataZen

Thanks for your interest in contributing. This guide covers the basics for
working on the main repository. For deeper architecture notes, see
[`AGENTS.md`](AGENTS.md) and [`docs/architecture/`](docs/architecture/).

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to contribute

- Bug reports and feature ideas via [Issues](https://github.com/flyxl/datazen/issues)
  (use the templates)
- Questions and discussion via [Discussions](https://github.com/flyxl/datazen/discussions)
- Pull requests for fixes and improvements
- Security issues: see [SECURITY.md](SECURITY.md) (do **not** open a public issue)

## Development setup

### Prerequisites

- Node.js ≥ 20
- pnpm ≥ 9
- Rust ≥ 1.77
- Tauri v2 system dependencies: https://v2.tauri.app/start/prerequisites/

### Run the app

```bash
pnpm install
pnpm tauri:dev
```

Plugin selection examples:

```bash
pnpm tauri:dev --plugins=none
pnpm tauri:dev --plugins=kiwi
```

### Tests (PR baseline)

```bash
pnpm test:unit
cargo test -p datazen --lib
```

Optional site SEO structural check (marketing site under `site/`):

```bash
node scripts/check-site-seo.mjs
```

E2E requires a webdriver debug build — see [`docs/e2e-testing.md`](docs/e2e-testing.md). Do not use a bare `cargo build` as the E2E binary.

## Pull requests

1. Fork and create a branch from `main`.
2. Keep changes focused; match existing Rust / TypeScript style.
3. Run the baseline tests above.
4. Fill out the PR template (summary, test plan, checklist).
5. Link related issues when applicable.

Frontend IPC args use `snake_case` keys to match the Rust commands. Avoid editing generated files (`src/plugins/generated.ts`, `src-tauri/src/plugin_init.rs`).

## Plugins

External drivers use the compile-time plugin system (`plugins-registry.json`,
`register_driver!`). See [`docs/plugin-development.md`](docs/plugin-development.md)
if present, or `AGENTS.md` for an overview.

## License

By contributing, you agree that your contributions will be licensed under the
same [GPL-3.0](LICENSE) license as the project.
```

- [ ] **Step 2: Write `SECURITY.md`**

```markdown
# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| Latest GitHub Release | ✅ |
| `main` branch | ✅ (best-effort) |
| Older releases | ❌ Please upgrade |

## Reporting a vulnerability

DataZen stores database credentials and related secrets on the local machine.
Please **do not** file public GitHub issues for security vulnerabilities.

**Email:** [wuxiaolongklws@gmail.com](mailto:wuxiaolongklws@gmail.com)

Please include:

- Affected version (release tag or commit)
- Platform (macOS / Windows / Linux)
- Impact description and steps to reproduce (if possible)
- Whether you plan a public write-up and preferred timeline

## Response expectations

We aim to acknowledge security reports within **7 days** and to share an
initial assessment or remediation plan within **30 days**. Complex issues may
take longer; we will keep you updated.

## Safe harbor

We welcome good-faith research. Please avoid privacy violations, service
disruption, or accessing data that is not yours.
```

- [ ] **Step 3: Commit**

```bash
git add CONTRIBUTING.md SECURITY.md
git commit -m "$(cat <<'EOF'
docs: add CONTRIBUTING and SECURITY policies

Document build/test/PR expectations and private vulnerability reporting.
EOF
)"
```

---

### Task 3: PR template + issue template polish

**Files:**
- Create: `.github/PULL_REQUEST_TEMPLATE.md`
- Modify: `.github/ISSUE_TEMPLATE/config.yml`
- Modify: `.github/ISSUE_TEMPLATE/bug_report.yml`
- Modify: `.github/ISSUE_TEMPLATE/feature_request.yml`

- [ ] **Step 1: Create `.github/PULL_REQUEST_TEMPLATE.md`**

```markdown
## Summary

<!-- What changed and why (1–3 bullets). -->

-

## Test plan

- [ ] `pnpm test:unit`
- [ ] `cargo test -p datazen --lib`
- [ ] Manual / E2E checks (describe):

## Checklist

- [ ] I followed [CONTRIBUTING.md](../CONTRIBUTING.md)
- [ ] No secrets or credentials in the diff
- [ ] Docs / i18n updated if user-facing behavior changed
```

Note: relative link `../CONTRIBUTING.md` from `.github/` may not resolve on GitHub’s PR UI — prefer absolute path style used by GitHub: `CONTRIBUTING.md` at repo root referenced as `/flyxl/datazen/blob/main/CONTRIBUTING.md` or simply `CONTRIBUTING.md` in backticks. Use:

```markdown
- [ ] I followed CONTRIBUTING.md
```

- [ ] **Step 2: Update `.github/ISSUE_TEMPLATE/config.yml`**

```yaml
blank_issues_enabled: false
contact_links:
  - name: Ask a question (Discussions)
    url: https://github.com/flyxl/datazen/discussions
    about: General questions, ideas, and community discussion
  - name: Email support
    url: mailto:wuxiaolongklws@gmail.com
    about: Questions, partnership, or private feedback via email
  - name: Download releases
    url: https://github.com/flyxl/datazen/releases
    about: Get the latest build for macOS, Windows, or Linux
  - name: Security vulnerability
    url: https://github.com/flyxl/datazen/blob/main/SECURITY.md
    about: Report security issues privately (do not file a public issue)
```

- [ ] **Step 3: Light polish on issue forms**

In `bug_report.yml` markdown intro, add a line pointing questions to Discussions.

In `feature_request.yml` markdown intro, add Discussions link similarly.

Bump version placeholder examples to a current-ish version (e.g. `0.0.8`) if still showing `0.0.3`.

Validate YAML:

```bash
python3 -c "import yaml,sys; yaml.safe_load(open('.github/ISSUE_TEMPLATE/config.yml')); yaml.safe_load(open('.github/ISSUE_TEMPLATE/bug_report.yml')); yaml.safe_load(open('.github/ISSUE_TEMPLATE/feature_request.yml')); print('YAML OK')"
```

If PyYAML missing: `node -e` is fine, or `ruby -ryaml -e '...'`. Fix until parse succeeds.

- [ ] **Step 4: Commit**

```bash
git add .github/PULL_REQUEST_TEMPLATE.md .github/ISSUE_TEMPLATE/
git commit -m "$(cat <<'EOF'
docs: add PR template and polish issue forms

Point community questions to Discussions and link SECURITY for vulns.
EOF
)"
```

---

### Task 4: README community links + audit

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Audit**

```bash
rg -n 'MIT|mit-licensed' README.md || true
# Verify screenshot files exist for every docs/screenshots/* referenced
rg -o 'docs/screenshots/[^")]+' README.md | while read f; do test -f "$f" && echo "OK $f" || echo "MISSING $f"; done
```

Fix any `MISSING` paths (point to existing assets under `docs/screenshots/` or `site/assets/screenshots/` — prefer keeping README screenshots in `docs/screenshots/` if files exist).

- [ ] **Step 2: Update header links**

Change the contact line (~18–24) to include Contributing, e.g.:

```html
<p align="center">
  <a href="https://github.com/flyxl/datazen/releases"><strong>Download</strong></a>
  ·
  <a href="https://flyxl.github.io/datazen/">Website</a>
  ·
  <a href="CONTRIBUTING.md">Contributing</a>
  ·
  <a href="mailto:wuxiaolongklws@gmail.com">Contact</a>
</p>
```

Update the TOC line (~34) to include Contributing / Security anchors if you add sections.

- [ ] **Step 3: Expand Contact section**

In `## Contact & feedback`, add rows:

| Channel | Link |
|---------|------|
| **Discussions** | https://github.com/flyxl/datazen/discussions |
| **Contributing** | [CONTRIBUTING.md](CONTRIBUTING.md) |
| **Security** | [SECURITY.md](SECURITY.md) |
| **Code of Conduct** | [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) |

Keep existing Email / Issues / Releases rows.

- [ ] **Step 4: Optional short Contributing section**

Before or after `## 开发`, add:

```markdown
## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, tests, and PR expectations.
Please follow the [Code of Conduct](CODE_OF_CONDUCT.md). Security reports go to [SECURITY.md](SECURITY.md).
```

Keep bilingual product copy; do not translate the whole README to English-only.

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "$(cat <<'EOF'
docs: link README to community health docs

Surface Contributing, Security, CoC, and Discussions for newcomers.
EOF
)"
```

---

### Task 5: GitHub repo settings (`gh`)

**Files:** none required in git (remote settings). Optionally add a short note to `docs/marketing/` or the plan report if branch protection is skipped — prefer recording in the task report only unless protection docs are needed long-term.

**Interfaces:**
- Produces: updated remote `description`, `homepage`, `topics`; branch protection on `main` **or** documented 403 skip

- [ ] **Step 1: Fix description + homepage**

```bash
gh repo edit flyxl/datazen \
  --description "Lightweight GPL-3.0 desktop database client (PostgreSQL / MySQL / SQLite / Redis). Built with Tauri + Rust, under 10MB." \
  --homepage "https://flyxl.github.io/datazen/"
```

Verify:

```bash
gh api repos/flyxl/datazen --jq '{description,homepage,license:.license.spdx_id}'
```

Expected: description contains `GPL-3.0`, **no** `MIT`; homepage correct; license `GPL-3.0`.

- [ ] **Step 2: Expand topics**

```bash
gh repo edit flyxl/datazen --add-topic sqlite --add-topic desktop --add-topic sql --add-topic ai --add-topic cross-platform --add-topic mariadb
```

Verify topics include previous ones plus new:

```bash
gh api -H "Accept: application/vnd.github+json" repos/flyxl/datazen/topics --jq '.names'
```

- [ ] **Step 3: Branch protection (best-effort)**

Discover exact CI check name from a recent run:

```bash
gh run list --workflow=ci.yml --limit 1
# Then open the run and note the job/check name, often "test" or "CI / test"
```

Attempt (adjust `contexts` to the real check name if different):

```bash
gh api -X PUT repos/flyxl/datazen/branches/main/protection \
  -H "Accept: application/vnd.github+json" \
  -f required_status_checks='{"strict":true,"contexts":["test"]}' \
  -F enforce_admins=false \
  -f required_pull_request_reviews='{"required_approving_review_count":0}' \
  -F restrictions= \
  -F allow_force_pushes=false \
  -F allow_deletions=false
```

If this fails (403 / validation), **do not block**. Record in the task report:

```text
Branch protection SKIPPED: <error>
Manual steps: Settings → Branches → Add rule for main →
  Require a pull request before merging;
  Require status checks to pass (select CI job).
```

Simpler alternative if PUT JSON is awkward — use:

```bash
gh api repos/flyxl/datazen/branches/main/protection -X PUT --input - <<'EOF'
{
  "required_status_checks": { "strict": true, "contexts": ["test"] },
  "enforce_admins": false,
  "required_pull_request_reviews": { "required_approving_review_count": 0 },
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false
}
EOF
```

- [ ] **Step 4: Commit nothing if only remote settings changed**

If you created a small `docs/...` note about manual branch protection, commit it; otherwise no commit required for this task — settings apply immediately on the remote.

---

### Task 6: Verify community health score

**Files:** none (verification)

- [ ] **Step 1: Ensure all local health files exist on the branch**

```bash
test -f CODE_OF_CONDUCT.md && test -f CONTRIBUTING.md && test -f SECURITY.md \
  && test -f .github/PULL_REQUEST_TEMPLATE.md && echo "files OK"
```

- [ ] **Step 2: Push is required for GitHub to recompute health**

Health percentage updates from **default branch content**. Until this branch is merged to `main` and pushed, local files won’t raise the score. For this task:

1. Confirm files are committed on `feature/github-community-health`.
2. Re-check remote description (Task 5 already applied to the repo object — description/topics update immediately).
3. Record baseline vs post-description:

```bash
gh api repos/flyxl/datazen/community/profile --jq '{health_percentage, description, files: {coc: .files.code_of_conduct_file.path, contributing: .files.contributing.path, pr: .files.pull_request_template.path, issue: (.files.issue_template != null), license: .files.license.name, readme: (.files.readme != null)}}'
```

After merge+push to `main` (user-gated later), expect CoC / CONTRIBUTING / PR template paths non-null and `health_percentage` > 42.

- [ ] **Step 3: Write verification notes into the implementer report** (no separate commit required unless adding a short checklist doc).

---

## Spec coverage self-check

| Spec item | Task |
|-----------|------|
| CODE_OF_CONDUCT.md | 1 |
| CONTRIBUTING.md + SECURITY.md | 2 |
| PR template + issue polish + Discussions | 3 |
| README links + audit | 4 |
| Description GPL fix + topics + branch protection | 5 |
| Health verification notes | 6 |
| English policy docs | Global |
| No heavy handbook / no Wiki | Global |

## Placeholder / consistency scan

- Email consistently `wuxiaolongklws@gmail.com`
- Description string matches Global Constraints verbatim
- LICENSE remains GPL-3.0; no MIT in new copy
