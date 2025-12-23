# Progressive Code Analysis System

This directory demonstrates **true progressive multi-stage transformation** where multiple pipelines work together as a system.

## The Pipeline Chain

```
Source Code
    │
    ▼
┌─────────────────────────┐
│  01-extract-symbols.toml │  Extract functions, classes, imports
└───────────┬─────────────┘
            │ symbols.intermediate
            ▼
┌─────────────────────────┐
│  02-build-graph.toml     │  Build dependency/call relationships
└───────────┬─────────────┘
            │ graph.intermediate
            ▼
┌─────────────────────────┐
│  03-analyze-patterns.toml│  Detect patterns, smells, opportunities
└───────────┬─────────────┘
            │ analysis.intermediate
            ▼
┌─────────────────────────┐
│  04-generate-report.toml │  Synthesize into actionable report
└───────────┴─────────────┘
            │
            ▼
      Final Report

```

## Usage

```bash
# Run the complete pipeline chain
cat src/*.py | \
  rexpipe -c 01-extract-symbols.toml | \
  rexpipe -c 02-build-graph.toml | \
  rexpipe -c 03-analyze-patterns.toml | \
  rexpipe -c 04-generate-report.toml

# Or save intermediate artifacts for inspection
rexpipe -c 01-extract-symbols.toml -R src/ > symbols.intermediate
rexpipe -c 02-build-graph.toml < symbols.intermediate > graph.intermediate
rexpipe -c 03-analyze-patterns.toml < graph.intermediate > analysis.intermediate
rexpipe -c 04-generate-report.toml < analysis.intermediate > report.md
```

## Why This Matters

Each pipeline:
1. **Consumes** the structured output of the previous stage
2. **Adds** new understanding through its transformation
3. **Outputs** a richer intermediate representation

This is fundamentally different from a single pipeline with multiple steps.
The intermediate representations can be:
- Cached for incremental processing
- Inspected for debugging
- Fed to alternative downstream pipelines
- Used as input to other tools

## Key Insight

The marker format (`@@TYPE:value@@`) becomes a **protocol** — a contract between pipeline stages. Each pipeline knows what markers to expect as input and what markers to produce as output.
