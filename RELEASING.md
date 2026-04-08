# Releasing rexpipe

Comprehensive guide for releasing new versions of rexpipe to crates.io.

**Version:** 0.1.0 | **MSRV:** 1.85 | **Edition:** 2024

---

## Critical: Read Before Any Release

### The Cardinal Rules

1. **NEVER manually run `cargo publish`** — Always use the automated GitHub Actions workflow triggered by pushing a version tag. The `just publish` recipe exists only for disaster recovery.

2. **NEVER push a tag until CI passes on main** — Always run `gh run watch` or `just ci-status` to verify CI passed before creating a tag.

3. **ALWAYS run `just release-check` before tagging** — This validates ALL features, documentation, and publishing requirements.

4. **Publishing to crates.io is IRREVERSIBLE** — You can yank a version, but you cannot delete or re-upload it. A yanked version still counts as "used" forever.

### Pre-Release Verification Checklist

Before creating a tag, **always** verify:

```bash
# 1. Run the FULL release check (validates ALL features)
just release-check

# 2. Verify CI passed on main (blocking check)
just ci-status  # Must show "completed" with green check

# 3. Only then create the tag
just tag
git push origin vX.Y.Z
```

---

## Quick Start

For routine releases:

```bash
# 1. Validate everything is ready
just release-check

# 2. Bump version and update CHANGELOG (manual edit)
#    - Edit Cargo.toml: version = "X.Y.Z"
#    - Update CHANGELOG.md with release date

# 3. Commit, push, and wait for CI
git add Cargo.toml CHANGELOG.md
git commit -m "chore: release vX.Y.Z"
git push origin main
gh run watch  # Wait for CI to pass

# 4. Tag and release
just tag                    # Creates annotated tag vX.Y.Z
git push origin vX.Y.Z      # Triggers automated publish
```

---

## Table of Contents

