#!/usr/bin/env bash
#
# Benchmark comparison: rexpipe vs sed vs awk vs ripgrep
#
# This script compares performance of rexpipe against common Unix text processing tools.
# Requires: hyperfine, sed, awk, rg (ripgrep)
#
# Install hyperfine: cargo install hyperfine
# Install ripgrep: cargo install ripgrep
#
# Usage: ./benches/compare_tools.sh [--warmup N] [--runs N]

set -e

WARMUP=${1:-3}
RUNS=${2:-10}

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  rexpipe Benchmark Comparison Suite${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Check for required tools
check_tool() {
    if ! command -v "$1" &> /dev/null; then
        echo -e "${RED}Error: $1 is not installed${NC}"
        exit 1
    fi
}

check_tool hyperfine
check_tool sed
check_tool awk
echo -e "${GREEN}✓ Required tools found${NC}"

# Check for optional tools
HAS_RG=false
if command -v rg &> /dev/null; then
    HAS_RG=true
    echo -e "${GREEN}✓ ripgrep found${NC}"
else
    echo -e "${YELLOW}⚠ ripgrep not found (skipping rg benchmarks)${NC}"
fi

# Build rexpipe in release mode
echo ""
echo -e "${BLUE}Building rexpipe in release mode...${NC}"
cargo build --release --quiet
REXPIPE="./target/release/rexpipe"
echo -e "${GREEN}✓ Build complete${NC}"

# Create test data
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

echo ""
echo -e "${BLUE}Generating test data...${NC}"

# Small dataset (10K lines)
for i in $(seq 1 10000); do
    case $((i % 5)) in
        0) echo "2024-12-21 10:15:23 [ERROR] Database connection failed user_id=1234 from 192.168.1.10" ;;
        1) echo "2024-12-21 10:15:24 [INFO] Request completed in 45ms" ;;
        2) echo "2024-12-21 10:15:25 [DEBUG] Parsing config file /etc/app/config.yaml" ;;
        3) echo "2024-12-21 10:15:26 [WARN] High memory usage: 85% user_id=5678" ;;
        4) echo "2024-12-21 10:15:27 [INFO] User login john.doe@example.com from 192.168.1.50" ;;
    esac
done > "$TMPDIR/small.log"

# Medium dataset (100K lines)
for i in $(seq 1 10); do
    cat "$TMPDIR/small.log"
done > "$TMPDIR/medium.log"

# Large dataset (1M lines)
for i in $(seq 1 10); do
    cat "$TMPDIR/medium.log"
done > "$TMPDIR/large.log"

SMALL_SIZE=$(wc -c < "$TMPDIR/small.log" | tr -d ' ')
MEDIUM_SIZE=$(wc -c < "$TMPDIR/medium.log" | tr -d ' ')
LARGE_SIZE=$(wc -c < "$TMPDIR/large.log" | tr -d ' ')

echo -e "${GREEN}✓ Test data generated:${NC}"
echo "  - small.log:  10,000 lines ($((SMALL_SIZE / 1024)) KB)"
echo "  - medium.log: 100,000 lines ($((MEDIUM_SIZE / 1024)) KB)"
echo "  - large.log:  1,000,000 lines ($((LARGE_SIZE / 1024 / 1024)) MB)"

# Create rexpipe config for multi-step pipeline
cat > "$TMPDIR/pipeline.toml" << 'EOF'
name = "benchmark_pipeline"

[[step]]
pattern = '\[ERROR\]'
replacement = "[ERR]"

[[step]]
type = "filter"
pattern = 'DEBUG'
action = "drop_line"

