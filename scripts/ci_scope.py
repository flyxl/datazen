"""Select frontend/Rust CI from changes since the previous successful run.

除逐路径的 frontend/rust 分类外，本脚本还给 pull_request 事件输出 docs_only
豁免：PR 的 merge-base diff（github.event.pull_request.base.sha ... head.sha）
若全部落入 docs-only 白名单，rust/frontend 重型 job 直接跳过。该判定只看
merge-base diff，刻意与 select_ci_baseline 的 run 间窗口无关——merge 过 main
的分支会把产品变更（版本号 bump、清单同步等）带进 run 间窗口（PR #182 教训），
那不是 PR 自身的变更，不能作为豁免依据。push / workflow_dispatch 事件不豁免。

半边基线（per-half baseline）：为每个 job 独立找「最近一次该 job
completed+success」的 run：该 sha 到当前的 direct 窗口 = 该半边全部未验证
变更（中间红 run 的提交都在窗口内，不会漏）。窗口内没有本半边文件即可跳过。
即使存在更早的整轮成功基线，只要其后某半边有更新的单独 green run，也应将该
半边推进到该 green sha（避免整轮成功后“单边绿、另一边红”的后续提交让已验证
半边陪跑）。
仅当有历史 API 证据且 jobs API 可用时启用；jobs API 故障按半边回退公共基线
（never narrower 不变）。同 sha 多次 run 只信最新一条记录；head_sha 等于当前
sha 的证据跳过（re-run 当前提交时窗口不能为空）。
"""

from __future__ import annotations

import json
import os
import subprocess
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime
from pathlib import PurePosixPath
from typing import Any, Callable


# ── docs-only 白名单：PR merge-base diff 全部落入其中才豁免 ───────────────
# 集合刻意偏窄（fail-closed），成员与理由：
#   - docs/**：产品文档目录，不参与任何构建、测试或打包产物；
#   - 根级 *.md（README / CHANGELOG / CONTRIBUTING / AGENTS 等）：顶层项目元
#     文档。classify 阶段本就跳过任意位置的 .md，但那是逐路径贡献判定；
#     docs_only 是 PR 级门禁，白名单取其严格子集——代码目录内的 README
#     （如 e2e/contract/README.md）常与代码变更同批出现，不给 PR 级豁免，
#     免得「文档 PR」标签掩盖真实代码 diff（它们单独出现时 frontend/rust
#     本就为 False，job 照样跳过，行为不受影响）；
#   - .github/**/*.md：issue / PR 模板等流程文档；.github/workflows/*.yml、
#     .github/actions/ 等非 markdown 不在集合内——改 CI 定义必须照常跑
#     重型 job 验证；
#   - LICENSE / .gitignore：元文件，不影响构建与测试结果。
DOCS_ONLY_FILES = frozenset({"LICENSE", ".gitignore"})

FRONTEND_ROOTS = (
    "packages/ui/",
    "packages/driver-sdk/",
    "packages/backend-client/",
    "packages/wapp-sdk/",
    "packages/extension-points/",
    "packages/wapps/",
)
SHARED_ROOTS = ("scripts/", ".github/workflows/", ".github/actions/")
# These files are validated by the lightweight detect-changes job itself.
# They affect CI scope selection, not application build/test inputs.
CI_SCOPE_ONLY_FILES = frozenset({"scripts/ci_scope.py", "scripts/test_ci_scope.py"})
FRONTEND_SUFFIXES = {".ts", ".tsx", ".js", ".jsx", ".css"}
ALL_ZERO_SHA = "0" * 40

# CI 的两个重型 job 名（ci.yml 的 job id）。半边基线按 job 名取「最近一次
# 该 job completed+success」的 run；名字与 ci.yml 保持同步。
HALF_JOB_NAMES = ("frontend", "rust")
# 半边基线最多回探的 run 数：连续这么多 run 该 job 都不绿就回退公共基线，
# 避免对久远历史的 jobs API 深挖（每次探查一个 run 的 jobs 是一次 API 调用）。
GREEN_JOB_PROBE_LIMIT = 5


