## ADDED Requirements

### Requirement: TypeScript upgraded to 6.0
The project SHALL upgrade TypeScript from 5.9 to 6.0 in both `interface/` and `docs/`. The tsconfig files SHALL be updated to explicitly set `"types": ["vite/client"]` (interface) and any other necessary defaults that changed in TS6.

#### Scenario: Interface typecheck passes
- **WHEN** `bunx tsc --noEmit` is run in `interface/`
- **THEN** the typecheck SHALL pass with zero errors

#### Scenario: Docs build passes
- **WHEN** `bun run build` is run in `docs/`
- **THEN** the Next.js static build SHALL complete successfully

### Requirement: Lucide-react upgraded to 1.8 in docs
The project SHALL upgrade lucide-react from 0.563 to 1.8 in the docs site. Any usage of removed brand icons (Chromium, Codepen, Codesandbox, Dribbble, Facebook, Figma, Framer, Github, Gitlab, Instagram, LinkedIn, Pocket, Slack) SHALL be replaced with alternatives.

#### Scenario: No removed brand icons in use
- **WHEN** a grep for removed brand icon names is run across `docs/`
- **THEN** zero matches SHALL be found

#### Scenario: Docs build succeeds after upgrade
- **WHEN** `bun run build` is run in `docs/`
- **THEN** the build SHALL complete successfully