1. [CI Parity](#ci-parity)
2. [Version Numbering](#version-numbering)
3. [Pre-Release Checklist](#pre-release-checklist)
4. [Feature-Specific Testing](#feature-specific-testing)
5. [Changelog Generation](#changelog-generation)
6. [Release Workflow](#release-workflow)
7. [Automated vs Manual Release](#automated-vs-manual-release)
8. [Post-Release Verification](#post-release-verification)
9. [CI Automation Coverage](#ci-automation-coverage)
10. [Manual Recovery Procedures](#manual-recovery-procedures)
11. [Justfile Recipe Reference](#justfile-recipe-reference)
12. [Troubleshooting](#troubleshooting)
13. [Platform-Specific Notes](#platform-specific-notes)
14. [Security Incident Response](#security-incident-response)
15. [Lessons Learned](#lessons-learned)
16. [Release Checklist Template](#release-checklist-template)

---

## CI Parity

### Local Must Match CI

**The local `just ci` command must produce identical results to the GitHub Actions CI pipeline.** This is critical because:

1. If local passes but CI fails, you'll waste time waiting for CI feedback
2. If local fails but CI passes, you're testing against the wrong environment
3. Discrepancies hide bugs that only appear in production/release builds

### Ensuring Parity

| Check | Local Command | CI Job | Must Match |
|-------|---------------|--------|------------|
| Format | `just fmt-check` | `lint` | Yes |
| Clippy | `just clippy` | `lint` | Yes |
| Tests | `just test-locked` | `test` | Yes |
| Doc build | `just doc-check` | `docs` | Yes |
| Deny | `just deny` | `deny` | Yes |
| Semver | `just semver` | `semver` | Yes |
| MSRV | `just msrv-check` | `msrv` | Yes |

### Key Commands

```bash
# Run the same checks as CI
just ci

# Run with locked Cargo.lock (same as CI)
just test-locked

# Run full release CI pipeline
just ci-release
```

### Common Parity Issues

1. **Different Rust versions** — Use `rustup override set stable` in project directory
2. **Different Cargo.lock** — Always commit Cargo.lock, use `--locked` flag
3. **Missing tools** — Run `just setup-tools` to install all required tools
4. **Feature flags** — Always use `--all-features` to match CI configuration

---

## Version Numbering

We follow [Semantic Versioning 2.0.0](https://semver.org/):

- **MAJOR.MINOR.PATCH** (e.g., 1.2.3)
- **Pre-1.0**: Minor bumps may contain breaking changes
- **Post-1.0**: Strictly follow semver

### Version Bump Guidelines

| Change Type | Version Bump | Example |
|-------------|--------------|---------|
| Breaking CLI change | MAJOR | 1.0.0 → 2.0.0 |
| Breaking API change | MAJOR | 1.0.0 → 2.0.0 |
| New command/feature | MINOR | 0.1.0 → 0.2.0 |
| New library feature | MINOR | 0.1.0 → 0.2.0 |
| Bug fix, patch | PATCH | 0.1.0 → 0.1.1 |
| Documentation only | PATCH | 0.1.0 → 0.1.1 |
| Security fix | PATCH | 0.1.0 → 0.1.1 |
| Performance improvement | PATCH | 0.1.0 → 0.1.1 |

---

## Pre-Release Checklist

### 0. Pre-flight Checks

```bash
just release-check  # Comprehensive validation
```

- [ ] Git working directory is clean (`git status`)
- [ ] On `main` branch
- [ ] CI is passing on main branch (`just ci-status`)
- [ ] `just release-check` completes successfully

### 1. Codebase Hygiene

```bash
just wip-check      # TODO/FIXME/XXX/HACK, todo!/unimplemented!
just panic-audit    # .unwrap()/.expect() audit
```

- [ ] No blocking `todo!()` or `unimplemented!()` in production code
- [ ] All `.unwrap()` and `.expect()` calls reviewed for safety
- [ ] WIP comments reviewed (TODO, FIXME, XXX, HACK)

### 2. Code Quality

```bash
just clippy         # Clippy with warnings-as-errors
just clippy-strict  # Pedantic lints
just typos          # Spell checking
```

- [ ] No clippy warnings
- [ ] No typos in code or documentation
- [ ] Code passes pedantic lints (or exclusions documented)

### 3. Testing

```bash
just test-locked    # Tests with locked dependencies
just test-features  # All feature combinations
just miri           # Undefined behavior detection (optional)
just careful        # Extra safety checks (optional)
```

- [ ] All tests pass
- [ ] All feature combinations compile
- [ ] No undefined behavior (miri check)

#### Feature-Specific Testing

rexpipe has several optional features. Ensure key features are tested:

| Feature | Test Command | Notes |
|---------|--------------|-------|
| Default | `just test` | No optional features |
| tree-sitter | `just test-with "tree-sitter"` | Syntax-aware scoping |
| pcre | `just test-with "pcre"` | PCRE regex engine |
| async | `just test-with "async"` | Async I/O via tokio |
| watch | `just test-with "watch"` | File watching via notify |
| remote | `just test-with "remote"` | Remote file fetching |
| fpe | `just test-with "fpe"` | Format-preserving encryption |
| All | `just test` | All features combined |

```bash
# Quick feature matrix test
just test-features
```

### 4. Version Consistency

```bash
just version-sync   # Check README matches Cargo.toml
```

Verify version is consistent in:
- [ ] `Cargo.toml`
- [ ] README.md installation instructions (if version-specific)
- [ ] CHANGELOG.md has entry with correct date

### 5. Security & Dependencies

```bash
just deny    # Licenses, bans, advisories
just audit   # Security vulnerabilities
just machete # Unused dependencies
```

- [ ] No license violations
- [ ] No banned dependencies
- [ ] No unaddressed security advisories
- [ ] No unused dependencies

### 6. Documentation

```bash
just doc-check   # Documentation builds without warnings
just link-check  # Markdown link validation (if lychee installed)
```

- [ ] Documentation builds without warnings
- [ ] Internal links resolve correctly
- [ ] CHANGELOG.md updated with new version section
- [ ] Breaking changes have migration notes
- [ ] All public APIs documented

### 7. API Compatibility

```bash
just semver    # Breaking change detection
```

- [ ] No unintended breaking changes
- [ ] Version bump accounts for any breaking changes
- [ ] Public API surface reviewed
- [ ] CLI interface changes documented

### 8. MSRV Compliance

```bash
just msrv-check    # Compile with declared MSRV
```

- [ ] Compiles with MSRV (check Cargo.toml for current MSRV)
- [ ] No features requiring newer Rust version

### 9. Build Verification

```bash
just ci-release    # Full CI simulation
```

- [ ] Full CI pipeline passes
- [ ] Release builds succeed
- [ ] All feature combinations compile

### 10. Publishing Preparation

```bash
just metadata-check   # Verify crates.io metadata
just publish-dry      # Dry-run publish
```

- [ ] Required metadata present (description, license, repository)
- [ ] Keywords and categories appropriate
- [ ] Dry-run publish succeeds

---

## Feature-Specific Testing

rexpipe has several optional features that must be tested before release.

### Feature Matrix

Starting in 2.1.0, rexpipe has a two-tier feature split: `core` (WASM-safe
library only) and `cli` (everything in `core` plus the binary and its
filesystem/terminal-only dependencies). `default = ["cli"]`, so existing
`cargo build` and `cargo install` users see identical behavior.

| Feature | Description | Test Command | Dependencies | Notes |
|---------|-------------|--------------|--------------|-------|
| `default` (`cli`) | Full CLI binary + library | `cargo test` | All below | Behaves like pre-2.1.0 default |
| `core` | WASM-safe library entry point | `cargo test --lib --no-default-features --features core` | regex, fancy-regex, serde, toml, web-time | Consumed by rexpipe-playground via WASM |
| `async` | Async I/O via Tokio | `cargo test --features async` | tokio | For streaming |
| `fpe` | Format-preserving encryption | `cargo test --features fpe` | fpe, aes | Data masking |
| `tree-sitter` | Syntax-aware scoping | `cargo test --features tree-sitter` | tree-sitter-* | Multi-language |
| `remote` | Remote file fetching | `cargo test --features remote` | ureq | HTTP/HTTPS |
| `watch` | File watching | `cargo test --features watch` | notify | Live reload |
| `all` | All features combined | `cargo test --all-features` | All above | Comprehensive |

> **Note on PCRE:** `fancy-regex` is now an unconditional dependency and
> rexpipe auto-detects which engine to use per pattern. There is no
> separate `pcre` feature — it was removed in 2.1.0.

### Tree-sitter Language Support

The `tree-sitter` feature includes parsers for multiple languages:

| Language | Parser Crate | Test |
|----------|-------------|------|
| Rust | tree-sitter-rust | `cargo test --features tree-sitter -- rust` |
| Python | tree-sitter-python | `cargo test --features tree-sitter -- python` |
| JavaScript | tree-sitter-javascript | `cargo test --features tree-sitter -- javascript` |
| TypeScript | tree-sitter-typescript | `cargo test --features tree-sitter -- typescript` |
| Go | tree-sitter-go | `cargo test --features tree-sitter -- go` |
| JSON | tree-sitter-json | `cargo test --features tree-sitter -- json` |
| YAML | tree-sitter-yaml | `cargo test --features tree-sitter -- yaml` |

### Feature Combination Testing

```bash
# Test no-default-features compiles (minimal build)
cargo check --no-default-features

# Test each feature in isolation
for feature in async pcre fpe tree-sitter remote watch; do
    cargo test --no-default-features --features "$feature"
done

# Test common feature combinations
cargo test --features "async,remote"       # Async remote fetching
cargo test --features "pcre,tree-sitter"   # Advanced regex + AST
cargo test --features "fpe,tree-sitter"    # Encryption with AST scoping

# Test full feature set (pre-release requirement)
cargo test --all-features
```

The `just test-features` recipe runs all combinations automatically.

---

## Changelog Generation

rexpipe uses [git-cliff](https://git-cliff.org/) for automated changelog generation.

### Generating the Changelog

```bash
# Generate/update CHANGELOG.md from git history
just changelog

# Preview what would be added for next release
just changelog-preview
```

### Version Bumping

Use the version-bump recipe to automatically update the version:

```bash
# Bump patch version (0.1.0 → 0.1.1)
just version-bump patch

# Bump minor version (0.1.0 → 0.2.0)
just version-bump minor

# Bump major version (0.1.0 → 1.0.0)
just version-bump major
```

After bumping, remember to update CHANGELOG.md with the release date and verify the changes.

---

## Release Workflow

**Publishing to crates.io is IRREVERSIBLE.** Follow this exact sequence:

```
┌─────────────────────────────────────────────────────────────┐
│  1. PREPARE: just release-check                             │
│                         ↓                                   │
│  2. VERSION: Edit Cargo.toml + CHANGELOG                    │
│                         ↓                                   │
│  3. COMMIT: git commit -m "chore: release vX.Y.Z"           │
│                         ↓                                   │
│  4. PUSH: git push origin main                              │
│                         ↓                                   │
│  5. WAIT: gh run watch (CI must pass)                       │
│                         ↓                                   │
│  6. TAG: just tag (creates vX.Y.Z)                          │
│                         ↓                                   │
│  7. RELEASE: git push origin vX.Y.Z                         │
│              (triggers automated release)                   │
└─────────────────────────────────────────────────────────────┘
```

### Step-by-Step Commands

#### Step 1: Pre-release Validation

```bash
just release-check
```

#### Step 2: Prepare Version

Edit `Cargo.toml`:

```toml
[package]
version = "X.Y.Z"
```

Update `CHANGELOG.md`:

```markdown
## [Unreleased]

## [X.Y.Z] - YYYY-MM-DD

### Added
- ...

### Changed
- ...

### Fixed
- ...
```

#### Step 3-5: Commit, Push, and Wait

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "chore: release vX.Y.Z"
git push origin main

# Wait for CI to pass
gh run watch                    # Interactive watch
# OR
gh run list --limit 1           # Check status
# OR
just ci-status                  # Quick check
```

#### Step 6-7: Tag and Release

```bash
# ONLY after CI passes on main!
just tag                        # Creates annotated tag vX.Y.Z
git push origin vX.Y.Z          # Triggers release workflow
```

---

## Automated vs Manual Release

| Aspect | Automated (tag push) | Manual (`just publish`) |
|--------|---------------------|------------------------|
| **Trigger** | Push `vX.Y.Z` tag | Run command locally |
| **CI checks** | Run automatically before publish | Must run manually first |
| **GitHub Release** | Created automatically | Must create manually |
| **Recommended** | **Always use this** | Last resort only |

**Recommendation:** Always use automated release via tag push. Only use manual process for disaster recovery when CI is down or partially failed.

---

## Post-Release Verification

### Immediate Checks (within 5 minutes)

```bash
# Verify GitHub release was created
gh release view vX.Y.Z

# Verify crate is on crates.io
cargo search rexpipe

# Verify crate is usable
cd /tmp && cargo new test-release && cd test-release
cargo add rexpipe@X.Y.Z
cargo check
cd - && rm -rf /tmp/test-release
```

- [ ] GitHub release exists
- [ ] `cargo search rexpipe` shows correct version
- [ ] `cargo add rexpipe` works in fresh project

### Delayed Checks (15-30 minutes)

```bash
# Check docs.rs (takes time to build)
curl -I https://docs.rs/rexpipe/X.Y.Z

# Check badges
curl -I https://img.shields.io/crates/v/rexpipe.svg
```

- [ ] docs.rs documentation is built and accessible
- [ ] README badges show correct version

### Repository Cleanup

- [ ] Update `[Unreleased]` section in CHANGELOG for next cycle
- [ ] Close related milestones/issues
- [ ] Announce release (if applicable)

---

## CI Automation Coverage

The following checks are **automated in CI** (triggered on push/PR to main):

| Check | Workflow | Local Recipe | Trigger |
|-------|----------|--------------|---------|
| Format | ci.yml | `just fmt-check` | Push/PR |
| Clippy | ci.yml | `just clippy` | Push/PR |
| Tests | ci.yml | `just test-locked` | Push/PR |
| Feature combos | ci.yml | `just test-features` | Push/PR |
| License/deps | ci.yml | `just deny` | Push/PR |
| Doc build | ci.yml | `just doc-check` | Push/PR |
| MSRV | ci.yml | `just msrv-check` | Push/PR |
| Security audit | audit.yml | `just audit` | Daily/Push |
| Semver check | semver.yml | `just semver` | Push/PR |

**Release workflow (on tag push):**

| Step | Automated |
|------|-----------|
| Create GitHub release | Yes |
| Publish to crates.io | Yes |

**Still requires manual verification:**
- Version string updates in documentation
- Post-release installation test
- Announcement/communication

---

## Manual Recovery Procedures

> **WARNING: Manual publishing should be a LAST RESORT only.**
>
> The automated GitHub Actions workflow is the **only** sanctioned way to publish.
> Manual publishing bypasses CI checks and has historically caused broken releases.
>
> **Only use manual publishing when:**
> - GitHub Actions is completely down/unavailable
> - The automated workflow failed mid-publish
> - You have explicitly verified ALL checks pass locally with `just release-check`

### If Tag Was Pushed But Release Failed Completely

```bash
# 1. Delete the remote tag
git push --delete origin vX.Y.Z

# 2. Delete the local tag
git tag -d vX.Y.Z

# 3. Fix the issue

# 4. Recreate tag and push
just tag
git push origin vX.Y.Z
```

### If GitHub Release Was Created But Publish Failed

```bash
# 1. Delete the GitHub release (keeps tag)
gh release delete vX.Y.Z --yes

# 2. Re-trigger workflow by force-pushing the tag
git push --delete origin vX.Y.Z
git push origin vX.Y.Z
```

### Rate Limited by crates.io

**Cause:** crates.io limits new crate publications.

**Fix:** Wait for the time specified in the error message, then retry:

```bash
cargo publish
```

---

## Justfile Recipe Reference

### Pre-Release Validation

| Section | Recipe | What It Does |
|---------|--------|--------------|
| **Full Validation** | `just release-check` | Complete release readiness check |
| CI Status | `just ci-status` | Check CI passed on main |
| Code hygiene | `just wip-check` | Find TODO/FIXME/todo!/unimplemented! |
| Panic audit | `just panic-audit` | Find .unwrap()/.expect() |
| Typos | `just typos` | Spell check code and docs |

### Version & Changelog

| Section | Recipe | What It Does |
|---------|--------|--------------|
| Changelog | `just changelog` | Generate changelog with git-cliff |
| Preview | `just changelog-preview` | Preview unreleased changes |
| Version bump | `just version-bump [level]` | Bump version (major/minor/patch) |

### CI Simulation

| Section | Recipe | What It Does |
|---------|--------|--------------|
| Standard CI | `just ci` | Match GitHub Actions CI |
| Fast CI | `just ci-fast` | Quick checks (no tests) |
| Full CI | `just ci-full` | CI + security audit |
| Release CI | `just ci-release` | Full release validation |

### Testing

| Section | Recipe | What It Does |
|---------|--------|--------------|
| Standard | `just test` | Run tests (all features) |
| Locked | `just test-locked` | Run tests with locked deps |
| Feature matrix | `just test-features` | Test all feature combinations |
| Miri | `just miri` | Undefined behavior detection |
| Careful | `just careful` | Extra safety checks |
| With features | `just test-with "..."` | Test with specific features |

### Coverage

| Section | Recipe | What It Does |
|---------|--------|--------------|
| HTML report | `just coverage` or `just cov` | Generate HTML coverage |
| LCOV | `just coverage-lcov` or `just cov-lcov` | Generate LCOV format |
| Summary | `just coverage-summary` or `just cov-summary` | Terminal summary |

### Security & Dependencies

| Section | Recipe | What It Does |
|---------|--------|--------------|
| Licenses | `just deny` | License/ban/advisory check |
| Vulnerabilities | `just audit` | Security vulnerability scan |
| Unused deps | `just machete` | Fast unused dependency check |
| Dep tree | `just tree-duplicates` | Show duplicate dependencies |

### Documentation

| Section | Recipe | What It Does |
|---------|--------|--------------|
| Doc check | `just doc-check` | Build docs without warnings |
| Link check | `just link-check` | Validate markdown links |
| Version sync | `just version-sync` | Check version consistency |

### Semver & Compatibility

| Section | Recipe | What It Does |
|---------|--------|--------------|
| Semver | `just semver` | Breaking change detection |
| MSRV | `just msrv-check` | Compile with MSRV |
| Features | `just test-features` | Check feature combinations |

### Publishing

| Section | Recipe | What It Does |
|---------|--------|--------------|
| Metadata | `just metadata-check` | Verify crates.io metadata |
| Dry run | `just publish-dry` | Test publish without uploading |
| Publish | `just publish` | Publish to crates.io (**LAST RESORT**) |
| Tag | `just tag` | Create annotated version tag |

### Development

| Section | Recipe | What It Does |
|---------|--------|--------------|
| Run | `just run [args]` | Run in debug mode |
| Debug | `just run-debug [args]` | Run with RUST_LOG=debug |
| Trace | `just run-trace [args]` | Run with RUST_LOG=trace |
| Release | `just run-release [args]` | Run in release mode |
| Watch | `just watch` | Watch mode (re-run tests) |
| Fix | `just fix` | Auto-fix all issues |

---

## Troubleshooting

### docs.rs Build Failed

**Cause:** Documentation requires features or dependencies not available in docs.rs environment.

**Fix:**
1. Check docs.rs build logs
2. Add `[package.metadata.docs.rs]` configuration if needed:
   ```toml
   [package.metadata.docs.rs]
   all-features = true
   ```
3. Ensure all doc examples compile (`cargo test --doc`)

### GitHub Release Not Created

**Cause:** Release workflow failed or tag format incorrect.

**Fix:**
1. Verify tag format is `vX.Y.Z` (not `X.Y.Z` or other variants)
2. Check workflow logs in GitHub Actions
3. Manually create release if needed:
   ```bash
   gh release create vX.Y.Z --generate-notes
   ```

### Semver Check Fails

**Cause:** Unintended breaking changes detected.

**Fix:**
1. Review the semver report: `just semver`
2. If changes are intentional, bump MAJOR version
3. If changes are unintentional, fix the API to maintain compatibility

### Local CI Passes But Remote Fails

**Cause:** Environment differences between local and CI.

**Fix:**
1. Ensure Cargo.lock is committed
2. Use `just test-locked` instead of `just test`
3. Run `just setup` to install same tool versions
4. Check for platform-specific issues

---

## Platform-Specific Notes

### Linux

- **Primary platform**: Full functionality, all features supported
- **Tree-sitter**: Requires C compiler for building parsers
- **PCRE**: Uses fancy-regex (pure Rust), no system PCRE needed
- **Static builds**: Use musl target for fully static binaries

```bash
# Build static Linux binary
cargo build --release --target x86_64-unknown-linux-musl

# Install C compiler for tree-sitter (Debian/Ubuntu)
sudo apt install build-essential
```

### macOS

- **Fully supported**: All features work on macOS
- **Universal binaries**: Build for both x86_64 and aarch64
- **File watching**: Uses FSEvents via notify crate

```bash
# Build for Apple Silicon
cargo build --release --target aarch64-apple-darwin

# Build for Intel
cargo build --release --target x86_64-apple-darwin
```

### Windows

- **Fully supported**: All features work on Windows
- **File watching**: Uses ReadDirectoryChangesW via notify crate
- **MSVC recommended**: Use the MSVC toolchain for best compatibility

```bash
# Build with MSVC toolchain
cargo build --release --target x86_64-pc-windows-msvc
```

### Cross-Compilation

```bash
# Install cross for cross-compilation
cargo install cross

# Build for different targets
cross build --release --target x86_64-unknown-linux-musl
cross build --release --target aarch64-unknown-linux-gnu
```

---

## Security Incident Response

This section documents procedures for handling security vulnerabilities in released versions.

### Severity Assessment

| Severity | CVSS Score | Response Time | Examples |
|----------|------------|---------------|----------|
| **Critical** | 9.0-10.0 | Immediate (same day) | Code injection via regex, encryption bypass |
| **High** | 7.0-8.9 | 24-48 hours | Path traversal, sensitive data exposure |
| **Medium** | 4.0-6.9 | 1 week | DoS via regex, information disclosure |
| **Low** | 0.1-3.9 | Next release | Minor information disclosure |

### Security Release Process

1. **Assess and Confirm**
   - Verify the vulnerability is real and reproducible
   - Determine affected versions and severity
   - Check if actively exploited

2. **Develop Fix**
   - Create fix on private branch
   - Ensure fix doesn't introduce new issues
   - Prepare minimal, targeted patch

3. **Coordinate Disclosure** (for Critical/High)
   - Notify affected downstream users privately if known
   - Coordinate with security researchers if externally reported
   - Prepare security advisory

4. **Release Security Patch**
   - Follow standard release process with expedited timeline
   - Use PATCH version bump (e.g., 0.1.0 → 0.1.1)
   - Document as security fix in CHANGELOG

5. **Post-Release**
   - Publish GitHub Security Advisory
   - Request CVE if applicable
   - Update RustSec advisory database

### Security Advisory Template

```markdown
## Security Advisory: [Brief Description]

**Severity**: [Critical/High/Medium/Low]
**CVE**: [CVE-YYYY-NNNNN or "Pending"]
**Affected Versions**: [e.g., < 0.1.1]
**Fixed Versions**: [e.g., >= 0.1.1]

### Description

[Detailed description of the vulnerability]

### Impact

[What can an attacker do with this vulnerability]

### Mitigation

[Immediate steps users can take before updating]

### Resolution

Update to version X.Y.Z or later:
\`\`\`bash
cargo update -p rexpipe
\`\`\`

### Credits

[Acknowledge reporters if they consent]
```

### Yanking Considerations

For severe security issues, yank affected versions:

```bash
cargo yank --version 0.X.Y rexpipe
```

**Note:** Yanking prevents new installations but doesn't break existing `Cargo.lock` files.

---

## Lessons Learned

This section documents issues encountered in past releases and patterns to avoid.

### 1. CI Parity is Non-Negotiable

**Issue**: Local tests pass but CI fails due to environment differences.

**Solution**: Always run `just ci` before pushing. Use `just test-locked` to match CI behavior exactly.

### 2. Feature Isolation Testing

**Issue**: Code compiles with `--all-features` but fails with specific feature combinations.

**Solution**: Run `just test-features` which tests each feature in isolation and common combinations.

### 3. Tree-sitter Build Dependencies

**Issue**: Tree-sitter feature fails to build on systems without a C compiler.

**Solution**: Document the C compiler requirement. Consider adding a CI job that tests without build tools to catch this.

### 4. Regex Catastrophic Backtracking

**Issue**: Certain user-provided regex patterns can cause exponential time complexity.

**Solution**: The regex crate has built-in protections. When using `pcre` feature with fancy-regex, patterns are still protected by timeouts.

### 5. MSRV Compliance

**Issue**: Accidentally using newer Rust features breaks MSRV compatibility.

**Solution**: Run `just msrv-check` before every release. CI enforces this automatically.

### 6. Tag Format Consistency

**Issue**: Tags without `v` prefix don't trigger release workflow.

**Solution**: Always use `just tag` which enforces the `vX.Y.Z` format.

### 7. Changelog Generation Order

**Issue**: Generating changelog after version bump includes the bump commit incorrectly.

**Solution**: Generate changelog first with `just changelog`, then bump version, then commit both together.

### 8. Binary Size Regression

**Issue**: New features unexpectedly increased binary size.

**Solution**: Use `just bloat` to analyze binary size before release. Tree-sitter adds significant size due to parser grammars.

### 9. File Watching Platform Differences

**Issue**: The `watch` feature behaves differently on macOS (FSEvents) vs Linux (inotify).

**Solution**: Test file watching on multiple platforms. CI includes cross-platform matrix testing.

### 10. FPE Key Material Handling

**Issue**: Format-preserving encryption requires careful key handling.

**Solution**: Document that keys should never be logged. The `fpe` feature uses secure memory handling from the aes crate.

---

## Release Checklist Template

Copy this for each release:

```markdown
## Release vX.Y.Z Checklist

### Pre-Release
- [ ] `just release-check` passes
- [ ] Version bumped in Cargo.toml
- [ ] CHANGELOG.md updated with date
- [ ] CI passing on main branch (`just ci-status`)

### Release Execution
- [ ] Release commit pushed to main
- [ ] CI passed on main (verified via `gh run watch`)
- [ ] Tag created with `just tag`
- [ ] Tag pushed to trigger release workflow
- [ ] Release workflow completed successfully

### Post-Release
- [ ] `cargo search rexpipe` shows new version
- [ ] `cargo add rexpipe` works in fresh project
- [ ] GitHub release exists
- [ ] docs.rs building/built (check after ~15 min)
- [ ] CHANGELOG [Unreleased] section reset
```

---

## Additional Resources

- [Publishing on crates.io](https://doc.rust-lang.org/cargo/reference/publishing.html) - Official documentation
- [Semver Specification](https://semver.org/) - Semantic Versioning 2.0.0
- [Keep a Changelog](https://keepachangelog.com/) - Changelog format guidelines
