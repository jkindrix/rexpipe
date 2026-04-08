# Release Readiness Checklist

A comprehensive checklist for validating release readiness for rexpipe.

---

## 0. Pre-flight Checks

Quick verification before detailed review:

```bash
# Option A: Use just recipe (recommended)
just ci

# Option B: Manual commands
git status  # Should show no uncommitted changes
cargo check --all-features
cargo test --all-features
cargo clippy --all-features -- -D warnings
```

- [ ] Git working directory is clean (or changes are intentional)
- [ ] CI is passing on the target branch
- [ ] Local build/test/lint all pass (`just ci`)

---

## 1. Codebase Hygiene & Safety

### Work-in-Progress Markers

```bash
# Use just recipe (recommended)
just wip-check

# Manual commands
grep -rn "TODO\|FIXME\|XXX\|HACK" --include="*.rs" src/
grep -rn "todo!\|unimplemented!" --include="*.rs" src/
```
- [ ] Run `just wip-check` or grep for `TODO`, `FIXME`, `XXX`, `HACK` comments
- [ ] Verify no `todo!()`, `unimplemented!()` macros in production code
- [ ] Ensure no incomplete logic ships to production

### Panic Path Audit

```bash
# Use just recipe (recommended)
just panic-audit

# Manual commands
grep -rn "\.unwrap()" src/ --include="*.rs"
grep -rn "\.expect(" src/ --include="*.rs"
```

- [ ] Run `just panic-audit` to audit `.unwrap()` and `.expect()` calls
- [ ] **Note:** Some unwraps are acceptable (e.g., regex compilation with known-valid patterns)
- [ ] Verify all production panic paths have documented justification

### Dead Code Analysis
- [ ] Review `#[allow(dead_code)]` suppressions
- [ ] Ensure suppressions are either documented (public API surface) or removed
- [ ] Check for unused imports and dependencies

### Strict Linting

```bash
just clippy          # Standard linting
just clippy-strict   # Pedantic linting
```

- [ ] Run `just clippy` (warnings-as-errors)
- [ ] Verify all feature flag combinations pass linting (`just test-features`)

---

## 2. Version Consistency (The "Blast Radius")

```bash
just version-sync    # Verify README matches Cargo.toml
```

### Core Manifest
- [ ] Bump version in `Cargo.toml`
- [ ] Update `rust-version` if MSRV changed

### Documentation Version References
Run `just version-sync` then grep for old version strings:
- [ ] README.md installation instructions
- [ ] CHANGELOG.md (move Unreleased to versioned section)
- [ ] Any example configurations in docs/

### Example Projects
- [ ] Verify `examples/` directory references work with current API
- [ ] Test example pipelines still function correctly

---

## 3. Environment & Infrastructure Alignment

### Minimum Supported Rust Version (MSRV) Sync

```bash
just msrv-check    # Verify code compiles with declared MSRV
```

Ensure MSRV is consistent across **all** locations:
- [ ] `.github/workflows/ci.yml` (MSRV job)
- [ ] `CONTRIBUTING.md` prerequisites
- [ ] `Cargo.toml` rust-version field
- [ ] README.md (if mentioned)

### CI Configuration Validity
- [ ] Verify CI workflow tests all feature combinations
- [ ] Check that MSRV job uses correct Rust version
- [ ] Ensure clippy and fmt jobs are current

---

## 4. Dependency & Security Compliance

### Vulnerability Scan

```bash
just audit    # Run cargo-audit (security vulnerabilities)
just deny     # Run cargo-deny (licenses, bans, advisories) - if configured
```

- [ ] Run `just audit` for security vulnerabilities
- [ ] Review and address all advisories
- [ ] **Note:** `duplicate` warnings are informational

### License Compliance
- [ ] Verify no new dependencies violate licensing policy (MIT OR Apache-2.0)
- [ ] Check transitive dependencies

---

## 5. Documentation Integrity

### Link Validation

```bash
just doc-check     # Check documentation builds without warnings
```

- [ ] Run `just doc-check` to verify docs build without warnings
- [ ] Verify internal relative links in README resolve
- [ ] Check that code examples in docs compile

### Changelog Maintenance
- [ ] Move "Unreleased" changes to versioned header
- [ ] Add release date
- [ ] Ensure semantic versioning adherence
- [ ] Include all breaking changes prominently

---

## 6. Final Build Verification

```bash
just ci-release    # Full release CI: ci + audit + msrv + test-features
```

### Clean Build
```bash
just check    # Fast type check
just build    # Full debug build
```
- [ ] Verify clean compilation with all feature combinations

### Test Suite
```bash
just test              # Standard test run
just test-features     # All feature combinations
```
- [ ] All tests pass (`just test`)
- [ ] No flaky tests
- [ ] Feature matrix tests pass (`just test-features`)

### Fuzz Testing (Optional)
```bash
just fuzz-all    # Quick fuzz of all targets
```
- [ ] Run fuzz tests if time permits

### Example/Benchmark Compilation
```bash
just bench    # Run benchmarks
```
- [ ] Benchmarks compile and run

---

## 7. API Compatibility & Semver

### Breaking Change Detection

```bash
just semver    # Run cargo-semver-checks (if installed)
```

- [ ] Run `just semver` or verify manually
- [ ] Review any flagged breaking changes
- [ ] Ensure breaking changes warrant version bump

### Public API Surface
- [ ] Audit public exports for unintended exposure
- [ ] Check that internal modules aren't accidentally public

---

## 8. Publishing Preparation

### Pre-publish Verification

```bash
just publish-dry    # Dry-run publish
```

