"""Compile uninstrumented consumer/test harnesses against the same Native formatter rlib."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def build_consumers(src, out, rlib):
    compiler = os.environ.get("RUSTC", "rustc")
    libdir = Path(subprocess.check_output([compiler, "--print", "target-libdir"], text=True).strip())
    [runtime] = libdir.glob("libprofiler_builtins-*.rlib")
    common = [compiler, "--edition=2021", "--extern", f"datazen_sql_formatter={rlib}",
              "-C", "link-arg=-Wl,-u,__llvm_profile_runtime", "-C", f"link-arg={runtime}"]
    cli, units = out / "format-sql", out / "tsql-units"
    subprocess.run([*common, src / "format_sql.rs", "-o", cli], check=True)
    subprocess.run([*common, "--test", src / "tsql_unit_harness.rs", "-o", units], check=True)
    return cli, units


def verified_profiles(folder, listed):
    paths = [Path(name).resolve(strict=True) for name in listed]
    found = {p.resolve() for p in folder.rglob("*.profraw")}
    if not paths or len(set(paths)) != len(paths) or set(paths) != found or not all(p.stat().st_size and p.is_relative_to(folder) for p in paths):
        raise ValueError("profile receipt does not match this fresh passing batch")
    return paths


def external_replays(src, out, exe, reference):
    profiles = []
    for script in ("public_process", "output_bounds", "incomplete_drafts"):
        dest = out / script
        cmd = [sys.executable, "-B", src / (script + ".py"), exe, "--output", dest]
        if script != "incomplete_drafts":
            cmd.extend(["--reference", reference])
        with (out / (script + ".log")).open("w") as log:
            subprocess.run(cmd, env={**os.environ, "PYTHONOPTIMIZE": "0"}, stdout=log,
                           stderr=subprocess.STDOUT, check=True)
        if script == "incomplete_drafts":
            cases = json.loads((dest / "passing.json").read_text())
            listed = [dest / row["profile"] for row in cases]
        else:
            listed = (dest / "accepted-profile-list").read_text().splitlines()
        profiles.extend(verified_profiles(dest, listed))
    return profiles


def focused_units(out, exe):
    # This executable includes only tests from the dedicated T-SQL module.
    env = {**os.environ, "LLVM_PROFILE_FILE": str(out / "tsql-%p.profraw")}
    result = subprocess.check_output([exe, "--test-threads=1"], env=env, text=True)
    (out / "tsql.log").write_text(result)
    tests = re.findall(r"^test (\S+) \.\.\. ok$", result, re.M)
    if len(tests) != 3 or not all(test.startswith("tsql_format::tsql_unit_") for test in tests):
        raise ValueError("expected exactly three approved T-SQL-only unit tests")
    [profile] = out.glob("tsql-*.profraw")
    if not profile.stat().st_size:
        raise ValueError("empty unit profile")
    return [profile]


def counters(cov, binary, profile):
    dataset = json.loads(subprocess.check_output([cov, "export", binary, f"-instr-profile={profile}"]))["data"][0]
    # Strict equality requires exact crate symbol IDs and complete region
    # coordinates, not just equal totals from two differently instrumented builds.
    result = {}
    for f in dataset["functions"]:
        key = (f["name"], tuple(f["filenames"]),
               tuple(tuple(r[:4] + r[5:]) for r in f["regions"]))
        if key in result:
            raise ValueError("duplicate LLVM function mapping")
        result[key] = (f["count"], *(r[4] for r in f["regions"]))
    if not result:
        raise ValueError("empty LLVM mapping")
    return result


def assert_sum(result, parts):
    if any(part.keys() != result.keys() for part in parts):
        raise ValueError("different formatter function / region mappings")
    if any(result[k] != tuple(map(sum, zip(*(part[k] for part in parts)))) for k in result):
        raise ValueError("merged execution counters do not equal component sums")


def physical_lines(cov, binary, profile, dest):
    """LCOV DA is a set of physical source locations, not LLVM's LF metric."""
    text = subprocess.check_output([cov, "export", binary, "-format=lcov",
                                    f"-instr-profile={profile}"], text=True)
    dest.write_text(text)
    src, rows = None, {}
    for line in text.splitlines():
        if line.startswith("SF:"):
            src = line[3:]
        elif line.startswith("DA:"):
            n, hit, *_ = line[3:].split(",")
            key = (src, int(n))
            if src is None or key in rows:
                raise ValueError("ambiguous physical LCOV source line")
            rows[key] = int(hit) != 0
        elif line == "end_of_record":
            src = None
    if not rows:
        raise ValueError("empty physical coverage")
    return rows


def assert_physical_or(result, components):
    if any(rows.keys() != result.keys() for rows in components):
        raise ValueError("physical coverage source-line maps differ")
    if any(result[key] != any(rows[key] for rows in components) for key in result):
        raise ValueError("physical line results do not equal logical OR of inputs")


def mixed_report(src, root, native, native_raw, rlib, reference, profdata, cov, summarize, digest):
    cli, units = build_consumers(src, root, rlib)
    public_raw = external_replays(src, root, cli, reference)
    tsql_raw = focused_units(root, units)
    groups = {"native": native_raw, "external": public_raw, "tsql-units": tsql_raw,
              "e2e": native_raw + public_raw, "combined": native_raw + public_raw + tsql_raw}
    reports, maps, physical = {}, {}, {}
    for key, raw in groups.items():
        path = root / ("report-" + key)
        path.mkdir()
        reports[key] = summarize(native, path, profdata, cov, raw)
        merged = path / "ui-only.profdata"
        maps[key] = counters(cov, native, merged)
        physical[key] = physical_lines(cov, native, merged, path / "physical-lines.lcov")
        if key in ("external", "tsql-units") and maps[key] != counters(cov, cli if key == "external" else units, merged):
            raise ValueError("component did not link exactly the stock formatter library")
    assert_sum(maps["e2e"], [maps["native"], maps["external"]])
    assert_sum(maps["combined"], [maps["e2e"], maps["tsql-units"]])
    assert_physical_or(physical["e2e"], [physical["native"], physical["external"]])
    assert_physical_or(physical["combined"], [physical["e2e"], physical["tsql-units"]])
    physical_totals = {k: {"covered": sum(m.values()), "count": len(m),
                           "percent": 100 * sum(m.values()) / len(m)} for k, m in physical.items()}
    def receipt(p):
        return {"path": str(p.relative_to(root)), "sha256": digest(p)}
    manifest = {"binary_sha256": digest(native), "rlib_sha256": digest(rlib),
                "consumer_sha256": digest(cli), "focused_unit_binary_sha256": digest(units),
                "functions_checked": len(maps["combined"]), "line_coverage": reports,
                "unique_physical_lines": physical_totals,
                "raw": {k: [receipt(p) for p in groups[k]] for k in ("native", "external", "tsql-units")}}
    (root / "combined-summary.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return reports, physical_totals
