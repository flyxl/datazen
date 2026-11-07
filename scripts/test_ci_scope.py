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
    is_docs_only_paths,
    resolve_docs_only,
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
    def test_scope_resolver_changes_only_run_their_own_unit_tests(self) -> None:
        self.assertEqual(
            classify_changed_paths([
                "scripts/ci_scope.py",
                "scripts/test_ci_scope.py",
            ]),
            (False, False),
        )

    def test_crates_rust_source_and_manifest_changes_skip_frontend(self) -> None:
        self.assertEqual(
            classify_changed_paths([
                "crates/datazen-server/tests/table_data_live_routes.rs",
                "crates/datazen-server/Cargo.toml",
            ]),
            (False, True),
        )

    def test_workspace_root_cargo_manifest_changes_still_run_both(self) -> None:
        self.assertEqual(
            classify_changed_paths(["Cargo.toml", "Cargo.lock"]),
            (True, True),
        )
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

    def test_unvalidated_prior_pr_run_uses_full_pr_diff_for_path_scoping(self) -> None:
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
            ("base-sha", "merge-base"),
        )

    def test_unvalidated_prior_push_run_still_runs_both_ci_halves(self) -> None:
        self.assertEqual(
            select_ci_baseline(
                event_name="push",
                previous_success=None,
                had_prior_run=True,
                api_ok=True,
                pr_base_sha="",
                event_before="before-sha",
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


def _repo_with_merged_main(repository: Path) -> tuple[str, str, str]:
    """构造「merge 过 main 的 docs-only 分支」夹具（PR #182 场景）。

    返回 (root_sha, main_tip_sha, merge_sha)：main_tip 即 PR base.sha，
    merge_sha 为 PR head。run 间窗口（root_sha...merge_sha）会被 main 的
    Cargo.toml bump 污染，而 PR 的 merge-base diff 只有 docs 文件。
    """
    _run_git(repository, "config", "user.name", "CI Scope Test")
    _run_git(repository, "config", "user.email", "ci-scope@example.invalid")
    root = _commit_file(repository, "README.md", "start\n", "base")
    main_tip = _commit_file(repository, "Cargo.toml", "bump\n", "release bump")
    _run_git(repository, "checkout", "-q", "-b", "docs-pr", root)
    _commit_file(repository, "docs/development/plan.md", "docs\n", "docs")
    _run_git(repository, "merge", "-q", "--no-ff", "-m", "merge main", main_tip)
    merge = _run_git(repository, "rev-parse", "HEAD")
    return root, main_tip, merge


class DocsOnlyExemptionTests(unittest.TestCase):
    """docs-only 豁免：merge-base diff 口径，push / 手动事件不受影响。"""

    def test_docs_only_pr_judged_from_merge_base_diff_not_run_window(self) -> None:
        with tempfile.TemporaryDirectory(
            prefix="datazen-ci-scope-", ignore_cleanup_errors=True
        ) as temporary:
            repository = Path(temporary)
            _init_repository(repository)
            root, main_tip, merge = _repo_with_merged_main(repository)

            # run 间窗口被 merge 进来的 Cargo.toml bump 污染（旧行为下被迫跑全量 CI）。
            window = changed_paths(root, merge, "direct", cwd=str(repository))
            self.assertIn("Cargo.toml", window or [])
            self.assertEqual(classify_changed_paths(window or []), (True, True))

            # 豁免只看 PR 的 merge-base diff：main_tip...merge 只有 docs 文件。
            merge_base_diff = changed_paths(main_tip, merge, "merge-base", cwd=str(repository))
            self.assertEqual(merge_base_diff, ["docs/development/plan.md"])
            self.assertEqual(classify_changed_paths(merge_base_diff or []), (False, False))
            self.assertTrue(
                resolve_docs_only(
                    event_name="pull_request",
                    pr_base_sha=main_tip,
                    head_sha=merge,
                    cwd=str(repository),
                )
            )

    def test_docs_plus_source_changes_are_not_docs_only(self) -> None:
        with tempfile.TemporaryDirectory(
            prefix="datazen-ci-scope-", ignore_cleanup_errors=True
        ) as temporary:
            repository = Path(temporary)
            _init_repository(repository)
            _run_git(repository, "config", "user.name", "CI Scope Test")
            _run_git(repository, "config", "user.email", "ci-scope@example.invalid")
            _commit_file(repository, "README.md", "start\n", "base")
            base = _commit_file(repository, "Cargo.toml", "bump\n", "release bump")
            _commit_file(repository, "docs/development/plan.md", "docs\n", "docs")
            head = _commit_file(repository, "src/app.tsx", "frontend\n", "feature")

            self.assertFalse(
                resolve_docs_only(
                    event_name="pull_request",
                    pr_base_sha=base,
                    head_sha=head,
                    cwd=str(repository),
                )
            )
            self.assertEqual(
                classify_changed_paths(
                    changed_paths(base, head, "merge-base", cwd=str(repository)) or []
                ),
                (True, False),
            )

    def test_empty_diff_keeps_current_skip_behavior(self) -> None:
        with tempfile.TemporaryDirectory(
            prefix="datazen-ci-scope-", ignore_cleanup_errors=True
        ) as temporary:
            repository = Path(temporary)
            _init_repository(repository)
            _run_git(repository, "config", "user.name", "CI Scope Test")
            _run_git(repository, "config", "user.email", "ci-scope@example.invalid")
            sha = _commit_file(repository, "README.md", "start\n", "base")

            # 空 diff：不做豁免声明（无证据不主张），与现状一致——
            # frontend/rust 为 False，两个 job 照样跳过。
            self.assertEqual(changed_paths(sha, sha, "merge-base", cwd=str(repository)), [])
            self.assertEqual(classify_changed_paths([]), (False, False))
            self.assertFalse(
                resolve_docs_only(
                    event_name="pull_request",
                    pr_base_sha=sha,
                    head_sha=sha,
                    cwd=str(repository),
                )
            )

    def test_push_and_workflow_dispatch_events_never_exempt(self) -> None:
        with tempfile.TemporaryDirectory(
            prefix="datazen-ci-scope-", ignore_cleanup_errors=True
        ) as temporary:
            repository = Path(temporary)
            _init_repository(repository)
            _, main_tip, merge = _repo_with_merged_main(repository)

            # 即便 diff 本身是纯 docs，事件不是 pull_request 就不豁免。
            for event_name in ("push", "workflow_dispatch"):
                self.assertFalse(
                    resolve_docs_only(
                        event_name=event_name,
                        pr_base_sha=main_tip,
                        head_sha=merge,
                        cwd=str(repository),
                    ),
                    msg=event_name,
                )

    def test_missing_or_unresolvable_base_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory(
            prefix="datazen-ci-scope-", ignore_cleanup_errors=True
        ) as temporary:
            repository = Path(temporary)
            _init_repository(repository)
            _run_git(repository, "config", "user.name", "CI Scope Test")
            _run_git(repository, "config", "user.email", "ci-scope@example.invalid")
            head = _commit_file(repository, "docs/a.md", "docs\n", "docs")

            self.assertFalse(
                resolve_docs_only(
                    event_name="pull_request", pr_base_sha="", head_sha=head,
                    cwd=str(repository),
                )
            )
            self.assertFalse(
                resolve_docs_only(
                    event_name="pull_request", pr_base_sha="missing-sha", head_sha=head,
                    cwd=str(repository),
                )
            )

    def test_docs_only_path_whitelist(self) -> None:
        # 纯 docs/**：豁免。
        self.assertTrue(
            is_docs_only_paths(["docs/development/plan.md", "docs/README.md"])
        )
        # 根级 markdown + 元文件 + .github 模板：豁免。
        self.assertTrue(
            is_docs_only_paths(
                [
                    "README.zh-CN.md",
                    "CHANGELOG.md",
                    "LICENSE",
                    ".gitignore",
                    ".github/PULL_REQUEST_TEMPLATE.md",
                    ".github/ISSUE_TEMPLATE/bug.md",
                ]
            )
        )
        # 混入代码 / CI 配置 / 代码目录内 md：不豁免。
        self.assertFalse(is_docs_only_paths(["docs/plan.md", "src/app.ts"]))
        self.assertFalse(is_docs_only_paths(["docs/plan.md", ".github/workflows/ci.yml"]))
        self.assertFalse(is_docs_only_paths(["e2e/contract/README.md"]))
        self.assertFalse(is_docs_only_paths(["site/manual.html"]))
        # 空 diff：不主张豁免。
        self.assertFalse(is_docs_only_paths([]))


if __name__ == "__main__":
    unittest.main()