- [ ] Run `just publish-dry` - crate succeeds
- [ ] No unexpected files included in package
- [ ] Package size is reasonable

### Cargo.toml Metadata

```bash
just metadata-check    # Verify required metadata for crates.io
```

Check metadata for crates.io display:
```bash
cargo metadata --no-deps --format-version 1 | jq '.packages[] | select(.name == "rexpipe") | {description, repository, keywords, categories, license}'
```

**Required fields:**
- [ ] `description` - concise crate description
- [ ] `license` - SPDX identifier ("MIT OR Apache-2.0")
- [ ] `repository` - GitHub URL

**Recommended fields:**
- [ ] `keywords` - up to 5 searchable keywords
- [ ] `categories` - crates.io categories

---

## 9. Git & Release Protocol

### Release Workflow (Follow This Order)

**Publishing to crates.io is IRREVERSIBLE.** Follow this exact sequence:

```
┌─────────────────────────────────────────────────────────────┐
│  1. PREPARE: Version bump + CHANGELOG + commit              │
│                         ↓                                   │
│  2. PUSH: git push origin main                              │
│                         ↓                                   │
│  3. WAIT: CI must pass on main                              │
│                         ↓                                   │
│  4. TAG: just tag (creates v<version>)                      │
│                         ↓                                   │
│  5. PUSH TAG: git push origin v<version>                    │
│                         ↓                                   │
│  6. PUBLISH: cargo publish                                  │
└─────────────────────────────────────────────────────────────┘
```

**Step-by-step commands:**

```bash
# Step 1: Prepare (already done if following this checklist)
# - Bump version in Cargo.toml
# - Update CHANGELOG.md with new version section
# - Commit: git commit -m "chore: release v X.Y.Z"

# Step 2: Push to main
git push origin main

# Step 3: Wait for CI to pass
# Check GitHub Actions or run `just ci` locally

# Step 4: Create tag (ONLY after CI passes!)
just tag                        # Creates annotated tag v<version>

# Step 5: Push tag
git push origin v<version>

# Step 6: Publish (manual for now)
cargo publish
```

### Pre-Tag Checklist

- [ ] Version bumped in Cargo.toml
- [ ] CHANGELOG.md updated with version and date
- [ ] Version bump committed and pushed to main
- [ ] **CI passing on main** (critical - verify before tagging!)

### Tagging

```bash
just tag    # Create annotated tag from Cargo.toml version
```

- [ ] Run `just tag` to create `v<version>` tag
- [ ] Tag matches version in Cargo.toml exactly
- [ ] Tag pushed: `git push origin v<version>`

---

## 10. Post-Release Verification

### Publication Verification
- [ ] Crate appears on crates.io
- [ ] Documentation builds on docs.rs
- [ ] Version number correct on registry

### Installation Test
```bash
cargo new test-install && cd test-install
cargo add rexpipe@<new-version>
cargo build
```
- [ ] Fresh installation from registry succeeds
- [ ] Basic functionality works: `echo "test123" | rexpipe -p '\d+'`

### Repository Cleanup
- [ ] Add new `[Unreleased]` section in CHANGELOG for next cycle
- [ ] Close related milestones/issues

---

## Summary of Feature Flags

rexpipe has optional features that should be tested:

| Feature | Description | Test Command |
|---------|-------------|--------------|
| `default` (`cli`) | Full CLI build | `cargo test` |
| `core` | WASM-safe library only | `cargo test --lib --no-default-features --features core` |
| `core` + wasm32 | Library compiles for WASM | `cargo check --no-default-features --features core --target wasm32-unknown-unknown` |
| `async` | Async file processing | `cargo test --features async` |
| `fpe` | Format-preserving encryption | `cargo test --features fpe` |
| `tree-sitter` | Syntax-aware processing | `cargo test --features tree-sitter` |
| `all` | All features | `cargo test --all-features` |

> **Note on PCRE:** There is no `pcre` feature in 2.1.0+. The `fancy-regex`
> engine is always compiled in and rexpipe auto-detects whether to use it
> based on the pattern. See the README's "Regex Engine Options" section.

---

## Justfile Recipe Mapping

Quick reference: which `just` recipes cover which checklist sections.

| Checklist Section | Just Recipe(s) | What It Covers |
|-------------------|----------------|----------------|
| **0. Pre-flight** | `just ci` | fmt, clippy, test, doc-check |
| **1. Code Hygiene** | `just wip-check` | TODO/FIXME/XXX/HACK, todo!/unimplemented! |
| **1. Code Hygiene** | `just panic-audit` | .unwrap()/.expect() in production code |
| **1. Code Hygiene** | `just clippy` | Linting with warnings-as-errors |
| **2. Version Consistency** | `just version-sync` | README version check |
| **3. Environment** | `just msrv-check` | MSRV compilation verification |
| **4. Security** | `just audit` | Security vulnerabilities |
| **5. Documentation** | `just doc-check` | Documentation builds without warnings |
| **6. Build Verification** | `just ci-release` | Full CI + security + msrv + features |
| **6. Build Verification** | `just test-features` | Feature matrix testing |
| **7. Semver** | `just semver` | Breaking change detection |
| **8. Publishing** | `just publish-dry` | Dry-run publish |
| **8. Publishing** | `just metadata-check` | Cargo.toml metadata verification |
| **9. Git Protocol** | `just tag` | Create annotated version tag |
| **9. Git Protocol** | `just release-check` | Full release validation + git state |

**Comprehensive Release Command:**
```bash
just release-check    # Runs: ci-release + wip-check + panic-audit + metadata-check + git checks
```