def classify_changed_paths(paths: list[str]) -> tuple[bool, bool]:
    """Return (frontend, rust), conservatively enabling both for unknown paths."""
    frontend = rust = False
    for name in paths:
        path = PurePosixPath(name)
        parts = path.parts
        suffix = path.suffix
        if name in CI_SCOPE_ONLY_FILES:
            continue
        elif name.startswith("docs/") or suffix == ".md" or name.startswith(".superpowers/"):
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
        elif name.startswith(("src-tauri/", "crates/")) or (
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


def is_docs_only_path(name: str) -> bool:
    """True when a single changed path cannot affect build or test outcomes."""
    path = PurePosixPath(name)
    if name in DOCS_ONLY_FILES:
        return True
    if name.startswith("docs/"):
        return True
    if name.startswith(".github/"):
        # 仅模板等 markdown；workflows / actions / dependabot 配置不豁免。
        return path.suffix == ".md"
    # 根级 *.md（PurePosixPath("README.md").parts 长度为 1）。
    return len(path.parts) == 1 and path.suffix == ".md"


def is_docs_only_paths(paths: list[str]) -> bool:
    """True when every changed path is docs-only; empty diff does not qualify."""
    return bool(paths) and all(is_docs_only_path(name) for name in paths)


def resolve_docs_only(
    *, event_name: str, pr_base_sha: str, head_sha: str, cwd: str | None = None
) -> bool:
    """docs-only 豁免：仅 pull_request，且只看 PR 的 merge-base diff。

    与 select_ci_baseline 的 run 间窗口刻意无关（见模块 docstring）。取不到
    diff（git 失败、base sha 缺失）或 diff 为空时返回 False，fail-closed；
    空 diff 时 frontend/rust 本就为 False，两个 job 照样跳过，行为不变。
    """
    if event_name != "pull_request" or not pr_base_sha or not head_sha:
        return False
    pr_changed = changed_paths(pr_base_sha, head_sha, "merge-base", cwd=cwd)
    if not pr_changed:
        return False
    return is_docs_only_paths(pr_changed)


def _parse_time(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def _is_before(run: dict[str, Any], current: dict[str, Any]) -> bool:
    run_time = _parse_time(run["created_at"])
    current_time = _parse_time(current["created_at"])
    if run_time != current_time:
        return run_time < current_time
    return int(run["id"]) < int(current["id"])


def _prior_runs(
    runs: list[dict[str, Any]],
    *,
    current_run_id: str,
    event_name: str,
    pr_number: str,
    head_branch: str,
) -> list[dict[str, Any]] | None:
    """当前 run 之前的同事件 / 同 PR / 同分支 run，按创建时间倒序。

    返回 None 表示 runs 列表里找不到当前 run（API 数据不一致，调用方
    fail-closed）。整轮成功基线与半边绿基线共用这套过滤，保证两者比较的
    是同一批历史 run。
    """
    current = next((run for run in runs if str(run.get("id")) == current_run_id), None)
    if current is None:
        return None

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
    return prior


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
    prior = _prior_runs(
        runs,
        current_run_id=current_run_id,
        event_name=event_name,
        pr_number=pr_number,
        head_branch=head_branch,
    )
    if prior is None:
        return None, False, False

    previous_success = next((
        run for run in prior
        if run.get("status") == "completed"
        and run.get("conclusion") == "success"
        and run.get("head_sha") != current_sha
        and run.get("head_sha")
    ), None)
    return previous_success, bool(prior), True


def find_latest_green_job_sha(
    runs: list[dict[str, Any]],
    *,
    job_name: str,
    current_run_id: str,
    current_sha: str,
    event_name: str,
    pr_number: str,
    head_branch: str,
    jobs_fetcher: "Callable[[int], list[dict[str, Any]]]",
    probe_limit: int = GREEN_JOB_PROBE_LIMIT,
) -> str | None:
    """最近一次「该 job completed+success」的 run 的 head_sha。

    半边基线的证据采集：按时间倒序回探 prior run，逐个查 jobs。正确性要点：
    - 取「最近的」绿 run：中间红 run 引入的变更全部落进 direct 窗口，不漏；
    - 同 sha 多次 run 只信最新一条记录（re-run 红了就不能再用旧的绿证据）；
    - head_sha 等于当前 sha 的跳过——re-run 当前提交时窗口不能为空，否则
      重跑失去意义（与 find_previous_successful_run 的同名约束同源）；
    - job 结论必须是 completed+success：skipped / cancelled / in_progress
      都不构成「该半边被真实验证过」的正面证据；
    - jobs API 异常 → None（fail-closed，该半边回退公共基线）。
    """
    prior = _prior_runs(
        runs,
        current_run_id=current_run_id,
        event_name=event_name,
        pr_number=pr_number,
        head_branch=head_branch,
    )
    if prior is None:
        return None

    seen_shas: set[str] = set()
    probed = 0
    for run in prior:
        sha = run.get("head_sha") or ""
        if not sha or sha == current_sha or sha in seen_shas:
            continue
        seen_shas.add(sha)
        if run.get("status") != "completed":
            # 同 sha 去重后仍非 completed（in_progress 的 re-run）：该提交
            # 暂无新鲜结论，不采信更早的旧绿，跳过。
            continue
        if probed >= probe_limit:
            break
        probed += 1
        try:
            jobs = jobs_fetcher(int(run["id"]))
        except (OSError, urllib.error.URLError, urllib.error.HTTPError, ValueError, KeyError):
            return None
        for job in jobs:
            if (
                str(job.get("name", "")) == job_name
                and job.get("status") == "completed"
                and job.get("conclusion") == "success"
            ):
                return sha
    return None


def resolve_half_baselines(
    *,
    common_base: tuple[str, str],
    runs: list[dict[str, Any]] | None,
    current_run_id: str,
    current_sha: str,
    event_name: str,
    pr_number: str,
    head_branch: str,
    jobs_fetcher: "Callable[[int], list[dict[str, Any]]] | None",
    probe_limit: int = GREEN_JOB_PROBE_LIMIT,
) -> dict[str, tuple[str, str]]:
    """每个半边的 (base SHA, mode)；无新鲜绿证据的半边沿用公共基线。

    jobs_fetcher=None（整轮成功 / API 不可用 / 无历史 run）时两半边
    都是 common_base——与旧版单基线行为完全一致。半边 direct 窗口的完备性
    论证见 find_latest_green_job_sha：窗口覆盖该半边全部未验证变更。
    """
    bases = {half: common_base for half in HALF_JOB_NAMES}
    if runs is None or jobs_fetcher is None:
        return bases
    for job_name in HALF_JOB_NAMES:
        green_sha = find_latest_green_job_sha(
            runs,
            job_name=job_name,
            current_run_id=current_run_id,
            current_sha=current_sha,
            event_name=event_name,
            pr_number=pr_number,
            head_branch=head_branch,
            jobs_fetcher=jobs_fetcher,
            probe_limit=probe_limit,
        )
        if green_sha:
            bases[job_name] = (green_sha, "direct")
    return bases


def select_ci_baseline(
    *,
    event_name: str,
    previous_success: dict[str, Any] | None,
    had_prior_run: bool,
    api_ok: bool,
    pr_base_sha: str,
    event_before: str,
) -> tuple[str, str]:
    """Choose an incremental base while preserving complete PR change coverage."""
    if event_name == "workflow_dispatch":
        return "", "all"
    if previous_success:
        return str(previous_success["head_sha"]), "direct"
    if event_name == "pull_request" and pr_base_sha:
        # With no successful PR baseline, classify the entire PR diff. This also
        # covers changes from failed/in-progress prior runs without forcing both
        # CI halves when the changed paths are safely scoped.
        return pr_base_sha, "merge-base"
    if api_ok and had_prior_run:
        # For push events, a failed/cancelled prior run may contain changes that
        # are absent from EVENT_BEFORE, so history is unsafe: run both halves.
        return "", "all"
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


def _fetch_run_jobs(*, repository: str, token: str, run_id: int) -> list[dict[str, Any]]:
    """单个 run 的 jobs 列表（ci.yml 只有 4 个 job，一页足够）。"""
    request = urllib.request.Request(
        f"https://api.github.com/repos/{repository}/actions/runs/{run_id}/jobs?per_page=100",
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2026-03-10",
        },
    )
    with urllib.request.urlopen(request, timeout=15) as response:
        data = json.load(response)
    jobs = data.get("jobs")
    if not isinstance(jobs, list):
        raise ValueError("unexpected jobs payload")
    return jobs


def _resolve_baseline(env: dict[str, str]) -> dict[str, tuple[str, str]]:
    """Return per-half baselines: {"frontend": (sha, mode), "rust": (sha, mode)}。

    mode 为 direct / merge-base / all。公共基线仍是 select_ci_baseline 的
    全套判定（整轮成功 → direct，PR 无成功基线 → merge-base，push 未知
    历史 → all）；随后在有历史证据时尝试半边细化：若某半边存在比公共基线
    更新的单独 green run，则将该半边推进到该 direct 基线，以避免已验证半边陪跑。
    """
    event_name = env["EVENT_NAME"]
    if event_name == "workflow_dispatch":
        return {half: ("", "all") for half in HALF_JOB_NAMES}

    current_sha = env["CURRENT_SHA"]
    head_branch = env["HEAD_BRANCH"]
    repository = env["REPOSITORY"]
    token = env.get("GITHUB_TOKEN", "")
    api_ok = False
    previous_success = None
    had_prior_run = False
    current_run_found = False
    runs: list[dict[str, Any]] | None = None

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
            runs = None

    common_base = select_ci_baseline(
        event_name=event_name,
        previous_success=previous_success,
        had_prior_run=had_prior_run,
        api_ok=api_ok,
        pr_base_sha=env.get("PR_BASE_SHA", ""),
        event_before=env.get("EVENT_BEFORE", ""),
    )

    if had_prior_run and api_ok and runs is not None and token:
        def fetch_jobs(run_id: int) -> list[dict[str, Any]]:
            return _fetch_run_jobs(repository=repository, token=token, run_id=run_id)
    else:
        fetch_jobs = None

    return resolve_half_baselines(
        common_base=common_base,
        runs=runs,
        current_run_id=env["CURRENT_RUN_ID"],
        current_sha=current_sha,
        event_name=event_name,
        pr_number=env.get("PR_NUMBER", ""),
        head_branch=head_branch,
        jobs_fetcher=fetch_jobs,
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


def classify_half_windows(
    frontend_window: list[str], rust_window: list[str]
) -> tuple[bool, bool]:
    """每个半边只取自己窗口的对应分量。

    半边窗口的语义是「该 job 尚未被绿 run 覆盖的变更」：rust 半边窗口（常见
    为 merge-base 全量）里的前端 / shared 文件已被更早的 frontend 绿 run 验证
    过，不再点亮 frontend——这是半边基线省掉陪跑的关键。两窗口相同（公共
    基线）时结果与 classify_changed_paths 单窗口完全一致。
    """
    frontend, _ = classify_changed_paths(frontend_window)
    _, rust = classify_changed_paths(rust_window)
    return frontend, rust


def main() -> None:
    env = dict(os.environ)
    bases = _resolve_baseline(env)
    frontend_window = changed_paths(
        bases["frontend"][0], env["CURRENT_SHA"], bases["frontend"][1]
    )
    rust_window = changed_paths(
        bases["rust"][0], env["CURRENT_SHA"], bases["rust"][1]
    )

    if frontend_window is None or rust_window is None:
        # 任一半边基线不可解析（sha 缺失 / git 失败）：fail-closed 全量。
        frontend = rust = True
        baseline_desc = "all"
        frontend_count = rust_count = 0
    else:
        frontend, rust = classify_half_windows(frontend_window, rust_window)
        baseline_desc = "/".join(
            f"{half}:{bases[half][1]}" for half in HALF_JOB_NAMES
        )
        frontend_count = len(frontend_window)
        rust_count = len(rust_window)

    docs_only = resolve_docs_only(
        event_name=env["EVENT_NAME"],
        pr_base_sha=env.get("PR_BASE_SHA", ""),
        head_sha=env["CURRENT_SHA"],
    )

    output_path = env["GITHUB_OUTPUT"]
    with open(output_path, "a", encoding="utf-8") as output:
        output.write(f"frontend={str(frontend).lower()}\n")
        output.write(f"rust={str(rust).lower()}\n")
        output.write(f"docs_only={str(docs_only).lower()}\n")
    print(
        f"::notice::CI scope: frontend={str(frontend).lower()}, "
        f"rust={str(rust).lower()}, docs_only={str(docs_only).lower()}, "
        f"baseline={baseline_desc}, "
        f"changed_paths=frontend:{frontend_count}/rust:{rust_count}"
    )


if __name__ == "__main__":
    main()
