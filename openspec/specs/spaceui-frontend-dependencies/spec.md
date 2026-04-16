# spaceui-frontend-dependencies Specification

## Purpose
TBD - created by archiving change spacebot-dependency-remediation. Update Purpose after archive.
## Requirements
### Requirement: Vite in spaceui workspace exits GHSA-4w7w-66w2-5vf9 vulnerability ranges
The three `package.json` files in `spaceui/`, `spaceui/.storybook/`, and `spaceui/examples/showcase/` that declare a `vite` dependency SHALL resolve via `spaceui/bun.lock` to a `vite` version that falls outside every range declared in GHSA-4w7w-66w2-5vf9 (i.e., outside `<= 6.4.1`, outside `[7.0.0, 7.3.1]`, and outside `[8.0.0, 8.0.4]`). The minimum acceptable version is 6.4.2.

#### Scenario: Lockfile resolves to safe version
- GIVEN `spaceui/`, `spaceui/.storybook/`, and `spaceui/examples/showcase/` package.json files are updated to allow resolution to 6.4.2+
- WHEN `bun install` is run in `spaceui/`
- THEN `spaceui/bun.lock` pins `vite` to a version that is not `<= 6.4.1`, not in `[7.0.0, 7.3.1]`, and not in `[8.0.0, 8.0.4]`

#### Scenario: Storybook manifest version range updated
- GIVEN the vite upgrade is being applied
- WHEN `spaceui/.storybook/package.json` is read
- THEN the `vite` version specifier allows resolution to 6.4.2 or later

#### Scenario: Showcase manifest version range updated
- GIVEN the vite upgrade is being applied
- WHEN `spaceui/examples/showcase/package.json` is read
- THEN the `vite` version specifier allows resolution to 6.4.2 or later

### Requirement: Storybook remains functional after vite upgrade
The spaceui storybook SHALL start or build without fatal errors after the vite version is upgraded. Because storybook 8.6.18 predates full vite 6.x support, this requirement may necessitate an accompanying storybook upgrade to a version that supports vite 6+.

#### Scenario: Storybook starts via workspace script
- GIVEN the vite upgrade is applied and (if needed) storybook is upgraded to a vite-6-compatible version
- WHEN `bun run storybook` is invoked from the `spaceui/` workspace root (which executes `cd .storybook && bun run dev` per `spaceui/package.json`)
- THEN the command produces no fatal errors and reports a running dev server

#### Scenario: Storybook incompatibility is documented, not dismissed
- GIVEN the vite upgrade breaks storybook in a way that cannot be resolved within this change
- WHEN the change is finalized
- THEN the storybook-related Dependabot alert is left open and the blocker is documented in `docs/security/deferred-advisories.md`, and no Dependabot or CodeQL dismissal API call is made

### Requirement: Showcase demo builds after vite upgrade
The spaceui showcase demo SHALL build to a static bundle without fatal errors after the vite version is upgraded.

#### Scenario: Showcase production build succeeds
- GIVEN the vite upgrade is applied
- WHEN `bun run build` is executed in `spaceui/examples/showcase/`
- THEN the command exits 0 and produces bundle output in `dist/` (or the configured output directory)

