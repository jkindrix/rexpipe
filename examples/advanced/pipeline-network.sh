#!/bin/bash
# Pipeline Network Example
# Demonstrates fan-out, fan-in, and parallel processing patterns
#
# Usage:
#   ./pipeline-network.sh ./src ./reports

set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================

INPUT_DIR="${1:-.}"
OUTPUT_DIR="${2:-./reports}"
INTERMEDIATE_DIR="/tmp/rexpipe-pipeline-$$"
PIPELINE_DIR="$(dirname "$0")/../pipelines"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# =============================================================================
# Setup
# =============================================================================

mkdir -p "$OUTPUT_DIR" "$INTERMEDIATE_DIR"
trap "rm -rf $INTERMEDIATE_DIR" EXIT

log_info "Pipeline Network Analysis"
log_info "Input: $INPUT_DIR"
log_info "Output: $OUTPUT_DIR"

# =============================================================================
# Stage 1: Extract (Single pass over source files)
# =============================================================================

log_info "Stage 1: Extracting symbols from source files..."

find "$INPUT_DIR" -name "*.py" -o -name "*.js" -o -name "*.ts" 2>/dev/null | \
  xargs cat 2>/dev/null | \
  rexpipe -c "$PIPELINE_DIR/progressive-system/01-extract-symbols.toml" \
  > "$INTERMEDIATE_DIR/symbols.intermediate"

SYMBOL_COUNT=$(wc -l < "$INTERMEDIATE_DIR/symbols.intermediate")
log_info "  Extracted $SYMBOL_COUNT symbol markers"

# =============================================================================
# Stage 2: Build Graph (Single pass)
# =============================================================================

log_info "Stage 2: Building dependency graph..."

rexpipe -c "$PIPELINE_DIR/progressive-system/02-build-graph.toml" \
  < "$INTERMEDIATE_DIR/symbols.intermediate" \
  > "$INTERMEDIATE_DIR/graph.intermediate"

NODE_COUNT=$(grep -c "@@NODE:" "$INTERMEDIATE_DIR/graph.intermediate" || echo 0)
EDGE_COUNT=$(grep -c "@@EDGE:" "$INTERMEDIATE_DIR/graph.intermediate" || echo 0)
log_info "  Built graph: $NODE_COUNT nodes, $EDGE_COUNT edges"

# =============================================================================
# Stage 3: Fan-Out - Parallel Analysis
# =============================================================================

log_info "Stage 3: Parallel analysis (fan-out pattern)..."

# Launch multiple analyzers in parallel
{
  # Security Analysis
  rexpipe -c "$PIPELINE_DIR/progressive-system/03-analyze-patterns.toml" \
    < "$INTERMEDIATE_DIR/graph.intermediate" 2>/dev/null | \
    grep -E "@@FINDING.*SECURITY|@@FINDING.*VULNERABILITY" \
    > "$INTERMEDIATE_DIR/security.findings" &
  SECURITY_PID=$!

  # Pattern Analysis
  rexpipe -c "$PIPELINE_DIR/progressive-system/03-analyze-patterns.toml" \
    < "$INTERMEDIATE_DIR/graph.intermediate" 2>/dev/null | \
    grep "@@FINDING.*PATTERN" \
    > "$INTERMEDIATE_DIR/patterns.findings" &
  PATTERNS_PID=$!

  # Smell Analysis
  rexpipe -c "$PIPELINE_DIR/progressive-system/03-analyze-patterns.toml" \
    < "$INTERMEDIATE_DIR/graph.intermediate" 2>/dev/null | \
    grep "@@FINDING.*SMELL" \
    > "$INTERMEDIATE_DIR/smells.findings" &
  SMELLS_PID=$!

  # Opportunity Analysis
  rexpipe -c "$PIPELINE_DIR/progressive-system/03-analyze-patterns.toml" \
    < "$INTERMEDIATE_DIR/graph.intermediate" 2>/dev/null | \
    grep "@@FINDING.*OPPORTUNITY" \
    > "$INTERMEDIATE_DIR/opportunities.findings" &
  OPPORTUNITIES_PID=$!

  # Wait for all parallel jobs
  wait $SECURITY_PID $PATTERNS_PID $SMELLS_PID $OPPORTUNITIES_PID
}

