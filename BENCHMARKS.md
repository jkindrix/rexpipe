# Performance Benchmarks

This document provides honest performance comparisons between rexpipe and traditional Unix tools.

## Executive Summary

**rexpipe is NOT faster than sed/grep for simple operations.**

rexpipe's value proposition is **maintainability, not raw speed**:

| Dimension | sed/awk/grep | rexpipe |
|-----------|--------------|---------|
| Speed (simple ops) | Faster | Slower |
| Memory usage | ~2-4 MB | ~24 MB (fixed) |
| Maintainability | Poor | Excellent |
| Reusability | None | High |
| Structured output | None | JSON |
| Error handling | Silent failures | Structured errors |
| Audit trail | None | Full verification |

## When to Use rexpipe

✅ **Use rexpipe when:**
- Building 5+ step transformation pipelines
- Pipelines need to be version-controlled and reviewed
- Pipelines are shared across team members
- You need audit trails or verification
- You want structured JSON output
- Pattern libraries would reduce duplication
- Maintainability matters more than microseconds

❌ **Use sed/grep when:**
- Quick one-off substitutions
- Interactive exploration
- Maximum speed is critical
- Simple single-pattern operations

## Benchmark Results

### Test Environment
- CPU: AMD64 (results may vary)
- Input: Log files with mixed content
- rexpipe: v2.0.0, release build

### Simple Substitution (replace all digits)

| Tool | 100K lines | 1M lines | Notes |
|------|------------|----------|-------|
| sed | 0.16s | 1.6s | Fastest for simple patterns |
| rexpipe | 0.74s | 7.3s | ~4-5x slower |

**Why slower:** rexpipe compiles regex patterns with the Rust regex crate which prioritizes safety (linear time, no ReDoS) over raw speed for simple patterns.

### Line Filtering (grep for ERROR)

| Tool | 100K lines | 1M lines | Notes |
|------|------------|----------|-------|
| grep | 0.001s | 0.001s | Highly optimized C |
| rexpipe | 0.02s | 0.17s | Still fast, but grep is faster |

**Why slower:** grep is one of the most optimized text processing tools in existence.

### Multi-Step Pipeline (3 transformations)

| Tool | 1M lines | Notes |
|------|----------|-------|
| sed x3 (piped) | 0.5s | Three sed processes piped |
| rexpipe | 5.1s | Single process, all steps |

**Why this matters less than it seems:**
- The 0.5s vs 5.1s difference is 4.6 seconds for 1 million lines
- For typical use cases (logs, configs, source files), files are much smaller
- The time saved maintaining cryptic sed one-liners far exceeds runtime difference

### Memory Usage

| Tool | 10K lines | 100K lines | 1M lines |
|------|-----------|------------|----------|
| sed | ~2 MB | ~2 MB | ~2 MB |
| rexpipe | ~24 MB | ~24 MB | ~24 MB |

**Key insight:** rexpipe's memory is **constant** regardless of file size due to streaming architecture. The 24 MB is fixed overhead for the Rust runtime and compiled patterns.

For 10 GB files, both tools maintain constant memory.

## Where rexpipe Shines

### 1. Pipeline Maintainability

**sed approach:**
```bash
cat log | sed 's/\[ERROR\]/[ERR]/g' | sed '/DEBUG/d' | \
  sed 's/user_id=\([0-9]*\)/uid=\1/g' | sed 's/192\.168\./10.0./g'
```

**rexpipe approach:**
```toml
name = "log-cleanup"
description = "Normalize production logs"

[[step]]
description = "Shorten error tags"
pattern = '\[ERROR\]'
replacement = "[ERR]"

[[step]]
description = "Remove debug noise"
type = "filter"
pattern = 'DEBUG'
action = "drop_line"

[[step]]
description = "Anonymize user IDs"
pattern = 'user_id=(\d+)'
replacement = "uid=${1}"

[[step]]
description = "Anonymize IPs"
pattern = '192\.168\.'
replacement = "10.0."
```

**Time saved:**
- New team member understanding: sed = 10 minutes, rexpipe = 30 seconds
- Debugging failed transformation: sed = trial and error, rexpipe = read description
- Code review: sed = "LGTM I guess", rexpipe = meaningful review

### 2. Pattern Libraries

```toml
# patterns/security.toml - use across all projects
[patterns.secrets]
aws_key = '\b(AKIA|ASIA)[A-Z0-9]{16}\b'
github_token = '\bgh[ps]_[A-Za-z0-9]{36}\b'
api_key = '(api[_-]?key|secret|token)\s*[:=]\s*["\x27][^\x27"]{8,}'

# Any pipeline can reference:
pattern = '${secrets.aws_key}'
```

### 3. Structured Output

```json
{
  "metadata": {
    "schema_version": "1.0",
    "mode": "processing",
    "tool_version": "2.0.0"
  },
  "data": {
    "lines_processed": 10000,
    "matches_found": 423,
    "transformations_applied": 423
  }
}
```

### 4. Verification Mode

```bash
$ rexpipe -c pipeline.toml --verify < log.txt
Verification Report
==================
Lines processed: 10,000
Step 1 (Shorten error tags): 2,000 matches, 2,000 replacements
Step 2 (Remove debug noise): 2,000 lines dropped
Step 3 (Anonymize user IDs): 4,000 matches, 4,000 replacements
Step 4 (Anonymize IPs): 6,000 matches, 6,000 replacements
```

## Running Your Own Benchmarks

```bash
# Install hyperfine for precise measurements
cargo install hyperfine

# Run the comparison script
./benches/compare_tools.sh

# Custom benchmark
hyperfine \
  "rexpipe -p '\d+' -r 'X' --text < large.log" \
  "sed 's/[0-9]*/X/g' < large.log"
```

## Optimization Tips

1. **Use `--text` for pure text output** (avoids JSON encoding overhead)
2. **Use `-F` for literal strings** (avoids regex compilation)
3. **Combine steps in one pipeline** (single pass over data)
4. **Use `--parallel` for multi-file operations**
5. **Consider streaming for very large files** (memory stays constant)

## Conclusion

rexpipe trades raw speed for:
- Self-documenting pipelines
- Team-friendly configurations
- Version-controllable transformations
- Structured error handling
- Audit trails

For production systems where **maintainability, correctness, and auditability** matter more than saving milliseconds, rexpipe is the right choice.

For quick interactive exploration or maximum-speed one-liners, use sed/grep.
