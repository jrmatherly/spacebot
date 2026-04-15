---
name: deps-update
description: Update and audit project dependencies across Rust, frontend, and docs. Use when the user mentions dependency updates, package upgrades, security vulnerabilities in deps, Dependabot alerts, cargo update, bun update, outdated packages, or wants to patch known CVEs. Also use proactively when cargo audit or bun audit reveals fixable vulnerabilities.
---

## Dependency Update Workflow

Update all project dependencies safely, audit for vulnerabilities, and verify nothing breaks. This skill covers Rust (cargo), frontend (bun/interface), and docs (bun/docs).

The core principle: update everything that's semver-compatible first (zero risk), then research and selectively bump version constraints for security patches.

### Modes

**Safe mode** (default) — only semver-compatible updates within existing Cargo.toml and package.json constraints. No version constraint changes. No breaking changes possible.

**Upgrade mode** (when the user asks to fix specific vulnerabilities or upgrade specific packages) — research breaking changes, bump version constraints, verify builds. Requires explicit user approval before applying.

### Step 1: Audit current state

Run all audits first to establish a baseline:

```bash
# Rust
cargo audit 2>&1 | tee /tmp/cargo-audit-before.txt
cargo audit 2>&1 | grep -c "^error:" || echo "0 vulnerabilities"

# Frontend (interface/)
cd interface && bun audit 2>&1 | tee /tmp/bun-audit-interface-before.txt; cd ..

# Docs (docs/)
cd docs && bun audit 2>&1 | tee /tmp/bun-audit-docs-before.txt; cd ..
```

Save the counts for the before/after comparison.

### Step 2: Safe updates (Rust)

```bash
# Preview what will change
cargo update --dry-run 2>&1 | grep -c "Updating"

# Apply semver-compatible updates
cargo update

# Verify compilation
cargo check --all-targets

# Verify lints
cargo clippy --all-targets

# Run tests
cargo test --lib
```

If `cargo check` fails, the update introduced a breaking transitive dependency. This is rare but possible when a dependency's dependency makes a semver-incompatible change. If this happens, use `cargo update --precise` to pin the offending crate to the last working version and report it.

### Step 3: Safe updates (Frontend)

```bash
# interface/
cd interface
bun update
bunx tsc --noEmit  # typecheck
cd ..

# docs/
cd docs
bun update
bun run build      # verify docs build
cd ..
```

The docs build depends on the spaceui sibling repo for the interface Vite build, but the docs site (Next.js + Fumadocs) builds independently. The interface cannot run `bun run build` in CI due to the spaceui dependency — typecheck is the verification gate there.

### Step 4: Re-audit and compare

```bash
# Rust
cargo audit

# Frontend
cd interface && bun audit; cd ..
cd docs && bun audit; cd ..
```

Compare vulnerability counts against the baseline from Step 1. Report what was fixed and what remains.

### Step 5: Upgrade mode (if requested)

When vulnerabilities remain that need version constraint bumps:

1. **Identify the fix version** — check the advisory for the patched version range
2. **Check if it's a direct or transitive dependency** — `cargo audit` shows the dependency tree
3. **For direct deps** — bump the version in Cargo.toml, run `cargo update`, verify
4. **For transitive deps** — check if the parent dependency has a newer version that pulls in the fix. If not, it's an upstream issue — document it and move on
5. **For npm packages** — check `npm view <package> versions` for available patches. Research breaking changes before bumping across minor/major versions. Use web search to find release notes
6. **Verify after each bump** — `cargo check`, `cargo clippy`, `cargo test --lib` for Rust. `bunx tsc --noEmit` and `bun run build` (docs only) for frontend

Always research breaking changes before bumping across minor or major versions. Check:
- Release notes / changelogs
- Whether the project uses any APIs that changed
- Whether the package is pinned for a reason (check git blame on the version constraint)

### Step 6: Commit

Stage only dependency files:

```bash
# Rust
git add Cargo.lock

# Frontend (only if package.json was modified in upgrade mode)
git add interface/bun.lock interface/package.json
git add docs/bun.lock docs/package.json
```

Commit with a descriptive message listing what was patched:

```
deps: update dependencies, patch N security vulnerabilities

Rust: cargo update resolves X semver-compatible updates including:
- <crate> <old> -> <new> (<advisory summary>)

docs/:
- <package> <old> -> <new> (<advisory summary>)

Verified: cargo check, cargo clippy, cargo test --lib (N passed),
docs build, interface typecheck.

Remaining unpatched: <list any that couldn't be fixed and why>
```

### Verification checklist

Before committing, verify ALL of these pass:

- [ ] `cargo check --all-targets`
- [ ] `cargo clippy --all-targets`
- [ ] `cargo test --lib`
- [ ] `cargo audit` (fewer or equal vulnerabilities vs. baseline)
- [ ] `cd interface && bunx tsc --noEmit`
- [ ] `cd docs && bun run build`
- [ ] `bun audit` in docs/ and interface/ (fewer or equal vs. baseline)

### Known constraints

- `protobuf 2.28.0` is pinned by chromiumoxide and can't be updated without a chromiumoxide major bump
- The interface Vite build requires the spaceui sibling repo and won't work in CI or on machines without it
- fumadocs-core@17.0.0 is an accidental publish (deprecated on npm) — 16.7.15 is the actual latest
- `pnpm-lock.yaml` in docs/ is a legacy upstream artifact — we use bun exclusively

### Package managers

- Rust: `cargo` (always)
- Frontend: `bun` (NEVER npm, pnpm, or yarn)