log_info "  Security findings: $(wc -l < "$INTERMEDIATE_DIR/security.findings")"
log_info "  Pattern findings: $(wc -l < "$INTERMEDIATE_DIR/patterns.findings")"
log_info "  Smell findings: $(wc -l < "$INTERMEDIATE_DIR/smells.findings")"
log_info "  Opportunity findings: $(wc -l < "$INTERMEDIATE_DIR/opportunities.findings")"

# =============================================================================
# Stage 4: Generate Individual Reports (Parallel)
# =============================================================================

log_info "Stage 4: Generating individual reports..."

{
  # Security Report
  cat "$INTERMEDIATE_DIR/security.findings" | \
    rexpipe -c "$PIPELINE_DIR/progressive-system/04-generate-report.toml" \
    > "$OUTPUT_DIR/security-report.md" &

  # Patterns Report
  cat "$INTERMEDIATE_DIR/patterns.findings" | \
    rexpipe -c "$PIPELINE_DIR/progressive-system/04-generate-report.toml" \
    > "$OUTPUT_DIR/patterns-report.md" &

  # Smells Report
  cat "$INTERMEDIATE_DIR/smells.findings" | \
    rexpipe -c "$PIPELINE_DIR/progressive-system/04-generate-report.toml" \
    > "$OUTPUT_DIR/smells-report.md" &

  # Opportunities Report
  cat "$INTERMEDIATE_DIR/opportunities.findings" | \
    rexpipe -c "$PIPELINE_DIR/progressive-system/04-generate-report.toml" \
    > "$OUTPUT_DIR/opportunities-report.md" &

  wait
}

# =============================================================================
# Stage 5: Fan-In - Merge Reports
# =============================================================================

log_info "Stage 5: Merging reports (fan-in pattern)..."

{
  echo "# Comprehensive Code Analysis Report"
  echo ""
  echo "Generated: $(date -Iseconds)"
  echo "Source: $INPUT_DIR"
  echo ""
  echo "---"
  echo ""

  echo "## Security Analysis"
  echo ""
  if [ -s "$OUTPUT_DIR/security-report.md" ]; then
    cat "$OUTPUT_DIR/security-report.md"
  else
    echo "_No security issues found._"
  fi
  echo ""

  echo "## Design Patterns"
  echo ""
  if [ -s "$OUTPUT_DIR/patterns-report.md" ]; then
    cat "$OUTPUT_DIR/patterns-report.md"
  else
    echo "_No patterns detected._"
  fi
  echo ""

  echo "## Code Smells"
  echo ""
  if [ -s "$OUTPUT_DIR/smells-report.md" ]; then
    cat "$OUTPUT_DIR/smells-report.md"
  else
    echo "_No code smells detected._"
  fi
  echo ""

  echo "## Improvement Opportunities"
  echo ""
  if [ -s "$OUTPUT_DIR/opportunities-report.md" ]; then
    cat "$OUTPUT_DIR/opportunities-report.md"
  else
    echo "_No opportunities identified._"
  fi
  echo ""

  echo "---"
  echo ""
  echo "## Summary Statistics"
  echo ""
  echo "| Category | Count |"
  echo "|----------|-------|"
  echo "| Symbols Extracted | $SYMBOL_COUNT |"
  echo "| Graph Nodes | $NODE_COUNT |"
  echo "| Graph Edges | $EDGE_COUNT |"
  echo "| Security Findings | $(wc -l < "$INTERMEDIATE_DIR/security.findings") |"
  echo "| Pattern Findings | $(wc -l < "$INTERMEDIATE_DIR/patterns.findings") |"
  echo "| Smell Findings | $(wc -l < "$INTERMEDIATE_DIR/smells.findings") |"
  echo "| Opportunity Findings | $(wc -l < "$INTERMEDIATE_DIR/opportunities.findings") |"

} > "$OUTPUT_DIR/full-report.md"

# =============================================================================
# Done
# =============================================================================

log_success "Analysis complete!"
log_success "Reports generated in: $OUTPUT_DIR"
log_info "  - full-report.md (comprehensive)"
log_info "  - security-report.md"
log_info "  - patterns-report.md"
log_info "  - smells-report.md"
log_info "  - opportunities-report.md"
