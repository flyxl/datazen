#!/usr/bin/env python3
"""Strict live E2E line coverage for the DataZen MySQL driver."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import tempfile
import time

helper = Path(__file__).resolve().parents[2] / "postgres/e2e/coverage.py"
spec = importlib.util.spec_from_file_location("live_coverage_helpers", helper)
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)
gate.SCOPE = Path("crates/drivers/mysql/src")
gate.CRATE = "datazen-driver-mysql"

LEGACY = {
    "backup_runtime": None,
    "migration_bound_writes": None,
    "monitor_scheduled_query": None,
    "mysql_cross_database": None,
    "real_driver_contract": ("support::live::", "support::live_faults::"),
    "restore_apply_job": None,
    "restore_sql": ("retained_resource_executes_restore_and_verifies_rows_before_closing",),
    "schema_dependency_catalog": None,
}
JOURNEYS = "mysql_live_e2e"
CROSS_TRANSFER = "mysql_transfer_postgres"
A, B, MIGRATION = "dz_fixture_mysql_a", "dz_fixture_mysql_b", "dz_mig_mysql"


class Mysql:
    def __init__(self, binaries, temp, output, env):
        self.binaries, self.temp, self.output, self.env = binaries, temp, output, env
        self.data = temp / "d"
        self.password = secrets.token_hex(24)
        self.marker = secrets.token_hex(24)
        self.process = None
        with socket.socket() as endpoint:
            endpoint.bind(("127.0.0.1", 0))
            self.port = endpoint.getsockname()[1]

    def local(self, sql):
        return gate.cmd([self.binaries / "mysql", "--no-defaults", "--protocol=TCP",
                         "--host=127.0.0.1", f"--port={self.port}", "--user=datazen",
                         "--batch", "--skip-column-names"], input=sql,
                        env={**self.env, "MYSQL_PWD": self.password}, capture_output=True,
                        timeout=5, check=True).stdout.strip()

    def start(self):
        args = [self.binaries / "mysqld", "--no-defaults",
                f"--basedir={self.binaries.parent}", f"--datadir={self.data}"]
        with (self.output / "initialize.log").open("w") as log:
            gate.cmd([*args, "--initialize-insecure"], env=self.env, stdout=log,
                     stderr=log, timeout=90, check=True)
        # --init-file runs before the new server accepts clients. Lock down
        # its bootstrap account and seed only this randomly marked instance.
        sql = [f"ALTER USER 'root'@'localhost' IDENTIFIED BY '{secrets.token_hex(24)}';",
               f"CREATE USER 'datazen'@'127.0.0.1' IDENTIFIED BY '{self.password}';",
               "GRANT ALL PRIVILEGES ON *.* TO 'datazen'@'127.0.0.1' WITH GRANT OPTION;"]
        for db in (A, B, MIGRATION):
            sql.extend([f"CREATE DATABASE {db} CHARACTER SET utf8mb4;",
                        f"CREATE TABLE {db}.rune_e2e_fixture(marker VARCHAR(64) NOT NULL);",
                        f"INSERT INTO {db}.rune_e2e_fixture VALUES ('{self.marker}');"])
        bootstrap = self.temp / "seed.sql"
        fd = os.open(bootstrap, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        with os.fdopen(fd, "w") as stream:
            stream.write("\n".join(sql) + "\n")
        with (self.output / "server.log").open("w") as log:
            self.process = subprocess.Popen(
                [*args, "--bind-address=127.0.0.1", f"--port={self.port}",
                 "--socket=", f"--init-file={bootstrap}", f"--pid-file={self.temp / 'mysql.pid'}",
                 "--mysqlx=OFF", "--skip-name-resolve", "--skip-log-bin",
                 "--local-infile=OFF", "--secure-file-priv=NULL",
                 "--innodb-buffer-pool-size=64M", "--performance-schema=ON"],
                env=self.env, stdout=log, stderr=log, start_new_session=True)
        deadline = time.monotonic() + 30
        while True:
            if self.process.poll() is not None:
                raise RuntimeError("MySQL fixture exited during startup; inspect server.log")
            try:
                version = self.local("SELECT VERSION(), @@version_comment;")
                marker = self.local(f"SELECT marker FROM {A}.rune_e2e_fixture;")
                break
            except subprocess.CalledProcessError:
                if time.monotonic() >= deadline:
                    raise RuntimeError("Local MySQL fixture did not become ready") from None
                time.sleep(0.2)
        if "MySQL" not in version or "MariaDB" in version:
            raise RuntimeError("The fixture is not genuine MySQL")
        if marker != self.marker:
            raise RuntimeError("MySQL fixture initialization marker does not match")
        bootstrap.unlink()
        return version

    def test_env(self):
        result = {**self.env, "DATAZEN_CONTRACT_REQUIRE_LIVE": "1",
                  "DATAZEN_MYSQL_E2E_MARKER": self.marker,
                  "TEST_MYSQL_DATABASE_B": B}
        for prefix, db in (("TEST_MYSQL_", A), ("MYSQL_MIGRATION_TEST_", MIGRATION)):
            result.update({prefix + key: value for key, value in {
                "HOST": "127.0.0.1", "PORT": str(self.port), "USER": "datazen",
                "PASSWORD": self.password, "DATABASE": db}.items()})
        return result

    def stop(self):
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
                raise RuntimeError("MySQL fixture did not shut down cleanly") from None


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--mysql-bin", type=Path, required=True)
    p.add_argument("--postgres-bin", type=Path,
                   help="optional native source for the MySQL cross-transfer E2E report")
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--target-dir", type=Path, default=gate.ROOT / "target/mysql-live-coverage")
    p.add_argument("--report-only", action="store_true")
    p.add_argument("--legacy-baseline", action="store_true")
    args = p.parse_args()
    if args.legacy_baseline and not args.report_only:
        p.error("--legacy-baseline requires --report-only")
    if args.postgres_bin and (not args.report_only or args.legacy_baseline):
        p.error("--postgres-bin requires --report-only and cannot be used with --legacy-baseline")
    if os.geteuid() == 0:
        p.error("the disposable fixture must run as non-root")
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    server = args.mysql_bin.resolve(strict=True)
    tools = gate.llvm_tools()
    targets = LEGACY if args.legacy_baseline else {**LEGACY, JOURNEYS: None}
    postgres_bin = args.postgres_bin.resolve(strict=True) if args.postgres_bin else None
    if postgres_bin:
        targets[CROSS_TRANSFER] = ("postgres_catalog_types_preflight_actual_mysql_target",)
    env = {k: v for k, v in os.environ.items() if not k.startswith((
        "MYSQL_", "TEST_MYSQL_", "MIGRATION_TEST_", "DATAZEN_CONTRACT_", "TEST_PG_", "PG"))}
    source = {"commit": gate.read(["git", "rev-parse", "HEAD"]),
              "working_tree_dirty": bool(gate.read(["git", "status", "--porcelain"])),
              "rustc": gate.read(["rustc", "--version"])}
    binaries = gate.build(targets, out, env, args.target_dir.resolve())
    source["executables_sha256"] = {}
    for name, path in binaries.items():
        with path.open("rb") as stream:
            source["executables_sha256"][name] = hashlib.file_digest(stream, "sha256").hexdigest()
    with tempfile.TemporaryDirectory(prefix="dz-mysql-", dir="/tmp") as d:
        fixture = Mysql(server, Path(d), out, env)
        postgres = None
        try:
            source["mysql"] = fixture.start()
            test_env = fixture.test_env()
            ignored = ["restore_sql"]
            if postgres_bin:
                pg_root, pg_out = Path(d) / "pg", out / "postgres-fixture"
                pg_root.mkdir()
                pg_out.mkdir()
                postgres = gate.Postgres(postgres_bin, pg_out, pg_root / "data", env)
                postgres.start()
                pg_env = postgres.env_for_tests()
                test_env.update({key: val for key, val in pg_env.items()
                                 if key.startswith("TEST_PG_") or key == "PGSSLROOTCERT"})
                test_env["DATAZEN_MYSQL_E2E_PSQL"] = str(postgres_bin / "psql")
                test_env["PGSSLMODE"] = "verify-full"
                source["postgres_transfer_source"] = gate.read([postgres_bin / "postgres", "--version"])
                ignored.append(CROSS_TRANSFER)
            counts, profiles = gate.run_live(binaries, targets, test_env, fixture.password,
                                              out, include_ignored=tuple(ignored))
        finally:
            try:
                if postgres:
                    postgres.stop()
            finally:
                fixture.stop()
    paths, hit, total = gate.measure(binaries, profiles, out, tools, lcov_filename="mysql-e2e.lcov")
    report = {"source": source, "scope": str(gate.SCOPE), "files": paths,
              "covered": hit, "total": total, "percent": 100 * hit / total,
              "live_e2e": counts, "minimum_percent": gate.TARGET,
              "meets_threshold": hit * 100 >= total * gate.TARGET,
              "legacy_baseline": args.legacy_baseline}
    (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"MySQL production E2E lines: {hit}/{total} ({report['percent']:.2f}%); required {gate.TARGET}%")
    return int(bool(counts["failed"] or counts["skipped"]) or
               not (args.report_only or report["meets_threshold"]))


if __name__ == "__main__":
    raise SystemExit(main())
