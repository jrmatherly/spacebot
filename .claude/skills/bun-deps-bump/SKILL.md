---
name: bun-deps-bump
description: Use when bumping a bun-managed dependency in interface/, docs/, packages/api-client/, or spaceui/. Avoids the "lockfile updated, manifest spec stale" drift class that opened PRs #124, #125, #130, #131 in 2026-04-26 even after the lockfile bumps had landed. Triggered when the user asks to "update vitest", "bump fumadocs", "upgrade <pkg>", or otherwise mentions a frontend-side dep bump.
---

# bun-deps-bump

Bump a bun-managed dependency so that **both** `package.json` (the spec range) **and** `bun.lock` (the resolved version) move. The lockfile alone is not enough — dependabot scans `package.json` and will reopen PRs forever if the spec range still allows the old version.

## The semantics gotcha

`bun update` has three behaviors depending on the spec format. This is the table that bit us in 2026-04-26 (cost ~30 minutes + 4 reopened dependabot PRs):

| Spec format | `bun update <pkg>` | `bun update <pkg> --latest` |
|---|---|---|
| `"^3.2.4"` | Bumps lockfile to latest within `^3.x`. **Spec stays.** | Bumps to latest on npm + **rewrites spec** to new major. |
| `"3.2.4"` (exact) | **No-op.** Cannot move within zero range. | Bumps to latest + **rewrites spec to exact-of-new-version**. |
| `"~3.2.4"` | Bumps lockfile to latest within `~3.2.x`. **Spec stays.** | Same as `^`. |

**Rule of thumb**: use `--latest` for any major-version bump OR for exact-pinned packages. Plain `bun update` is only correct for "pull patches within current spec range."

## Workspaces in scope

| Workspace | Where to run | Test command |
|---|---|---|
| `interface/` | `cd interface && bun update <pkg> --latest` | `bunx vitest run` |
| `docs/` | `cd docs && bun update <pkg> --latest` | `bun run build` |
| `packages/api-client/` | `cd packages/api-client && bun update <pkg> --latest` | `bun run test` |
| `spaceui/` | `cd spaceui && bun update <pkg> --latest` (then `just spaceui-build`) | `just spaceui-gate` |

Note: spaceui has its own `bun.lock` and its own root manifest. `interface/` declares `../spaceui/packages/*` and `../packages/*` as workspace members — bun resolves `@spacedrive/*` and `@spacebot/*` via local symlink. Do not bump packages from inside `interface/` if they belong to spaceui or packages/api-client.

## Procedure

```bash
# 1. Bump the spec + lockfile in one shot
cd <workspace>
bun update <pkg> --latest

# 2. Verify BOTH moved (catches the silent-no-op case for exact pins)
grep '"<pkg>"' package.json
grep -E '"<pkg>@' bun.lock | head -1

# 3. If only bun.lock moved, the spec was an exact pin and `bun update`
#    silently no-op'd. Edit package.json by hand to the new version and
#    re-run `bun install`.

# 4. Run workspace-appropriate tests (see table above)

# 5. Commit BOTH package.json AND bun.lock
git add <workspace>/package.json <workspace>/bun.lock
git commit -m "deps(<workspace>): bump <pkg> <old> → <new>"
```

## When to expect breaking changes

Major bumps (e.g., 3.x → 4.x) need investigation BEFORE merge. Check:
- The package's CHANGELOG / migration guide
- Whether the package introduces new peer-dep requirements (vitest 4 needed Vite ≥ 6 + jsdom < 27)
- Whether transitive deps your code uses (e.g., `@radix-ui/*` under `@spacedrive/primitives`) hit React-resolution issues at runtime

Past sessions surface in `.serena/memories/phase11_dual_backend.md` and the `feat-/dependabot-/...` commit log. The vitest 3 → 4 upgrade in PR #125/#130 (commit `2b63a14`) needed a custom `reactSingletonPlugin` in `interface/vitest.config.ts` because of a workspace-symlink interaction with `spaceui/node_modules/.bun/react@19.2.5`. Read that commit message before any future Vite-ecosystem major bump.

## Type-check after a major-version bump

`bunx tsc --noEmit` validates against the type defs in `node_modules/.bun/<pkg>@<version>/`. After a major-version bump, the local node_modules cache may still hold the OLD version's type defs even after `bun.lock` resolves to the new version. Two failure modes:

1. **Local tsc passes, CI fails** — your local `node_modules/.bun/<pkg>@<oldver>/` is still on disk and gets resolved before the new version. CI does a clean install → uses only the new defs → catches stricter typing.
2. **Local tsc fails immediately** — the new defs are correctly resolved.

**Always** run a clean tsc verification after a major bump:

```bash
cd <workspace>
bun install --force      # rebuild node_modules from lockfile
bunx tsc --noEmit        # validates against the new type defs
```

Concrete example (caught at commit `4f6c1c8`, 2026-04-26): vitest 4 made `mock.calls` typing stricter than v3. A test using `consoleErrorSpy.mock.calls.find((call) => ...)` had a callback param that auto-inferred under v3 but fell back to implicit `any` under v4. Local tsc passed (vitest 3 type defs still cached); CI failed with `TS7006: Parameter 'call' implicitly has an 'any' type`. Fix: explicit `(call: unknown[])` annotation at the call site, matching the `as string` cast on the next line.

## Related dependabot pinning

If a major bump cannot land cleanly, document the deferral in `.github/dependabot.yml` with an `ignore` rule + an inline comment that names the unblock condition. Pattern from commit `faca85b` (jsonwebtoken + nom) and `2b63a14` (jsdom):

```yaml
ignore:
  # <pkg> X.x has breaking API changes in <areas>. Migration tracked
  # for after <unblock condition>. See PR #N for the failure surface.
  - dependency-name: "<pkg>"
    update-types: ["version-update:semver-major"]
```

## Related skills

- `dependabot-response` — triages individual dependabot PRs (SAFE-FOLD / DEFER / SKIP verdict)
- `dep-spec-auditor` (subagent) — parallel-audits all workspaces for manifest/lockfile drift at PR time
- `pr-gates` — final pre-push gate that includes `cd interface && bun run build`
