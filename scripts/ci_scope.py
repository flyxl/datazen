"""Select frontend/Rust CI from changes since the previous successful run.

除逐路径的 frontend/rust 分类外，本脚本还给 pull_request 事件输出 docs_only
豁免：PR 的 merge-base diff（github.event.pull_request.base.sha ... head.sha）
若全部落入 docs-only 白名单，rust/frontend 重型 job 直接跳过。该判定只看
merge-base diff，刻意与 select_ci_baseline 的 run 间窗口无关——merge 过 main
的分支会把产品变更（版本号 bump、清单同步等）带进 run 间窗口（PR #182 教训），
那不是 PR 自身的变更，不能作为豁免依据。push / workflow_dispatch 事件不豁免。
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
from typing import Any


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
        f"baseline={mode}, changed_paths={changed_count}"
    )


if __name__ == "__main__":
    main()
