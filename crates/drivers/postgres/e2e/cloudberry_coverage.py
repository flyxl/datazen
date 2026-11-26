#!/usr/bin/env python3
"""Run Cloudberry alias E2E against a fresh native coordinator and two segments."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import tempfile

# Reuse the verifier's strict libtest and LLVM parsing; never reuse its
# PostgreSQL fixture or any profiles from another engine.
import coverage as gate

JOURNEYS = "cloudberry_live_e2e"
DATABASE = "dz_fixture_cloudberry_mpp"


def loopback_ports(count):
    listeners = []
    try:
        for _ in range(count):
            listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            listeners.append(listener)
            listener.bind(("127.0.0.1", 0))
        return [listener.getsockname()[1] for listener in listeners]
    finally:
        for listener in listeners:
            listener.close()


def literal(value):
    return "'" + str(value).replace("'", "''") + "'"


class Cluster:
    def __init__(self, bin_dir, output, data, env):
        self.bin_dir, self.output, self.data = bin_dir, output, data
        self.env = {k: v for k, v in env.items()
                    if not k.startswith(("PG", "TEST_CLOUDBERRY_"))}
        self.env["LC_ALL"] = "C"
        self.password = secrets.token_hex(24)
        self.port, seg0, seg1 = loopback_ports(3)
        self.instances = (("coord", -1, 1, self.port), ("seg0", 0, 2, seg0), ("seg1", 1, 3, seg1))

    def invoke(self, program, *argv, **kwargs):
        extra = kwargs.pop("extra", {})
        return gate.cmd([self.bin_dir / program, *argv], env={**self.env, **extra},
                        check=True, timeout=45, **kwargs)

    def start_instance(self, name, role):
        options = f"-c gp_role={role}"
        if role == "utility":
            options += " -m"
        self.invoke("pg_ctl", "-D", self.data / name, "-w", "-t", "20",
                    "-l", self.output / f"{name}-{role}.log", "-o", options, "start")

    def stop_instance(self, name):
        if (self.data / name / "postmaster.pid").is_file():
            self.invoke("pg_ctl", "-D", self.data / name, "-w", "-t", "20",
                        "-m", "fast", "stop", stdout=subprocess.DEVNULL)

    def psql(self, db, sql, role=None):
        env = {"PGPASSWORD": self.password, "PGCONNECT_TIMEOUT": "5"}
        if role is not None:
            env["PGOPTIONS"] = f"-c gp_role={role}"
        return self.invoke("psql", "-X", "-Aqt", "-h", "127.0.0.1", "-p", str(self.port),
                           "-d", db, "-U", "datazen", "-v", "ON_ERROR_STOP=1",
                           input=sql, capture_output=True, extra=env).stdout.strip()

    def start(self):
        version = self.invoke("postgres", "--version", capture_output=True).stdout
        if "Apache Cloudberry" not in version:
            raise RuntimeError("--cloudberry-bin must contain real Apache Cloudberry, not PostgreSQL")
        password_file = self.data / "initial-password"
        password_file.touch(mode=0o600)
        password_file.write_text(self.password + "\n")
        try:
            for name, content, dbid, port in self.instances:
                path = self.data / name
                with (self.output / f"{name}-init.log").open("w") as log:
                    self.invoke("initdb", "-D", path, "--locale=C", "--encoding=UTF8",
                                "--shared_buffers=32MB", "--max_connections=32",
                                "--auth=scram-sha-256", f"--pwfile={password_file}",
                                "-U", "datazen", stdout=log, stderr=subprocess.STDOUT)
                # Real MPP exchanges go over TCP, never the optional Unix
                # socket proxy. Start the coordinator in dispatch mode below.
                with (path / "postgresql.conf").open("a") as conf:
                    conf.write(f"\nlisten_addresses = '127.0.0.1'\nunix_socket_directories = ''\n"
                               f"port = {port}\ngp_contentid = {content}\n"
                               "gp_internal_is_singlenode = off\ngp_interconnect_type = 'tcp'\n")
                with (path / "internal.auto.conf").open("a") as conf:
                    conf.write(f"\ngp_dbid = {dbid}\n")
        finally:
            password_file.unlink(missing_ok=True)

        # Same native provisioning steps as gpinitsystem, without remote
        # processes, SSH keys, root privileges, or system configuration.
        self.start_instance("coord", "utility")
        for name, content, dbid, port in self.instances:
            self.psql("postgres",
                      f"SELECT pg_catalog.gp_add_segment({dbid}::int2, {content}::int2,"
                      f" 'p','p','s','u',{port},'127.0.0.1','127.0.0.1',{literal(self.data / name)});",
                      role="utility")
        self.stop_instance("coord")
        self.start_instance("seg0", "execute")
        self.start_instance("seg1", "execute")
        self.start_instance("coord", "dispatch")
        # This must happen AFTER dispatch mode is up so both real segments
        # receive their catalog changes, not just the coordinator.
        self.psql("postgres", f"CREATE DATABASE {DATABASE};")
        self.psql(DATABASE, "CREATE TABLE public.rune_e2e_fixture(marker text NOT NULL) DISTRIBUTED BY (marker);\n"
                  "INSERT INTO public.rune_e2e_fixture VALUES ('datazen-rune-disposable');")
        version = self.psql(DATABASE, "SELECT version();")
        smoke = self.psql(DATABASE,
                          "CREATE TABLE public.fixture_probe(id int) DISTRIBUTED BY (id);\n"
                          "INSERT INTO public.fixture_probe SELECT generate_series(1,128);\n"
                          "SELECT gp_segment_id FROM public.fixture_probe GROUP BY gp_segment_id ORDER BY gp_segment_id;\n"
                          "DROP TABLE public.fixture_probe;")
        if "Apache Cloudberry" not in version or smoke.splitlines() != ["0", "1"]:
            raise RuntimeError("Could not prove Cloudberry owns real data on both segments")
        (self.output / "fixture-proof.txt").write_text(
            f"{version}\nReal distributed fixture rows observed on segment IDs 0 and 1.\n")
        return version

    def test_env(self):
        return {**self.env, "TEST_CLOUDBERRY_PORT": str(self.port),
                "TEST_CLOUDBERRY_DATABASE": DATABASE,
                "TEST_CLOUDBERRY_USER": "datazen",
                "TEST_CLOUDBERRY_PASSWORD": self.password}

    def stop(self):
        errors = []
        for name, *_ in self.instances:
            try:
                self.stop_instance(name)
            except (OSError, subprocess.SubprocessError) as error:
                errors.append(f"{name}: {error}")
        if errors:
            raise RuntimeError("Cloudberry fixture cleanup failed: " + "; ".join(errors))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cloudberry-bin", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target-dir", type=Path, required=True)
    parser.add_argument("--report-only", action="store_true",
                        help="Report a below-threshold run; never waive failed/skipped tests.")
    args = parser.parse_args()
    cloudberry_bin = args.cloudberry_bin.resolve()
    if not all((cloudberry_bin / name).is_file() for name in ("postgres", "psql", "initdb", "pg_ctl")):
        parser.error("--cloudberry-bin requires postgres, psql, initdb and pg_ctl")
    out = args.output.resolve()
    out.mkdir(parents=True)
    tools = gate.llvm_tools()
    source = {"commit": gate.read(["git", "rev-parse", "HEAD"]),
              "working_tree_dirty": bool(gate.read(["git", "status", "--porcelain"])),
              "rustc": gate.read(["rustc", "--version"])}
    selected = {JOURNEYS: None}
    # Keep the same complete production source universe as the PostgreSQL
    # gate. The extra binaries are coverage mappings only; none is replayed.
    universe = {**gate.LEGACY, gate.JOURNEYS: None, **selected}
    binaries = gate.build(universe, out, dict(os.environ), args.target_dir.resolve())
    source["executables_sha256"] = {
        name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in binaries.items()
    }
    with tempfile.TemporaryDirectory(prefix="cloudberry-cluster-", dir=out) as tmp:
        cluster = Cluster(cloudberry_bin, out, Path(tmp), os.environ)
        try:
            source["apache_cloudberry"] = cluster.start()
            counts, profiles = gate.run_live(
                {JOURNEYS: binaries[JOURNEYS]}, selected, cluster.test_env(), cluster.password, out
            )
        finally:
            cluster.stop()
    # Do not present incomplete profile data as a verified coverage result.
    if counts["failed"] or counts["skipped"]:
        return 1
    paths, hit, total = gate.measure(binaries, profiles, out, tools)
    meets = hit * 100 >= total * gate.TARGET
    report = {"source": source, "engine": "cloudberry", "segments": [0, 1],
              "scope": str(gate.SCOPE), "scope_kind": "shared PostgreSQL crate, Cloudberry replay only",
              "files": paths, "covered": hit, "total": total, "percent": 100 * hit / total,
              "live_e2e": counts, "minimum_percent": gate.TARGET, "meets_threshold": meets}
    (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"Cloudberry-only production E2E lines in shared PostgreSQL crate: {hit}/{total} ({report['percent']:.2f}%)")
    return int(not (args.report_only or meets))


if __name__ == "__main__":
    raise SystemExit(main())
