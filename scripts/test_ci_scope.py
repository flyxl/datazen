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
    classify_half_windows,
    find_latest_green_job_sha,
    find_previous_successful_run,
    is_docs_only_paths,
    resolve_docs_only,
    resolve_half_baselines,
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


class HalfBaselineTests(unittest.TestCase):
    """半边基线：整轮 run 失败但单边 job 绿时，绿的那半边用 direct 增量窗口。

    场景原型（PR #205）：frontend job 绿、rust job 红 → 修复 rust 后推送，
    旧逻辑退 merge-base 全量让 frontend 陪跑；新逻辑 frontend 半边取绿
    run 的 sha 做 direct 基线，窗口内无前端文件即跳过。
    """

    PR_NUMBER = "76"

    def _prior_run(self, run_id: int, created_at: str, sha: str,
                   status: str, conclusion: str) -> dict[str, Any]:
        return CiScopeTests._run(run_id, created_at, sha, 76, status, conclusion)

    @staticmethod
    def _job(name: str, status: str, conclusion: str | None) -> dict[str, Any]:
        return {"name": name, "status": status, "conclusion": conclusion}

    def _find_green(
        self,
        runs: list[dict[str, Any]],
        *,
        jobs_fetcher: Any,
        job_name: str = "frontend",
        current_run_id: str,
        current_sha: str = "sha-current",
    ) -> str | None:
        return find_latest_green_job_sha(
            runs,
            job_name=job_name,
            current_run_id=current_run_id,
            current_sha=current_sha,
            event_name="pull_request",
            pr_number=self.PR_NUMBER,
            head_branch="feature",
            jobs_fetcher=jobs_fetcher,
        )

    def test_green_half_job_yields_direct_baseline(self) -> None:
        """rust 红 + frontend 绿：frontend 半边取该 run 的 sha，rust 沿用公共基线。"""
        runs = [
            self._prior_run(30, "2026-11-07T12:00:00Z", "sha-current", "in_progress", None),
            self._prior_run(29, "2026-11-07T11:00:00Z", "sha-failed", "completed", "failure"),
        ]
        jobs = {
            29: [
                self._job("detect-changes", "completed", "success"),
                self._job("frontend", "completed", "success"),
                self._job("rust", "completed", "failure"),
            ],
        }
        bases = resolve_half_baselines(
            common_base=("base-sha", "merge-base"),
            runs=runs,
            current_run_id="30",
            current_sha="sha-current",
            event_name="pull_request",
            pr_number=self.PR_NUMBER,
            head_branch="feature",
            jobs_fetcher=lambda run_id: jobs[run_id],
        )
        self.assertEqual(bases["frontend"], ("sha-failed", "direct"))
        self.assertEqual(bases["rust"], ("base-sha", "merge-base"))

    def test_green_sha_covers_later_red_runs_changes(self) -> None:
        """中间有 frontend 红 run：窗口起点取「最近的」绿，红 run 的变更不漏。"""
        runs = [
            self._prior_run(31, "2026-11-07T13:00:00Z", "sha-current", "in_progress", None),
            # 更近的 run：frontend 也红（如依赖安装故障），不能当基线。
            self._prior_run(30, "2026-11-07T12:00:00Z", "sha-frontend-red", "completed", "failure"),
            # 再往前：frontend 绿（rust 红）——最近的正面证据。
            self._prior_run(29, "2026-11-07T11:00:00Z", "sha-frontend-green", "completed", "failure"),
        ]
        jobs = {
            30: [self._job("frontend", "completed", "failure")],
            29: [self._job("frontend", "completed", "success")],
        }
        sha = self._find_green(
            runs, current_run_id="31", jobs_fetcher=lambda run_id: jobs[run_id]
        )
        # 取 sha-frontend-green 而非更近的 sha-frontend-red：direct 窗口
        # sha-frontend-green..sha-current 覆盖 sha-frontend-red 的前端变更。
        self.assertEqual(sha, "sha-frontend-green")

    def test_same_sha_re_run_trusts_latest_record_only(self) -> None:
        """同 sha 的 re-run 红了：不采信更早同 sha 的旧绿（最新证据优先）。"""
        runs = [
            self._prior_run(31, "2026-11-07T13:00:00Z", "sha-current", "in_progress", None),
            # 同 sha-re 的两条记录：最新的 re-run frontend 红，较早的原 run 绿。
            self._prior_run(30, "2026-11-07T12:00:00Z", "sha-re", "completed", "failure"),
            self._prior_run(29, "2026-11-07T11:00:00Z", "sha-re", "completed", "failure"),
            # 更早的正常绿 run。
            self._prior_run(28, "2026-11-07T10:00:00Z", "sha-old-green", "completed", "failure"),
        ]
        jobs = {
            30: [self._job("frontend", "completed", "failure")],  # re-run 后的红
            29: [self._job("frontend", "completed", "success")],  # 旧绿，已过时
            28: [self._job("frontend", "completed", "success")],
        }
        sha = self._find_green(
            runs, current_run_id="31", jobs_fetcher=lambda run_id: jobs[run_id]
        )
        self.assertEqual(sha, "sha-old-green")

    def test_in_progress_run_provides_no_evidence(self) -> None:
        """in_progress 的 run（含其 sha 上的旧绿记录）不作为基线。"""
        runs = [
            self._prior_run(31, "2026-11-07T13:00:00Z", "sha-current", "in_progress", None),
            # re-run 进行中：该 sha 暂无新鲜结论。
            self._prior_run(30, "2026-11-07T12:00:00Z", "sha-rerunning", "in_progress", None),
            self._prior_run(29, "2026-11-07T11:00:00Z", "sha-rerunning", "completed", "failure"),
            self._prior_run(28, "2026-11-07T10:00:00Z", "sha-old-green", "completed", "failure"),
        ]
        jobs = {
            29: [self._job("frontend", "completed", "success")],  # 旧绿，不作数
            28: [self._job("frontend", "completed", "success")],
        }
        sha = self._find_green(
            runs, current_run_id="31", jobs_fetcher=lambda run_id: jobs[run_id]
        )
        self.assertEqual(sha, "sha-old-green")

    def test_green_sha_equal_to_current_is_ignored(self) -> None:
        """re-run 当前提交：绿证据与当前同 sha 时跳过，窗口不能为空。"""
        runs = [
            self._prior_run(31, "2026-11-07T13:00:00Z", "sha-current", "in_progress", None),
            # 与当前同 sha 的绿 run（re-run 的前身）——不可用作基线。
            self._prior_run(30, "2026-11-07T12:00:00Z", "sha-current", "completed", "success"),
            self._prior_run(29, "2026-11-07T11:00:00Z", "sha-older", "completed", "failure"),
        ]
        jobs = {
            30: [self._job("frontend", "completed", "success")],
            29: [self._job("frontend", "completed", "failure")],
        }
        sha = self._find_green(
            runs, current_run_id="31", jobs_fetcher=lambda run_id: jobs[run_id]
        )
        self.assertIsNone(sha)

    def test_non_success_job_conclusions_are_not_green(self) -> None:
        """skipped / cancelled / in_progress / 缺名的 job 都不是正面证据。"""
        runs = [
            self._prior_run(31, "2026-11-07T13:00:00Z", "sha-current", "in_progress", None),
            self._prior_run(30, "2026-11-07T12:00:00Z", "sha-skipped", "completed", "failure"),
            self._prior_run(29, "2026-11-07T11:00:00Z", "sha-cancelled", "completed", "cancelled"),
            self._prior_run(28, "2026-11-07T10:00:00Z", "sha-running", "completed", "failure"),
            self._prior_run(27, "2026-11-07T09:00:00Z", "sha-none", "completed", "failure"),
        ]
        jobs = {
            30: [self._job("frontend", "completed", "skipped")],
            29: [self._job("frontend", "completed", "cancelled")],
            28: [self._job("frontend", "in_progress", None)],
            27: [self._job("frontend", "completed", None)],
        }
        sha = self._find_green(
            runs, current_run_id="31", jobs_fetcher=lambda run_id: jobs[run_id]
        )
        self.assertIsNone(sha)

    def test_jobs_api_failure_fails_closed_to_common_base(self) -> None:
        """jobs API 异常：半边回退公共基线（never narrower）。"""
        runs = [
            self._prior_run(31, "2026-11-07T13:00:00Z", "sha-current", "in_progress", None),
            self._prior_run(30, "2026-11-07T12:00:00Z", "sha-failed", "completed", "failure"),
        ]

        def fetch(_run_id: int) -> list[dict[str, Any]]:
            raise ValueError("api boom")

        bases = resolve_half_baselines(
            common_base=("base-sha", "merge-base"),
            runs=runs,
            current_run_id="31",
            current_sha="sha-current",
            event_name="pull_request",
            pr_number=self.PR_NUMBER,
            head_branch="feature",
            jobs_fetcher=fetch,
        )
        self.assertEqual(bases["frontend"], ("base-sha", "merge-base"))
        self.assertEqual(bases["rust"], ("base-sha", "merge-base"))

    def test_probe_limit_bounds_jobs_api_calls(self) -> None:
        """连续不绿超过回探上限即放弃，jobs API 调用有界。"""
        runs = [
            self._prior_run(40, "2026-11-07T15:00:00Z", "sha-current", "in_progress", None),
        ] + [
            self._prior_run(
                39 - i, f"2026-11-07T{14 - i:02d}:00:00Z", f"sha-red-{i}", "completed", "failure"
            )
            for i in range(7)
        ]
        calls: list[int] = []

        def fetch(run_id: int) -> list[dict[str, Any]]:
            calls.append(run_id)
            return [self._job("frontend", "completed", "failure")]

        sha = self._find_green(runs, current_run_id="40", jobs_fetcher=fetch)
        self.assertIsNone(sha)
        self.assertEqual(len(calls), 5)

    def test_no_fetcher_keeps_common_base_for_both_halves(self) -> None:
        """无 jobs_fetcher（整轮成功 / API 不可用）：两半边沿用公共基线。"""
        bases = resolve_half_baselines(
            common_base=("sha-success", "direct"),
            runs=[],
            current_run_id="30",
            current_sha="sha-current",
            event_name="pull_request",
            pr_number=self.PR_NUMBER,
            head_branch="feature",
            jobs_fetcher=None,
        )
        self.assertEqual(bases["frontend"], ("sha-success", "direct"))
        self.assertEqual(bases["rust"], ("sha-success", "direct"))

    def test_classify_half_windows_takes_own_component_only(self) -> None:
        """半边独立：rust 全量窗口里的前端 / shared 文件不点亮 frontend。

        那些文件已被更早的 frontend 绿 run 验证过（frontend 半边窗口才是
        frontend job 的未验证变更集）——这是省掉陪跑的关键语义。
        """
        frontend_window: list[str] = []  # 绿基线之后无前端变更
        rust_window = [
            "crates/datazen-server/tests/mcp_green_path.rs",
            "packages/frontend/src/App.tsx",   # PR 早期前端变更，已验证
            "scripts/ci_scope.py",            # shared，同样已验证
        ]
        self.assertEqual(classify_half_windows(frontend_window, rust_window), (False, True))

    def test_classify_half_windows_equivalent_when_windows_match(self) -> None:
        """公共基线（两窗口相同）时与旧版单窗口判定完全一致。"""
        window = ["packages/frontend/src/App.tsx", "crates/datazen/src/lib.rs"]
        self.assertEqual(
            classify_half_windows(window, window),
            classify_changed_paths(window),
        )
        self.assertEqual(classify_half_windows([], []), (False, False))

    def test_frontend_half_window_still_runs_frontend(self) -> None:
        """对称保护：frontend 半边窗口里的前端文件照样点亮 frontend。"""
        frontend_window = ["packages/frontend/src/stores/ai/sessions.ts"]
        rust_window: list[str] = []
        self.assertEqual(classify_half_windows(frontend_window, rust_window), (True, False))


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
