#!/usr/bin/env bash
# Build datazen-jdbc-agent.jar (Java 17+).
# Downloads HikariCP + SLF4J into lib/ (cached) and packs a fat jar.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
OUT="$ROOT/build"
CLASSES="$OUT/classes"
LIB="$ROOT/lib"
mkdir -p "$CLASSES" "$LIB" "$OUT"

HIKARI_VER="5.1.0"
SLF4J_VER="2.0.13"
HIKARI_JAR="$LIB/HikariCP-${HIKARI_VER}.jar"
SLF4J_API_JAR="$LIB/slf4j-api-${SLF4J_VER}.jar"
SLF4J_NOP_JAR="$LIB/slf4j-nop-${SLF4J_VER}.jar"

download() {
  local url="$1" dest="$2"
  if [[ -f "$dest" ]]; then
    return 0
  fi
  echo "Downloading $(basename "$dest")..."
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL -o "$dest" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$dest" "$url"
  else
    echo "Need curl or wget to download $url" >&2
    exit 1
  fi
}

download \
  "https://repo1.maven.org/maven2/com/zaxxer/HikariCP/${HIKARI_VER}/HikariCP-${HIKARI_VER}.jar" \
  "$HIKARI_JAR"
download \
  "https://repo1.maven.org/maven2/org/slf4j/slf4j-api/${SLF4J_VER}/slf4j-api-${SLF4J_VER}.jar" \
  "$SLF4J_API_JAR"
download \
  "https://repo1.maven.org/maven2/org/slf4j/slf4j-nop/${SLF4J_VER}/slf4j-nop-${SLF4J_VER}.jar" \
  "$SLF4J_NOP_JAR"

CP="$HIKARI_JAR:$SLF4J_API_JAR:$SLF4J_NOP_JAR"
rm -rf "$CLASSES"
mkdir -p "$CLASSES"
find "$ROOT/src/main/java" -name '*.java' > "$OUT/sources.txt"
javac -encoding UTF-8 -source 17 -target 17 -cp "$CP" -d "$CLASSES" @"$OUT/sources.txt"

# Fat jar: agent classes + Hikari + SLF4J (nop binding)
STAGE="$OUT/stage"
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp -R "$CLASSES"/. "$STAGE"/
(
  cd "$STAGE"
  jar xf "$HIKARI_JAR"
  jar xf "$SLF4J_API_JAR"
  jar xf "$SLF4J_NOP_JAR"
  rm -rf META-INF/maven META-INF/INDEX.LIST 2>/dev/null || true
  find META-INF -name '*.SF' -delete 2>/dev/null || true
  find META-INF -name '*.RSA' -delete 2>/dev/null || true
  find META-INF -name '*.DSA' -delete 2>/dev/null || true
)
jar cfe "$OUT/datazen-jdbc-agent.jar" com.datazen.jdbcagent.AgentMain -C "$STAGE" .
echo "Built $OUT/datazen-jdbc-agent.jar (HikariCP ${HIKARI_VER})"
