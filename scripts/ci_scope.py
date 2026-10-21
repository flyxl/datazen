"""Select frontend/Rust CI from changes since the previous successful run."""

from __future__ import annotations

import json
import os
import subprocess
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime
from pathlib import PurePosixPath
from typing import Any


FRONTEND_ROOTS = (
    "packages/ui/",
    "packages/driver-sdk/",
    "packages/backend-client/",
    "packages/wapp-sdk/",
    "packages/extension-points/",
    "packages/wapps/",
)
SHARED_ROOTS = ("scripts/", ".github/workflows/", ".github/actions/")
FRONTEND_SUFFIXES = {".ts", ".tsx", ".js", ".jsx", ".css"}
ALL_ZERO_SHA = "0" * 40


def classify_changed_paths(paths: list[str]) -> tuple[bool, bool]:
    """Return (frontend, rust), conservatively enabling both for unknown paths."""
    frontend = rust = False
    for name in paths:
        path = PurePosixPath(name)
        parts = path.parts
        suffix = path.suffix
        if name.startswith("docs/") or suffix == ".md" or name.startswith(".superpowers/"):
            continue
        if name.startswith(SHARED_ROOTS) or name in {
            "package.json",
            "pnpm-lock.yaml",
            "pnpm-workspace.yaml",
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            "drivers-registry.json",
        }:
            frontend = rust = True
        elif name.startswith(("src/", "e2e/", "site/", "test/", "public/", "fixtures/themes/")):
            frontend = True
        elif name.startswith("src-tauri/") or (
            name.startswith("packages/") and suffix in {".rs", ".toml"}
        ):
            rust = True
        elif name.startswith((
            "tsconfig",
            "vite.config.",
            "vitest",
            "tailwind.config.",
            "postcss.config.",
        )) or name in {"index.html", "window.html"}:
            frontend = True
        elif name.startswith(FRONTEND_ROOTS):
            frontend = True
        elif (
            len(parts) >= 4
            and parts[0] == "packages"
            and parts[1] == "drivers"
            and parts[3] == "ui"
        ):
            frontend = True
        elif name.startswith("packages/") and suffix in FRONTEND_SUFFIXES:
            frontend = True
        else:
            # Unknown package assets/configuration are shared until ownership is explicit.
            frontend = rust = True
    return frontend, rust