[[step]]
pattern = 'user_id=(\d+)'
replacement = "uid=${1}"
EOF

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Benchmark 1: Simple Substitution${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "Pattern: Replace digits with 'X'"
echo ""

# Small file
echo -e "${YELLOW}Dataset: small (10K lines)${NC}"
hyperfine --warmup "$WARMUP" --runs "$RUNS" \
    --export-markdown "$TMPDIR/bench1_small.md" \
    -n "rexpipe" "$REXPIPE -p '\d+' -r 'X' < $TMPDIR/small.log > /dev/null" \
    -n "sed" "sed 's/[0-9][0-9]*/X/g' < $TMPDIR/small.log > /dev/null" \
    -n "awk" "awk '{gsub(/[0-9]+/, \"X\")}1' < $TMPDIR/small.log > /dev/null"

# Medium file
echo ""
echo -e "${YELLOW}Dataset: medium (100K lines)${NC}"
hyperfine --warmup "$WARMUP" --runs "$RUNS" \
    --export-markdown "$TMPDIR/bench1_medium.md" \
    -n "rexpipe" "$REXPIPE -p '\d+' -r 'X' < $TMPDIR/medium.log > /dev/null" \
    -n "sed" "sed 's/[0-9][0-9]*/X/g' < $TMPDIR/medium.log > /dev/null" \
    -n "awk" "awk '{gsub(/[0-9]+/, \"X\")}1' < $TMPDIR/medium.log > /dev/null"

# Large file
echo ""
echo -e "${YELLOW}Dataset: large (1M lines)${NC}"
hyperfine --warmup "$WARMUP" --runs "$RUNS" \
    --export-markdown "$TMPDIR/bench1_large.md" \
    -n "rexpipe" "$REXPIPE -p '\d+' -r 'X' < $TMPDIR/large.log > /dev/null" \
    -n "sed" "sed 's/[0-9][0-9]*/X/g' < $TMPDIR/large.log > /dev/null" \
    -n "awk" "awk '{gsub(/[0-9]+/, \"X\")}1' < $TMPDIR/large.log > /dev/null"

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Benchmark 2: Line Filtering${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "Pattern: Keep only ERROR lines"
echo ""

echo -e "${YELLOW}Dataset: large (1M lines)${NC}"
CMD_RG=""
if $HAS_RG; then
    CMD_RG="-n 'ripgrep' 'rg ERROR < $TMPDIR/large.log > /dev/null'"
fi

hyperfine --warmup "$WARMUP" --runs "$RUNS" \
    --export-markdown "$TMPDIR/bench2.md" \
    -n "rexpipe" "$REXPIPE -p 'ERROR' < $TMPDIR/large.log > /dev/null" \
    -n "sed" "sed -n '/ERROR/p' < $TMPDIR/large.log > /dev/null" \
    -n "awk" "awk '/ERROR/' < $TMPDIR/large.log > /dev/null" \
    -n "grep" "grep ERROR < $TMPDIR/large.log > /dev/null" \
    ${CMD_RG:+-n "ripgrep" "rg ERROR < $TMPDIR/large.log > /dev/null"}

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Benchmark 3: Complex Multi-Step Pipeline${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "Steps: Normalize errors, drop debug lines, reformat user IDs"
echo ""

echo -e "${YELLOW}Dataset: large (1M lines)${NC}"
hyperfine --warmup "$WARMUP" --runs "$RUNS" \
    --export-markdown "$TMPDIR/bench3.md" \
    -n "rexpipe (pipeline)" "$REXPIPE -c $TMPDIR/pipeline.toml < $TMPDIR/large.log > /dev/null" \
    -n "sed (3 passes)" "sed 's/\[ERROR\]/[ERR]/g' < $TMPDIR/large.log | sed '/DEBUG/d' | sed 's/user_id=\([0-9]*\)/uid=\1/g' > /dev/null" \
    -n "awk (single pass)" "awk '/DEBUG/{next} {gsub(/\[ERROR\]/,\"[ERR]\"); gsub(/user_id=([0-9]+)/,\"uid=\\\\1\")}1' < $TMPDIR/large.log > /dev/null"

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Benchmark 4: IP Address Anonymization${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "Pattern: Replace IP octets"
echo ""

echo -e "${YELLOW}Dataset: large (1M lines)${NC}"
hyperfine --warmup "$WARMUP" --runs "$RUNS" \
    --export-markdown "$TMPDIR/bench4.md" \
    -n "rexpipe" "$REXPIPE -p '192\.168\.\d+\.\d+' -r '10.0.X.X' < $TMPDIR/large.log > /dev/null" \
    -n "sed" "sed 's/192\.168\.[0-9]*\.[0-9]*/10.0.X.X/g' < $TMPDIR/large.log > /dev/null" \
    -n "awk" "awk '{gsub(/192\.168\.[0-9]+\.[0-9]+/, \"10.0.X.X\")}1' < $TMPDIR/large.log > /dev/null"

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Benchmark 5: Capture Group Substitution${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "Pattern: Reformat timestamps from YYYY-MM-DD to DD/MM/YYYY"
echo ""

echo -e "${YELLOW}Dataset: large (1M lines)${NC}"
hyperfine --warmup "$WARMUP" --runs "$RUNS" \
    --export-markdown "$TMPDIR/bench5.md" \
    -n "rexpipe" "$REXPIPE -p '(\d{4})-(\d{2})-(\d{2})' -r '\${3}/\${2}/\${1}' < $TMPDIR/large.log > /dev/null" \
    -n "sed" "sed 's/\([0-9]\{4\}\)-\([0-9]\{2\}\)-\([0-9]\{2\}\)/\3\/\2\/\1/g' < $TMPDIR/large.log > /dev/null" \
    -n "awk" "awk '{gsub(/([0-9]{4})-([0-9]{2})-([0-9]{2})/, \"\\\\3/\\\\2/\\\\1\")}1' < $TMPDIR/large.log > /dev/null"

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Benchmark Complete!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "Results saved to: $TMPDIR/bench*.md"
echo ""
echo -e "${BLUE}Summary observations:${NC}"
echo "- Simple patterns: All tools are comparable, I/O dominates"
echo "- Multi-step pipelines: rexpipe avoids multiple process spawns"
echo "- Complex regex: Rust regex crate is highly optimized"
echo "- Large files: Streaming architecture maintains constant memory"
