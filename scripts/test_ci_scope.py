"""Tests for incremental frontend/Rust CI selection."""

from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path
from typing import Any

from scripts.ci_scope import (
    changed_paths,
    classify_changed_paths,
    find_previous_successful_run,
    select_ci_baseline,
)


def _run_git(repository: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=repository,
        check=True,
        text=True,
        capture_output=True,
    )
    return result.stdout.strip()


def _commit_file(repository: Path, name: str, contents: str, message: str) -> str:
    path = repository / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents, encoding="utf-8")
    _run_git(repository, "add", name)
    _run_git(repository, "commit", "-m", message)
    return _run_git(repository, "rev-parse", "HEAD")


def _init_repository(repository: Path) -> None:
    """初始化夹具仓库，并关掉 git 的后台自动维护。

    `git commit` 末尾会触发 `git gc --auto`；autoDetach（默认开启）让它 fork
    后台进程，与测试结束时 TemporaryDirectory 的 rmtree 竞态——ubuntu-latest
    git 2.55 实测复现 `OSError: [Errno 39] Directory not empty: 'pack'`。
    夹具仓库只有两三个提交，无需 gc，直接关闭。
    """
    _run_git(repository, "init", "-q")
    _run_git(repository, "config", "gc.auto", "0")
    _run_git(repository, "config", "gc.autoDetach", "false")


class CiScopeTests(unittest.TestCase):
    def test_diff_covers_all_commits_since_last_success_not_only_head_commit(self) -> None:
        with tempfile.TemporaryDirectory(
            prefix="datazen-ci-scope-", ignore_cleanup_errors=True
        ) as temporary:
            repository = Path(temporary)
            _init_repository(repository)
            _run_git(repository, "config", "user.name", "CI Scope Test")
            _run_git(repository, "config", "user.email", "ci-scope@example.invalid")
            _commit_file(repository, "README.md", "start\n", "base")
            last_success = _commit_file(repository, "src/app.tsx", "frontend\n", "frontend")
            _commit_file(repository, "src-tauri/src/lib.rs", "backend\n", "backend")
            current = _commit_file(repository, "docs/change.md", "docs\n", "docs")

            changed = changed_paths(last_success, current, "direct", cwd=str(repository))
            self.assertEqual(changed, ["docs/change.md", "src-tauri/src/lib.rs"])
            self.assertEqual(classify_changed_paths(changed), (False, True))

    def test_only_frontend_changes_since_last_success_skips_rust(self) -> None:
        with tempfile.TemporaryDirectory(
            prefix="datazen-ci-scope-", ignore_cleanup_errors=True
        ) as temporary:
            repository = Path(temporary)
            _init_repository(repository)
            _run_git(repository, "config", "user.name", "CI Scope Test")
            _run_git(repository, "config", "user.email", "ci-scope@example.invalid")
            last_success = _commit_file(repository, "src-tauri/src/lib.rs", "backend\n", "backend")
            current = _commit_file(repository, "src/app.tsx", "frontend\n", "frontend")

            changed = changed_paths(last_success, current, "direct", cwd=str(repository))
            self.assertEqual(classify_changed_paths(changed or []), (True, False))

    def test_selects_latest_success_before_current_and_same_pr(self) -> None:
        runs: list[dict[str, Any]] = [
            self._run(30, "2026-10-21T12:00:00Z", "sha-current", 76, "completed", "success"),
            self._run(29, "2026-10-21T11:00:00Z", "sha-failed", 76, "completed", "failure"),
            self._run(28, "2026-10-21T10:00:00Z", "sha-last-good", 76, "completed", "success"),
            self._run(27, "2026-10-21T09:00:00Z", "sha-other-pr", 75, "completed", "success"),
        ]

        previous, had_prior, current_found = find_previous_successful_run(
            runs,
            current_run_id="30",
            current_sha="sha-current",
            event_name="pull_request",
            pr_number="76",
            head_branch="feature",
        )

        self.assertTrue(current_found)
        self.assertTrue(had_prior)
        self.assertEqual(previous["head_sha"], "sha-last-good")

    def test_unvalidated_prior_run_is_reported_without_using_failed_head_as_base(self) -> None:
        runs = [
            self._run(30, "2026-10-21T12:00:00Z", "sha-current", 76, "completed", "success"),
            self._run(29, "2026-10-21T11:00:00Z", "sha-failed", 76, "completed", "failure"),
        ]

        previous, had_prior, current_found = find_previous_successful_run(
            runs,
            current_run_id="30",
            current_sha="sha-current",
            event_name="pull_request",
            pr_number="76",
            head_branch="feature",
        )

        self.assertIsNone(previous)
        self.assertTrue(had_prior)
        self.assertTrue(current_found)

        self.assertEqual(
            select_ci_baseline(
                event_name="pull_request",
                previous_success=previous,
                had_prior_run=had_prior,
                api_ok=current_found,
                pr_base_sha="base-sha",
                event_before="",
            ),
            ("", "all"),
        )

    def test_first_pull_request_run_checks_full_pr_change_range(self) -> None:
        self.assertEqual(
            select_ci_baseline(
                event_name="pull_request",
                previous_success=None,
                had_prior_run=False,
                api_ok=True,
                pr_base_sha="base-sha",
                event_before="",
            ),
            ("base-sha", "merge-base"),
        )

    def test_unavailable_push_history_runs_both_halves(self) -> None:
        self.assertEqual(
            select_ci_baseline(
                event_name="push",
                previous_success=None,
                had_prior_run=False,
                api_ok=False,
                pr_base_sha="",
                event_before="before-sha",
            ),
            ("", "all"),
        )

    @staticmethod
    def _run(
        run_id: int,
        created_at: str,
        sha: str,
        pr_number: int,
        status: str,
        conclusion: str,
    ) -> dict[str, Any]:
        return {
            "id": run_id,
            "created_at": created_at,
            "head_sha": sha,
            "head_branch": "feature",
            "event": "pull_request",
            "status": status,
            "conclusion": conclusion,
            "pull_requests": [{"number": pr_number}],
        }


if __name__ == "__main__":
    unittest.main()