def _parse_time(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def _is_before(run: dict[str, Any], current: dict[str, Any]) -> bool:
    run_time = _parse_time(run["created_at"])
    current_time = _parse_time(current["created_at"])
    if run_time != current_time:
        return run_time < current_time
    return int(run["id"]) < int(current["id"])


def find_previous_successful_run(
    runs: list[dict[str, Any]],
    *,
    current_run_id: str,
    current_sha: str,
    event_name: str,
    pr_number: str,
    head_branch: str,
) -> tuple[dict[str, Any] | None, bool, bool]:
    """Return (last success, has_prior_run, current_run_was_found)."""
    current = next((run for run in runs if str(run.get("id")) == current_run_id), None)
    if current is None:
        return None, False, False

    prior: list[dict[str, Any]] = []
    for run in runs:
        if run is current or not _is_before(run, current):
            continue
        if run.get("event") != event_name:
            continue
        if event_name == "pull_request":
            associated_prs = run.get("pull_requests") or []
            if not any(str(pr.get("number")) == pr_number for pr in associated_prs):
                continue
        elif run.get("head_branch") != head_branch:
            continue
        prior.append(run)

    prior.sort(
        key=lambda run: (_parse_time(run["created_at"]), int(run["id"])),
        reverse=True,
    )
    previous_success = next((
        run for run in prior
        if run.get("status") == "completed"
        and run.get("conclusion") == "success"
        and run.get("head_sha") != current_sha
        and run.get("head_sha")
    ), None)
    return previous_success, bool(prior), True


def select_ci_baseline(
    *,
    event_name: str,
    previous_success: dict[str, Any] | None,
    had_prior_run: bool,
    api_ok: bool,
    pr_base_sha: str,
    event_before: str,
) -> tuple[str, str]:
    """Choose an incremental base, or run both CI halves when history is unsafe."""
    if event_name == "workflow_dispatch":
        return "", "all"
    if previous_success:
        return str(previous_success["head_sha"]), "direct"
    if api_ok and had_prior_run:
        # A previous failed/cancelled run has unvalidated changes. Run both halves.
        return "", "all"
    if event_name == "pull_request" and pr_base_sha:
        # No prior run (or API unavailable): cover the full PR from its merge base.
        return pr_base_sha, "merge-base"
    if event_name == "push" and api_ok:
        if event_before and event_before != ALL_ZERO_SHA:
            return event_before, "direct"
    # Unknown history, unavailable API on push, or an unsupported event: fail closed.
    return "", "all"


def _list_workflow_runs(
    *, repository: str, event_name: str, head_branch: str, token: str
) -> list[dict[str, Any]]:
    runs: list[dict[str, Any]] = []
    for page in range(1, 11):  # GitHub limits one query to 1,000 runs.
        query = urllib.parse.urlencode({
            "event": event_name,
            "branch": head_branch,
            "per_page": 100,
            "page": page,
        })
        request = urllib.request.Request(
            f"https://api.github.com/repos/{repository}/actions/workflows/ci.yml/runs?{query}",
            headers={
                "Accept": "application/vnd.github+json",
                "Authorization": f"Bearer {token}",
                "X-GitHub-Api-Version": "2026-03-10",
            },
        )
        with urllib.request.urlopen(request, timeout=15) as response:
            data = json.load(response)
        page_runs = data.get("workflow_runs", [])
        runs.extend(page_runs)
        if len(page_runs) < 100:
            break
    return runs


def _resolve_baseline(env: dict[str, str]) -> tuple[str, str]:
    """Return (base SHA, mode); mode is direct, merge-base, or all."""
    event_name = env["EVENT_NAME"]
    if event_name == "workflow_dispatch":
        return "", "all"

    current_sha = env["CURRENT_SHA"]
    head_branch = env["HEAD_BRANCH"]
    repository = env["REPOSITORY"]
    token = env.get("GITHUB_TOKEN", "")
    api_ok = False
    previous_success = None
    had_prior_run = False
    current_run_found = False

    if event_name in {"pull_request", "push"} and token:
        try:
            runs = _list_workflow_runs(
                repository=repository,
                event_name=event_name,
                head_branch=head_branch,
                token=token,
            )
            previous_success, had_prior_run, current_run_found = find_previous_successful_run(
                runs,
                current_run_id=env["CURRENT_RUN_ID"],
                current_sha=current_sha,
                event_name=event_name,
                pr_number=env.get("PR_NUMBER", ""),
                head_branch=head_branch,
            )
            api_ok = current_run_found
        except (OSError, urllib.error.URLError, urllib.error.HTTPError, ValueError, KeyError):
            # The fallback below is deliberately broader, never narrower.
            api_ok = False

    return select_ci_baseline(
        event_name=event_name,
        previous_success=previous_success,
        had_prior_run=had_prior_run,
        api_ok=api_ok,
        pr_base_sha=env.get("PR_BASE_SHA", ""),
        event_before=env.get("EVENT_BEFORE", ""),
    )


def _git_output(args: list[str], cwd: str | None = None) -> str | None:
    result = subprocess.run(
        ["git", *args], check=False, text=True, capture_output=True, cwd=cwd
    )
    return result.stdout.strip() if result.returncode == 0 else None


def changed_paths(
    base_sha: str, current_sha: str, mode: str, cwd: str | None = None
) -> list[str] | None:
    if mode not in {"direct", "merge-base"} or not base_sha:
        return None
    if _git_output(["rev-parse", "--verify", f"{current_sha}^{{commit}}"], cwd) is None:
        return None
    if _git_output(["rev-parse", "--verify", f"{base_sha}^{{commit}}"], cwd) is None:
        return None
    if mode == "merge-base":
        merge_base = _git_output(["merge-base", base_sha, current_sha], cwd)
        if merge_base is None:
            return None
        base_sha = merge_base
    result = subprocess.run(
        ["git", "diff", "--name-only", "--no-renames", base_sha, current_sha],
        check=False,
        text=True,
        capture_output=True,
        cwd=cwd,
    )
    return result.stdout.splitlines() if result.returncode == 0 else None


def main() -> None:
    env = dict(os.environ)
    base_sha, mode = _resolve_baseline(env)
    changed = changed_paths(base_sha, env["CURRENT_SHA"], mode)
    if changed is None:
        frontend = rust = True
        changed_count = 0
        mode = "all"
    else:
        frontend, rust = classify_changed_paths(changed)
        changed_count = len(changed)

    output_path = env["GITHUB_OUTPUT"]
    with open(output_path, "a", encoding="utf-8") as output:
        output.write(f"frontend={str(frontend).lower()}\n")
        output.write(f"rust={str(rust).lower()}\n")
    print(
        f"::notice::CI scope: frontend={str(frontend).lower()}, "
        f"rust={str(rust).lower()}, baseline={mode}, changed_paths={changed_count}"
    )


if __name__ == "__main__":
    main()
