#!/usr/bin/env python3
"""Measure PostgreSQL driver production line coverage against a fresh real server."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[4]
SCOPE = Path("crates/drivers/postgres/src")
CRATE = "datazen-driver-postgres"
TARGET = 90
# These test binaries contain live cases only, unless a fully qualified prefix
# is supplied. Tests that merely exercise Rust APIs without PostgreSQL are excluded.
LEGACY = {
    "backup_runtime": None,
    "data_transfer_identity_sequences": None,
    "migration_bound_writes": None,
    "migration_schema_qualification": None,
    "monitor_scheduled_query": None,
    "postgres_cross_database": None,
    "postgres_dump_empty_schema": None,
    "postgres_empty_query_columns": None,
    "postgres_resource_close_handle": ("live::",),
    "postgres_resource_contract": ("live::",),
    "real_driver_contract": ("support::live::", "support::live_faults::"),
    "restore_apply_job": None,
    "schema_dependency_catalog": None,
    "schema_foreign_key_introspection": None,
    "schema_function_dependency_catalog": None,
    "schema_missing_table": None,
}
JOURNEYS = "postgres_live_e2e"
# The shared CM-26 case explicitly cannot verify reset/reuse: DatabaseDriver
# has no such API. Keep it in the shared diagnostic suite, never count its
# partially verified result or profile as an acceptance case here.
PARTIAL_DIAGNOSTICS = {
    ("real_driver_contract", "support::live_faults::cm26_failed_statement_resource_is_not_reused_blindly"),
}
UNVERIFIED = re.compile(r"\bskipp(?:ing|ed)\b|未验证|⏭", re.I)
TEST_RESULT = re.compile(r"(\d+) passed; (\d+) failed; (\d+) ignored;")


def cmd(args, **kwargs):
    return subprocess.run(args, cwd=ROOT, text=True, **kwargs)


def read(args):
    return cmd(args, capture_output=True, check=True).stdout.strip()


def llvm_tools():
    sysroot = Path(read(["rustc", "--print", "sysroot"]))
    host = next(line.split()[1] for line in read(["rustc", "-vV"]).splitlines()
                if line.startswith("host: "))
    directory = sysroot / "lib" / "rustlib" / host / "bin"
    tools = directory / "llvm-profdata", directory / "llvm-cov"
    if not all(p.is_file() for p in tools):
        raise SystemExit("Install the matching LLVM tools: rustup component add llvm-tools-preview")
    return tools


def build(bins, output, env, target):
    args = ["cargo", "test", "--locked", "-p", CRATE, "--no-run", "--message-format=json"]
    for name in bins:
        args.extend(["--test", name])
    env = {**env, "CARGO_TARGET_DIR": str(target), "RUSTFLAGS": "-C instrument-coverage",
           "CARGO_PROFILE_TEST_DEBUG": "0", "CARGO_INCREMENTAL": "0"}
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    with tempfile.TemporaryDirectory(prefix="build-profiles-", dir=output) as d:
        env["LLVM_PROFILE_FILE"] = str(Path(d) / "%m-%p.profraw")
        with (output / "cargo-build.jsonl").open("w") as stdout:
            cmd(args, env=env, stdout=stdout, check=True)
    found = {}
    for line in (output / "cargo-build.jsonl").read_text().splitlines():
        try:
            artifact = json.loads(line)
        except json.JSONDecodeError:
            continue
        if artifact.get("reason") != "compiler-artifact" or not artifact.get("executable"):
            continue
        actual = artifact["target"]
        if actual["kind"] == ["test"] and actual["name"] in bins:
            found[actual["name"]] = Path(artifact["executable"])
    if found.keys() != bins.keys():
        raise SystemExit(f"Missing instrumented targets: {sorted(bins.keys() - found.keys())}")
    frozen = output / "executables"
    frozen.mkdir()
    for name, path in found.items():
        dest = frozen / name
        shutil.copy2(path, dest)
        found[name] = dest
    return found


class Postgres:
    def __init__(self, pg_bin, out, cluster, env):
        self.pg_bin, self.out, self.cluster = pg_bin, out, cluster
        self.env = {k: v for k, v in env.items()
                    if not k.startswith(("PG", "TEST_PG_", "MIGRATION_TEST_"))}
        self.password = secrets.token_hex(24)
        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
            self.port = s.getsockname()[1]

    def invoke(self, program, *args, **kwargs):
        return cmd([self.pg_bin / program, *args], env=self.env, check=True, **kwargs)

    def tls(self):
        """A private, short-lived CA and leaf: IP-only identity is intentional."""
        fixtures = self.cluster.parent
        ca = fixtures / "ca.crt"
        key = fixtures / "ca.key"
        csr = fixtures / "server.csr"
        config = fixtures / "server-cert.cnf"
        config.write_text(
            "[req]\ndistinguished_name=dn\nx509_extensions=ca\n[dn]\n"
            "[ca]\nbasicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\n"
            "[leaf]\nbasicConstraints=critical,CA:FALSE\n"
            "keyUsage=critical,digitalSignature,keyEncipherment\n"
            "extendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n"
        )
        def openssl(*args):
            cmd(["openssl", *args], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        openssl("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256",
                "-days", "2", "-config", config, "-subj", "/CN=DataZen test CA",
                "-keyout", key, "-out", ca)
        openssl("req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=DataZen test server",
                "-keyout", self.cluster / "server.key", "-out", csr)
        openssl("x509", "-req", "-in", csr, "-CA", ca, "-CAkey", key, "-CAcreateserial",
                "-days", "2", "-sha256", "-extfile", config, "-extensions", "leaf",
                "-out", self.cluster / "server.crt")
        (self.cluster / "server.key").chmod(0o600)
        key.unlink()
        self.ca = ca

    def start(self):
        # Everything (including administrative operations) targets a new cluster.
        # No input DSN/user credentials are accepted and no Unix listening is needed.
        password_file = self.cluster.parent / "init-secret"
        fd = os.open(password_file, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        with os.fdopen(fd, "w") as f:
            f.write(self.password + "\n")
        try:
            self.invoke("initdb", "-D", self.cluster, "--username=datazen", "--auth=scram-sha-256",
                        "--no-locale", "--encoding=UTF8", "--pwfile", password_file,
                        stdout=subprocess.DEVNULL)
        finally:
            password_file.unlink()
        self.tls()
        self.invoke("pg_ctl", "-D", self.cluster, "-l", self.out / "server.log", "-w", "start",
                    "-o", f"-h 127.0.0.1 -p {self.port} -c unix_socket_directories='' "
                          "-c max_connections=60 -c shared_buffers=32MB -c ssl=on",
                    stdout=subprocess.DEVNULL)
        self.env["PGPASSWORD"] = self.password

    def psql(self, database, sql):
        return self.invoke("psql", "-XqAtw", "-h", "127.0.0.1", "-p", str(self.port),
                           "-U", "datazen", "-d", database, "-v", "ON_ERROR_STOP=1",
                           input=sql, capture_output=True).stdout.strip()

    def env_for_tests(self):
        names = ("dz_fixture_pg_a", "dz_fixture_pg_b", "dz_mig_postgres_e2e")
        for name in names:
            self.psql("postgres", f'CREATE DATABASE "{name}"')
            self.psql(name, "CREATE TABLE public.rune_e2e_fixture (marker text NOT NULL);"
                           "INSERT INTO public.rune_e2e_fixture VALUES ('datazen-rune-disposable');")
            if self.psql(name, "SELECT marker FROM public.rune_e2e_fixture") != "datazen-rune-disposable":
                raise RuntimeError("Disposable PostgreSQL fixture verification failed")
        result = dict(self.env)
        result.pop("PGPASSWORD")
        result.pop("DATAZEN_CONTRACT_ALLOW_UNVERIFIED", None)
        result["PGSSLROOTCERT"] = str(self.ca)
        result["DATAZEN_CONTRACT_REQUIRE_LIVE"] = "1"
        result["TEST_PG_DATABASE_B"] = names[1]
        for prefix, db in (("TEST_PG_", names[0]), ("MIGRATION_TEST_", names[2])):
            result.update({prefix + key: value for key, value in {
                "DATABASE": db, "HOST": "127.0.0.1", "PORT": str(self.port),
                "USER": "datazen", "PASSWORD": self.password,
            }.items()})
        return result

    def stop(self):
        if (self.cluster / "postmaster.pid").is_file():
            self.invoke("pg_ctl", "-D", self.cluster, "-w", "-m", "fast", "stop",
                        stdout=subprocess.DEVNULL)


def run_live(bins, selected, env, password, out):
    profiles = out / "profiles"
    profiles.mkdir()
    counts = {"passed": 0, "failed": 0, "skipped": 0}
    for name, binary in sorted(bins.items()):
        # Listing also executes the binary: put its profiles outside the real run.
        with tempfile.TemporaryDirectory(prefix="list-", dir=out) as d:
            result = cmd([binary, "--list"], env={**env, "LLVM_PROFILE_FILE": str(Path(d) / "%m-%p.profraw")},
                         capture_output=True, check=True)
        prefix = selected[name]
        names = [line.removesuffix(": test") for line in result.stdout.splitlines()
                 if line.endswith(": test") and (prefix is None or line.startswith(prefix))]
        names = [test for test in names if (name, test) not in PARTIAL_DIAGNOSTICS]
        if not names:
            raise SystemExit(f"No eligible live cases in {name}")
        output = out / f"{name}.log"
        with output.open("w") as log:
            for test in names:
                before = set(profiles.glob(f"{name}-*.profraw"))
                try:
                    run = cmd([binary, "--exact", test, "--nocapture"], stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, env={
                                  **env, "LLVM_PROFILE_FILE": str(profiles / f"{name}-%m-%p.profraw")
                              }, timeout=240)
                    raw = run.stdout or ""
                    verdict = TEST_RESULT.search(raw)
                    new_profiles = set(profiles.glob(f"{name}-*.profraw")) - before
                    if run.returncode or not verdict or not new_profiles:
                        counts["failed"] += 1
                    elif UNVERIFIED.search(raw) or verdict.groups() != ("1", "0", "0"):
                        counts["skipped"] += 1
                    else:
                        counts["passed"] += 1
                except subprocess.TimeoutExpired as e:
                    raw = e.stdout or b""
                    if isinstance(raw, bytes):
                        raw = raw.decode(errors="replace")
                    raw += "\nLive case timed out\n"
                    counts["failed"] += 1
                log.write(f"===== {test} =====\n{raw.replace(password, '[fixture password redacted]')}\n")
            print(f"{name}: {len(names)} live cases; cumulative {counts}", flush=True)
    return counts, sorted(profiles.glob("*.profraw"))


def included(raw):
    p = Path(raw)
    if ".." in p.parts:
        return None
    path = (ROOT / p).resolve()
    return path.relative_to(ROOT) if path.is_relative_to(ROOT / SCOPE) else None


def measure(bins, files, output, tools):
    profdata, cov = tools
    merged = output / "e2e.profdata"
    if not files:
        raise SystemExit("No live profiles generated")
    result = cmd([profdata, "merge", "-sparse", "-o", merged, *files],
                 check=True, stderr=subprocess.PIPE)
    if result.stderr.strip():
        raise SystemExit(f"LLVM profile merge was not clean: {result.stderr.strip()}")
    binary, *others = bins.values()
    opts = [binary, *(f"-object={p}" for p in others), f"-instr-profile={merged}",
            "-ignore-filename-regex=.*/crates/drivers/postgres/src/\\.\\./"]
    def export(mode, destination):
        with destination.open("w") as target:
            result = cmd([cov, "export", *opts, mode], stdout=target,
                         stderr=subprocess.PIPE, check=True)
        if result.stderr.strip():
            raise SystemExit(f"LLVM coverage export was not clean: {result.stderr.strip()}")
    raw = output / "llvm-summary.json"
    export("-summary-only", raw)
    paths = {}
    for unit in json.loads(raw.read_text())["data"]:
        for file in unit["files"]:
            p = included(file["filename"])
            if p:
                lines = file["summary"]["lines"]
                counts = lines["covered"], lines["count"]
                if str(p) in paths and paths[str(p)] != counts:
                    raise SystemExit(f"Conflicting LLVM line counts: {p}")
                paths[str(p)] = counts
    hit, total = (sum(row[n] for row in paths.values()) for n in (0, 1))
    if not total:
        raise SystemExit("No PostgreSQL driver production source lines in LLVM output")
    with tempfile.TemporaryDirectory(dir=output) as d:
        raw_lcov = Path(d) / "full.lcov"
        export("-format=lcov", raw_lcov)
        keep = False
        lcov_counts = [0, 0]
        with raw_lcov.open() as src, (output / "postgres-e2e.lcov").open("w") as dst:
            for line in src:
                if line.startswith("SF:"):
                    src_path = included(line[3:].rstrip("\n"))
                    keep = bool(src_path)
                    if keep:
                        line = f"SF:{src_path}\n"
                if keep:
                    dst.write(line)
                    if line.startswith("LH:"):
                        lcov_counts[0] += int(line[3:])
                    elif line.startswith("LF:"):
                        lcov_counts[1] += int(line[3:])
                if line == "end_of_record\n":
                    keep = False
    if (hit, total) != tuple(lcov_counts):
        raise SystemExit("Scoped LLVM summary and LCOV disagree")
    return paths, hit, total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pg-bin", type=Path, required=True, help="directory with initdb, pg_ctl and psql")
    parser.add_argument("--output", type=Path, required=True, help="new output directory")
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target/postgres-e2e-coverage")
    parser.add_argument("--report-only", action="store_true", help="report below threshold, never waive failed tests")
    parser.add_argument("--legacy-baseline", action="store_true", help="exclude new journeys; diagnostic only")
    args = parser.parse_args()
    if args.legacy_baseline and not args.report_only:
        parser.error("--legacy-baseline requires --report-only")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    pg_bin = args.pg_bin.resolve(strict=True)
    env = {**os.environ}
    tools = llvm_tools()
    selected = LEGACY if args.legacy_baseline else {**LEGACY, JOURNEYS: None}
    source = {"commit": read(["git", "rev-parse", "HEAD"]),
              "working_tree_dirty": bool(read(["git", "status", "--porcelain"])),
              "rustc": read(["rustc", "--version"]), "postgres": read([pg_bin / "initdb", "--version"])}
    binaries = build(selected, output, env, args.target_dir.resolve())
    source["executables_sha256"] = {}
    for name, path in binaries.items():
        with path.open("rb") as f:
            source["executables_sha256"][name] = hashlib.file_digest(f, "sha256").hexdigest()
    with tempfile.TemporaryDirectory(prefix="cluster-", dir=output) as d:
        server = Postgres(pg_bin, output, Path(d) / "data", env)
        try:
            server.start()
            tests, files = run_live(binaries, selected, server.env_for_tests(), server.password, output)
        finally:
            server.stop()
    paths, hit, total = measure(binaries, files, output, tools)
    report = {"source": source, "scope": str(SCOPE), "files": paths, "covered": hit,
              "total": total, "percent": 100 * hit / total, "live_e2e": tests,
              "minimum_percent": TARGET, "meets_threshold": hit * 100 >= total * TARGET,
              "legacy_baseline": args.legacy_baseline}
    (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"PostgreSQL production E2E lines: {hit}/{total} ({report['percent']:.2f}%); required {TARGET}%")
    return int(bool(tests["failed"] or tests["skipped"]) or not (args.report_only or report["meets_threshold"]))


if __name__ == "__main__":
    raise SystemExit(main())
