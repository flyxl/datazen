#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
OUT="$ROOT/build"
CLASSES="$OUT/classes"
rm -rf "$CLASSES"
mkdir -p "$CLASSES"

find "$ROOT/src/main/java" -name '*.java' > "$OUT/sources.txt"
javac -encoding UTF-8 -source 17 -target 17 -d "$CLASSES" @"$OUT/sources.txt"
jar cfe "$OUT/datazen-jdbc-agent.jar" com.datazen.jdbcagent.AgentMain -C "$CLASSES" .
echo "Built $OUT/datazen-jdbc-agent.jar"
